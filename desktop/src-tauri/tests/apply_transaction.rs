use unscroll_desktop_lib::{
    adb::{Component, PackageId, UserId},
    policy::{Operation, Plan, PlannedOperation, Requirement},
    recovery::{mirror::{MirrorStore}, model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1}},
    transaction::{apply, ApplyDevice, ApplyOutcome, Decision, DeviceFailure},
};

fn package(value: &str) -> PackageId { PackageId::parse(value).unwrap() }
fn component(value: &str) -> Component { Component::parse(value).unwrap() }
fn step(operation: Operation, inverse: Operation, requirement: Requirement) -> PlannedOperation { PlannedOperation { operation, inverse, requirement, precondition: "checked".into(), postcondition: "verified".into(), description: "test".into() } }
fn baseline() -> RecoveryEnvelopeV1 { RecoveryEnvelopeV1::new_baseline(BaselineInput { binding: DeviceBinding { serial: "device".into(), fingerprint: "maker/device".into(), user_id: 0 }, baseline_id: "11111111-1111-1111-1111-111111111111".into(), baseline_launcher: "com.base".into(), initial_home: "com.base/.Home".into(), initial_packages: vec![InitialPackageSuspension { package: "com.app".into(), suspended: false, user_id: 0 }, InitialPackageSuspension { package: "com.store".into(), suspended: false, user_id: 0 }], allowed_packages: vec![] }).unwrap() }

#[derive(Default)] struct FakeAdb { private: Option<String>, shared: Option<String>, suspended: Vec<String>, home: String, fail_verify: Option<String>, fail_mutate: Option<String>, fail_appop: bool, false_appop: bool, fail_home: bool, disconnect_mutate: bool, disconnect_verify: bool, disconnect_unsuspend_once: bool, fail_unsuspend: bool, fail_inverse_verify: bool, calls: Vec<String>, verified_ops: Vec<String>, mirror_at_calls: Vec<(Option<String>,Option<String>)>, chooser_calls: usize, policy_disconnect_once: bool, home_verify_disconnect_after_chooser: bool, disconnect_home_mutate: bool }
impl MirrorStore for FakeAdb { type Error = (); fn write_private(&mut self, v: &str) -> Result<(),()> { self.private=Some(v.into()); Ok(()) } fn write_shared(&mut self, v:&str)->Result<(),()> { self.shared=Some(v.into()); Ok(()) } fn read_private(&mut self)->Result<Option<String>,()> { Ok(self.private.clone()) } fn read_shared(&mut self)->Result<Option<String>,()> { Ok(self.shared.clone()) } fn delete_private(&mut self)->Result<(),()> { Ok(()) } fn delete_shared(&mut self)->Result<(),()> { Ok(()) } }
impl ApplyDevice for FakeAdb { fn mutate(&mut self, operation: &Operation) -> Result<(), DeviceFailure> { self.calls.push(format!("{operation:?}")); self.mirror_at_calls.push((self.private.clone(),self.shared.clone())); if self.disconnect_mutate || (matches!(operation, Operation::Home { .. }) && self.disconnect_home_mutate) { return Err(DeviceFailure::Disconnect); } if matches!(operation, Operation::Suspend { suspended: false, .. }) && self.disconnect_unsuspend_once { self.disconnect_unsuspend_once=false; return Err(DeviceFailure::Disconnect); } if matches!(operation, Operation::Suspend { package, .. } if self.fail_mutate.as_deref()==Some(package.as_str())) || (matches!(operation, Operation::Suspend { suspended: false, .. }) && self.fail_unsuspend) || (matches!(operation, Operation::AppOp { .. }) && self.fail_appop) { return Err(DeviceFailure::Command); } match operation { Operation::Suspend { package, suspended, .. } => { if *suspended { self.suspended.push(package.as_str().into()) } else { self.suspended.retain(|p| p != package.as_str()) } }, Operation::Home { component } => self.home=component.as_str().into(), _ => () }; Ok(()) } fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure> { self.verified_ops.push(format!("{operation:?}")); self.mirror_at_calls.push((self.private.clone(),self.shared.clone())); if matches!(operation, Operation::LauncherPolicy { .. }) && self.policy_disconnect_once { self.policy_disconnect_once=false; return Err(DeviceFailure::Disconnect); } if matches!(operation, Operation::Home { .. }) && self.chooser_calls > 0 && self.home_verify_disconnect_after_chooser { return Err(DeviceFailure::Disconnect); } if self.disconnect_verify { return Err(DeviceFailure::Disconnect); } Ok(!matches!(operation, Operation::Suspend { package, suspended: true, .. } if self.fail_verify.as_deref()==Some(package.as_str())) && !(matches!(operation, Operation::AppOp { .. }) && self.false_appop) && !(matches!(operation, Operation::Suspend { suspended: false, .. }) && self.fail_inverse_verify) && !(matches!(operation, Operation::Home { .. }) && self.fail_home)) } fn chooser(&mut self) -> Result<(), DeviceFailure> { self.chooser_calls += 1; Ok(()) } }

