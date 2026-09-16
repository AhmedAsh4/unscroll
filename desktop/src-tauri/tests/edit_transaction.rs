use unscroll_desktop_lib::{
    adb::PackageId,
    policy::Operation,
    recovery::{
        mirror::MirrorStore,
        model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1},
    },
    transaction::{ApplyDevice, DeviceFailure, EditError, EditOutcome, edit},
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
            InitialPackageSuspension { package: "com.keep".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.pre".into(), suspended: true, user_id: 0 },
        ],
        allowed_packages: vec!["com.keep".into()],
    })
    .unwrap()
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

fn edit_id(sequence: u64) -> String {
    format!("00000000-0000-4000-8000-{sequence:012x}")
}

#[derive(Default)]
struct Fake {
    private: Option<String>,
    shared: Option<String>,
    suspended: Vec<String>,
    calls: Vec<String>,
    disconnect_mutate: bool,
    disconnect_verify: bool,
    fail_verify_pkg: Option<String>,
}

impl Fake {
    fn seeded(envelope: &RecoveryEnvelopeV1) -> Self {
        Self {
            private: Some(envelope.canonical_json()),
            shared: Some(envelope.canonical_json()),
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
        self.private = None;
        Ok(())
    }
    fn delete_shared(&mut self) -> Result<(), ()> {
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
        if let Operation::Suspend { package, suspended, .. } = operation {
            if *suspended {
                if !self.suspended.iter().any(|item| item == package.as_str()) {
                    self.suspended.push(package.as_str().into());
                }
            } else {
                self.suspended.retain(|item| item != package.as_str());
            }
        }
        Ok(())
    }
    fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure> {
        if self.disconnect_verify {
            return Err(DeviceFailure::Disconnect);
        }
        Ok(match operation {
            Operation::Suspend { package, suspended, .. }
                if self.fail_verify_pkg.as_deref() == Some(package.as_str()) =>
            {
                false
            }
            Operation::Suspend { package, suspended, .. } => {
                self.suspended.iter().any(|item| item == package.as_str()) == *suspended
            }
            _ => true,
        })
    }
}

#[test]
fn applies_only_the_allowlist_delta() {
    let base = baseline();
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let mut fake = Fake::seeded(&base);
    let result = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::Complete);
    assert!(fake.suspended.iter().any(|item| item == "com.keep"));
    assert!(!fake.suspended.iter().any(|item| item == "com.app"));
    assert!(!fake.calls.iter().any(|call| call.contains("com.pre")));
    assert_eq!(fake.private, fake.shared);
    assert!(fake.private.is_some());
    assert_eq!(result.envelope.baseline_hash(), base.baseline_hash());
    assert_eq!(result.envelope.active_allowed_packages(), vec!["com.app".to_owned()]);
    assert_eq!(result.envelope.pending_id(), None);
}

