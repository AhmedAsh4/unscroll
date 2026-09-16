use std::collections::BTreeMap;

use unscroll_desktop_lib::{
    adb::PackageId,
    policy::Operation,
    recovery::{
        mirror::MirrorStore,
        model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1},
    },
    transaction::{
        allows_new_installs, can_exit, classify, close, ensure_no_forced_close,
        is_maintenance_open, maintenance_state, open, requires_forced_close, try_exit,
        ApplyDevice, CloseOutcome, DeviceFailure, MaintenanceError, MaintenanceState,
        ObservedState, OpenOutcome, ScannedPackage, SessionKind, MAINTENANCE_CONFIRMATION,
    },
};

fn package(value: &str) -> PackageId {
    PackageId::parse(value).unwrap()
}

fn baseline() -> RecoveryEnvelopeV1 {
    RecoveryEnvelopeV1::new_baseline(BaselineInput {
        binding: DeviceBinding { serial: "device".into(), fingerprint: "maker/device".into(), user_id: 0 },
        baseline_id: "11111111-1111-1111-1111-111111111111".into(),
        baseline_launcher: "com.base".into(),
        initial_home: "com.base/.Home".into(),
        initial_packages: vec![
            InitialPackageSuspension { package: "com.app".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.blocked".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.pre".into(), suspended: true, user_id: 0 },
            InitialPackageSuspension { package: "com.source".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.store".into(), suspended: false, user_id: 0 },
        ],
        allowed_packages: vec!["com.app".into()],
    })
    .unwrap()
}

fn id(index: u64) -> String {
    format!("00000000-0000-4000-8000-{index:012x}")
}

fn maintenance_id(sequence: u64) -> String {
    format!("00000000-0000-4000-8000-{sequence:012x}")
}

/// Applied policy mirroring a prior setup: blocked + store suspended, source
/// app-op ignored. Ids 0..2 live in the apply range, leaving 4M+ for
/// maintenance generations.
fn applied_policy() -> RecoveryEnvelopeV1 {
    let base = baseline();
    let first = base
        .append_pending(
            &id(0),
            r#"{"kind":"package_suspension","package_id":"com.blocked","suspended":true,"user_id":0}"#,
            r#"{"kind":"package_suspension","package_id":"com.blocked","suspended":false,"user_id":0}"#,
        )
        .unwrap()
        .mark_applied(&id(0))
        .unwrap();
    let second = first
        .append_pending(
            &id(1),
            r#"{"kind":"package_suspension","package_id":"com.store","suspended":true,"user_id":0}"#,
            r#"{"kind":"package_suspension","package_id":"com.store","suspended":false,"user_id":0}"#,
        )
        .unwrap()
        .mark_applied(&id(1))
        .unwrap();
    second
        .append_pending(
            &id(2),
            r#"{"kind":"app_op","mode":"ignore","op":"REQUEST_INSTALL_PACKAGES","package_id":"com.source","user_id":0}"#,
            r#"{"kind":"app_op","mode":"allow","op":"REQUEST_INSTALL_PACKAGES","package_id":"com.source","user_id":0}"#,
        )
        .unwrap()
        .mark_applied(&id(2))
        .unwrap()
}

fn scanned(name: &str, launchable: bool, is_store: bool) -> ScannedPackage {
    ScannedPackage::new(package(name), launchable, is_store)
}

fn full_scan() -> Vec<ScannedPackage> {
    vec![
        scanned("com.app", true, false),
        scanned("com.blocked", true, false),
        scanned("com.pre", true, false),
        scanned("com.source", true, false),
        scanned("com.store", true, true),
    ]
}

#[derive(Default)]
struct Fake {
    private: Option<String>,
    shared: Option<String>,
    suspended: Vec<String>,
    appops: BTreeMap<(String, String), String>,
    calls: Vec<String>,
    device_calls: usize,
    disconnect_device_at: Option<usize>,
    disconnect_read: bool,
    disconnect_write: bool,
    fail_mutate_pkg: Option<String>,
    fail_verify_pkg: Option<String>,
}

impl Fake {
    fn seeded(envelope: &RecoveryEnvelopeV1) -> Self {
        Self {
            private: Some(envelope.canonical_json()),
            shared: Some(envelope.canonical_json()),
            suspended: vec!["com.blocked".into(), "com.store".into(), "com.pre".into()],
            appops: BTreeMap::from([(
                ("com.source".into(), "REQUEST_INSTALL_PACKAGES".into()),
                "ignore".into(),
            )]),
            ..Default::default()
        }
    }

    fn device_hit(&mut self) -> Result<(), DeviceFailure> {
        self.device_calls += 1;
        if self.disconnect_device_at == Some(self.device_calls) {
            return Err(DeviceFailure::Disconnect);
        }
        Ok(())
    }
}

impl MirrorStore for Fake {
    type Error = DeviceFailure;
    fn write_private(&mut self, value: &str) -> Result<(), DeviceFailure> {
        if self.disconnect_write {
            return Err(DeviceFailure::Disconnect);
        }
        self.private = Some(value.into());
        Ok(())
    }
    fn write_shared(&mut self, value: &str) -> Result<(), DeviceFailure> {
        if self.disconnect_write {
            return Err(DeviceFailure::Disconnect);
        }
        self.shared = Some(value.into());
        Ok(())
    }
    fn read_private(&mut self) -> Result<Option<String>, DeviceFailure> {
        if self.disconnect_read {
            return Err(DeviceFailure::Disconnect);
        }
        Ok(self.private.clone())
    }
    fn read_shared(&mut self) -> Result<Option<String>, DeviceFailure> {
        if self.disconnect_read {
            return Err(DeviceFailure::Disconnect);
        }
        Ok(self.shared.clone())
    }
    fn delete_private(&mut self) -> Result<(), DeviceFailure> {
        self.private = None;
        Ok(())
    }
    fn delete_shared(&mut self) -> Result<(), DeviceFailure> {
        self.shared = None;
        Ok(())
    }
}

impl ApplyDevice for Fake {
    fn mutate(&mut self, operation: &Operation) -> Result<(), DeviceFailure> {
        self.calls.push(format!("mutate:{operation:?}"));
        self.device_hit()?;
        if let Some(fail) = &self.fail_mutate_pkg {
            if format!("{operation:?}").contains(fail) {
                return Err(DeviceFailure::Command);
            }
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
                self.appops.insert(
                    (package.as_str().into(), app_op.as_str().into()),
                    mode.into(),
                );
            }
            _ => (),
        }
        Ok(())
    }
    fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure> {
        self.calls.push(format!("verified:{operation:?}"));
        self.device_hit()?;
        if let Some(fail) = &self.fail_verify_pkg {
            if format!("{operation:?}").contains(fail) {
                return Ok(false);
            }
        }
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
                self.appops
                    .get(&(package.as_str().into(), app_op.as_str().into()))
                    .is_some_and(|mode| mode == wanted)
            }
            _ => true,
        })
    }
}

