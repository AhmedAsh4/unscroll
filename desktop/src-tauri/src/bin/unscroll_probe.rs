use std::{fs, path::Path};
use unscroll_desktop_lib::{
    adb::{
        require_success, Adb, AdbCommand, AppOp, AppOpMode, BridgeOperation, BundledAdb, Component,
        DeviceOperation, Fingerprint, PackageId, Property, Serial, StreamId, UserId,
    },
    device::{discover, Discovery},
};

const FIXTURE: &str = "org.unscroll.fixture";
const LAUNCHER: &str = "org.unscroll.launcher";

#[derive(Default, Clone, Copy)]
pub struct ProbeConfig {
    disposable_snapshot: bool,
}
impl ProbeConfig {
    pub fn disposable_snapshot() -> Self {
        Self {
            disposable_snapshot: true,
        }
    }
}

pub struct ProbeReport {
    passed: bool,
    outcomes: Vec<(&'static str, bool)>,
    diagnostic_retained: bool,
}
impl ProbeReport {
    pub fn passed(&self) -> bool {
        self.passed
    }
    pub fn machine_json(&self) -> String {
        let outcomes = self
            .outcomes
            .iter()
            .map(|(name, ok)| format!("{{\"capability\":\"{name}\",\"passed\":{ok}}}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"telemetry\":false,\"outcomes\":[{outcomes}],\"passed\":{},\"diagnostic_retained\":{}}}", self.passed, self.diagnostic_retained)
    }
    pub fn summary(&self) -> String {
        let failed = self
            .outcomes
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "API evidence is local only. {}{}",
            if self.passed {
                "compatibility gate passed"
            } else {
                "compatibility gate failed"
            },
            if self.diagnostic_retained {
                "; diagnostic retained"
            } else if failed.is_empty() {
                ""
            } else {
                "; failed: "
            }
        ) + if failed.is_empty() { "" } else { &failed }
    }
    pub fn write_local(&self, directory: &Path) -> std::io::Result<()> {
        fs::create_dir_all(directory)?;
        fs::write(directory.join("probe-results.json"), self.machine_json())?;
        fs::write(directory.join("probe-summary.txt"), self.summary())
    }
}

fn command(
    adb: &mut impl Adb,
    serial: &Serial,
    operation: DeviceOperation,
    name: &'static str,
    outcomes: &mut Vec<(&'static str, bool)>,
) -> bool {
    let ok = adb
        .execute(AdbCommand::Device {
            serial: serial.clone(),
            operation,
        })
        .and_then(require_success)
        .is_ok();
    outcomes.push((name, ok));
    ok
}

pub fn run(adb: &mut impl Adb, serial: Serial, config: ProbeConfig) -> ProbeReport {
    if !config.disposable_snapshot {
        return ProbeReport {
            passed: false,
            outcomes: vec![("disposable fixture or snapshot required", false)],
            diagnostic_retained: false,
        };
    }
    let user = UserId::parse(0).expect("primary user");
    let fixture = PackageId::parse(FIXTURE).expect("fixed fixture package");
    let launcher = PackageId::parse(LAUNCHER).expect("fixed launcher package");
    let app_op = AppOp::parse("POST_NOTIFICATION").expect("fixed app op");
    let mut outcomes = Vec::new();
    let core = [
        command(
            adb,
            &serial,
            DeviceOperation::GetProperty(Property::SdkInt),
            "api",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::GetProperty(Property::Model),
            "model",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::GetProperty(Property::Fingerprint),
            "fingerprint",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::PackageInfo {
                package: fixture.clone(),
                user,
            },
            "fixture installation",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::Bridge(BridgeOperation::Health),
            "bridge protection",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::Bridge(BridgeOperation::DeviceFacts {
                device_serial: serial.clone(),
            }),
            "bridge facts",
            &mut outcomes,
        ),
    ]
    .into_iter()
    .all(|ok| ok);
    if !core {
        return ProbeReport {
            passed: false,
            outcomes,
            diagnostic_retained: true,
        };
    }
    let fingerprint = Fingerprint::parse("probe-fingerprint").expect("fixed fake binding");
    let icon = StreamId::parse("0123456789abcdef0123456789abcdef").expect("fixed fake stream");
    let component =
        Component::parse("org.unscroll.fixture/.FixtureActivity").expect("fixed fixture component");
    for (name, operation) in [
        (
            "catalog and protected facts",
            DeviceOperation::Bridge(BridgeOperation::Catalog {
                device_serial: serial.clone(),
                fingerprint: fingerprint.clone(),
            }),
        ),
        (
            "icon metadata",
            DeviceOperation::Bridge(BridgeOperation::Icon {
                package: fixture.clone(),
                component,
            }),
        ),
        (
            "icon streaming",
            DeviceOperation::Bridge(BridgeOperation::ReadIcon(icon)),
        ),
        (
            "recovery access",
            DeviceOperation::Bridge(BridgeOperation::ReadEnvelope {
                device_serial: serial.clone(),
                fingerprint,
            }),
        ),
        (
            "notification baseline",
            DeviceOperation::AppOpGet {
                package: fixture.clone(),
                user,
                app_op: app_op.clone(),
            },
        ),
        ("home baseline", DeviceOperation::HomeResolve),
        ("guided chooser", DeviceOperation::HomeChooser),
        (
            "shell home selection",
            DeviceOperation::HomeSelect(launcher.clone()),
        ),
        ("resolved home", DeviceOperation::HomeResolve),
        (
            "suspension",
            DeviceOperation::Suspend {
                package: fixture.clone(),
                user,
                suspended: true,
            },
        ),
        (
            "suspension verified",
            DeviceOperation::PackageInfo {
                package: fixture.clone(),
                user,
            },
        ),
        (
            "notification suppression",
            DeviceOperation::AppOpSet {
                package: fixture.clone(),
                user,
                app_op: app_op.clone(),
                mode: AppOpMode::Ignore,
            },
        ),
        (
            "notification suppression verified",
            DeviceOperation::AppOpGet {
                package: fixture.clone(),
                user,
                app_op: app_op.clone(),
            },
        ),
        (
            "store detection",
            DeviceOperation::PackageInfo {
                package: PackageId::parse("com.android.vending").unwrap(),
                user,
            },
        ),
    ] {
        command(adb, &serial, operation, name, &mut outcomes);
    }
    let restore = [
        command(
            adb,
            &serial,
            DeviceOperation::AppOpSet {
                package: fixture.clone(),
                user,
                app_op: app_op.clone(),
                mode: AppOpMode::Default,
            },
            "notification restored",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::AppOpGet {
                package: fixture.clone(),
                user,
                app_op,
            },
            "notification restoration verified",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::Suspend {
                package: fixture.clone(),
                user,
                suspended: false,
            },
            "unsuspension",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::PackageInfo {
                package: fixture,
                user,
            },
            "unsuspension verified",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::HomeSelect(launcher),
            "home restored",
            &mut outcomes,
        ),
        command(
            adb,
            &serial,
            DeviceOperation::HomeResolve,
            "home restoration verified",
            &mut outcomes,
        ),
    ]
    .into_iter()
    .all(|ok| ok);
    let passed = core && restore && outcomes.iter().all(|(_, ok)| *ok);
    ProbeReport {
        passed,
        outcomes,
        diagnostic_retained: !restore,
    }
}

#[allow(dead_code)]
fn main() {
    let config = std::env::args()
        .any(|arg| arg == "--disposable-snapshot")
        .then(ProbeConfig::disposable_snapshot)
        .unwrap_or_default();
    let mut adb = match BundledAdb::for_development() {
        Ok(adb) => adb,
        Err(_) => return,
    };
    let serial = match discover(&mut adb) {
        Ok(Discovery::One { serial }) => serial,
        _ => return,
    };
    let report = run(&mut adb, serial, config);
    let _ = report.write_local(Path::new("probe-results"));
    if !report.passed() {
        std::process::exit(1);
    }
}
