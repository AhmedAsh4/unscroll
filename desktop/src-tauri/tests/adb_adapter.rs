use std::{path::PathBuf, time::Duration};
use unscroll_desktop_lib::adb::{
    parse_devices, redacted_diagnostic, require_success, stop_server, AdbCommand, AdbError,
    AdbOutput, AdbResponse, AppOp, AppOpMode, BridgeOperation, BundledAdb, Component, Cursor,
    Destination, DeviceOperation, FakeAdb, PackageId, Property, RecoveryEnvelope, Serial, StreamId,
    UserId, ValidationError,
};
use unscroll_desktop_lib::device::{
    classify_api, classify_bootstrap, discover, ApiSupport, BootstrapError, Discovery,
    DiscoveryError,
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/adb")
            .join(name),
    )
    .expect("fixture")
}

fn content_string_value(binding: &str) -> String {
    let mut fields = vec![String::new()];
    let mut escaping = false;
    for ch in binding.chars() {
        if escaping { fields.last_mut().unwrap().push(ch); escaping = false; }
        else if ch == '\\' { escaping = true; }
        else if ch == ':' { fields.push(String::new()); }
        else { fields.last_mut().unwrap().push(ch); }
    }
    assert!(!escaping);
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0], "request");
    assert_eq!(fields[1], "s");
    fields.pop().unwrap()
}

#[test]
fn discovers_one_authorized_windows_usb_device_without_optional_usb_metadata() {
    let mut adb = FakeAdb::scripted([
        AdbResponse::success("* daemon started successfully\r\n"),
        AdbResponse::success(fixture("one-device.txt")),
    ]);
    assert_eq!(
        discover(&mut adb),
        Ok(Discovery::One {
            serial: Serial::parse("R58M1234ABC").unwrap()
        })
    );
    assert_eq!(adb.seen(), &[AdbCommand::StartServer, AdbCommand::Devices]);
}

#[test]
fn discovery_classifies_non_ready_network_and_unexpected_device_states() {
    for (fixture_name, expected) in [
        ("no-devices.txt", DiscoveryError::NoDevice),
        ("multiple-devices.txt", DiscoveryError::MultipleDevices),
        ("unauthorized.txt", DiscoveryError::Unauthorized),
        ("offline.txt", DiscoveryError::Offline),
        ("reconnecting.txt", DiscoveryError::Reconnecting),
        ("unexpected.txt", DiscoveryError::UnexpectedOutput),
        ("network-device.txt", DiscoveryError::NetworkTransport),
        ("mdns-device.txt", DiscoveryError::NetworkTransport),
    ] {
        let mut adb = FakeAdb::scripted([
            AdbResponse::success(""),
            AdbResponse::success(fixture(fixture_name)),
        ]);
        assert_eq!(discover(&mut adb), Err(expected), "{fixture_name}");
    }
}

#[test]
fn fake_adapter_preserves_failure_timeout_disconnect_reconnect_and_inconsistent_facts() {
    let serial = Serial::parse("R58M1234ABC").unwrap();
    let mut adb = FakeAdb::scripted([
        AdbResponse::ordinary_failure(),
        AdbResponse::required_failure(),
        AdbResponse::failure(1, "partial", "error: device disconnected"),
        AdbResponse::timeout(),
        AdbResponse::success("List of devices attached\nR58M1234ABC\tdevice\n"),
    ]);
    assert_eq!(
        adb.execute(AdbCommand::Devices),
        Ok(AdbOutput::failure(1, "ordinary failure"))
    );
    assert_eq!(
        adb.execute(AdbCommand::Devices),
        Ok(AdbOutput::failure(2, "required failure"))
    );
    assert_eq!(
        adb.execute(AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::GetProperty(Property::SdkInt)
        }),
        Ok(AdbOutput::failure_with_stdout(
            1,
            "partial",
            "error: device disconnected"
        ))
    );
    assert_eq!(adb.execute(AdbCommand::Devices), Ok(AdbOutput::timeout()));
    assert!(adb.execute(AdbCommand::Devices).unwrap().succeeded());
    assert_eq!(
        adb.execute(AdbCommand::Devices),
        Err(AdbError::InconsistentState)
    );
}

