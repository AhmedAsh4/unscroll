use flate2::{write::DeflateEncoder, Compression};
use std::{fs, io::Write};
use unscroll_desktop_lib::{
    adb::{AdbCommand, AdbResponse, FakeAdb},
    device::{inspect, InspectionError, LauncherArtifact, RecoveryObservation},
    recovery::shared_copy::{
        bounded_read, destination, write_from, MAX_SHARED_COPY_BYTES, SHARED_RECOVERY_PATH,
    },
};

fn crc32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(!0, |crc, &byte| {
        (0..8).fold(crc ^ byte as u32, |crc, _| {
            (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)))
        })
    }) ^ !0
}

fn zip(entry: &str, contents: &[u8], method: u16, compressed: &[u8]) -> Vec<u8> {
    let name = entry.as_bytes();
    let crc = crc32(contents);
    let length = contents.len() as u32;
    let compressed_length = compressed.len() as u32;
    let mut zip = Vec::new();
    zip.extend_from_slice(b"PK\x03\x04");
    zip.extend_from_slice(&[20, 0]);
    zip.extend_from_slice(&[0; 2]);
    zip.extend_from_slice(&method.to_le_bytes());
    zip.extend_from_slice(&[0; 4]);
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&compressed_length.to_le_bytes());
    zip.extend_from_slice(&length.to_le_bytes());
    zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(name);
    zip.extend_from_slice(compressed);
    let central_directory = zip.len() as u32;
    zip.extend_from_slice(b"PK\x01\x02");
    zip.extend_from_slice(&[20, 0, 20, 0]);
    zip.extend_from_slice(&[0; 2]);
    zip.extend_from_slice(&method.to_le_bytes());
    zip.extend_from_slice(&[0; 4]);
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&compressed_length.to_le_bytes());
    zip.extend_from_slice(&length.to_le_bytes());
    zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
    zip.extend_from_slice(&[0; 16]);
    zip.extend_from_slice(name);
    let central_directory_size = zip.len() as u32 - central_directory;
    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&[0; 4]);
    zip.extend_from_slice(&[1, 0, 1, 0]);
    zip.extend_from_slice(&central_directory_size.to_le_bytes());
    zip.extend_from_slice(&central_directory.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip
}

fn stored_zip(entry: &str, contents: &[u8]) -> Vec<u8> {
    zip(entry, contents, 0, contents)
}

fn deflated_zip(entry: &str, contents: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(contents).unwrap();
    zip(entry, contents, 8, &encoder.finish().unwrap())
}

fn binary_manifest() -> Vec<u8> {
    vec![
        3, 0, 8, 0, 112, 0, 0, 0, 1, 0, 28, 0, 44, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 32,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 8, b'm', b'a', b'n', b'i', b'f', b'e', b's', b't', 0,
        0, 2, 1, 16, 0, 36, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0,
        0, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 1, 16, 0, 24, 0, 0, 0, 0, 0, 0, 0, 255, 255,
        255, 255, 255, 255, 255, 255, 0, 0, 0, 0,
    ]
}

fn binary_non_manifest() -> Vec<u8> {
    let mut xml = binary_manifest();
    xml[42..50].copy_from_slice(b"activity");
    xml
}

