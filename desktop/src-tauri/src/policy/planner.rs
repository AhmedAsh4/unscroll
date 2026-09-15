use std::collections::BTreeSet;
use crate::{adb::{AppOp, AppOpMode, Component, PackageId, UserId}, device::DeviceSnapshot};
use super::{operation::{Operation, Plan, PlannedOperation, Requirement}, protection, stores};

const UNKNOWN_SOURCES: &str = "REQUEST_INSTALL_PACKAGES";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError { UnsupportedDevice, UnknownPackage, InconsistentFacts, InvalidRecordedOperation }

fn user(snapshot: &DeviceSnapshot) -> Result<UserId, PlanError> { UserId::parse(snapshot.current_user).map_err(|_| PlanError::UnsupportedDevice) }
fn known(snapshot: &DeviceSnapshot, package: &PackageId) -> bool { snapshot.catalog.iter().any(|app| app.package == *package) }
fn launcher() -> Component { Component::parse("org.unscroll.launcher/.MainActivity").expect("constant component") }
fn state(snapshot: &DeviceSnapshot, package: &PackageId) -> Result<bool, PlanError> { snapshot.catalog.iter().find(|app| app.package == *package).map(|app| app.suspended).ok_or(PlanError::UnknownPackage) }
fn policy(plan: &mut Plan, allowed: Vec<PackageId>, inverse: Vec<PackageId>) {
    plan.push(Operation::LauncherPolicy { allowed }, "valid inspected catalog", "launcher policy saved", Operation::LauncherPolicy { allowed: inverse }, Requirement::Required, "Update Unscroll allowlist");
}
fn suspension(plan: &mut Plan, package: PackageId, user: UserId, before: bool, requirement: Requirement) {
    transition(plan, package, user, true, before, requirement, "Suspend app");
}
fn transition(plan: &mut Plan, package: PackageId, user: UserId, target: bool, before: bool, requirement: Requirement, description: &str) {
    if target == before { return; }
    plan.push(Operation::Suspend { package: package.clone(), user, suspended: target }, "package remains installed", if target { "package suspended" } else { "package available" }, Operation::Suspend { package, user, suspended: before }, requirement, description);
}
fn validate_allowed(snapshot: &DeviceSnapshot, allowed: &[PackageId]) -> Result<Vec<PackageId>, PlanError> {
    if !snapshot.capabilities.package_suspension || !snapshot.capabilities.home_path || snapshot.current_user != 0 { return Err(PlanError::UnsupportedDevice); }
    if !snapshot.catalog.iter().any(|app| app.component == snapshot.home) || !snapshot.catalog.iter().any(|app| app.package.as_str() == "org.unscroll.launcher" && app.component == launcher()) { return Err(PlanError::UnknownPackage); }
    if snapshot.stores.iter().any(|fact| !known(snapshot, &fact.package)) || snapshot.install_sources.iter().any(|fact| !known(snapshot, &fact.package)) || snapshot.protected.iter().any(|fact| !known(snapshot, &fact.package)) { return Err(PlanError::InconsistentFacts); }
    if allowed.iter().any(|package| snapshot.stores.iter().any(|store| store.package == *package)) { return Err(PlanError::InconsistentFacts); }
    let protected = protection::protected(snapshot);
    if snapshot.stores.iter().any(|fact| protected.contains(fact.package.as_str())) { return Err(PlanError::InconsistentFacts); }
    let mut unique = BTreeSet::new();
    for package in allowed { if !known(snapshot, package) { return Err(PlanError::UnknownPackage); } unique.insert(package.as_str().to_owned()); }
    let baseline = snapshot.home.as_str().split('/').next().unwrap_or_default();
    for fact in &snapshot.protected { if fact.package.as_str() != baseline && fact.package.as_str() != "org.unscroll.launcher" { unique.insert(fact.package.as_str().to_owned()); } }
    Ok(unique.into_iter().filter(|name| name != "org.unscroll.launcher" && name != snapshot.home.as_str().split('/').next().unwrap_or_default()).map(|name| PackageId::parse(&name).expect("catalog package")).collect())
}