#[test]
fn closed_commands_construct_only_validated_argument_arrays() {
    for invalid in [
        "",
        "R58M1234ABC -s other",
        "R58M; rm -rf /",
        "line\nbreak",
        "192.168.1.2:5555",
    ] {
        assert!(Serial::parse(invalid).is_err(), "serial {invalid:?}");
    }
    for invalid in [
        "com.good;rm",
        "com.good extra",
        "com.good\n--user",
        "com.$bad",
        "onepart",
    ] {
        assert!(PackageId::parse(invalid).is_err(), "package {invalid:?}");
    }
    for invalid in ["com.example/.Main;rm", "com.example/line\nbreak"] {
        assert!(Component::parse(invalid).is_err());
    }
    assert!(AppOp::parse("REQUEST_INSTALL_PACKAGES;rm").is_err());
    assert!(UserId::parse(10).is_err());
    assert!(Destination::parse("/sdcard/Documents/Unscroll/../../evil").is_err());

    let serial = Serial::parse("R58M1234ABC").unwrap();
    let user = UserId::parse(0).unwrap();
    let package = PackageId::parse("com.example.camera").unwrap();
    let component = Component::parse("com.example.camera/.MainActivity").unwrap();
    let app_op = AppOp::parse("REQUEST_INSTALL_PACKAGES").unwrap();
    let destination = Destination::parse("/sdcard/Documents/Unscroll/recovery-v1.json").unwrap();
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::GetProperty(Property::SdkInt)
        }
        .arguments(),
        vec![
            "-s",
            "R58M1234ABC",
            "shell",
            "getprop",
            "ro.build.version.sdk"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::PackageState {
                package: PackageId::parse("org.unscroll.fixture").unwrap(),
                user,
            },
        }
        .arguments(),
        vec![
            "-s",
            "R58M1234ABC",
            "shell",
            "dumpsys",
            "package",
            "--user",
            "0",
            "org.unscroll.fixture"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::PackageInfo {
                package: package.clone(),
                user
            }
        }
        .arguments(),
        vec![
            "-s",
            "R58M1234ABC",
            "shell",
            "cmd",
            "package",
            "list",
            "packages",
            "--user",
            "0",
            "com.example.camera"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::AppOpGet {
                package,
                user,
                app_op
            }
        }
        .arguments(),
        vec![
            "-s",
            "R58M1234ABC",
            "shell",
            "appops",
            "get",
            "--user",
            "0",
            "com.example.camera",
            "REQUEST_INSTALL_PACKAGES"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::ComponentInfo(component)
        }
        .arguments(),
        vec![
            "-s",
            "R58M1234ABC",
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "--components",
            "--user",
            "0",
            "-n",
            "com.example.camera/.MainActivity"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial,
            operation: DeviceOperation::ReadDestination(destination)
        }
        .arguments(),
        vec![
            "-s",
            "R58M1234ABC",
            "shell",
            "cat",
            "/sdcard/Documents/Unscroll/recovery-v1.json"
        ]
    );
    assert_eq!(AdbCommand::Devices.timeout(), Duration::from_secs(10));
}