#[test]
fn applies_only_after_mirrored_pending_and_verified_applied() {
    let mut adb = FakeAdb::default(); let plan = Plan { operations: vec![step(Operation::Suspend { package: package("com.app"), user: UserId::parse(0).unwrap(), suspended: true }, Operation::Suspend { package: package("com.app"), user: UserId::parse(0).unwrap(), suspended: false }, Requirement::UserResolvable), step(Operation::Home { component: component("org.unscroll.launcher/.Main") }, Operation::Home { component: component("com.base/.Home") }, Requirement::Required)] };
    let result = apply(&mut adb, baseline(), &plan, None).unwrap();
    assert_eq!(result.outcome, ApplyOutcome::Complete); assert_eq!(adb.suspended, ["com.app"]); assert_eq!(adb.home, "org.unscroll.launcher/.Main"); assert_eq!(adb.private, adb.shared); assert!(RecoveryEnvelopeV1::parse(adb.private.as_deref().unwrap()).is_ok()); assert_eq!(result.envelope.baseline_hash(), baseline().baseline_hash()); let pending=RecoveryEnvelopeV1::parse(adb.mirror_at_calls[0].0.as_deref().unwrap()).unwrap(); let after_verify=RecoveryEnvelopeV1::parse(adb.mirror_at_calls[2].0.as_deref().unwrap()).unwrap(); assert_eq!(adb.mirror_at_calls[0].0,adb.mirror_at_calls[0].1); assert_eq!(adb.mirror_at_calls[1].0,adb.mirror_at_calls[1].1); assert_eq!(pending.journal_state("00000000-0000-4000-8000-000000000000"),Some(unscroll_desktop_lib::recovery::model::JournalState::Pending)); assert!(baseline().is_strict_prefix_of(&pending).is_ok()); let applied=pending.mark_applied("00000000-0000-4000-8000-000000000000").unwrap(); assert!(pending.is_strict_prefix_of(&applied).is_ok()); assert_eq!(after_verify.journal_state("00000000-0000-4000-8000-000000000000"),Some(unscroll_desktop_lib::recovery::model::JournalState::Applied)); assert_eq!(pending.baseline_hash(),baseline().baseline_hash());
}

#[test]
fn ordinary_failure_pauses_or_rolls_back_without_changing_home() {
    let plan = Plan { operations: vec![step(Operation::Suspend { package: package("com.app"), user: UserId::parse(0).unwrap(), suspended: true }, Operation::Suspend { package: package("com.app"), user: UserId::parse(0).unwrap(), suspended: false }, Requirement::UserResolvable), step(Operation::Home { component: component("org.unscroll.launcher/.Main") }, Operation::Home { component: component("com.base/.Home") }, Requirement::Required)] };
    let mut paused = FakeAdb { fail_verify: Some("com.app".into()), ..Default::default() }; let result = apply(&mut paused, baseline(), &plan, None).unwrap(); assert!(matches!(result.outcome, ApplyOutcome::DecisionRequired { .. })); assert!(paused.home.is_empty());
    let mut rollback = FakeAdb { fail_mutate: Some("com.app".into()), fail_verify: Some("com.app".into()), ..Default::default() }; let result = apply(&mut rollback, baseline(), &plan, Some(Decision::Rollback)).unwrap(); assert_eq!(result.outcome, ApplyOutcome::RolledBack); assert!(rollback.suspended.is_empty());
}





