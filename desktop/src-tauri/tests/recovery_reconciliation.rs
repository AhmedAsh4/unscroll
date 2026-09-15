use std::path::PathBuf;

use unscroll_desktop_lib::adb::{AdbResponse, FakeAdb, Fingerprint, Serial};
use unscroll_desktop_lib::recovery::mirror::{cleanup, persist_adb, MirrorError, MirrorStore};
use unscroll_desktop_lib::recovery::model::RecoveryEnvelopeV1;
use unscroll_desktop_lib::recovery::reconcile::{reconcile_checked, Outcome};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/recovery-v1")
            .join(name),
    )
    .unwrap()
}

#[test]
fn repairs_only_a_missing_or_strictly_stale_copy() {
    let baseline = fixture("valid/new-baseline.json");
    let pending = fixture("valid/pending-mutation.json");

    assert_eq!(reconcile(Some(&baseline), Some(&baseline)), Outcome::Consistent);
    assert_eq!(
        reconcile(Some(&baseline), Some(&pending)),
        Outcome::RepairPrivate(pending.trim().to_owned())
    );
    assert_eq!(
        reconcile(None, Some(&pending)),
        Outcome::RepairPrivate(pending.trim().to_owned())
    );
    assert_eq!(reconcile(Some(&pending), None), Outcome::MissingShared);
    assert_eq!(reconcile(None, None), Outcome::BothMissing);
}

#[test]
fn blocks_corruption_instead_of_using_metadata_or_device_guesses() {
    let baseline = fixture("valid/new-baseline.json");
    let corrupt = fixture("invalid/checksum-failure.json");
    assert_eq!(reconcile(Some(&baseline), Some(&corrupt)), Outcome::Corruption);
}

#[test]
fn blocks_a_revision_gap_instead_of_repairing_it() {
    let baseline = fixture("valid/new-baseline.json");
    let gapped = fixture("valid/strict-extension.json");
    assert_eq!(reconcile(Some(&baseline), Some(&gapped)), Outcome::Fork);
}

#[test]
fn appends_pending_then_applied_as_linked_single_revisions() {
    let baseline = RecoveryEnvelopeV1::parse(&fixture("valid/new-baseline.json")).unwrap();
    let pending = baseline
        .append_pending(
            "22222222-2222-2222-2222-222222222222",
            r#"{"kind":"package_suspension","package_id":"com.social","suspended":true,"user_id":0}"#,
            r#"{"kind":"package_suspension","package_id":"com.social","suspended":false,"user_id":0}"#,
        )
        .unwrap();
    assert_eq!(pending.canonical_json(), fixture("valid/pending-mutation.json").trim());
    let applied = pending.mark_applied("22222222-2222-2222-2222-222222222222").unwrap();
    assert_eq!(applied.canonical_json(), fixture("valid/applied-mutation.json").trim());
    assert!(baseline.is_strict_prefix_of(&pending).is_ok());
    assert!(pending.is_strict_prefix_of(&applied).is_ok());
}

#[derive(Default)]
struct CleanupStore { private: Option<String>, shared: Option<String>, fail_shared_delete: bool }
impl MirrorStore for CleanupStore {
    type Error = ();
    fn write_private(&mut self, value: &str) -> Result<(), ()> { self.private = Some(value.into()); Ok(()) }
    fn write_shared(&mut self, value: &str) -> Result<(), ()> { self.shared = Some(value.into()); Ok(()) }
    fn read_private(&mut self) -> Result<Option<String>, ()> { Ok(self.private.clone()) }
    fn read_shared(&mut self) -> Result<Option<String>, ()> { Ok(self.shared.clone()) }
    fn delete_private(&mut self) -> Result<(), ()> { self.private = None; Ok(()) }
    fn delete_shared(&mut self) -> Result<(), ()> { if self.fail_shared_delete { self.fail_shared_delete = false; Err(()) } else { self.shared = None; Ok(()) } }
}

