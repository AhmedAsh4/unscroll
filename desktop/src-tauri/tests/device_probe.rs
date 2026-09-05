use std::fs;

#[path = "../src/bin/unscroll_probe.rs"]
mod probe;

use unscroll_desktop_lib::adb::{AdbCommand, AdbResponse, DeviceOperation, FakeAdb, Serial};

fn successful_probe() -> Vec<AdbResponse> {
    [
        "24",
        "Pixel API 24",
        "google/sdk_gphone",
        "package:org.unscroll.fixture",
        "health",
        "facts",
        "catalog",
        "stream_id=0123456789abcdef0123456789abcdef",
        "png",
        "envelope",
        "POST_NOTIFICATION: allow",
        "com.android.launcher/.Home",
        "chooser",
        "selected",
        "org.unscroll.launcher/.MainActivity",
        "suspended",
        "suspended=true",
        "ignored",
        "POST_NOTIFICATION: ignore",
        "package:com.android.vending",
        "restored",
        "POST_NOTIFICATION: allow",
        "unsuspended",
        "suspended=false",
        "home restored",
        "com.android.launcher/.Home",
    ]
    .into_iter()
    .map(AdbResponse::success)
    .collect()
}

#[test]
fn disposable_fixture_probe_writes_local_machine_and_human_results_after_verified_restore() {
    let mut adb = FakeAdb::scripted(successful_probe());
    let report = probe::run(
        &mut adb,
        Serial::parse("emulator-5554").unwrap(),
        probe::ProbeConfig::disposable_snapshot(),
    );

    assert!(report.passed(), "{}", report.summary());
    assert!(report.machine_json().contains("\"telemetry\":false"));
    assert!(report.machine_json().contains("\"api\":\"24\""));
    assert!(report.machine_json().contains("google/sdk_gphone"));
    assert!(report.summary().contains("API"));
    let directory = std::env::temp_dir().join("unscroll-probe-test");
    report.write_local(&directory).unwrap();
    assert!(fs::read_to_string(directory.join("probe-results.json"))
        .unwrap()
        .contains("\"passed\":true"));
    let _ = fs::remove_dir_all(directory);
    assert!(adb.seen().iter().any(|command| matches!(
        command,
        AdbCommand::Device {
            operation: DeviceOperation::Suspend {
                suspended: true,
                ..
            },
            ..
        }
    )));
    assert!(adb.seen().iter().any(|command| matches!(
        command,
        AdbCommand::Device {
            operation: DeviceOperation::Suspend {
                suspended: false,
                ..
            },
            ..
        }
    )));
}

#[test]
fn failed_restore_is_a_failure_and_retains_local_diagnostics() {
    let mut responses = successful_probe();
    responses.truncate(22);
    responses.push(AdbResponse::failure(1, "", "restore failed"));
    responses.push(AdbResponse::success("fixture remains suspended"));
    let mut adb = FakeAdb::scripted(responses);
    let report = probe::run(
        &mut adb,
        Serial::parse("emulator-5554").unwrap(),
        probe::ProbeConfig::disposable_snapshot(),
    );

    assert!(!report.passed());
    assert!(report.machine_json().contains("restore"));
    assert!(report.machine_json().contains("command_class"));
    let directory = std::env::temp_dir().join("unscroll-probe-failed-restore-test");
    report.write_local(&directory).unwrap();
    assert!(fs::read_to_string(directory.join("probe-results.json"))
        .unwrap()
        .contains("command_class"));
    let _ = fs::remove_dir_all(directory);
    assert!(report.summary().contains("diagnostic retained"));
}

#[test]
fn parses_multifield_dumpsys_package_user_state() {
    assert!(probe::suspended("Package [org.unscroll.fixture]\n  User 0: installed=true hidden=false suspended=true stopped=false", true));
    assert!(probe::suspended(
        "User 0: installed=true suspended=false enabled=0",
        false
    ));
}

#[test]
fn refuses_to_mutate_without_a_disposable_fixture_or_snapshot() {
    let mut adb = FakeAdb::scripted([]);
    let report = probe::run(
        &mut adb,
        Serial::parse("emulator-5554").unwrap(),
        probe::ProbeConfig::default(),
    );

    assert!(!report.passed());
    assert!(adb.seen().is_empty());
    assert!(report.summary().contains("disposable"));
}
