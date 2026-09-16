use unscroll_desktop_lib::{
    adb::{Component, PackageId, UserId},
    policy::{Operation, Plan, PlannedOperation, Requirement},
    recovery::{
        mirror::{MirrorStore, persist},
        model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1},
    },
    transaction::{
        ApplyDevice, ApplyOutcome, DeviceFailure, EditOutcome, ObservedState, RESTORE_CONFIRMATION,
        RestoreDevice, RestoreError, RestoreOutcome, SessionKind, apply, classify, edit, preview,
        redacted_json, restore, retry_cleanup,
    },
};

use std::collections::BTreeMap;

const SERIAL: &str = "SERIAL-ABC-123";
const FINGERPRINT: &str = "brand/model/device:14/XYZ/full-fingerprint-value";
const UNSCROLL_HOME: &str = "org.unscroll.launcher/.Main";
const BASE_HOME: &str = "com.base/.Home";

const SUSPEND_APP: &str =
    r#"{"kind":"package_suspension","package_id":"com.app","suspended":true,"user_id":0}"#;
const UNSUSPEND_APP: &str =
    r#"{"kind":"package_suspension","package_id":"com.app","suspended":false,"user_id":0}"#;
const APPOP_IGNORE: &str = r#"{"kind":"app_op","mode":"ignore","op":"REQUEST_INSTALL_PACKAGES","package_id":"com.app","user_id":0}"#;
const APPOP_ALLOW: &str = r#"{"kind":"app_op","mode":"allow","op":"REQUEST_INSTALL_PACKAGES","package_id":"com.app","user_id":0}"#;
const SUSPEND_PRE_TRUE: &str =
    r#"{"kind":"package_suspension","package_id":"com.pre","suspended":true,"user_id":0}"#;
const UNSUSPEND_PRE: &str =
    r#"{"kind":"package_suspension","package_id":"com.pre","suspended":false,"user_id":0}"#;

fn home_op(component: &str) -> String {
    format!(r#"{{"component":"{component}","kind":"home"}}"#)
}

fn id(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012x}")
}