fn suspend(name: &str, requirement: Requirement) -> PlannedOperation { step(Operation::Suspend { package: package(name), user: UserId::parse(0).unwrap(), suspended: true }, Operation::Suspend { package: package(name), user: UserId::parse(0).unwrap(), suspended: false }, requirement) }
fn home() -> PlannedOperation { step(Operation::Home { component: component("org.unscroll.launcher/.Main") }, Operation::Home { component: component("com.base/.Home") }, Requirement::Required) }

#[test]
fn required_store_failure_reverses_prior_changes_and_keeps_mirrors() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),suspend("com.store",Requirement::Required),home()]};
    let mut adb=FakeAdb{fail_mutate:Some("com.store".into()),fail_verify:Some("com.store".into()),..Default::default()}; let result=apply(&mut adb,baseline(),&plan,None).unwrap();
    assert_eq!(result.outcome,ApplyOutcome::RolledBack); assert!(adb.suspended.is_empty()); assert_eq!(adb.private,adb.shared); assert_eq!(result.envelope.baseline_hash(),baseline().baseline_hash()); assert!(!adb.calls.iter().any(|call|call.contains("Home")));
}
#[test]
fn optional_app_op_failure_is_honest_partial_protection() {
    let app=package("com.app"); let op=unscroll_desktop_lib::adb::AppOp::parse("REQUEST_INSTALL_PACKAGES").unwrap();
    let plan=Plan{operations:vec![step(Operation::AppOp{package:app.clone(),user:UserId::parse(0).unwrap(),app_op:op.clone(),mode:unscroll_desktop_lib::adb::AppOpMode::Ignore},Operation::AppOp{package:app,user:UserId::parse(0).unwrap(),app_op:op,mode:unscroll_desktop_lib::adb::AppOpMode::Allow},Requirement::Optional),home()]};
    let mut adb=FakeAdb{fail_appop:true,false_appop:true,..Default::default()}; let result=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(result.outcome,ApplyOutcome::Complete); assert_eq!(result.partial_protection.len(),1); assert_eq!(adb.private,adb.shared); assert_eq!(adb.home,"org.unscroll.launcher/.Main");
}
#[test]
fn home_ignored_requires_chooser_and_does_not_claim_completion() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),home()]}; let mut adb=FakeAdb{fail_home:true,..Default::default()}; let result=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(result.outcome,ApplyOutcome::ChooserRequired); assert_eq!(adb.private,adb.shared); assert_eq!(result.envelope.baseline_hash(),baseline().baseline_hash());
}

struct Boundary { inner: FakeAdb, at: usize, calls: usize }
impl Boundary { fn hit(&mut self)->Result<(),()> { self.calls+=1; if self.calls==self.at { Err(()) } else { Ok(()) } } }
impl MirrorStore for Boundary { type Error=(); fn write_private(&mut self,v:&str)->Result<(),()>{self.hit()?;self.inner.write_private(v)} fn write_shared(&mut self,v:&str)->Result<(),()>{self.hit()?;self.inner.write_shared(v)} fn read_private(&mut self)->Result<Option<String>,()>{self.hit()?;self.inner.read_private()} fn read_shared(&mut self)->Result<Option<String>,()>{self.hit()?;self.inner.read_shared()} fn delete_private(&mut self)->Result<(),()>{self.inner.delete_private()} fn delete_shared(&mut self)->Result<(),()>{self.inner.delete_shared()} }
impl ApplyDevice for Boundary { fn mutate(&mut self,op:&Operation)->Result<(),DeviceFailure>{self.inner.mutate(op)} fn verified(&mut self,op:&Operation)->Result<bool,DeviceFailure>{self.inner.verified(op)} }