fn cleanup_envelope() -> RecoveryEnvelopeV1 {
    let baseline = RecoveryEnvelopeV1::parse(&fixture("valid/new-baseline.json")).unwrap();
    baseline.append_pending("33333333-3333-3333-3333-333333333333", r#"{"kind":"cleanup","removed":true,"target":"private_envelope"}"#, r#"{"kind":"cleanup","removed":false,"target":"private_envelope"}"#).unwrap().mark_applied("33333333-3333-3333-3333-333333333333").unwrap()
}

#[test]
fn cleanup_removes_a_normal_checked_pair() {
    let value = cleanup_envelope().canonical_json(); let expected = RecoveryEnvelopeV1::parse(&value).unwrap(); let mut store = CleanupStore { private: Some(value.clone()), shared: Some(value), ..Default::default() };
    assert_eq!(cleanup(&mut store, true, &expected), Ok(())); assert!(store.private.is_none() && store.shared.is_none());
}
#[test]
fn cleanup_retains_shared_after_accidental_private_loss() {
    let baseline = RecoveryEnvelopeV1::parse(&fixture("valid/new-baseline.json")).unwrap(); let value = baseline.canonical_json(); let mut store = CleanupStore { private: None, shared: Some(value), ..Default::default() };
    assert_eq!(cleanup(&mut store, true, &baseline), Err(MirrorError::NotRestored)); assert!(store.shared.is_some());
}
#[test]
fn cleanup_retries_after_shared_delete_failure() {
    let value = cleanup_envelope().canonical_json(); let expected = RecoveryEnvelopeV1::parse(&value).unwrap(); let mut store = CleanupStore { private: Some(value.clone()), shared: Some(value), fail_shared_delete: true };
    assert_eq!(cleanup(&mut store, true, &expected), Err(MirrorError::SharedDelete)); assert!(store.private.is_none() && store.shared.is_some()); assert_eq!(cleanup(&mut store, true, &expected), Ok(()));
}
fn private_reply(envelope: &str) -> String {
    format!(r#"Result: Bundle[{{response={{"protocol_version":"bridge-v1","ok":true,"result":{{"envelope":{envelope:?}}}}}}}]"#)
}

#[test]
fn initializes_a_snapshot_baseline_at_revision_zero_before_mirroring() {
    use unscroll_desktop_lib::adb::{AdbCommand, BridgeOperation, Component, DeviceOperation, PackageId};
    use unscroll_desktop_lib::device::{AppCatalogEntry, CapabilityReport, DeviceSnapshot, RecoveryObservation};
    use unscroll_desktop_lib::recovery::mirror::initialize_adb;
    use unscroll_desktop_lib::recovery::model::{BaselineInput, DeviceBinding, InitialPackageSuspension};

    let snapshot = DeviceSnapshot { serial: "ABC123".into(), api: 35, model: "Pixel".into(), manufacturer: "Google".into(), fingerprint: "google/pixel/test".into(), current_user: 0, home: Component::parse("com.android.launcher/.Home").unwrap(), catalog: vec![AppCatalogEntry { package: PackageId::parse("com.example.music").unwrap(), component: Component::parse("com.example.music/.Main").unwrap(), label: "Music".into(), icon: Vec::new(), suspended: true, enabled: true }, AppCatalogEntry { package: PackageId::parse("org.unscroll.launcher").unwrap(), component: Component::parse("org.unscroll.launcher/.Main").unwrap(), label: "Unscroll".into(), icon: Vec::new(), suspended: false, enabled: true }], protected: Vec::new(), stores: Vec::new(), install_sources: Vec::new(), profiles: Vec::new(), private_recovery: RecoveryObservation::Missing, shared_recovery: RecoveryObservation::Missing, xiaomi_guidance: Vec::new(), capabilities: CapabilityReport { package_suspension: true, bridge: true, recovery_storage: true, app_op_inspection: true, home_path: true } };
    let launcher = PackageId::parse("org.unscroll.launcher").unwrap(); let allowed = vec![launcher.clone(), PackageId::parse("com.example.music").unwrap()];
    let baseline = RecoveryEnvelopeV1::new_baseline(BaselineInput { binding: DeviceBinding { serial: snapshot.serial.clone(), fingerprint: snapshot.fingerprint.clone(), user_id: 0 }, baseline_id: "11111111-1111-1111-1111-111111111111".into(), baseline_launcher: launcher.as_str().into(), initial_home: snapshot.home.as_str().into(), initial_packages: snapshot.catalog.iter().map(|app| InitialPackageSuspension { package: app.package.as_str().into(), suspended: app.suspended, user_id: 0 }).collect(), allowed_packages: allowed.iter().map(|package| package.as_str().into()).collect() }).unwrap(); let canonical = baseline.canonical_json();
    assert!(canonical.contains("\"revision\":0")); assert!(canonical.contains("\"previous_revision_hash\":null")); assert!(canonical.contains("\"journal\":[]")); assert!(canonical.contains("\"baseline_id\":\"11111111-1111-1111-1111-111111111111\"")); assert_eq!(baseline.checksum(), baseline.computed_checksum());
    let source = std::env::temp_dir().join(format!("unscroll-baseline-{}.json", std::process::id())); let mut adb = FakeAdb::scripted([AdbResponse::success(""), AdbResponse::success(private_reply(&canonical)), AdbResponse::success(""), AdbResponse::success(""), AdbResponse::success(canonical)]);
    assert_eq!(initialize_adb(&mut adb, &snapshot, "11111111-1111-1111-1111-111111111111", launcher, allowed, &source), Ok(()));
    assert!(matches!(adb.seen()[0], AdbCommand::Device { operation: DeviceOperation::Bridge(BridgeOperation::WriteEnvelope { .. }), .. })); assert!(matches!(adb.seen()[1], AdbCommand::Device { operation: DeviceOperation::Bridge(BridgeOperation::ReadEnvelope { .. }), .. })); assert!(matches!(adb.seen()[2], AdbCommand::PushSharedRecovery { .. })); assert!(matches!(adb.seen()[3], AdbCommand::Device { operation: DeviceOperation::CommitSharedRecovery, .. })); assert!(matches!(adb.seen()[4], AdbCommand::Device { operation: DeviceOperation::ReadDestination(_), .. })); let _ = std::fs::remove_file(source);
}
#[test]
fn typed_mirror_stops_at_each_write_boundary_and_reads_back_both_copies() {
    let value = fixture("valid/new-baseline.json");
    let envelope = RecoveryEnvelopeV1::parse(&value).unwrap();
    let source = std::env::temp_dir().join(format!("unscroll-recovery-{}.json", std::process::id()));
    std::fs::write(&source, envelope.canonical_json()).unwrap();
    let serial = Serial::parse("ABC123").unwrap();
    let fingerprint = Fingerprint::parse("google/pixel/test").unwrap();
    let cases = [
        (vec![AdbResponse::failure(1, "", "write")], MirrorError::PrivateWrite, 1),
        (vec![AdbResponse::success(""), AdbResponse::success("corrupt")], MirrorError::PrivateReadback, 2),
        (vec![AdbResponse::success(""), AdbResponse::success(private_reply(&envelope.canonical_json())), AdbResponse::failure(1, "", "push")], MirrorError::SharedWrite, 3),
        (vec![AdbResponse::success(""), AdbResponse::success(private_reply(&envelope.canonical_json())), AdbResponse::success(""), AdbResponse::failure(1, "", "commit")], MirrorError::SharedWrite, 4),
        (vec![AdbResponse::success(""), AdbResponse::success(private_reply(&envelope.canonical_json())), AdbResponse::success(""), AdbResponse::success(""), AdbResponse::success("corrupt")], MirrorError::SharedReadback, 5),
    ];
    for (responses, expected, calls) in cases {
        let mut adb = FakeAdb::scripted(responses);
        assert_eq!(persist_adb(&mut adb, &serial, &fingerprint, &envelope, &source), Err(expected));
        assert_eq!(adb.seen().len(), calls);
    }
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""), AdbResponse::success(private_reply(&envelope.canonical_json())),
        AdbResponse::success(""), AdbResponse::success(""), AdbResponse::success(envelope.canonical_json()),
    ]);
    assert_eq!(persist_adb(&mut adb, &serial, &fingerprint, &envelope, &source), Ok(()));
    assert_eq!(adb.seen().len(), 5);
}