#[test]
fn probe_commands_are_closed_validated_argument_arrays() {
    let serial = Serial::parse("emulator-5554").unwrap();
    let user = UserId::parse(0).unwrap();
    let fixture = PackageId::parse("org.unscroll.fixture").unwrap();
    let icon = StreamId::parse("0123456789abcdef0123456789abcdef").unwrap();
    assert!(StreamId::parse("not-a-stream-id").is_err());
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::Suspend {
                package: fixture.clone(),
                user,
                suspended: true
            }
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "shell",
            "cmd",
            "package",
            "suspend",
            "--user",
            "0",
            "org.unscroll.fixture"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::AppOpSet {
                package: fixture.clone(),
                user,
                app_op: AppOp::parse("POST_NOTIFICATION").unwrap(),
                mode: AppOpMode::Ignore
            }
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "shell",
            "appops",
            "set",
            "--user",
            "0",
            "org.unscroll.fixture",
            "POST_NOTIFICATION",
            "ignore"
        ]
    );
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::Bridge(BridgeOperation::Health)
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "shell",
            "content",
            "call",
            "--uri",
            "content://org.unscroll.launcher.bridge",
            "--method",
            "bridge-v1",
            "--extra",
            r#"request:s:{"protocol_version"\:"bridge-v1","operation"\:"health","arguments"\:{}}"#
        ]
    );
    let health = AdbCommand::Device { serial: serial.clone(), operation: DeviceOperation::Bridge(BridgeOperation::Health) }.arguments();
    assert_eq!(content_string_value(health.last().unwrap()), "{\"protocol_version\":\"bridge-v1\",\"operation\":\"health\",\"arguments\":{}}");
    let envelope = RecoveryEnvelope::parse(include_str!("../../../contracts/fixtures/recovery-v1/valid/new-baseline.json").trim()).unwrap();
    let write = AdbCommand::Device { serial: serial.clone(), operation: DeviceOperation::Bridge(BridgeOperation::WriteEnvelope { device_serial: serial.clone(), fingerprint: unscroll_desktop_lib::adb::Fingerprint::parse("google/pixel/test").unwrap(), envelope }) }.arguments();
    let restored = content_string_value(write.last().unwrap());
    assert!(write.last().unwrap().contains("\\\\"));
    assert!(restored.contains("\"envelope\":\"{\\\"active_policy\\\""));
    assert_eq!(
        AdbCommand::Device { serial, operation: DeviceOperation::Bridge(BridgeOperation::ReadIcon(icon)) }.arguments(),
        vec!["-s", "emulator-5554", "shell", "content", "read", "--uri", "content://org.unscroll.launcher.bridge/bridge-v1/icon/0123456789abcdef0123456789abcdef"]
    );
}

#[test]
fn bootstrap_commands_keep_install_and_cleanup_typed() {
    let serial = Serial::parse("emulator-5554").unwrap();
    let package = PackageId::parse("org.unscroll.launcher").unwrap();
    assert_eq!(
        AdbCommand::Install {
            serial: serial.clone(),
            user: UserId::parse(0).unwrap(),
            apk: PathBuf::from("resources/unscroll-launcher.apk")
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "install",
            "-r",
            "--user",
            "0",
            "resources/unscroll-launcher.apk"
        ]
    );
    assert_eq!(
        AdbCommand::Uninstall {
            serial,
            user: UserId::parse(0).unwrap(),
            package
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "uninstall",
            "--user",
            "0",
            "org.unscroll.launcher"
        ]
    );
    assert_eq!(
        Cursor::parse("not-a-cursor"),
        Err(ValidationError::InvalidCursor)
    );
    assert_eq!(
        AdbCommand::Device {
            serial: Serial::parse("emulator-5554").unwrap(),
            operation: DeviceOperation::PackageAnyUser {
                package: PackageId::parse("org.unscroll.launcher").unwrap(),
            },
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "shell",
            "cmd",
            "package",
            "list",
            "packages",
            "--user",
            "all",
            "org.unscroll.launcher"
        ]
    );
}