#[test]
fn no_op_edit_writes_nothing() {
    let base = baseline();
    let same = vec![package("com.keep")];
    let mut fake = Fake::seeded(&base);
    let result = edit(&mut fake, base.clone(), &same, &same, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::Complete);
    assert_eq!(result.envelope.canonical_json(), base.canonical_json());
    assert_eq!(fake.private.as_deref(), Some(base.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
    assert!(fake.calls.is_empty());
}

#[test]
fn unknown_package_is_refused_without_mutation() {
    let base = baseline();
    let before = vec![package("com.keep")];
    let after = vec![package("com.keep"), package("com.stranger")];
    let mut fake = Fake::seeded(&base);
    let result = edit(&mut fake, base.clone(), &before, &after, &[]);
    assert!(result.is_err());
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(base.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn stale_before_is_refused_without_mutation() {
    let base = baseline();
    let before = vec![package("com.app")];
    let after = vec![package("com.keep")];
    let mut fake = Fake::seeded(&base);
    let result = edit(&mut fake, base.clone(), &before, &after, &[]);
    assert!(result.is_err());
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(base.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn disconnect_preserves_pending_in_both_copies() {
    let base = baseline();
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let mut fake = Fake { private: Some(base.canonical_json()), shared: Some(base.canonical_json()), disconnect_mutate: true, ..Default::default() };
    let result = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::RecoverableDisconnect);
    assert_eq!(fake.private, fake.shared);
    let stored = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert!(stored.pending_id().is_some());
    assert_eq!(stored.baseline_hash(), base.baseline_hash());
    assert!(!fake.suspended.iter().any(|item| item == "com.keep"));
}

#[test]
fn failed_verification_blocks_with_failed_journal() {
    let base = baseline();
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let mut fake = Fake { private: Some(base.canonical_json()), shared: Some(base.canonical_json()), fail_verify_pkg: Some("com.app".into()), ..Default::default() };
    let result = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::Blocked);
    assert_eq!(fake.private, fake.shared);
    let stored = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert!(stored.canonical_json().contains("\"failed\""));
    assert_eq!(stored.baseline_hash(), base.baseline_hash());
}

#[test]
fn edit_resume_after_disconnect_reuses_ids_without_duplicates() {
    // Finding 1: hex-formatted edit ids must round-trip, so a disconnect
    // mid-edit followed by a retry completes with the same id range.
    let base = baseline();
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let mut fake = Fake {
        private: Some(base.canonical_json()),
        shared: Some(base.canonical_json()),
        disconnect_mutate: true,
        ..Default::default()
    };
    let first = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(first.outcome, EditOutcome::RecoverableDisconnect);
    let pending_id = first.envelope.pending_id().expect("pending edit entry");
    assert_eq!(pending_id, edit_id(3_000_000));
    assert_eq!(fake.private, fake.shared);
    let hash = base.baseline_hash();
    assert_eq!(first.envelope.baseline_hash(), hash);
    // Retry with the persisted envelope once the device is back.
    fake.disconnect_mutate = false;
    let persisted = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert_eq!(persisted.canonical_json(), first.envelope.canonical_json());
    let second = edit(&mut fake, persisted, &before, &after, &[]).unwrap();
    assert_eq!(second.outcome, EditOutcome::Complete);
    assert_eq!(second.envelope.pending_id(), None);
    assert_eq!(second.envelope.baseline_hash(), hash);
    let canonical = second.envelope.canonical_json();
    // Same id range, no duplicates: exactly the three step ids, each once.
    for sequence in [3_000_000u64, 3_000_001, 3_000_002] {
        assert_eq!(canonical.matches(&edit_id(sequence)).count(), 1, "id {sequence} once");
    }
    assert!(!canonical.contains(&edit_id(3_000_003)));
    assert_eq!(fake.private, fake.shared);
    assert_eq!(fake.private.as_deref(), Some(canonical.as_str()));
    assert_eq!(second.envelope.active_allowed_packages(), vec!["com.app".to_owned()]);
}

#[test]
fn stale_envelope_edit_is_refused_without_mutation() {
    // Finding 2: a stale envelope must not overwrite newer history.
    let base = baseline();
    let (op, inv) = suspend_op("com.app", true);
    let newer = base.append_pending(&edit_id(0), &op, &inv).unwrap().mark_applied(&edit_id(0)).unwrap();
    let mut fake = Fake::seeded(&newer);
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let result = edit(&mut fake, base.clone(), &before, &after, &[]);
    assert!(matches!(result, Err(unscroll_desktop_lib::transaction::EditError::Blocked(_))));
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(newer.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
    // Documented: when the private copy is missing but the shared copy is
    // valid, the caller must persist the repair first (see session guidance
    // and the persist-heals-mirrors test); edit refuses the stale view rather
    // than healing implicitly.
}

#[test]
fn protected_package_is_not_suspended_but_policy_updates() {
    // Finding 3: caller-supplied protection keeps the package available while
    // the launcher-policy write still records `after`.
    let base = baseline();
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let protected = vec![package("com.keep")];
    let mut fake = Fake::seeded(&base);
    let result = edit(&mut fake, base.clone(), &before, &after, &protected).unwrap();
    assert_eq!(result.outcome, EditOutcome::Complete);
    assert_eq!(result.envelope.baseline_hash(), base.baseline_hash());
    assert_eq!(result.envelope.active_allowed_packages(), vec!["com.app".to_owned()]);
    assert!(!fake.suspended.iter().any(|item| item == "com.keep"));
    assert!(!fake.calls.iter().any(|call| call.contains("Suspend") && call.contains("com.keep")));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn journaled_package_counts_as_known() {
    // Finding 8: maintenance-journaled packages are part of the known universe.
    let base = baseline();
    let (op, inv) = suspend_op("com.journaled", true);
    let journaled = base.append_pending(&edit_id(0), &op, &inv).unwrap().mark_applied(&edit_id(0)).unwrap();
    let mut fake = Fake::seeded(&journaled);
    let before = vec![package("com.keep")];
    let after = vec![package("com.keep"), package("com.journaled")];
    let result = edit(&mut fake, journaled.clone(), &before, &after, &[]);
    assert!(result.is_ok(), "journaled package must not be UnknownPackage: {result:?}");
    let result = result.unwrap();
    assert_eq!(result.outcome, EditOutcome::Complete);
    assert_eq!(result.envelope.baseline_hash(), base.baseline_hash());
    assert!(result.envelope.active_allowed_packages().contains(&"com.journaled".to_owned()));
    assert_eq!(fake.private, fake.shared);
}

#[derive(Default)]
struct DisconnectFake {
    private: Option<String>,
    shared: Option<String>,
    suspended: Vec<String>,
    calls: Vec<String>,
    fail_read_private_disconnect: bool,
    fail_write_private_disconnect: bool,
    fail_write_private_corrupt: bool,
}

impl DisconnectFake {
    fn seeded(envelope: &RecoveryEnvelopeV1) -> Self {
        Self {
            private: Some(envelope.canonical_json()),
            shared: Some(envelope.canonical_json()),
            ..Default::default()
        }
    }
}

impl MirrorStore for DisconnectFake {
    type Error = DeviceFailure;
    fn write_private(&mut self, value: &str) -> Result<(), DeviceFailure> {
        if self.fail_write_private_disconnect {
            return Err(DeviceFailure::Disconnect);
        }
        if self.fail_write_private_corrupt {
            return Err(DeviceFailure::Inconsistent);
        }
        self.private = Some(value.into());
        Ok(())
    }
    fn write_shared(&mut self, value: &str) -> Result<(), DeviceFailure> {
        self.shared = Some(value.into());
        Ok(())
    }
    fn read_private(&mut self) -> Result<Option<String>, DeviceFailure> {
        if self.fail_read_private_disconnect {
            return Err(DeviceFailure::Disconnect);
        }
        Ok(self.private.clone())
    }
    fn read_shared(&mut self) -> Result<Option<String>, DeviceFailure> {
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

impl ApplyDevice for DisconnectFake {
    fn mutate(&mut self, operation: &Operation) -> Result<(), DeviceFailure> {
        self.calls.push(format!("{operation:?}"));
        if let Operation::Suspend { package, suspended, .. } = operation {
            if *suspended {
                if !self.suspended.iter().any(|item| item == package.as_str()) {
                    self.suspended.push(package.as_str().into());
                }
            } else {
                self.suspended.retain(|item| item != package.as_str());
            }
        }
        Ok(())
    }
    fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure> {
        Ok(match operation {
            Operation::Suspend { package, suspended, .. } => {
                self.suspended.iter().any(|item| item == package.as_str()) == *suspended
            }
            _ => true,
        })
    }
}

#[test]
fn pre_read_disconnect_is_recoverable_without_mutation() {
    // WR-04: a disconnect while reading mirrors must surface as
    // RecoverableDisconnect with the unchanged envelope, not a Mirror error.
    let base = baseline();
    let hash = base.baseline_hash();
    let mut fake = DisconnectFake::seeded(&base);
    fake.fail_read_private_disconnect = true;
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let result = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::RecoverableDisconnect);
    assert_eq!(result.envelope.canonical_json(), base.canonical_json());
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(base.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn persist_disconnect_preserves_envelope_as_recoverable() {
    // WR-01: a disconnect while persisting must return RecoverableDisconnect
    // with the pending envelope, not Err(Mirror) that drops it.
    let base = baseline();
    let hash = base.baseline_hash();
    let mut fake = DisconnectFake::seeded(&base);
    fake.fail_write_private_disconnect = true;
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let result = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::RecoverableDisconnect);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(result.envelope.pending_id().is_some());
    assert!(fake.calls.is_empty());
}

#[test]
fn persist_corruption_stays_an_error() {
    // WR-01: genuine mirror corruption (non-disconnect) must stay Err.
    let base = baseline();
    let mut fake = DisconnectFake::seeded(&base);
    fake.fail_write_private_corrupt = true;
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let result = edit(&mut fake, base.clone(), &before, &after, &[]);
    assert!(matches!(result, Err(EditError::Mirror(_))), "expected Mirror, got {result:?}");
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(base.canonical_json().as_str()));
}

fn baseline_with_policy_only_extra() -> RecoveryEnvelopeV1 {
    RecoveryEnvelopeV1::new_baseline(BaselineInput {
        binding: DeviceBinding { serial: "device".into(), fingerprint: "maker/device".into(), user_id: 0 },
        baseline_id: "11111111-1111-1111-1111-111111111111".into(),
        baseline_launcher: "com.base".into(),
        initial_home: "com.base/.Home".into(),
        initial_packages: vec![
            InitialPackageSuspension { package: "com.app".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.keep".into(), suspended: false, user_id: 0 },
            InitialPackageSuspension { package: "com.pre".into(), suspended: true, user_id: 0 },
        ],
        allowed_packages: vec!["com.keep".into(), "com.policyonly".into()],
    })
    .unwrap()
}

#[test]
fn policy_only_churn_does_not_yield_false_unknown_package() {
    // WR-02: LauncherPolicy.allowed packages are part of the known universe,
    // so a protected (policy-only) remove followed by re-add succeeds.
    let base = baseline_with_policy_only_extra();
    let hash = base.baseline_hash();
    let mut fake = Fake::seeded(&base);
    let protected = vec![package("com.policyonly")];
    let remove_before = vec![package("com.keep"), package("com.policyonly")];
    let remove_after = vec![package("com.keep")];
    let removed = edit(&mut fake, base.clone(), &remove_before, &remove_after, &protected).unwrap();
    assert_eq!(removed.outcome, EditOutcome::Complete);
    assert_eq!(removed.envelope.baseline_hash(), hash);
    assert_eq!(fake.private, fake.shared);
    let readd = edit(&mut fake, removed.envelope, &remove_after, &remove_before, &protected).unwrap();
    assert_eq!(readd.outcome, EditOutcome::Complete);
    assert_eq!(readd.envelope.baseline_hash(), hash);
    assert!(readd.envelope.active_allowed_packages().contains(&"com.policyonly".to_owned()));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn mismatched_before_retry_is_refused_without_completion() {
    // WR-03: a resuming retry with a different `before` (here
    // before==after==active) must not build a 1-step window and Complete.
    let base = baseline();
    let hash = base.baseline_hash();
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let mut fake = Fake {
        private: Some(base.canonical_json()),
        shared: Some(base.canonical_json()),
        disconnect_mutate: true,
        ..Default::default()
    };
    let first = edit(&mut fake, base.clone(), &before, &after, &[]).unwrap();
    assert_eq!(first.outcome, EditOutcome::RecoverableDisconnect);
    let persisted = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert_eq!(persisted.baseline_hash(), hash);
    let calls_after_first = fake.calls.len();
    fake.disconnect_mutate = false;
    let wrong_before = vec![package("com.app")];
    let wrong_after = vec![package("com.app")];
    let retry = edit(&mut fake, persisted, &wrong_before, &wrong_after, &[]);
    assert!(matches!(retry, Err(EditError::Blocked(_))), "expected Blocked, got {retry:?}");
    assert_eq!(fake.calls.len(), calls_after_first);
    assert_eq!(fake.private, fake.shared);
    assert_eq!(
        RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap().baseline_hash(),
        hash
    );
}

#[test]
fn resume_reverifies_applied_steps_before_skipping() {
    // WR-03: an applied suspend that no longer verifies must Block on resume
    // instead of being silently skipped to Complete.
    let base = baseline();
    let hash = base.baseline_hash();
    let policy = base
        .append_pending_launcher_policy(&edit_id(3_000_000), vec!["com.app".into()])
        .unwrap()
        .mark_applied(&edit_id(3_000_000))
        .unwrap();
    let (keep_op, keep_inv) = suspend_op("com.keep", true);
    let suspended = policy
        .append_pending(&edit_id(3_000_001), &keep_op, &keep_inv)
        .unwrap()
        .mark_applied(&edit_id(3_000_001))
        .unwrap();
    let (app_op, app_inv) = suspend_op("com.app", false);
    let pending = suspended.append_pending(&edit_id(3_000_002), &app_op, &app_inv).unwrap();
    assert_eq!(pending.baseline_hash(), hash);
    let mut fake = Fake::seeded(&pending);
    let before = vec![package("com.keep")];
    let after = vec![package("com.app")];
    let result = edit(&mut fake, pending.clone(), &before, &after, &[]).unwrap();
    assert_eq!(result.outcome, EditOutcome::Blocked);
    assert_eq!(result.envelope.baseline_hash(), hash);
    assert!(fake.calls.is_empty());
    assert_eq!(fake.private.as_deref(), Some(pending.canonical_json().as_str()));
    assert_eq!(fake.private, fake.shared);
}

#[test]
fn retry_after_disconnect_does_not_reuse_previous_generation_tail() {
    // CR-03: a retry of gen2 (pending at 3000003) must anchor to its own
    // policy entry and reuse exactly [3000003, 3000004], never reach back
    // into gen1's applied tail at 3000002. Without the anchor the retry
    // reports Complete with no journaled suspend — a later restore then
    // permanently misses that unsuspend.
    let base = baseline();
    let hash = base.baseline_hash();
    let mut fake = Fake::seeded(&base);
    // Gen1 before=[keep] after=[app] completes: 3000000-2 Applied.
    let gen1 = edit(&mut fake, base.clone(), &[package("com.keep")], &[package("com.app")], &[]).unwrap();
    assert_eq!(gen1.outcome, EditOutcome::Complete);
    assert_eq!(gen1.envelope.pending_id(), None);
    assert_eq!(gen1.envelope.baseline_hash(), hash);
    assert_eq!(gen1.envelope.active_allowed_packages(), vec!["com.app".to_owned()]);
    assert!(fake.suspended.iter().any(|item| item == "com.keep"));
    assert_eq!(fake.private, fake.shared);
    // Gen2 before=[app] after=[] disconnects on its first step; both mirrors
    // hold the pending-3000003 envelope.
    fake.disconnect_mutate = true;
    let gen2_before = vec![package("com.app")];
    let gen2_after: Vec<PackageId> = vec![];
    let attempt = edit(&mut fake, gen1.envelope.clone(), &gen2_before, &gen2_after, &[]).unwrap();
    assert_eq!(attempt.outcome, EditOutcome::RecoverableDisconnect);
    assert_eq!(attempt.envelope.pending_id().as_deref(), Some(edit_id(3_000_003).as_str()));
    assert_eq!(attempt.envelope.baseline_hash(), hash);
    assert_eq!(fake.private, fake.shared);
    // Repair path: retry identical args against the persisted envelope.
    fake.disconnect_mutate = false;
    let persisted = RecoveryEnvelopeV1::parse(fake.private.as_deref().unwrap()).unwrap();
    assert_eq!(persisted.canonical_json(), attempt.envelope.canonical_json());
    assert_eq!(persisted.baseline_hash(), hash);
    let retry = edit(&mut fake, persisted, &gen2_before, &gen2_after, &[]).unwrap();
    assert_eq!(retry.outcome, EditOutcome::Complete);
    assert_eq!(retry.envelope.pending_id(), None);
    assert_eq!(retry.envelope.baseline_hash(), hash);
    assert_eq!(fake.private, fake.shared);
    assert_eq!(fake.private.as_deref(), Some(retry.envelope.canonical_json().as_str()));
    let canonical = retry.envelope.canonical_json();
    // Exact id reuse: gen1 ids plus gen2's own [S, S+1], each exactly once.
    for sequence in [3_000_000u64, 3_000_001, 3_000_002, 3_000_003, 3_000_004] {
        assert_eq!(canonical.matches(&edit_id(sequence)).count(), 1, "id {sequence} once");
    }
    assert!(!canonical.contains(&edit_id(3_000_005)));
    // The suspend step must be journaled, not just executed: the journal
    // holds the suspend op JSON for the removed package.
    assert!(
        canonical.contains(r#""package_id":"com.app","suspended":true"#),
        "journal must contain the gen2 suspend op"
    );
    assert!(fake.suspended.iter().any(|item| item == "com.app"));
    assert_eq!(retry.envelope.active_allowed_packages(), Vec::<String>::new());
}