fn baseline() -> RecoveryEnvelopeV1 {
    RecoveryEnvelopeV1::new_baseline(BaselineInput {
        binding: DeviceBinding { serial: SERIAL.into(), fingerprint: FINGERPRINT.into(), user_id: 0 },
        baseline_id: "11111111-1111-1111-1111-111111111111".into(),
        baseline_launcher: "com.base".into(),
        initial_home: BASE_HOME.into(),
        initial_packages: vec![
            InitialPackageSuspension { package: "com.app".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.pre".into(), suspended: true, user_id: 0 },
        ],
        allowed_packages: vec!["com.app".into()],
    })
    .unwrap()
}

fn applied_state() -> RecoveryEnvelopeV1 {
    let base = baseline();
    let first = base.append_pending(&id(0), SUSPEND_APP, UNSUSPEND_APP).unwrap().mark_applied(&id(0)).unwrap();
    let second = first.append_pending(&id(1), APPOP_IGNORE, APPOP_ALLOW).unwrap().mark_applied(&id(1)).unwrap();
    second
        .append_pending(&id(2), &home_op(UNSCROLL_HOME), &home_op(BASE_HOME))
        .unwrap()
        .mark_applied(&id(2))
        .unwrap()
}

#[derive(Default)]
struct Fake {
    private: Option<String>,
    shared: Option<String>,
    suspended: Vec<String>,
    home: String,
    appops: BTreeMap<(String, String), String>,
    calls: Vec<String>,
    chooser_calls: usize,
    launcher_present: bool,
    fail_home: bool,
    fail_uninstall: bool,
    fail_delete_private: bool,
    fail_delete_shared: bool,
    disconnect_mutate: bool,
}

impl Fake {
    fn connected(envelope: &RecoveryEnvelopeV1) -> Self {
        Self {
            private: Some(envelope.canonical_json()),
            shared: Some(envelope.canonical_json()),
            suspended: vec!["com.app".into(), "com.pre".into()],
            home: UNSCROLL_HOME.into(),
            appops: BTreeMap::from([(("com.app".into(), "REQUEST_INSTALL_PACKAGES".into()), "ignore".into())]),
            launcher_present: true,
            ..Default::default()
        }
    }
}

impl MirrorStore for Fake {
    type Error = ();
    fn write_private(&mut self, value: &str) -> Result<(), ()> {
        self.private = Some(value.into());
        Ok(())
    }
    fn write_shared(&mut self, value: &str) -> Result<(), ()> {
        self.shared = Some(value.into());
        Ok(())
    }
    fn read_private(&mut self) -> Result<Option<String>, ()> {
        Ok(self.private.clone())
    }
    fn read_shared(&mut self) -> Result<Option<String>, ()> {
        Ok(self.shared.clone())
    }
    fn delete_private(&mut self) -> Result<(), ()> {
        if self.fail_delete_private {
            return Err(());
        }
        self.private = None;
        Ok(())
    }
    fn delete_shared(&mut self) -> Result<(), ()> {
        if self.fail_delete_shared {
            return Err(());
        }
        self.shared = None;
        Ok(())
    }
}

impl ApplyDevice for Fake {
    fn mutate(&mut self, operation: &Operation) -> Result<(), DeviceFailure> {
        self.calls.push(format!("{operation:?}"));
        if self.disconnect_mutate {
            return Err(DeviceFailure::Disconnect);
        }
        match operation {
            Operation::Suspend { package, suspended, .. } => {
                if *suspended {
                    if !self.suspended.iter().any(|item| item == package.as_str()) {
                        self.suspended.push(package.as_str().into());
                    }
                } else {
                    self.suspended.retain(|item| item != package.as_str());
                }
            }
            Operation::AppOp { package, app_op, mode, .. } => {
                let mode = match mode {
                    unscroll_desktop_lib::adb::AppOpMode::Default => "default",
                    unscroll_desktop_lib::adb::AppOpMode::Allow => "allow",
                    unscroll_desktop_lib::adb::AppOpMode::Ignore => "ignore",
                };
                self.appops.insert((package.as_str().into(), app_op.as_str().into()), mode.into());
            }
            Operation::Home { component } => {
                self.home = component.as_str().into();
            }
            _ => (),
        }
        Ok(())
    }
    fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure> {
        Ok(match operation {
            Operation::Suspend { package, suspended, .. } => {
                self.suspended.iter().any(|item| item == package.as_str()) == *suspended
            }
            Operation::AppOp { package, app_op, mode, .. } => {
                let wanted = match mode {
                    unscroll_desktop_lib::adb::AppOpMode::Default => "default",
                    unscroll_desktop_lib::adb::AppOpMode::Allow => "allow",
                    unscroll_desktop_lib::adb::AppOpMode::Ignore => "ignore",
                };
                self.appops.get(&(package.as_str().into(), app_op.as_str().into())).is_some_and(|mode| mode == wanted)
            }
            Operation::Home { component } => {
                if self.fail_home {
                    false
                } else {
                    self.home == component.as_str()
                }
            }
            _ => true,
        })
    }
    fn chooser(&mut self) -> Result<(), DeviceFailure> {
        self.chooser_calls += 1;
        Ok(())
    }
}

impl RestoreDevice for Fake {
    fn uninstall_launcher(&mut self) -> Result<(), DeviceFailure> {
        self.calls.push("uninstall-launcher".into());
        if self.fail_uninstall {
            return Err(DeviceFailure::Command);
        }
        self.launcher_present = false;
        Ok(())
    }
}

#[test]
fn full_restore_success_restores_recorded_state_appops_first() {
    let applied = applied_state();
    let mut fake = Fake::connected(&applied);
    let result = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(result.outcome, RestoreOutcome::Complete);
    assert!(!fake.suspended.iter().any(|item| item == "com.app"));
    assert!(fake.suspended.iter().any(|item| item == "com.pre"));
    assert_eq!(fake.home, BASE_HOME);
    assert_eq!(
        fake.appops.get(&("com.app".into(), "REQUEST_INSTALL_PACKAGES".into())),
        Some(&"allow".to_owned())
    );
    let appop = fake.calls.iter().position(|call| call.contains("AppOp")).unwrap();
    let suspend = fake.calls.iter().position(|call| call.contains("Suspend")).unwrap();
    let home = fake.calls.iter().position(|call| call.contains("Home")).unwrap();
    assert!(appop < suspend && suspend < home);
    assert!(!fake.launcher_present);
    assert!(fake.private.is_none() && fake.shared.is_none());
    assert_eq!(result.envelope.baseline_hash(), applied.baseline_hash());
}

#[test]
fn chooser_fallback_completes_on_retry() {
    let applied = applied_state();
    let mut fake = Fake::connected(&applied);
    fake.fail_home = true;
    let first = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(first.outcome, RestoreOutcome::ChooserRequired);
    assert_eq!(fake.chooser_calls, 1);
    assert_eq!(fake.private.as_deref(), Some(applied.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
    fake.fail_home = false;
    let second = restore(&mut fake, first.envelope, RESTORE_CONFIRMATION).unwrap();
    assert_eq!(second.outcome, RestoreOutcome::Complete);
    assert_eq!(fake.home, BASE_HOME);
    assert!(fake.private.is_none() && fake.shared.is_none());
}

#[test]
fn confirmation_mismatch_mutates_nothing() {
    let applied = applied_state();
    let mut fake = Fake::connected(&applied);
    let result = restore(&mut fake, applied.clone(), "restore my phone");
    assert!(matches!(result, Err(RestoreError::ConfirmationMismatch)));
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(applied.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
    assert!(fake.suspended.iter().any(|item| item == "com.app"));
    assert_eq!(fake.home, UNSCROLL_HOME);
}

#[test]
fn never_unsuspends_pre_baseline_suspended_package() {
    let applied = applied_state()
        .append_pending(&id(3), SUSPEND_PRE_TRUE, UNSUSPEND_PRE)
        .unwrap()
        .mark_applied(&id(3))
        .unwrap();
    let mut fake = Fake::connected(&applied);
    let result = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(result.outcome, RestoreOutcome::Complete);
    assert!(fake.suspended.iter().any(|item| item == "com.pre"));
    assert!(!fake.suspended.iter().any(|item| item == "com.app"));
    assert!(!fake.calls.iter().any(|call| call.contains("com.pre")));
}

#[test]
fn launcher_uninstall_failure_retains_evidence() {
    let applied = applied_state();
    let mut fake = Fake::connected(&applied);
    fake.fail_uninstall = true;
    let result = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(result.outcome, RestoreOutcome::Blocked);
    assert!(fake.launcher_present);
    assert_eq!(fake.private.as_deref(), Some(applied.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
    assert!(fake.private.is_some());
}

#[test]
fn shared_cleanup_failure_offers_retry_without_repeating_mutations() {
    let applied = applied_state();
    let mut fake = Fake::connected(&applied);
    fake.fail_delete_shared = true;
    let result = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(result.outcome, RestoreOutcome::CleanupRetry);
    assert!(fake.private.is_none());
    assert!(fake.shared.is_some());
    let calls = fake.calls.len();
    fake.fail_delete_shared = false;
    retry_cleanup(&mut fake, result.envelope).unwrap();
    assert!(fake.shared.is_none());
    assert_eq!(fake.calls.len(), calls);
}

#[test]
fn missing_copies_do_not_mutate() {
    let applied = applied_state();
    let mut fake = Fake::default();
    let result = restore(&mut fake, applied, RESTORE_CONFIRMATION);
    assert!(matches!(result, Err(RestoreError::MissingCopies)));
    assert!(fake.calls.is_empty());
}

#[test]
fn fresh_install_rebuilds_private_from_shared_then_restores() {
    let base = baseline();
    let hash = base.baseline_hash();
    let suspend = Operation::Suspend {
        package: PackageId::parse("com.app").unwrap(),
        user: UserId::parse(0).unwrap(),
        suspended: true,
    };
    let unsuspend = Operation::Suspend {
        package: PackageId::parse("com.app").unwrap(),
        user: UserId::parse(0).unwrap(),
        suspended: false,
    };
    let plan = Plan {
        operations: vec![
            PlannedOperation {
                operation: suspend,
                inverse: unsuspend,
                requirement: Requirement::UserResolvable,
                precondition: "checked".into(),
                postcondition: "verified".into(),
                description: "suspend".into(),
            },
            PlannedOperation {
                operation: Operation::Home { component: Component::parse(UNSCROLL_HOME).unwrap() },
                inverse: Operation::Home { component: Component::parse(BASE_HOME).unwrap() },
                requirement: Requirement::Required,
                precondition: "checked".into(),
                postcondition: "verified".into(),
                description: "home".into(),
            },
        ],
    };
    let mut fake =
        Fake { launcher_present: true, suspended: vec!["com.pre".into()], ..Default::default() };
    let applied = apply(&mut fake, base, &plan, None).unwrap();
    assert_eq!(applied.outcome, ApplyOutcome::Complete);
    let edited = edit(&mut fake, applied.envelope, &[PackageId::parse("com.app").unwrap()], &[], &[]).unwrap();
    assert_eq!(edited.outcome, EditOutcome::Complete);
    assert_eq!(edited.envelope.baseline_hash(), hash);
    fake.private = None;
    let shared = fake.shared.clone().unwrap();
    let state = ObservedState {
        explained: true,
        maintenance_open: false,
        pending_id: None,
        failed_required: false,
        restore_verified: false,
        cleanup_remaining: false,
        policy_traces: true,
    };
    let session = classify(None, Some(&shared), SERIAL, FINGERPRINT, &hash, &state);
    assert_eq!(session.kind, SessionKind::ActivePolicy);
    persist(&mut fake, session.envelope.as_ref().unwrap()).unwrap();
    assert_eq!(fake.private, fake.shared);
    let restored = restore(&mut fake, session.envelope.unwrap(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(restored.outcome, RestoreOutcome::Complete);
    assert_eq!(fake.home, BASE_HOME);
    assert_eq!(fake.suspended, vec!["com.pre".to_owned()]);
    assert_eq!(restored.envelope.baseline_hash(), hash);
    assert!(fake.private.is_none() && fake.shared.is_none());
}

#[test]
fn diagnostic_redaction_hides_secrets() {
    let applied = applied_state();
    let redacted = redacted_json(&applied);
    assert!(!redacted.contains(SERIAL));
    assert!(!redacted.contains(FINGERPRINT));
    assert!(redacted.contains("com.app"));
    assert!(redacted.contains("package_suspension"));
    assert!(redacted.contains("applied"));
    let shown = preview(&applied, "Pixel 8", FINGERPRINT);
    assert_eq!(shown.device_model, "Pixel 8");
    assert_ne!(shown.fingerprint_redacted, FINGERPRINT);
    assert!(!shown.fingerprint_redacted.contains(FINGERPRINT));
    assert_eq!(shown.allowlist_count, 1);
    assert_eq!(shown.initial_package_count, 2);
    assert_eq!(shown.operations.len(), 3);
    assert!(shown.errors.is_empty());
    let (op, inv) = (SUSPEND_APP, UNSUSPEND_APP);
    let failed = baseline().append_pending(&id(0), op, inv).unwrap().mark_failed(&id(0)).unwrap();
    let failed_shown = preview(&failed, "Pixel 8", FINGERPRINT);
    assert_eq!(failed_shown.errors.len(), 1);
}

struct ReadDisconnectFake {
    private: Option<String>,
    shared: Option<String>,
    calls: Vec<String>,
}

impl MirrorStore for ReadDisconnectFake {
    type Error = RestoreError;
    fn write_private(&mut self, value: &str) -> Result<(), RestoreError> {
        self.private = Some(value.into());
        Ok(())
    }
    fn write_shared(&mut self, value: &str) -> Result<(), RestoreError> {
        self.shared = Some(value.into());
        Ok(())
    }
    fn read_private(&mut self) -> Result<Option<String>, RestoreError> {
        Err(RestoreError::Store("device failed: Disconnect".into()))
    }
    fn read_shared(&mut self) -> Result<Option<String>, RestoreError> {
        Err(RestoreError::Store("device failed: Disconnect".into()))
    }
    fn delete_private(&mut self) -> Result<(), RestoreError> {
        self.private = None;
        Ok(())
    }
    fn delete_shared(&mut self) -> Result<(), RestoreError> {
        self.shared = None;
        Ok(())
    }
}

impl ApplyDevice for ReadDisconnectFake {
    fn mutate(&mut self, operation: &Operation) -> Result<(), DeviceFailure> {
        self.calls.push(format!("{operation:?}"));
        Ok(())
    }
    fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure> {
        self.calls.push(format!("verify:{operation:?}"));
        Ok(true)
    }
}

impl RestoreDevice for ReadDisconnectFake {}

#[test]
fn pre_mutation_read_disconnect_is_recoverable_without_mutation() {
    // Finding 5: a disconnect while reading mirrors must not surface as Store.
    let applied = applied_state();
    let hash = applied.baseline_hash();
    let mut fake = ReadDisconnectFake {
        private: Some(applied.canonical_json()),
        shared: Some(applied.canonical_json()),
        calls: Vec::new(),
    };
    let result = restore(&mut fake, applied, RESTORE_CONFIRMATION).unwrap();
    assert_eq!(result.outcome, RestoreOutcome::RecoverableDisconnect);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(fake.calls.is_empty());
    assert!(fake.private.is_some() && fake.shared.is_some());
}

#[test]
fn restore_after_cleanup_retry_is_refused_without_repeating_mutations() {
    // Finding 7: after CleanupRetry the correct call is retry_cleanup(), not
    // restore() again — a repeated restore is conservatively refused and must
    // not duplicate device mutations.
    let applied = applied_state();
    let hash = applied.baseline_hash();
    let mut fake = Fake::connected(&applied);
    fake.fail_delete_private = true;
    let first = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(first.outcome, RestoreOutcome::CleanupRetry);
    assert_eq!(first.envelope.baseline_hash(), hash);
    let calls_after_first = fake.calls.len();
    let mirrors_after_first = (fake.private.clone(), fake.shared.clone());
    let second = restore(&mut fake, first.envelope, RESTORE_CONFIRMATION);
    assert!(matches!(second, Err(RestoreError::NotReady(_))), "expected NotReady, got {second:?}");
    assert_eq!(fake.calls.len(), calls_after_first);
    assert_eq!((fake.private, fake.shared), mirrors_after_first);
}

#[test]
fn applied_cleanup_second_restore_is_refused_without_repeating_mutations() {
    // CR-01: after a shared-delete failure the envelope carries an APPLIED
    // private-cleanup entry. A second restore() must refuse verified-cleanup
    // envelopes without re-executing uninstall_launcher.
    let applied = applied_state();
    let hash = applied.baseline_hash();
    let mut fake = Fake::connected(&applied);
    fake.fail_delete_shared = true;
    let first = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(first.outcome, RestoreOutcome::CleanupRetry);
    assert_eq!(first.envelope.baseline_hash(), hash);
    assert!(first.envelope.has_applied_private_cleanup());
    assert!(fake.private.is_none());
    assert!(fake.shared.is_some());
    let calls_after_first = fake.calls.len();
    assert!(calls_after_first > 0);
    let mirrors_after_first = (fake.private.clone(), fake.shared.clone());
    let second = restore(&mut fake, first.envelope.clone(), RESTORE_CONFIRMATION);
    assert!(matches!(second, Err(RestoreError::NotReady(_))), "expected NotReady, got {second:?}");
    assert_eq!(fake.calls.len(), calls_after_first);
    assert_eq!((fake.private, fake.shared), mirrors_after_first);
    assert_eq!(first.envelope.baseline_hash(), hash);
}

#[test]
fn private_cleanup_failure_retry_cleanup_succeeds_without_repeating_mutations() {
    // CR-02: a delete_private failure must still mark the cleanup applied and
    // persist so the returned envelope satisfies retry_cleanup's precondition.
    let applied = applied_state();
    let hash = applied.baseline_hash();
    let mut fake = Fake::connected(&applied);
    fake.fail_delete_private = true;
    let first = restore(&mut fake, applied.clone(), RESTORE_CONFIRMATION).unwrap();
    assert_eq!(first.outcome, RestoreOutcome::CleanupRetry);
    assert_eq!(first.envelope.baseline_hash(), hash);
    assert!(first.envelope.has_applied_private_cleanup());
    assert_eq!(first.envelope.pending_id(), None);
    assert_eq!(fake.private, fake.shared);
    assert!(fake.private.is_some());
    let calls_after_first = fake.calls.len();
    fake.fail_delete_private = false;
    retry_cleanup(&mut fake, first.envelope).unwrap();
    assert!(fake.private.is_none() && fake.shared.is_none());
    assert_eq!(fake.calls.len(), calls_after_first);
}
