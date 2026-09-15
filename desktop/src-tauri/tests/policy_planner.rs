use unscroll_desktop_lib::{
    adb::{AppOpMode, Component, PackageId},
    device::{AppCatalogEntry, CapabilityReport, DeviceSnapshot, InstallSourceFact, ProfileFact,
        ProtectedPackageFact, RecoveryObservation, StoreFact},
    policy::{initial_apply, edit, full_restore, maintenance, rollback, Operation, Requirement},
};

fn package(value: &str) -> PackageId { PackageId::parse(value).unwrap() }
fn component(value: &str) -> Component { Component::parse(value).unwrap() }
fn snapshot() -> DeviceSnapshot {
    DeviceSnapshot {
        serial: "device".into(), api: 36, model: "model".into(), manufacturer: "maker".into(), fingerprint: "maker/device".into(), current_user: 0,
        home: component("com.baseline/.Main"),
        catalog: ["com.baseline", "com.social", "com.store", "com.permission", "org.unscroll.launcher"].into_iter().map(|name| AppCatalogEntry { package: package(name), component: component(&if name == "org.unscroll.launcher" { format!("{name}/.MainActivity") } else { format!("{name}/.Main") }), label: name.into(), icon: vec![], suspended: false, enabled: true }).collect(),
        protected: vec![ProtectedPackageFact { package: package("com.permission"), reason: "shared permission controller".into() }],
        stores: vec![StoreFact { package: package("com.store") }],
        install_sources: vec![InstallSourceFact { package: package("com.store"), allowed: true }], profiles: vec![ProfileFact { user_id: 0, name: "owner".into() }],
        private_recovery: RecoveryObservation::Missing, shared_recovery: RecoveryObservation::Missing,
        xiaomi_guidance: vec![], capabilities: CapabilityReport { package_suspension: true, bridge: true, recovery_storage: true, app_op_inspection: true, home_path: true },
    }
}

fn suspended(plan: &unscroll_desktop_lib::policy::Plan) -> Vec<String> {
    plan.operations.iter().filter_map(|step| match &step.operation { Operation::Suspend { package, suspended: true, .. } => Some(package.as_str().into()), _ => None }).collect()
}

#[test]
fn initial_apply_protects_even_explicit_or_mislabeled_inputs_and_orders_home_last() {
    let plan = initial_apply(&snapshot(), &[package("com.permission"), package("com.baseline"), package("org.unscroll.launcher"), package("com.social")]).unwrap();
    assert_eq!(suspended(&plan), vec!["com.store"]);
    assert!(plan.operations.iter().all(|step| !matches!(&step.operation, Operation::Suspend { package, .. } if package.as_str() == "com.baseline" || package.as_str() == "com.permission" || package.as_str() == "org.unscroll.launcher")));
    assert!(matches!(plan.operations.last().unwrap().operation, Operation::Home { .. }));
    assert!(plan.operations.iter().any(|step| matches!(step.operation, Operation::Suspend { ref package, .. } if package.as_str() == "com.store") && step.requirement == Requirement::Required));
    assert!(plan.operations.iter().any(|step| matches!(step.operation, Operation::AppOp { ref mode, .. } if *mode == AppOpMode::Ignore) && step.requirement == Requirement::Optional));
}

#[test]
fn stores_are_dynamic_and_edits_only_change_the_delta() {
    let before = initial_apply(&snapshot(), &[package("com.social")]).unwrap();
    let unchanged = edit(&snapshot(), &[package("com.social")], &[package("com.social")]).unwrap();
    assert!(unchanged.operations.is_empty());
    let delta = edit(&snapshot(), &[package("com.social")], &[]).unwrap();
    assert_eq!(suspended(&delta), vec!["com.social"]);
    assert!(suspended(&before).contains(&"com.store".into()));
}

#[test]
fn rejects_unknown_inputs_and_unsupported_app_ops() {
    assert!(initial_apply(&snapshot(), &[package("com.unknown")]).is_err());
    let mut unsupported = snapshot(); unsupported.capabilities.app_op_inspection = false;
    let plan = initial_apply(&unsupported, &[package("com.social")]).unwrap();
    assert!(!plan.operations.iter().any(|step| matches!(step.operation, Operation::AppOp { .. })));
}

#[test]
fn full_restore_reverses_the_recorded_operations_exactly() {
    let apply = initial_apply(&snapshot(), &[package("com.social")]).unwrap();
    let restore = full_restore(&apply.operations).unwrap();
    assert_eq!(restore.operations.iter().map(|step| &step.operation).collect::<Vec<_>>(), apply.operations.iter().rev().map(|step| &step.inverse).collect::<Vec<_>>());
}

