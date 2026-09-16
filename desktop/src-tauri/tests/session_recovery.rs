use unscroll_desktop_lib::{
    recovery::{
        mirror::{MirrorStore, persist},
        model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1},
    },
    transaction::{SessionAction, SessionKind, classify, ObservedState},
};

const SERIAL: &str = "device";
const FINGERPRINT: &str = "maker/device";

fn baseline() -> RecoveryEnvelopeV1 {
    RecoveryEnvelopeV1::new_baseline(BaselineInput {
        binding: DeviceBinding { serial: SERIAL.into(), fingerprint: FINGERPRINT.into(), user_id: 0 },
        baseline_id: "11111111-1111-1111-1111-111111111111".into(),
        baseline_launcher: "com.base".into(),
        initial_home: "com.base/.Home".into(),
        initial_packages: vec![
            InitialPackageSuspension { package: "com.app".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.pre".into(), suspended: true, user_id: 0 },
        ],
        allowed_packages: vec!["com.app".into()],
    })
    .unwrap()
}

fn clean_state() -> ObservedState {
    ObservedState {
        explained: true,
        maintenance_open: false,
        pending_id: None,
        failed_required: false,
        restore_verified: false,
        cleanup_remaining: false,
        policy_traces: false,
    }
}

fn suspend_op(package: &str, suspended: bool) -> (String, String) {
    (
        format!(
            r#"{{"kind":"package_suspension","package_id":"{package}","suspended":{suspended},"user_id":0}}"#
        ),
        format!(
            r#"{{"kind":"package_suspension","package_id":"{package}","suspended":{},"user_id":0}}"#,
            !suspended
        ),
    )
}

fn id(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012x}")
}

#[test]
fn stale_copy_repair_is_classified_not_blocked() {
    let base = baseline();
    let (op, inv) = suspend_op("com.app", true);
    let stale = base.append_pending(&id(0), &op, &inv).unwrap();
    let newer = stale.mark_applied(&id(0)).unwrap();
    let session = classify(
        Some(&stale.canonical_json()),
        Some(&newer.canonical_json()),
        SERIAL,
        FINGERPRINT,
        &base.baseline_hash(),
        &clean_state(),
    );
    assert_eq!(session.kind, SessionKind::ActivePolicy);
    assert_eq!(session.envelope.unwrap().canonical_json(), newer.canonical_json());
    assert!(session.guidance.contains("stale") || session.guidance.contains("repair"));
}

#[test]
fn interrupted_transaction_offers_safe_resume() {
    let base = baseline();
    let (op, inv) = suspend_op("com.app", true);
    let pending = base.append_pending(&id(0), &op, &inv).unwrap();
    let canonical = pending.canonical_json();
    let mut state = clean_state();
    state.pending_id = Some(id(0));
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::ResumableTransaction);
    assert!(session.allowed_actions().contains(&SessionAction::Resume));
    assert!(session.allowed_actions().contains(&SessionAction::ExportDiagnostics));
    assert!(!session.allowed_actions().contains(&SessionAction::BeginSetup));
    assert_eq!(session.envelope.unwrap().canonical_json(), canonical);
}

#[test]
fn failed_required_step_offers_rollback_only() {
    let base = baseline();
    let (op, inv) = suspend_op("com.app", true);
    let failed = base.append_pending(&id(0), &op, &inv).unwrap().mark_failed(&id(0)).unwrap();
    let canonical = failed.canonical_json();
    let mut state = clean_state();
    state.failed_required = true;
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::RollbackOnly);
    assert!(session.allowed_actions().contains(&SessionAction::Rollback));
    assert!(!session.allowed_actions().contains(&SessionAction::Resume));
}

#[test]
fn unexplained_device_state_blocks_without_guessing() {
    let base = baseline();
    let canonical = base.canonical_json();
    let mut state = clean_state();
    state.explained = false;
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::BlockedInconsistency);
    assert_eq!(session.allowed_actions(), vec![SessionAction::ExportDiagnostics]);
}

#[test]
fn both_missing_without_traces_is_new_setup() {
    let session = classify(None, None, SERIAL, FINGERPRINT, &baseline().baseline_hash(), &clean_state());
    assert_eq!(session.kind, SessionKind::NewSetup);
    assert!(session.envelope.is_none());
    assert_eq!(session.allowed_actions(), vec![SessionAction::BeginSetup]);
    assert!(!session.guidance.is_empty());
}

#[test]
fn both_missing_with_policy_traces_is_blocked_with_manual_guidance() {
    let mut state = clean_state();
    state.policy_traces = true;
    let session = classify(None, None, SERIAL, FINGERPRINT, &baseline().baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::BlockedInconsistency);
    assert!(session.envelope.is_none());
    assert_eq!(session.allowed_actions(), vec![SessionAction::ExportDiagnostics]);
    assert!(session.guidance.contains("manual"));
    assert!(session.guidance.contains("factory") || session.guidance.contains("ADB"));
}