pub fn initial_apply(snapshot: &DeviceSnapshot, allowed: &[PackageId]) -> Result<Plan, PlanError> {
    let allowed = validate_allowed(snapshot, allowed)?;
    let user = user(snapshot)?;
    let mut plan = Plan::default();
    policy(&mut plan, allowed.clone(), Vec::new());
    let stores = stores::packages(snapshot);
    for app in &snapshot.catalog { if !allowed.contains(&app.package) && !stores.contains(app.package.as_str()) && protection::may_suspend(snapshot, &app.package) { suspension(&mut plan, app.package.clone(), user, app.suspended, Requirement::UserResolvable); } }
    for app in &snapshot.catalog { if stores.contains(app.package.as_str()) && !allowed.contains(&app.package) && protection::may_suspend(snapshot, &app.package) { suspension(&mut plan, app.package.clone(), user, app.suspended, Requirement::Required); } }
    if snapshot.capabilities.app_op_inspection {
        let op = AppOp::parse(UNKNOWN_SOURCES).expect("constant app-op");
        for source in &snapshot.install_sources { if source.allowed && protection::may_suspend(snapshot, &source.package) { plan.push(Operation::AppOp { package: source.package.clone(), user, app_op: op.clone(), mode: AppOpMode::Ignore }, "source app-op inspected", "unknown-source installs ignored", Operation::AppOp { package: source.package.clone(), user, app_op: op.clone(), mode: AppOpMode::Allow }, Requirement::Optional, "Disable unknown-source installs"); } }
    }
    plan.push(Operation::Verify, "planned changes applied", "policy, packages, stores and app-ops verified", Operation::Verify, Requirement::Required, "Verify policy changes");
    plan.push(Operation::Home { component: launcher() }, "aggregate verification passed", "Unscroll is HOME", Operation::Home { component: snapshot.home.clone() }, Requirement::Required, "Set Unscroll as HOME");
    Ok(plan)
}

pub fn edit(snapshot: &DeviceSnapshot, before: &[PackageId], after: &[PackageId]) -> Result<Plan, PlanError> {
    let before = validate_allowed(snapshot, before)?; let after = validate_allowed(snapshot, after)?;
    if before == after { return Ok(Plan::default()); }
    let user = user(snapshot)?; let mut plan = Plan::default(); policy(&mut plan, after.clone(), before.clone());
    for package in before.iter().filter(|package| !after.contains(package)) { if protection::may_suspend(snapshot, package) { transition(&mut plan, package.clone(), user, true, state(snapshot, package)?, Requirement::UserResolvable, "Suspend removed allowlist app"); } }
    for package in after.iter().filter(|package| !before.contains(package)) { if protection::may_suspend(snapshot, package) { transition(&mut plan, package.clone(), user, false, state(snapshot, package)?, Requirement::UserResolvable, "Restore added allowlist app"); } }
    Ok(plan)
}

pub fn maintenance(open: bool) -> Plan { let mut plan = Plan::default(); plan.push(Operation::Maintenance { open }, "valid recovery state", if open { "maintenance open" } else { "maintenance closed" }, Operation::Maintenance { open: !open }, Requirement::Required, if open { "Open maintenance" } else { "Close maintenance" }); plan }
pub fn rollback(recorded: &[PlannedOperation]) -> Result<Plan, PlanError> { full_restore(recorded) }
pub fn full_restore(recorded: &[PlannedOperation]) -> Result<Plan, PlanError> {
    if recorded.iter().any(|step| step.precondition.is_empty() || step.postcondition.is_empty() || step.description.is_empty()) { return Err(PlanError::InvalidRecordedOperation); }
    Ok(Plan { operations: recorded.iter().rev().map(|step| PlannedOperation { operation: step.inverse.clone(), precondition: step.postcondition.clone(), postcondition: step.precondition.clone(), inverse: step.operation.clone(), requirement: step.requirement, description: format!("Restore: {}", step.description) }).collect() })
}