fn open_applied() -> (Fake, RecoveryEnvelopeV1) {
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    let result = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap();
    assert_eq!(result.outcome, OpenOutcome::Opened);
    assert_eq!(result.envelope.baseline_hash(), hash);
    (fake, result.envelope)
}

#[test]
fn tdd_probe_maintenance_confirmation_exists() {
    assert!(!MAINTENANCE_CONFIRMATION.is_empty());
    let _ = baseline();
}

#[test]
fn typed_ack_mismatch_mutates_nothing() {
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    let calls_before = fake.calls.len();
    let result = open(&mut fake, applied.clone(), "open please", &[package("com.store")]);
    assert!(matches!(result, Err(MaintenanceError::ConfirmationMismatch)));
    assert_eq!(fake.calls.len(), calls_before);
    assert_eq!(fake.private.as_deref(), Some(applied.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
    assert!(fake.suspended.iter().any(|item| item == "com.store"));
    assert_eq!(applied.baseline_hash(), hash);
}

#[test]
fn normal_open_journals_before_store_restore() {
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    let result = open(&mut fake, applied.clone(), MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap();
    assert_eq!(result.outcome, OpenOutcome::Opened);
    assert_eq!(result.envelope.baseline_hash(), hash);
    // Maintenance-open is journaled as applied.
    assert!(is_maintenance_open(&result.envelope));
    assert!(requires_forced_close(&result.envelope));
    let canonical = result.envelope.canonical_json();
    assert!(canonical.contains(r#""kind":"maintenance","open":true"#));
    assert!(canonical.contains(&maintenance_id(4_000_000)));
    // Recorded store is restored; blocked apps stay suspended.
    assert!(!fake.suspended.iter().any(|item| item == "com.store"));
    assert!(fake.suspended.iter().any(|item| item == "com.blocked"));
    assert!(fake.suspended.iter().any(|item| item == "com.pre"));
    // Recorded install source is restored to its prior (allow) value.
    assert_eq!(
        fake.appops.get(&("com.source".into(), "REQUEST_INSTALL_PACKAGES".into())),
        Some(&"allow".to_owned())
    );
    // Mirrored persist with readback.
    assert_eq!(fake.private, fake.shared);
    assert_eq!(fake.private.as_deref(), Some(canonical.as_str()));
    // While open the desktop must prevent exit and state new installs possible.
    assert_eq!(maintenance_state(&result.envelope), MaintenanceState::Open { prevents_exit: true });
    assert!(maintenance_state(&result.envelope).prevents_exit());
    assert!(maintenance_state(&result.envelope).allows_new_installs());
    assert!(allows_new_installs(&result.envelope));
    assert!(!can_exit(&result.envelope));
}

#[test]
fn open_store_restore_failure_keeps_journaled_open_in_mirrors() {
    // MI-01: when the post-journal store restore fails, open returns
    // Blocked but the mirrors already hold the applied-open envelope.
    // Callers must re-read before close; the device stays fail-closed.
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    fake.fail_mutate_pkg = Some("com.store".into());
    let result = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")]);
    assert!(matches!(result, Err(MaintenanceError::Blocked(_))));
    assert_eq!(fake.private, fake.shared);
    let mirrored = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert_eq!(mirrored.baseline_hash(), hash);
    assert!(is_maintenance_open(&mirrored));
    assert!(requires_forced_close(&mirrored));
    // Fail-closed: the store was not unsuspended.
    assert!(fake.suspended.iter().any(|item| item == "com.store"));
    assert!(mirrored.canonical_json().contains(r#""kind":"maintenance","open":true"#));
}

#[test]
fn normal_close_with_no_changes_reapplies_and_verifies() {
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    // Device currently has the open exposure: store unsuspended, source allow.
    assert!(!fake.suspended.iter().any(|item| item == "com.store"));
    let result = close(&mut fake, opened, &full_scan(), &[]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::Closed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(!is_maintenance_open(&result.envelope));
    assert!(!requires_forced_close(&result.envelope));
    assert_eq!(maintenance_state(&result.envelope), MaintenanceState::Closed);
    assert!(can_exit(&result.envelope));
    // Restrictions are re-established.
    assert!(fake.suspended.iter().any(|item| item == "com.store"));
    assert!(fake.suspended.iter().any(|item| item == "com.blocked"));
    assert_eq!(
        fake.appops.get(&("com.source".into(), "REQUEST_INSTALL_PACKAGES".into())),
        Some(&"ignore".to_owned())
    );
    let canonical = result.envelope.canonical_json();
    assert!(canonical.contains(r#""kind":"maintenance","open":false"#));
    assert_eq!(fake.private, fake.shared);
    assert_eq!(fake.private.as_deref(), Some(canonical.as_str()));
}

#[test]
fn approved_update_for_known_package_needs_no_new_suspend() {
    // An update to an already-known package is not a new install: the same
    // scan list closes without journaling any new suspension.
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    let result = close(&mut fake, opened.clone(), &full_scan(), &[]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::Closed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    // No suspension entries beyond the recorded universe + maintenance ids.
    let canonical = result.envelope.canonical_json();
    assert!(!canonical.contains("com.newapp"));
    assert!(!fake.suspended.iter().any(|item| item == "com.newapp"));
    assert_eq!(result.envelope.active_allowed_packages(), vec!["com.app".to_owned()]);
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn new_allowed_app_extends_allowlist_without_suspend() {
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    let mut scan = full_scan();
    scan.push(scanned("com.newapp", true, false));
    let result = close(&mut fake, opened, &scan, &[package("com.newapp")]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::Closed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(result.envelope.active_allowed_packages().contains(&"com.newapp".to_owned()));
    assert!(!fake.suspended.iter().any(|item| item == "com.newapp"));
    assert!(fake.suspended.iter().any(|item| item == "com.store"));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn new_blocked_app_is_journaled_before_suspension() {
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    let mut scan = full_scan();
    scan.push(scanned("com.evil", true, false));
    let result = close(&mut fake, opened, &scan, &[]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::Closed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    // Journaled before suspension: the envelope records the new suspend and
    // the device enforces it, so a later restore can undo exactly this.
    let canonical = result.envelope.canonical_json();
    assert!(canonical.contains(r#""package_id":"com.evil","suspended":true"#));
    assert!(fake.suspended.iter().any(|item| item == "com.evil"));
    assert!(!result.envelope.active_allowed_packages().contains(&"com.evil".to_owned()));
    assert_eq!(fake.private, fake.shared);
    // Non-launchable newcomers are ignored entirely.
    let (mut fake2, opened2) = open_applied();
    let mut scan2 = full_scan();
    scan2.push(scanned("com.hidden", false, false));
    let result2 = close(&mut fake2, opened2, &scan2, &[]).unwrap();
    assert_eq!(result2.outcome, CloseOutcome::Closed);
    assert!(!result2.envelope.canonical_json().contains("com.hidden"));
    assert!(!fake2.suspended.iter().any(|item| item == "com.hidden"));
}

#[test]
fn new_store_app_is_suspended_and_never_allowlisted() {
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    let mut scan = full_scan();
    scan.push(scanned("com.newstore", true, true));
    // Even when the caller approves it, a store can never enter the allowlist.
    let result = close(&mut fake, opened, &scan, &[package("com.newstore")]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::Closed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(!result.envelope.active_allowed_packages().contains(&"com.newstore".to_owned()));
    assert!(fake.suspended.iter().any(|item| item == "com.newstore"));
    assert!(result.envelope.canonical_json().contains(r#""package_id":"com.newstore","suspended":true"#));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn close_failure_on_store_suspend_retains_open_state() {
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    fake.fail_mutate_pkg = Some("com.newstore".into());
    let mut scan = full_scan();
    scan.push(scanned("com.newstore", true, true));
    let result = close(&mut fake, opened.clone(), &scan, &[]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::CloseFailed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    // Stores are hard-gated: recovery data is retained and maintenance stays
    // open (close is never journaled as an operation; the only open:false
    // present is the inverse of the open entry).
    assert!(is_maintenance_open(&result.envelope));
    assert!(requires_forced_close(&result.envelope));
    let canonical = result.envelope.canonical_json();
    assert_eq!(canonical.matches(r#""open":false"#).count(), 1);
    assert_eq!(canonical.matches(r#""open":true"#).count(), 1);
    assert!(canonical.contains("\"failed\""));
    assert_eq!(fake.private, fake.shared);
    assert_eq!(fake.private.as_deref(), Some(result.envelope.canonical_json().as_str()));
    // The failed store was not left suspended-and-claimed.
    assert!(!fake.suspended.iter().any(|item| item == "com.newstore"));
}

#[test]
fn close_retry_after_store_failure_suspends_and_closes() {
    // BL-01: a failed new-store suspend must not become "known" on retry.
    // Retry must suspend it under a new id and only then journal close.
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    let opened = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")])
        .unwrap()
        .envelope;
    assert_eq!(opened.baseline_hash(), hash);
    let mut scan = full_scan();
    scan.push(scanned("com.newstore", true, true));
    fake.fail_mutate_pkg = Some("com.newstore".into());
    let first = close(&mut fake, opened, &scan, &[]).unwrap();
    assert_eq!(first.outcome, CloseOutcome::CloseFailed);
    assert!(is_maintenance_open(&first.envelope));
    // Clear the fault and retry from the persisted envelope.
    fake.fail_mutate_pkg = None;
    let persisted = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert_eq!(persisted.canonical_json(), first.envelope.canonical_json());
    let second = close(&mut fake, persisted, &scan, &[]).unwrap();
    assert_eq!(second.outcome, CloseOutcome::Closed);
    assert_eq!(second.envelope.baseline_hash(), hash);
    assert!(!is_maintenance_open(&second.envelope));
    assert!(fake.suspended.iter().any(|item| item == "com.newstore"));
    let canonical = second.envelope.canonical_json();
    assert!(canonical.contains(r#""package_id":"com.newstore","suspended":true"#));
    // The retry suspends under a fresh id: the failed entry stays failed and
    // exactly one applied suspend records the store.
    assert!(canonical.contains("\"failed\""));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn close_failure_on_recorded_store_reapply_retains_open() {
    let (mut fake, opened) = open_applied();
    let hash = opened.baseline_hash();
    // Fail verification for the recorded store resuspend.
    fake.fail_verify_pkg = Some("com.store".into());
    let result = close(&mut fake, opened, &full_scan(), &[]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::CloseFailed);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(is_maintenance_open(&result.envelope));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn disconnect_after_every_device_op_is_recoverable() {
    // Successful close issues N device calls; disconnecting at any single
    // one must surface RecoverableDisconnect without speculative cleanup.
    let _ = open_applied();
    let probe = {
        let applied = applied_policy();
        let mut fake = Fake::seeded(&applied);
        let opened = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap().envelope;
        let mut scan = full_scan();
        scan.push(scanned("com.evil", true, false));
        let mut fake2 = Fake {
            private: fake.private.clone(),
            shared: fake.shared.clone(),
            suspended: fake.suspended.clone(),
            appops: fake.appops.clone(),
            ..Default::default()
        };
        let result = close(&mut fake2, opened, &scan, &[]).unwrap();
        assert_eq!(result.outcome, CloseOutcome::Closed);
        fake2.device_calls
    };
    assert!(probe > 3, "expected several device calls, got {probe}");
    for at in 1..=probe {
        let applied = applied_policy();
        let mut fake = Fake::seeded(&applied);
        let opened = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap().envelope;
        let mut scan = full_scan();
        scan.push(scanned("com.evil", true, false));
        let mut fake2 = Fake {
            private: fake.private.clone(),
            shared: fake.shared.clone(),
            suspended: fake.suspended.clone(),
            appops: fake.appops.clone(),
            disconnect_device_at: Some(at),
            ..Default::default()
        };
        let result = close(&mut fake2, opened.clone(), &scan, &[]).unwrap();
        assert_eq!(result.outcome, CloseOutcome::RecoverableDisconnect, "disconnect at device call {at}");
        assert_eq!(result.envelope.baseline_hash(), opened.baseline_hash());
        // No speculative cleanup: at least one mirror still holds a valid
        // envelope from the same baseline.
        let held = fake2.private.as_deref().or(fake2.shared.as_deref()).expect("mirror evidence retained");
        let parsed = RecoveryEnvelopeV1::parse(held).expect("valid envelope retained");
        assert_eq!(parsed.baseline_hash(), opened.baseline_hash());
    }
    // Mirror-read and mirror-write disconnects are recoverable too.
    for disconnect_read in [true, false] {
        let applied = applied_policy();
        let mut fake = Fake::seeded(&applied);
        fake.disconnect_read = disconnect_read;
        fake.disconnect_write = !disconnect_read;
        let base_hash = applied.baseline_hash();
        let opened_result = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap();
        assert_eq!(opened_result.outcome, OpenOutcome::RecoverableDisconnect);
        assert_eq!(opened_result.envelope.baseline_hash(), base_hash);
        assert!(fake.calls.is_empty() || !disconnect_read);
    }
}

#[test]
fn open_post_journal_disconnect_is_recoverable_with_agreeing_mirrors() {
    // MI-02(i): disconnecting at any device op of open's post-journal
    // verified/mutate phase must surface RecoverableDisconnect with agreeing
    // mirrors and the baseline intact.
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let probe_calls = {
        let mut fake = Fake::seeded(&applied);
        let result = open(&mut fake, applied.clone(), MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap();
        assert_eq!(result.outcome, OpenOutcome::Opened);
        fake.device_calls
    };
    assert!(probe_calls > 3, "expected several open device calls, got {probe_calls}");
    for at in 1..=probe_calls {
        let mut fake = Fake::seeded(&applied);
        fake.disconnect_device_at = Some(at);
        let result = open(&mut fake, applied.clone(), MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap();
        assert_eq!(result.outcome, OpenOutcome::RecoverableDisconnect, "open disconnect at device call {at}");
        assert_eq!(result.envelope.baseline_hash(), hash);
        // No speculative cleanup: mirrors agree on a valid same-baseline
        // envelope (pending-open or applied-open depending on where the
        // disconnect hit).
        assert_eq!(fake.private, fake.shared);
        let held = fake.private.as_deref().expect("mirror evidence retained");
        let parsed = RecoveryEnvelopeV1::parse(held).expect("valid envelope retained");
        assert_eq!(parsed.baseline_hash(), hash);
    }
}

#[test]
fn close_mirror_write_disconnect_recovers_via_reread_retry() {
    // MI-02(ii): a mid-close mirror-write disconnect is recoverable. The
    // in-memory pending envelope is discarded; retrying from the re-read
    // mirrors reallocates the same id and closes.
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    let opened = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")])
        .unwrap()
        .envelope;
    let mut scan = full_scan();
    scan.push(scanned("com.evil", true, false));
    fake.disconnect_write = true;
    let first = close(&mut fake, opened.clone(), &scan, &[]).unwrap();
    assert_eq!(first.outcome, CloseOutcome::RecoverableDisconnect);
    assert_eq!(first.envelope.baseline_hash(), hash);
    // Mirrors still hold the pre-pending opened envelope.
    assert_eq!(fake.private, fake.shared);
    assert_eq!(fake.private.as_deref(), Some(opened.canonical_json().as_str()));
    // Recovery: re-read the mirrors (as the existing resume test does) and
    // retry once the transport is back.
    fake.disconnect_write = false;
    let reread = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert_eq!(reread.canonical_json(), opened.canonical_json());
    let second = close(&mut fake, reread, &scan, &[]).unwrap();
    assert_eq!(second.outcome, CloseOutcome::Closed);
    assert_eq!(second.envelope.baseline_hash(), hash);
    assert!(!is_maintenance_open(&second.envelope));
    assert!(fake.suspended.iter().any(|item| item == "com.evil"));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn next_session_with_open_journal_forces_close_first() {
    let (_, opened) = open_applied();
    let hash = opened.baseline_hash();
    let canonical = opened.canonical_json();
    // Observed maintenance-open classifies to MaintenanceRecovery with
    // diagnostics/export only — edit/restore/resume are not offered.
    let state = ObservedState {
        explained: true,
        maintenance_open: true,
        pending_id: None,
        failed_required: false,
        restore_verified: false,
        cleanup_remaining: false,
        policy_traces: false,
    };
    let session = classify(
        Some(&canonical),
        Some(&canonical),
        "device",
        "maker/device",
        &hash,
        &state,
    );
    assert_eq!(session.kind, SessionKind::MaintenanceRecovery);
    assert!(!session.allowed_actions().iter().any(|action| {
        matches!(
            action,
            unscroll_desktop_lib::transaction::SessionAction::Resume
                | unscroll_desktop_lib::transaction::SessionAction::Rollback
                | unscroll_desktop_lib::transaction::SessionAction::Restore
        )
    }));
    // The maintenance helper wires the same gate for direct callers.
    assert!(requires_forced_close(&opened));
    assert!(ensure_no_forced_close(&opened).is_err());
    // A second open while open is refused; other actions must close first.
    let mut fake = Fake::seeded(&opened);
    // Mirrors hold the opened envelope; pass it as the reconciled view.
    fake.private = Some(canonical.clone());
    fake.shared = Some(canonical.clone());
    let again = open(&mut fake, opened.clone(), MAINTENANCE_CONFIRMATION, &[package("com.store")]);
    assert!(matches!(again, Err(MaintenanceError::AlreadyOpen)));
    assert!(fake.calls.is_empty());
}

#[test]
fn no_exit_path_silently_abandons_open_session() {
    let (mut fake, opened) = open_applied();
    // Attempting ordinary exit/finish while open is refused without mutation.
    assert!(try_exit(&opened).is_err());
    assert!(!can_exit(&opened));
    let calls_before = fake.calls.len();
    assert!(try_exit(&opened).is_err());
    assert_eq!(fake.calls.len(), calls_before);
    assert_eq!(fake.private, fake.shared);
    // After a verified close, exit is allowed.
    let result = close(&mut fake, opened, &full_scan(), &[]).unwrap();
    assert_eq!(result.outcome, CloseOutcome::Closed);
    assert!(try_exit(&result.envelope).is_ok());
    assert!(can_exit(&result.envelope));
}

#[test]
fn close_resume_after_disconnect_reuses_ids_without_duplicates() {
    let applied = applied_policy();
    let hash = applied.baseline_hash();
    let mut fake = Fake::seeded(&applied);
    let opened = open(&mut fake, applied, MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap().envelope;
    let mut scan = full_scan();
    scan.push(scanned("com.evil", true, false));
    // Disconnect on the first close device call (after the allowlist/new
    // suspend persist path begins); both mirrors agree on the pending shape.
    let mut failing = Fake {
        private: fake.private.clone(),
        shared: fake.shared.clone(),
        suspended: fake.suspended.clone(),
        appops: fake.appops.clone(),
        disconnect_device_at: Some(1),
        ..Default::default()
    };
    let first = close(&mut failing, opened.clone(), &scan, &[]).unwrap();
    assert_eq!(first.outcome, CloseOutcome::RecoverableDisconnect);
    assert_eq!(first.envelope.baseline_hash(), hash);
    assert_eq!(failing.private, failing.shared);
    // Retry with the persisted envelope once the device is back.
    let persisted = RecoveryEnvelopeV1::parse(failing.private.as_deref().unwrap()).unwrap();
    let mut retry = Fake {
        private: failing.private.clone(),
        shared: failing.shared.clone(),
        suspended: failing.suspended.clone(),
        appops: failing.appops.clone(),
        ..Default::default()
    };
    let second = close(&mut retry, persisted, &scan, &[]).unwrap();
    assert_eq!(second.outcome, CloseOutcome::Closed);
    assert_eq!(second.envelope.baseline_hash(), hash);
    assert!(!is_maintenance_open(&second.envelope));
    let canonical = second.envelope.canonical_json();
    for sequence in [4_000_001u64, 4_000_002] {
        assert_eq!(canonical.matches(&maintenance_id(sequence)).count(), 1, "id {sequence} once");
    }
    assert_eq!(retry.private, retry.shared);
    assert!(retry.suspended.iter().any(|item| item == "com.evil"));
    assert!(retry.suspended.iter().any(|item| item == "com.store"));
}

#[test]
fn stale_envelope_is_refused_without_mutation() {
    let applied = applied_policy();
    let mut fake = Fake::seeded(&applied);
    let opened = open(&mut fake, applied.clone(), MAINTENANCE_CONFIRMATION, &[package("com.store")]).unwrap().envelope;
    // Pass the pre-open envelope while mirrors hold the opened one.
    let mut stale_fake = Fake {
        private: fake.private.clone(),
        shared: fake.shared.clone(),
        suspended: fake.suspended.clone(),
        appops: fake.appops.clone(),
        ..Default::default()
    };
    let calls_before = stale_fake.calls.len();
    let result = close(&mut stale_fake, applied, &full_scan(), &[]);
    assert!(matches!(result, Err(MaintenanceError::Stale(_))));
    assert_eq!(stale_fake.calls.len(), calls_before);
    assert_eq!(stale_fake.private, stale_fake.shared);
    let _ = opened;
}

#[test]
fn unknown_approved_package_is_refused() {
    let (_, opened) = open_applied();
    let mut fake = Fake::seeded(&opened);
    fake.private = Some(opened.canonical_json());
    fake.shared = Some(opened.canonical_json());
    let result = close(&mut fake, opened, &full_scan(), &[package("com.stranger")]);
    assert!(matches!(result, Err(MaintenanceError::UnknownPackage(_))));
}