#[test]
fn recovery_persistence_commands_are_fixed_and_typed() {
    let serial = Serial::parse("emulator-5554").unwrap();
    let fingerprint = unscroll_desktop_lib::adb::Fingerprint::parse("google/pixel/test").unwrap();
    let envelope = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/recovery-v1/valid/new-baseline.json"),
    )
    .unwrap();
    assert_eq!(
        AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::Bridge(BridgeOperation::WriteEnvelope {
                device_serial: serial.clone(),
                fingerprint,
                envelope: RecoveryEnvelope::parse(envelope.trim()).unwrap(),
            }),
        }
        .class(),
        "bridge"
    );
    assert_eq!(
        AdbCommand::Device {
            serial,
            operation: DeviceOperation::RemoveSharedRecovery,
        }
        .arguments(),
        vec![
            "-s",
            "emulator-5554",
            "shell",
            "rm",
            "-f",
            "/sdcard/Documents/Unscroll/recovery-v1.json"
        ]
    );
}

#[test]
fn nonzero_server_and_overflow_outputs_fail_with_their_structured_facts() {
    let mut stop = FakeAdb::scripted([AdbResponse::failure(3, "", "server failure")]);
    assert_eq!(
        stop_server(&mut stop),
        Err(AdbError::NonZero(AdbOutput::failure(3, "server failure")))
    );
    let mut discovery = FakeAdb::scripted([AdbResponse::failure(1, "", "server failure")]);
    assert_eq!(discover(&mut discovery), Err(DiscoveryError::Transport));
    let mut overflow = FakeAdb::scripted([AdbResponse::success(""), AdbResponse::overflow()]);
    assert_eq!(
        discover(&mut overflow),
        Err(DiscoveryError::UnexpectedOutput)
    );
}

#[test]
fn parser_classifies_partial_output_driver_bootstrap_and_api_states() {
    assert_eq!(
        parse_devices("List of devices attached\nR58M1234ABC\n"),
        Err(AdbError::UnexpectedOutput)
    );
    let mut adb = FakeAdb::scripted([
        AdbResponse::success(""),
        AdbResponse::failure(1, "partial", "OEM driver missing"),
    ]);
    assert_eq!(discover(&mut adb), Err(DiscoveryError::MissingDriver));
    assert_eq!(
        classify_bootstrap("INSTALL_FAILED_USER_RESTRICTED"),
        BootstrapError::MiuiUsbInstallRestricted
    );
    assert_eq!(
        classify_bootstrap("SecurityException: denied"),
        BootstrapError::SecurityException
    );
    assert_eq!(classify_api(23), ApiSupport::Unsupported(23));
    assert_eq!(classify_api(37), ApiSupport::UnverifiedNewer(37));
}

#[test]
fn diagnostics_redact_short_and_multibyte_secrets_but_keep_the_command_class() {
    let command = AdbCommand::Device {
        serial: Serial::parse("R58M1234ABC").unwrap(),
        operation: DeviceOperation::GetProperty(Property::SdkInt),
    };
    let diagnostic = redacted_diagnostic(&command, &AdbOutput::failure_with_stdout(1, "秘密", "x"));
    assert_eq!(diagnostic.command_class, "device-property");
    assert_eq!(diagnostic.serial.as_deref(), Some("[redacted]"));
    assert_eq!(diagnostic.stderr, "[redacted]");
}

#[test]
fn public_output_boundary_caps_lossy_and_fake_output_and_development_resolver_is_fixed() {
    let expected = AdbOutput::success("\u{fffd}".repeat(65_537));
    let output = expected.clone();
    assert!(output.is_overflow());
    assert!(output.stdout().len() <= 65_536);
    assert_eq!(output.exit(), Some(0));
    assert!(!output.timed_out());
    assert_eq!(
        require_success(output),
        Err(AdbError::OutputOverflow(expected))
    );

    let mut fake = FakeAdb::scripted([AdbResponse::success("x".repeat(1_000_000))]);
    let output = fake.execute(AdbCommand::Devices).unwrap();
    assert!(output.is_overflow());
    assert_eq!(output.stdout().len(), 65_536);
    assert!(BundledAdb::for_development().is_ok());
}
