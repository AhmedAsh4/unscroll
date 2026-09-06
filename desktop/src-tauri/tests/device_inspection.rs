use std::fs;
use unscroll_desktop_lib::{
    adb::{AdbCommand, AdbResponse, FakeAdb},
    device::{inspect, InspectionError, LauncherArtifact, RecoveryObservation},
    recovery::shared_copy::{
        bounded_read, destination, MAX_SHARED_COPY_BYTES, SHARED_RECOVERY_PATH,
    },
};

fn artifact() -> (std::path::PathBuf, LauncherArtifact) {
    let path = std::env::temp_dir().join(format!(
        "unscroll-task-9-{}-{}.apk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, b"fixture").unwrap();
    let artifact = LauncherArtifact::from_path(&path, "a".repeat(64)).unwrap();
    (path, artifact)
}

#[test]
fn absent_artifact_fails_before_any_adb_command() {
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        inspect(&mut adb, None),
        Err(InspectionError::LauncherArtifactUnavailable)
    );
    assert!(adb.seen().is_empty());
    assert!(LauncherArtifact::from_path("missing.apk", "a".repeat(64)).is_err());
}

#[test]
fn deleted_artifact_is_revalidated_before_any_adb_command() {
    let (path, artifact) = artifact();
    fs::remove_file(&path).unwrap();
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::LauncherArtifactUnavailable)
    );
    assert!(adb.seen().is_empty());
}

#[test]
fn shared_recovery_reads_are_fixed_path_and_bounded() {
    assert_eq!(destination().as_str(), SHARED_RECOVERY_PATH);
    assert!(bounded_read(&vec![0; MAX_SHARED_COPY_BYTES]).is_ok());
    assert!(bounded_read(&vec![0; MAX_SHARED_COPY_BYTES + 1]).is_err());
}

#[test]
fn inspection_rejects_zero_multiple_unauthorized_offline_and_unverified_apis_without_bootstrap() {
    for devices in [
        "List of devices attached\n",
        "List of devices attached\nA\tdevice\nB\tdevice\n",
        "List of devices attached\nA\tunauthorized\n",
        "List of devices attached\nA\toffline\n",
    ] {
        let (path, artifact) = artifact();
        let mut adb = FakeAdb::scripted([AdbResponse::success(""), AdbResponse::success(devices)]);
        assert_eq!(
            inspect(&mut adb, Some(artifact)),
            Err(InspectionError::Discovery)
        );
        assert!(!adb
            .seen()
            .iter()
            .any(|command| matches!(command, AdbCommand::Install { .. })));
        fs::remove_file(path).unwrap();
    }
    for api in ["23", "37"] {
        let (path, artifact) = artifact();
        let mut adb = FakeAdb::scripted([
            AdbResponse::success(""),
            AdbResponse::success("List of devices attached\nA\tdevice\n"),
            AdbResponse::success(api),
        ]);
        assert_eq!(
            inspect(&mut adb, Some(artifact)),
            Err(InspectionError::UnsupportedApi)
        );
        assert!(!adb
            .seen()
            .iter()
            .any(|command| matches!(command, AdbCommand::Install { .. })));
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn missing_shared_storage_and_miui_install_failures_do_not_change_existing_state() {
    let (path, first_launcher) = artifact();
    let mut missing = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nA\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::failure(1, "", "No such file or directory"),
    ]);
    assert_eq!(
        inspect(&mut missing, Some(first_launcher)),
        Err(InspectionError::Preflight)
    );
    assert!(!missing
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Install { .. })));
    fs::remove_file(path).unwrap();

    let (path2, launcher) = artifact();
    let mut miui = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nA\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Redmi"),
        AdbResponse::success("Xiaomi"),
        AdbResponse::success("xiaomi/redmi/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success(""),
        AdbResponse::failure(1, "", "INSTALL_FAILED_USER_RESTRICTED"),
    ]);
    assert_eq!(
        inspect(&mut miui, Some(launcher)),
        Err(InspectionError::Xiaomi(vec![
            unscroll_desktop_lib::device::XiaomiGuidance::InstallViaUsb,
            unscroll_desktop_lib::device::XiaomiGuidance::MiAccount,
            unscroll_desktop_lib::device::XiaomiGuidance::Sim,
            unscroll_desktop_lib::device::XiaomiGuidance::Network,
        ]))
    );
    assert!(!miui
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Uninstall { .. })));
    fs::remove_file(path2).unwrap();
}

#[test]
fn cleanup_only_uninstalls_the_launcher_installed_by_this_run() {
    let (path, artifact) = artifact();
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nR58M1234ABC\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success(""),
        AdbResponse::success("Success"),
        AdbResponse::success("signing_sha256=".to_owned() + &"a".repeat(64)),
        AdbResponse::success(
            r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher"}}}]"#,
        ),
    ]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::BridgeMismatch)
    );
    assert!(adb
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Uninstall { .. })));
    fs::remove_file(path).unwrap();
}

#[test]
fn existing_launcher_is_never_uninstalled_after_a_preflight_failure() {
    let (path, artifact) = artifact();
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nR58M1234ABC\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success("package:org.unscroll.launcher"),
        AdbResponse::success("signing_sha256=".to_owned() + &"a".repeat(64)),
        AdbResponse::success("not bridge json"),
    ]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::BridgeMismatch)
    );
    assert!(!adb
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Uninstall { .. })));
    fs::remove_file(path).unwrap();
}