#[test]
fn mirrored_write_and_readback_boundaries_leave_recovery_evidence() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]};
    for at in 1..=8 { let mut adb=Boundary{inner:FakeAdb::default(),at,calls:0}; let result=apply(&mut adb,baseline(),&plan,None); assert!(result.is_err(),"boundary {at}"); assert!(adb.inner.private.is_some() || adb.inner.shared.is_some() || at==1,"boundary {at}"); if at <= 4 { assert!(adb.inner.suspended.is_empty(),"boundary {at}"); } }
}



#[test]
fn command_and_verify_disconnects_preserve_pending_recovery() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]};
    for adb in [FakeAdb{disconnect_mutate:true,..Default::default()},FakeAdb{disconnect_verify:true,..Default::default()}] { let mut adb=adb; let result=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(result.outcome,ApplyOutcome::RecoverableDisconnect); assert_eq!(adb.private,adb.shared); assert!(adb.private.is_some()); }
}
#[test]
fn false_success_for_suspend_appop_and_home_never_claims_protection() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]}; let mut adb=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; assert!(matches!(apply(&mut adb,baseline(),&plan,None).unwrap().outcome,ApplyOutcome::DecisionRequired{..}));
    let app=package("com.app"); let op=unscroll_desktop_lib::adb::AppOp::parse("REQUEST_INSTALL_PACKAGES").unwrap(); let app_plan=Plan{operations:vec![step(Operation::AppOp{package:app.clone(),user:UserId::parse(0).unwrap(),app_op:op.clone(),mode:unscroll_desktop_lib::adb::AppOpMode::Ignore},Operation::AppOp{package:app,user:UserId::parse(0).unwrap(),app_op:op,mode:unscroll_desktop_lib::adb::AppOpMode::Allow},Requirement::Optional)]}; let mut adb=FakeAdb{false_appop:true,..Default::default()}; assert_eq!(apply(&mut adb,baseline(),&app_plan,None).unwrap().partial_protection.len(),1);
    let mut adb=FakeAdb{fail_home:true,..Default::default()}; assert_eq!(apply(&mut adb,baseline(),&Plan{operations:vec![home()]},Some(Decision::HomeCancelled)).unwrap().outcome,ApplyOutcome::ChooserRequired);
}
#[test]
fn rollback_inverse_failures_retain_recovery_evidence() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),suspend("com.store",Requirement::Required)]};
    for adb in [FakeAdb{fail_mutate:Some("com.store".into()),fail_verify:Some("com.store".into()),fail_unsuspend:true,..Default::default()},FakeAdb{fail_mutate:Some("com.store".into()),fail_verify:Some("com.store".into()),fail_inverse_verify:true,..Default::default()}] { let mut adb=adb; let result=apply(&mut adb,baseline(),&plan,None).unwrap(); assert!(matches!(result.outcome,ApplyOutcome::RolledBack|ApplyOutcome::InconsistentState)); assert_eq!(adb.private,adb.shared); assert!(adb.private.is_some()); }
}





#[test]
fn command_failure_with_proven_forward_state_is_recorded_and_rolled_back() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),suspend("com.store",Requirement::Required)]};
    let mut adb=FakeAdb{fail_mutate:Some("com.store".into()),fail_verify:Some("com.store".into()),..Default::default()};
    let result=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(result.outcome,ApplyOutcome::RolledBack); assert!(adb.calls.iter().filter(|call|call.contains("com.app")).count()>=2); assert!(adb.suspended.is_empty()); assert_eq!(adb.private,adb.shared);
}