#[test]
fn typed_mirror_rejects_a_valid_but_divergent_source_before_private_write() {
    let baseline = RecoveryEnvelopeV1::parse(&fixture("valid/new-baseline.json")).unwrap();
    let source = std::env::temp_dir().join(format!("unscroll-divergent-{}.json", std::process::id()));
    std::fs::write(&source, fixture("valid/pending-mutation.json")).unwrap();
    let serial = Serial::parse("ABC123").unwrap();
    let fingerprint = Fingerprint::parse("google/pixel/test").unwrap();
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        persist_adb(&mut adb, &serial, &fingerprint, &baseline, &source),
        Err(MirrorError::SourceMismatch)
    );
    assert!(adb.seen().is_empty());
}

#[test]
fn only_an_applied_private_cleanup_record_proves_missing_private_cleanup() {
    let baseline = RecoveryEnvelopeV1::parse(&fixture("valid/new-baseline.json")).unwrap();
    let pending = baseline.append_pending(
        "33333333-3333-3333-3333-333333333333",
        r#"{"kind":"cleanup","removed":true,"target":"private_envelope"}"#,
        r#"{"kind":"cleanup","removed":false,"target":"private_envelope"}"#,
    ).unwrap();
    assert!(!pending.has_applied_private_cleanup());
    let applied = pending.mark_applied("33333333-3333-3333-3333-333333333333").unwrap();
    assert!(applied.has_applied_private_cleanup());
}