#[test]
fn maintenance_open_takes_precedence_over_resume() {
    let base = baseline();
    let (op, inv) = suspend_op("com.app", true);
    let pending = base.append_pending(&id(0), &op, &inv).unwrap();
    let canonical = pending.canonical_json();
    let mut state = clean_state();
    state.maintenance_open = true;
    state.pending_id = Some(id(0));
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::MaintenanceRecovery);
    assert!(!session.allowed_actions().contains(&SessionAction::Resume));
    assert!(!session.allowed_actions().contains(&SessionAction::Rollback));
    assert!(!session.allowed_actions().contains(&SessionAction::Restore));
}

#[test]
fn forked_histories_block() {
    let base = baseline();
    let (op_a, inv_a) = suspend_op("com.app", true);
    let (op_b, inv_b) = suspend_op("com.pre", false);
    let first = base.append_pending(&id(0), &op_a, &inv_a).unwrap();
    let second = base.append_pending(&id(0), &op_b, &inv_b).unwrap();
    let session = classify(
        Some(&first.canonical_json()),
        Some(&second.canonical_json()),
        SERIAL,
        FINGERPRINT,
        &base.baseline_hash(),
        &clean_state(),
    );
    assert_eq!(session.kind, SessionKind::BlockedInconsistency);
    assert_eq!(session.allowed_actions(), vec![SessionAction::ExportDiagnostics]);
}

#[test]
fn restore_verified_without_cleanup_is_restore_ready() {
    let base = baseline();
    let canonical = base.canonical_json();
    let mut state = clean_state();
    state.restore_verified = true;
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::RestoreReady);
    assert!(session.allowed_actions().contains(&SessionAction::Restore));
}

#[test]
fn cleanup_remaining_without_verification_blocks() {
    let base = baseline();
    let canonical = base.canonical_json();
    let mut state = clean_state();
    state.cleanup_remaining = true;
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::BlockedInconsistency);
}

#[test]
fn verified_cleanup_retry_offers_retry_cleanup_action() {
    let base = baseline();
    let canonical = base.canonical_json();
    let mut state = clean_state();
    state.restore_verified = true;
    state.cleanup_remaining = true;
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::CleanupRetry);
    assert!(session.allowed_actions().contains(&SessionAction::RetryCleanup));
    assert!(!session.allowed_actions().contains(&SessionAction::Restore));
    assert!(session.allowed_actions().contains(&SessionAction::ExportDiagnostics));
}

#[test]
fn pending_mismatch_blocks() {
    let base = baseline();
    let (op, inv) = suspend_op("com.app", true);
    let pending = base.append_pending(&id(0), &op, &inv).unwrap();
    let canonical = pending.canonical_json();
    let mut state = clean_state();
    state.pending_id = Some(id(7));
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &base.baseline_hash(), &state);
    assert_eq!(session.kind, SessionKind::BlockedInconsistency);
}

#[test]
fn baseline_mismatch_blocks() {
    let base = baseline();
    let canonical = base.canonical_json();
    let other = RecoveryEnvelopeV1::new_baseline(BaselineInput {
        binding: DeviceBinding { serial: SERIAL.into(), fingerprint: FINGERPRINT.into(), user_id: 0 },
        baseline_id: "22222222-2222-2222-2222-222222222222".into(),
        baseline_launcher: "com.base".into(),
        initial_home: "com.base/.Home".into(),
        initial_packages: vec![InitialPackageSuspension {
            package: "com.app".into(),
            suspended: false,
            user_id: 0,
        }],
        allowed_packages: vec![],
    })
    .unwrap();
    let session =
        classify(Some(&canonical), Some(&canonical), SERIAL, FINGERPRINT, &other.baseline_hash(), &clean_state());
    assert_eq!(session.kind, SessionKind::BlockedInconsistency);
}

#[derive(Default)]
struct FakeMirrors {
    private: Option<String>,
    shared: Option<String>,
}

impl MirrorStore for FakeMirrors {
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
        self.private = None;
        Ok(())
    }
    fn delete_shared(&mut self) -> Result<(), ()> {
        self.shared = None;
        Ok(())
    }
}

#[test]
fn stale_repair_guidance_plus_persist_heals_both_mirrors() {
    // Finding 9: classify is pure — it reports the repair; the caller heals by
    // persisting the returned envelope to both copies.
    let base = baseline();
    let hash = base.baseline_hash();
    let (op, inv) = suspend_op("com.app", true);
    let stale = base.append_pending(&id(0), &op, &inv).unwrap();
    let newer = stale.mark_applied(&id(0)).unwrap();
    let session = classify(
        Some(&stale.canonical_json()),
        Some(&newer.canonical_json()),
        SERIAL,
        FINGERPRINT,
        &hash,
        &clean_state(),
    );
    assert_eq!(session.kind, SessionKind::ActivePolicy);
    let repaired = session.envelope.as_ref().unwrap();
    assert_eq!(repaired.canonical_json(), newer.canonical_json());
    assert_eq!(repaired.baseline_hash(), hash);
    assert!(session.guidance.contains("persist") || session.guidance.contains("stale"));
    let mut mirrors = FakeMirrors {
        private: Some(stale.canonical_json()),
        shared: Some(newer.canonical_json()),
    };
    persist(&mut mirrors, repaired).unwrap();
    assert_eq!(mirrors.private, mirrors.shared);
    assert_eq!(mirrors.private.as_deref(), Some(newer.canonical_json().as_str()));
}