#[test]
fn failed_ordinary_step_resumes_once_with_continue_or_rollback() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]};
    let mut paused=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; let first=apply(&mut paused,baseline(),&plan,None).unwrap(); assert!(matches!(first.outcome,ApplyOutcome::DecisionRequired{..}));
    paused.fail_verify=None; let continued=apply(&mut paused,first.envelope.clone(),&plan,Some(Decision::Continue)).unwrap(); assert_eq!(continued.outcome,ApplyOutcome::Complete); let value=continued.envelope.canonical_json(); assert!(value.contains("\"failed\"") && value.contains("launcher_policy")); assert_eq!(paused.private,paused.shared);
    let mut paused=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; let first=apply(&mut paused,baseline(),&plan,None).unwrap(); let rolled=apply(&mut paused,first.envelope,&plan,Some(Decision::Rollback)).unwrap(); assert_eq!(rolled.outcome,ApplyOutcome::RolledBack);
}

#[test]
fn pending_step_retries_without_duplicate_id() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]}; let pending=baseline().append_pending("00000000-0000-4000-8000-000000000000",r#"{"kind":"package_suspension","package_id":"com.app","suspended":true,"user_id":0}"#,r#"{"kind":"package_suspension","package_id":"com.app","suspended":false,"user_id":0}"#).unwrap();
    let mut adb=FakeAdb::default(); let result=apply(&mut adb,pending,&plan,None).unwrap(); assert_eq!(result.outcome,ApplyOutcome::Complete); assert_eq!(result.envelope.canonical_json().matches("00000000-0000-4000-8000-000000000000").count(),1); assert_eq!(adb.private,adb.shared);
}

#[test]
fn pending_home_cancellation_reenters_chooser_without_home_select_then_confirms() {
    let plan=Plan{operations:vec![home()]}; let mut adb=FakeAdb{fail_home:true,..Default::default()};
    let first=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(first.outcome,ApplyOutcome::ChooserRequired);
    let calls=adb.calls.len(); let cancelled=apply(&mut adb,first.envelope.clone(),&plan,Some(Decision::HomeCancelled)).unwrap();
    assert_eq!(cancelled.outcome,ApplyOutcome::ChooserRequired); assert_eq!(adb.calls.len(),calls); assert_eq!(cancelled.envelope.pending_id().as_deref(),Some("00000000-0000-4000-8000-000000000000"));
    adb.fail_home=false; let confirmed=apply(&mut adb,cancelled.envelope,&plan,Some(Decision::HomeConfirmed)).unwrap();
    assert_eq!(confirmed.outcome,ApplyOutcome::Complete); assert_eq!(adb.calls.len(),calls); assert_eq!(confirmed.envelope.canonical_json().matches("00000000-0000-4000-8000-000000000000").count(),1); assert_eq!(adb.private,adb.shared);
}

#[test]
fn pending_rollback_inverse_disconnect_resumes_without_duplicate_entry() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),suspend("com.store",Requirement::Required)]};
    let mut adb=FakeAdb{fail_mutate:Some("com.store".into()),fail_verify:Some("com.store".into()),disconnect_unsuspend_once:true,..Default::default()};
    let first=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(first.outcome,ApplyOutcome::RecoverableDisconnect);
    let resumed=apply(&mut adb,first.envelope,&plan,None).unwrap(); assert_eq!(resumed.outcome,ApplyOutcome::RolledBack); assert_eq!(resumed.envelope.canonical_json().matches("00000000-0000-4000-8000-0000000f4240").count(),1);
}