fn reconcile(private: Option<&str>, shared: Option<&str>) -> Outcome {
    let baseline = RecoveryEnvelopeV1::parse(&fixture("valid/new-baseline.json")).unwrap();
    reconcile_checked(
        private, shared, "ABC123", "google/pixel/test", &baseline.baseline_hash(), true,
    )
}

#[test]
fn corrupt_private_only_blocks_reconciliation() {
    let baseline = fixture("valid/new-baseline.json"); let hash = RecoveryEnvelopeV1::parse(&baseline).unwrap().baseline_hash();
    assert_eq!(reconcile_checked(Some(&fixture("invalid/checksum-failure.json")), None, "ABC123", "google/pixel/test", &hash, true), Outcome::Corruption);
}
#[test]
fn wrong_binding_private_only_blocks_reconciliation() {
    let baseline = fixture("valid/new-baseline.json"); let hash = RecoveryEnvelopeV1::parse(&baseline).unwrap().baseline_hash();
    assert_eq!(reconcile_checked(Some(&baseline), None, "OTHER", "google/pixel/test", &hash, true), Outcome::DeviceBindingMismatch);
}
#[test]
fn mutated_baseline_private_only_blocks_reconciliation() {
    let baseline = fixture("valid/new-baseline.json"); let hash = RecoveryEnvelopeV1::parse(&baseline).unwrap().baseline_hash();
    assert_eq!(reconcile_checked(Some(&fixture("invalid/baseline-mismatch.json")), None, "ABC123", "google/pixel/test", &hash, true), Outcome::BaselineMismatch);
}
#[test]
fn reconciliation_outcomes_are_history_and_evidence_only() {
    let baseline = fixture("valid/new-baseline.json");
    let pending = fixture("valid/pending-mutation.json");
    let corrupt = fixture("invalid/checksum-failure.json");
    let baseline_mismatch = fixture("invalid/baseline-mismatch.json");
    let unknown = fixture("invalid/unknown-operation.json");
    let gap = fixture("valid/strict-extension.json");
    let cases = [
        (Some(baseline.as_str()), Some(pending.as_str()), Outcome::RepairPrivate(pending.trim().to_owned()), "stale private"),
        (Some(pending.as_str()), Some(baseline.as_str()), Outcome::RepairShared(pending.trim().to_owned()), "stale shared"),
        (None, Some(pending.as_str()), Outcome::RepairPrivate(pending.trim().to_owned()), "missing private"),
        (Some(pending.as_str()), None, Outcome::MissingShared, "missing shared"),
        (None, None, Outcome::BothMissing, "both missing"),
        (Some(baseline.as_str()), Some(corrupt.as_str()), Outcome::Corruption, "corrupt newest"),
        (Some(baseline.as_str()), Some(baseline_mismatch.as_str()), Outcome::BaselineMismatch, "baseline mutation"),
        (Some(baseline.as_str()), Some(unknown.as_str()), Outcome::Corruption, "unknown operation"),
        (Some(baseline.as_str()), Some(gap.as_str()), Outcome::Fork, "revision gap"),
    ];
    for (private, shared, expected, name) in cases {
        assert_eq!(reconcile(private, shared), expected, "{name}");
    }
    let baseline_hash = RecoveryEnvelopeV1::parse(&baseline).unwrap().baseline_hash();
    assert_eq!(reconcile_checked(Some(&corrupt), None, "ABC123", "google/pixel/test", &baseline_hash, true), Outcome::Corruption);
    assert_eq!(reconcile_checked(Some(&baseline_mismatch), None, "ABC123", "google/pixel/test", &baseline_hash, true), Outcome::BaselineMismatch);
    assert_eq!(reconcile_checked(Some(&baseline), None, "OTHER", "google/pixel/test", &baseline_hash, true), Outcome::DeviceBindingMismatch);
    assert_eq!(
        reconcile_checked(Some(&baseline), Some(&baseline), "OTHER", "google/pixel/test", &baseline_hash, true),
        Outcome::DeviceBindingMismatch,
    );
    assert_eq!(
        reconcile_checked(Some(&baseline), Some(&baseline), "ABC123", "google/pixel/test", &baseline_hash, false),
        Outcome::UnexplainedDeviceState,
    );
}
