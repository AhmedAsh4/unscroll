use std::fs;

#[path = "../src/bin/unscroll_probe.rs"]
mod probe;

use unscroll_desktop_lib::adb::{AdbCommand, AdbResponse, DeviceOperation, FakeAdb, Serial};

fn success(count: usize) -> Vec<AdbResponse> {
    (0..count).map(|_| AdbResponse::success("ok")).collect()
}

#[test]
fn disposable_fixture_probe_writes_local_machine_and_human_results_after_verified_restore() {
    let mut adb = FakeAdb::scripted(success(26));
    let report = probe::run(
        &mut adb,
        Serial::parse("emulator-5554").unwrap(),
        probe::ProbeConfig::disposable_snapshot(),
    );

    assert!(report.passed(), "{}", report.summary());
    assert!(report.machine_json().contains("\"telemetry\":false"));
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
    let mut responses = success(22);
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
    assert!(report.summary().contains("diagnostic retained"));
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