#[test]
fn launcher_signature_mismatch_cleans_up_only_a_bootstrap_install() {
    let (path, artifact) = artifact();
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nA\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success(""),
        AdbResponse::success("Success"),
        AdbResponse::success("signing_sha256=".to_owned() + &"b".repeat(64)),
    ]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::SignatureMismatch)
    );
    assert!(adb
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Install { .. })));
    assert!(adb
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Uninstall { .. })));
    fs::remove_file(path).unwrap();
}

#[test]
fn launcher_in_any_profile_is_never_bootstrapped_or_uninstalled() {
    let (path, artifact) = artifact();
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nA\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success("package:org.unscroll.launcher"),
        AdbResponse::success("signing_sha256=not-the-pinned-signer"),
    ]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::SignatureMismatch)
    );
    assert!(!adb.seen().iter().any(|command| matches!(
        command,
        AdbCommand::Install { .. } | AdbCommand::Uninstall { .. }
    )));
    fs::remove_file(path).unwrap();
}

#[test]
fn signer_digest_requires_an_exact_field_match() {
    let (path, artifact) = artifact();
    let digest = "a".repeat(64);
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nA\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success(""),
        AdbResponse::success("Success"),
        AdbResponse::success(format!("other={digest}x")),
    ]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::SignatureMismatch)
    );
    assert!(adb
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Uninstall { .. })));
    fs::remove_file(path).unwrap();
}

#[test]
fn inspection_collects_a_typed_snapshot_without_mutating_existing_state() {
    let (path, artifact) = artifact();
    let health = r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher"}}}]"#;
    let facts = r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"device_binding":{"serial":"R58M1234ABC","fingerprint":"google/pixel/release","user_id":0},"capabilities":{"app_ops":true,"home_selection":true,"package_suspension":true,"recovery_storage":true}}}}]"#;
    let catalog = r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"entries":[{"package_id":"com.example.camera","activity_name":"com.example.camera.MainActivity","label":"Camera","user_id":0,"launchable":true,"enabled":true,"suspended":false,"activity_icon_available":true,"application_icon_available":true,"icon_available":true,"protected_reason":null},{"package_id":"com.android.settings","activity_name":"com.android.settings.Settings","label":"Settings","user_id":0,"launchable":true,"enabled":true,"suspended":false,"activity_icon_available":false,"application_icon_available":false,"icon_available":false,"protected_reason":"Settings"}],"next_cursor":null}}}]"#;
    let icon = r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"byte_length":8,"mime_type":"image/png","sha256":"4c4b6a3be1314ab86138bef4314dde022e600960d8689a2c8f8631802d20dab6","stream_id":"0123456789abcdef0123456789abcdef"}}}]"#;
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::success("List of devices attached\nR58M1234ABC\tdevice\n"),
        AdbResponse::success("34"),
        AdbResponse::success("Pixel"),
        AdbResponse::success("Google"),
        AdbResponse::success("google/pixel/release"),
        AdbResponse::success("0"),
        AdbResponse::success("com.android.launcher/.Home"),
        AdbResponse::success("package help suspend unsuspend set-home-activity"),
        AdbResponse::success("/sdcard/Documents/Unscroll"),
        AdbResponse::success(""),
        AdbResponse::success("Success"),
        AdbResponse::success("signing_sha256=".to_owned() + &"a".repeat(64)),
        AdbResponse::success(health),
        AdbResponse::success(facts),
        AdbResponse::success(catalog),
        AdbResponse::success(icon),
        AdbResponse::binary_success(vec![137, 80, 78, 71, 13, 10, 26, 10]),
        AdbResponse::success("com.example.camera/.Install"),
        AdbResponse::success("REQUEST_INSTALL_PACKAGES: default"),
        AdbResponse::success(
            "Users:\n\tUserInfo{0:Owner:13} running\n\tUserInfo{10:Work:30} running",
        ),
        AdbResponse::failure(1, "", "cat: No such file or directory"),
        AdbResponse::failure(1, "", "not found"),
    ]);
    let snapshot = inspect(&mut adb, Some(artifact)).unwrap();
    assert_eq!(snapshot.api, 34);
    assert_eq!(snapshot.current_user, 0);
    assert_eq!(snapshot.catalog.len(), 2);
    assert_eq!(
        snapshot.catalog[0].icon,
        vec![137, 80, 78, 71, 13, 10, 26, 10]
    );
    assert_eq!(snapshot.protected.len(), 1);
    assert_eq!(snapshot.profiles.len(), 1);
    assert_eq!(snapshot.private_recovery, RecoveryObservation::Missing);
    assert_eq!(snapshot.shared_recovery, RecoveryObservation::Missing);
    assert!(snapshot.capabilities.ready());
    assert!(!adb.seen().iter().any(|command| matches!(
        command,
        AdbCommand::Uninstall { .. }
            | AdbCommand::Device {
                operation: unscroll_desktop_lib::adb::DeviceOperation::Suspend { .. }
                    | unscroll_desktop_lib::adb::DeviceOperation::HomeSelect(_)
                    | unscroll_desktop_lib::adb::DeviceOperation::AppOpSet { .. },
                ..
            }
    )));
    fs::remove_file(path).unwrap();
}