#[test]
fn rejects_unknown_dynamic_facts_and_keeps_protected_apps_visible_but_protected_stores_block() {
    let mut unknown_store = snapshot(); unknown_store.stores.push(StoreFact { package: package("com.missing") });
    assert!(initial_apply(&unknown_store, &[]).is_err());
    let mut protected_store = snapshot(); protected_store.protected.push(ProtectedPackageFact { package: package("com.store"), reason: "manufacturer permission controller".into() });
    assert!(initial_apply(&protected_store, &[]).is_err());
    let plan = initial_apply(&snapshot(), &[]).unwrap();
    let policy = plan.operations.iter().find_map(|step| match &step.operation { Operation::LauncherPolicy { allowed } => Some(allowed), _ => None }).unwrap();
    assert!(policy.iter().any(|package| package.as_str() == "com.permission"));
    assert!(!policy.iter().any(|package| package.as_str() == "com.baseline" || package.as_str() == "org.unscroll.launcher"));
}

#[test]
fn validates_home_and_catalog_changes_and_has_complete_typed_steps() {
    let mut bad_home = snapshot(); bad_home.home = component("com.missing/.Home");
    assert!(initial_apply(&bad_home, &[]).is_err());
    let mut new_app = snapshot(); new_app.catalog.push(AppCatalogEntry { package: package("com.new"), component: component("com.new/.Main"), label: "New".into(), icon: vec![], suspended: false, enabled: true });
    assert!(suspended(&initial_apply(&new_app, &[]).unwrap()).contains(&"com.new".into()));
    let mut removed = snapshot(); removed.catalog.retain(|app| app.package.as_str() != "com.social");
    assert!(initial_apply(&removed, &[package("com.social")]).is_err());
    let applied = initial_apply(&snapshot(), &[]).unwrap();
    assert!(applied.operations.iter().all(|step| !step.precondition.is_empty() && !step.postcondition.is_empty() && !step.description.is_empty()));
    assert_eq!(rollback(&applied.operations).unwrap(), full_restore(&applied.operations).unwrap());
    let open = maintenance(true); let close = maintenance(false);
    assert!(matches!(open.operations[0].operation, Operation::Maintenance { open: true }));
    assert_eq!(open.operations[0].inverse, close.operations[0].operation);
}


#[test]
fn telephony_and_multiple_dynamic_stores_are_classified_without_static_names() {
    let mut device = snapshot();
    device.catalog.push(AppCatalogEntry { package: package("com.phone"), component: component("com.phone/.Main"), label: "Phone".into(), icon: vec![], suspended: false, enabled: true });
    device.catalog.push(AppCatalogEntry { package: package("com.oem.market"), component: component("com.oem.market/.Main"), label: "Manufacturer Market".into(), icon: vec![], suspended: false, enabled: true });
    device.protected.push(ProtectedPackageFact { package: package("com.phone"), reason: "telephony role".into() });
    device.stores.push(StoreFact { package: package("com.oem.market") });
    let plan = initial_apply(&device, &[]).unwrap();
    assert!(!suspended(&plan).contains(&"com.phone".into()));
    for package in ["com.store", "com.oem.market"] { assert!(plan.operations.iter().any(|step| matches!(&step.operation, Operation::Suspend { package: candidate, suspended: true, .. } if candidate.as_str() == package) && step.requirement == Requirement::Required)); }
}

#[test]
fn rejects_allowlisted_store_and_unknown_protected_or_unscroll_component() {
    assert!(initial_apply(&snapshot(), &[package("com.store")]).is_err());
    let mut unknown_protected = snapshot(); unknown_protected.protected.push(ProtectedPackageFact { package: package("com.missing"), reason: "bad fact".into() });
    assert!(initial_apply(&unknown_protected, &[]).is_err());
    let mut missing_launcher = snapshot(); missing_launcher.catalog.retain(|app| app.package.as_str() != "org.unscroll.launcher");
    assert!(initial_apply(&missing_launcher, &[]).is_err());
    let mut mismatched_launcher = snapshot(); mismatched_launcher.catalog.iter_mut().find(|app| app.package.as_str() == "org.unscroll.launcher").unwrap().component = component("org.unscroll.launcher/.Other");
    assert!(initial_apply(&mismatched_launcher, &[]).is_err());
    let mut wrong_owner = snapshot(); wrong_owner.catalog.iter_mut().find(|app| app.package.as_str() == "org.unscroll.launcher").unwrap().package = package("com.other");
    assert!(initial_apply(&wrong_owner, &[]).is_err());
}

#[test]
fn edit_uses_the_exact_inspected_suspension_state() {
    let mut already_suspended = snapshot(); already_suspended.catalog.iter_mut().find(|app| app.package.as_str() == "com.social").unwrap().suspended = true;
    assert!(!suspended(&edit(&already_suspended, &[package("com.social")], &[]).unwrap()).contains(&"com.social".into()));
    let added = edit(&snapshot(), &[], &[package("com.social")]).unwrap();
    assert!(!added.operations.iter().any(|step| matches!(&step.operation, Operation::Suspend { package, .. } if package.as_str() == "com.social")));
}