#[test]
fn pending_forward_false_retries_then_uses_requirement_handling() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]}; let pending=baseline().append_pending("00000000-0000-4000-8000-000000000000",r#"{"kind":"package_suspension","package_id":"com.app","suspended":true,"user_id":0}"#,r#"{"kind":"package_suspension","package_id":"com.app","suspended":false,"user_id":0}"#).unwrap();
    let mut adb=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; let result=apply(&mut adb,pending,&plan,None).unwrap(); assert!(matches!(result.outcome,ApplyOutcome::DecisionRequired{..})); assert_eq!(adb.private,adb.shared);
}
#[test]
fn optional_failure_resumes_later_pending_home_disconnect() {
    let app=package("com.app"); let op=unscroll_desktop_lib::adb::AppOp::parse("REQUEST_INSTALL_PACKAGES").unwrap();
    let plan=Plan{operations:vec![step(Operation::AppOp{package:app.clone(),user:UserId::parse(0).unwrap(),app_op:op.clone(),mode:unscroll_desktop_lib::adb::AppOpMode::Ignore},Operation::AppOp{package:app,user:UserId::parse(0).unwrap(),app_op:op,mode:unscroll_desktop_lib::adb::AppOpMode::Allow},Requirement::Optional),home()]};
    let mut adb=FakeAdb{fail_appop:true,false_appop:true,disconnect_home_mutate:true,..Default::default()}; let first=apply(&mut adb,baseline(),&plan,None).unwrap(); assert_eq!(first.outcome,ApplyOutcome::RecoverableDisconnect);
    adb.disconnect_home_mutate=false; let resumed=apply(&mut adb,first.envelope,&plan,None).unwrap(); assert_eq!(resumed.outcome,ApplyOutcome::Complete); assert_eq!(resumed.partial_protection,["test"]);
}

#[test]
fn fresh_home_post_chooser_disconnect_is_recoverable() {
    let mut adb=FakeAdb{fail_home:true,home_verify_disconnect_after_chooser:true,..Default::default()}; let result=apply(&mut adb,baseline(),&Plan{operations:vec![home()]},Some(Decision::HomeConfirmed)).unwrap(); assert_eq!(result.outcome,ApplyOutcome::RecoverableDisconnect); assert!(result.envelope.pending_id().is_some());
}

#[test]
fn pending_policy_verify_disconnect_resumes_with_persisted_allowlist() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable)]}; let mut adb=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; let paused=apply(&mut adb,baseline(),&plan,None).unwrap(); adb.policy_disconnect_once=true;
    let disconnected=apply(&mut adb,paused.envelope,&plan,Some(Decision::Continue)).unwrap(); assert_eq!(disconnected.outcome,ApplyOutcome::RecoverableDisconnect); assert!(disconnected.envelope.pending_id().is_some());
    let resumed=apply(&mut adb,disconnected.envelope,&plan,Some(Decision::Continue)).unwrap(); assert_eq!(resumed.outcome,ApplyOutcome::Complete); assert!(adb.verified_ops.iter().any(|op|op.contains("LauncherPolicy")));
}
#[test]
fn continue_policy_proof_resumes_required_store_and_home() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),suspend("com.store",Requirement::Required),home()]};
    let mut adb=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; let result=apply(&mut adb,baseline(),&plan,Some(Decision::Continue)).unwrap();
    assert_eq!(result.outcome,ApplyOutcome::Complete); assert!(adb.suspended.iter().any(|package|package=="com.store")); assert_eq!(adb.home,"org.unscroll.launcher/.Main"); assert!(result.partial_protection.iter().any(|warning|warning=="com.app"));
}
#[test]
fn pending_failed_continue_resumes_required_store_and_home() {
    let plan=Plan{operations:vec![suspend("com.app",Requirement::UserResolvable),suspend("com.store",Requirement::Required),home()]};
    let pending=baseline().append_pending("00000000-0000-4000-8000-000000000000",r#"{"kind":"package_suspension","package_id":"com.app","suspended":true,"user_id":0}"#,r#"{"kind":"package_suspension","package_id":"com.app","suspended":false,"user_id":0}"#).unwrap();
    let mut adb=FakeAdb{fail_verify:Some("com.app".into()),..Default::default()}; let paused=apply(&mut adb,pending,&plan,None).unwrap(); assert!(matches!(paused.outcome,ApplyOutcome::DecisionRequired{..}));
    adb.fail_verify=None; let result=apply(&mut adb,paused.envelope,&plan,Some(Decision::Continue)).unwrap(); assert_eq!(result.outcome,ApplyOutcome::Complete); assert!(adb.suspended.iter().any(|package|package=="com.store")); assert_eq!(adb.home,"org.unscroll.launcher/.Main");
}