fn artifact() -> (std::path::PathBuf, LauncherArtifact) {
    let path = std::env::temp_dir().join(format!(
        "unscroll-task-9-{}-{}.apk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, stored_zip("AndroidManifest.xml", &binary_manifest())).unwrap();
    let artifact = LauncherArtifact::from_path(&path, "a".repeat(64)).unwrap();
    (path, artifact)
}

#[test]
fn non_apk_zip_fails_before_any_adb_command() {
    let path = std::env::temp_dir().join(format!(
        "unscroll-not-apk-{}-{}.apk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, stored_zip("fixture", b"")).unwrap();
    let artifact = LauncherArtifact::from_path(&path, "a".repeat(64)).ok();
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        inspect(&mut adb, artifact),
        Err(InspectionError::LauncherArtifactUnavailable)
    );
    assert!(adb.seen().is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn text_manifest_fails_before_any_adb_command() {
    let path =
        std::env::temp_dir().join(format!("unscroll-text-manifest-{}.apk", std::process::id()));
    fs::write(&path, stored_zip("AndroidManifest.xml", b"<manifest/>")).unwrap();
    let artifact = LauncherArtifact::from_path(&path, "a".repeat(64)).ok();
    assert!(artifact.is_none());
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        inspect(&mut adb, artifact),
        Err(InspectionError::LauncherArtifactUnavailable)
    );
    assert!(adb.seen().is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn non_manifest_binary_xml_fails_before_any_adb_command() {
    let path = std::env::temp_dir().join(format!(
        "unscroll-non-manifest-{}-{}.apk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        stored_zip("AndroidManifest.xml", &binary_non_manifest()),
    )
    .unwrap();
    let artifact = LauncherArtifact::from_path(&path, "a".repeat(64)).ok();
    assert!(artifact.is_none());
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        inspect(&mut adb, artifact),
        Err(InspectionError::LauncherArtifactUnavailable)
    );
    assert!(adb.seen().is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn deflated_binary_manifest_is_accepted() {
    let path = std::env::temp_dir().join(format!(
        "unscroll-deflated-manifest-{}-{}.apk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        deflated_zip("AndroidManifest.xml", &binary_manifest()),
    )
    .unwrap();
    assert!(LauncherArtifact::from_path(&path, "a".repeat(64)).is_ok());
    fs::remove_file(path).unwrap();
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
fn non_apk_bytes_fail_before_any_adb_command() {
    let path = std::env::temp_dir().join(format!(
        "unscroll-not-apk-{}-{}.apk",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, b"fixture").unwrap();
    assert!(LauncherArtifact::from_path(&path, "a".repeat(64)).is_err());
    let mut adb = FakeAdb::scripted([]);
    assert_eq!(
        inspect(&mut adb, None),
        Err(InspectionError::LauncherArtifactUnavailable)
    );
    assert!(adb.seen().is_empty());
    fs::remove_file(path).unwrap();
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
fn shared_recovery_write_uses_only_the_fixed_temp_then_commit() {
    let source =
        std::env::temp_dir().join(format!("unscroll-recovery-{}.json", std::process::id()));
    fs::write(&source, b"{}").unwrap();
    let serial = unscroll_desktop_lib::adb::Serial::parse("A").unwrap();
    let mut adb = FakeAdb::scripted([AdbResponse::success(""), AdbResponse::success("")]);
    write_from(&mut adb, &serial, &source).unwrap();
    assert!(matches!(
        adb.seen()[0],
        AdbCommand::PushSharedRecovery { .. }
    ));
    assert!(matches!(
        adb.seen()[1],
        AdbCommand::Device {
            operation: unscroll_desktop_lib::adb::DeviceOperation::CommitSharedRecovery,
            ..
        }
    ));
    fs::remove_file(source).unwrap();
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
        Err(InspectionError::Xiaomi {
            state: unscroll_desktop_lib::device::XiaomiBootstrapState::InstallRestricted,
            guidance: vec![
                unscroll_desktop_lib::device::XiaomiGuidance::InstallViaUsb,
                unscroll_desktop_lib::device::XiaomiGuidance::MiAccount,
                unscroll_desktop_lib::device::XiaomiGuidance::Sim,
                unscroll_desktop_lib::device::XiaomiGuidance::Network,
            ],
        })
    );
    assert!(!miui
        .seen()
        .iter()
        .any(|command| matches!(command, AdbCommand::Uninstall { .. })));
    fs::remove_file(path2).unwrap();
}

#[test]
fn non_xiaomi_security_failure_is_not_rewritten_as_xiaomi_guidance() {
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
        AdbResponse::failure(1, "", "SecurityException"),
    ]);
    assert_eq!(
        inspect(&mut adb, Some(artifact)),
        Err(InspectionError::Bootstrap)
    );
    fs::remove_file(path).unwrap();
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
        AdbResponse::success(
            r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher","launcher_signing_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}]"#,
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
        AdbResponse::success(
            r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher","launcher_signing_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}]"#,
        ),
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
        AdbResponse::success(
            r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher","launcher_signing_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}]"#,
        ),
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
fn malformed_bridge_signer_field_is_rejected_before_comparison() {
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
        AdbResponse::success(format!(
            r#"Result: Bundle[{{response={{"protocol_version":"bridge-v1","ok":true,"result":{{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher","launcher_signing_sha256":"{digest}x"}}}}}}]"#
        )),
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
fn inspection_collects_a_typed_snapshot_without_mutating_existing_state() {
    let (path, artifact) = artifact();
    let health = r#"Result: Bundle[{response={"protocol_version":"bridge-v1","ok":true,"result":{"protocol_version":"bridge-v1","recovery_schema":"recovery-v1","launcher_package":"org.unscroll.launcher","launcher_signing_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}]"#;
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
