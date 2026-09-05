use std::{fs, path::Path};
use unscroll_desktop_lib::{
    adb::{
        require_success, Adb, AdbCommand, AdbOutput, AppOp, AppOpMode, BridgeOperation, BundledAdb,
        Component, DeviceOperation, Fingerprint, PackageId, Property, Serial, StreamId, UserId,
    },
    device::{discover, Discovery},
};
const FIXTURE: &str = "org.unscroll.fixture";
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
    api: String,
    model: String,
    fingerprint: String,
    outcomes: Vec<(&'static str, bool)>,
    diagnostics: Vec<String>,
}
impl ProbeReport {
    pub fn passed(&self) -> bool {
        self.passed
    }
    pub fn machine_json(&self) -> String {
        let o = self
            .outcomes
            .iter()
            .map(|(n, x)| format!("{{\"capability\":\"{n}\",\"passed\":{x}}}"))
            .collect::<Vec<_>>()
            .join(",");
        let d = self.diagnostics.to_vec().join(",");
        format!("{{\"telemetry\":false,\"api\":\"{}\",\"model\":\"{}\",\"fingerprint\":\"{}\",\"outcomes\":[{o}],\"diagnostics\":[{d}],\"passed\":{}}}",self.api,self.model,self.fingerprint,self.passed)
    }
    pub fn summary(&self) -> String {
        format!(
            "API {} {}: {}{}{}",
            self.api,
            self.model,
            if self.passed {
                "compatibility gate passed"
            } else {
                "compatibility gate failed"
            },
            if self.diagnostics.is_empty() {
                ""
            } else {
                "; diagnostic retained"
            },
            self.outcomes
                .first()
                .filter(|(_, ok)| !*ok)
                .map(|(name, _)| format!("; {name}"))
                .unwrap_or_default()
        )
    }
    pub fn write_local(&self, d: &Path) -> std::io::Result<()> {
        fs::create_dir_all(d)?;
        fs::write(d.join("probe-results.json"), self.machine_json())?;
        fs::write(d.join("probe-summary.txt"), self.summary())
    }
}
fn exec(
    adb: &mut impl Adb,
    s: &Serial,
    op: DeviceOperation,
    n: &'static str,
    o: &mut Vec<(&'static str, bool)>,
    d: &mut Vec<String>,
) -> Option<AdbOutput> {
    match adb
        .execute(AdbCommand::Device {
            serial: s.clone(),
            operation: op,
        })
        .and_then(require_success)
    {
        Ok(x) => {
            o.push((n, true));
            Some(x)
        }
        Err(_) => {
            o.push((n, false));
            d.push(format!("{{\"command_class\":\"{}\",\"error\":\"failed\",\"stdout\":\"redacted\",\"stderr\":\"redacted\"}}", n));
            None
        }
    }
}
fn text(x: Option<AdbOutput>) -> Option<String> {
    x.map(|v| v.stdout().trim().to_owned())
        .filter(|v| !v.is_empty())
}
fn installed(x: &str, p: &PackageId) -> bool {
    x.lines()
        .any(|l| l.trim() == format!("package:{}", p.as_str()))
}
fn appop(x: &str) -> Option<AppOpMode> {
    ["default", "allow", "ignore"].into_iter().find_map(|m| {
        x.contains(&format!(": {m}"))
            .then(|| AppOpMode::parse(m).ok())
            .flatten()
    })
}
pub fn suspended(x: &str, want: bool) -> bool {
    x.split_whitespace()
        .any(|field| field == format!("suspended={want}"))
}
fn stream(x: &str) -> Option<StreamId> {
    x.split(|c: char| !c.is_ascii_hexdigit())
        .find_map(|v| StreamId::parse(v).ok())
}
pub fn run(adb: &mut impl Adb, s: Serial, c: ProbeConfig) -> ProbeReport {
    let (mut o, mut d) = (Vec::new(), Vec::new());
    if !c.disposable_snapshot {
        return ProbeReport {
            passed: false,
            api: "unavailable".into(),
            model: "unavailable".into(),
            fingerprint: "unavailable".into(),
            outcomes: vec![("disposable fixture or snapshot required", false)],
            diagnostics: d,
        };
    };
    let u = UserId::parse(0).unwrap();
    let f = PackageId::parse(FIXTURE).unwrap();
    let op = AppOp::parse("POST_NOTIFICATION").unwrap();
    let api = text(exec(
        adb,
        &s,
        DeviceOperation::GetProperty(Property::SdkInt),
        "api",
        &mut o,
        &mut d,
    ))
    .unwrap_or_else(|| "unavailable".into());
    let model = text(exec(
        adb,
        &s,
        DeviceOperation::GetProperty(Property::Model),
        "model",
        &mut o,
        &mut d,
    ))
    .unwrap_or_else(|| "unavailable".into());
    let fp = text(exec(
        adb,
        &s,
        DeviceOperation::GetProperty(Property::Fingerprint),
        "fingerprint",
        &mut o,
        &mut d,
    ))
    .unwrap_or_else(|| "unavailable".into());
    let binding = Fingerprint::parse(&fp).ok();
    let fixture = text(exec(
        adb,
        &s,
        DeviceOperation::PackageInfo {
            package: f.clone(),
            user: u,
        },
        "fixture installation",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| installed(&x, &f));
    o.last_mut().unwrap().1 = fixture;
    let health = text(exec(
        adb,
        &s,
        DeviceOperation::Bridge(BridgeOperation::Health),
        "bridge protection",
        &mut o,
        &mut d,
    ))
    .is_some();
    let facts = text(exec(
        adb,
        &s,
        DeviceOperation::Bridge(BridgeOperation::DeviceFacts {
            device_serial: s.clone(),
        }),
        "bridge facts",
        &mut o,
        &mut d,
    ))
    .is_some();
    if !fixture || !health || !facts || binding.is_none() {
        return ProbeReport {
            passed: false,
            api,
            model,
            fingerprint: fp,
            outcomes: o,
            diagnostics: d,
        };
    };
    let binding = binding.unwrap();
    let catalog = text(exec(
        adb,
        &s,
        DeviceOperation::Bridge(BridgeOperation::Catalog {
            device_serial: s.clone(),
            fingerprint: binding.clone(),
        }),
        "catalog and protected facts",
        &mut o,
        &mut d,
    ))
    .is_some();
    let component = Component::parse("org.unscroll.fixture/.FixtureActivity").unwrap();
    let icon = text(exec(
        adb,
        &s,
        DeviceOperation::Bridge(BridgeOperation::Icon {
            package: f.clone(),
            component,
        }),
        "icon metadata",
        &mut o,
        &mut d,
    ))
    .and_then(|x| stream(&x));
    let icon = icon.is_some_and(|id| {
        text(exec(
            adb,
            &s,
            DeviceOperation::Bridge(BridgeOperation::ReadIcon(id)),
            "icon streaming",
            &mut o,
            &mut d,
        ))
        .is_some()
    });
    let recovery = text(exec(
        adb,
        &s,
        DeviceOperation::Bridge(BridgeOperation::ReadEnvelope {
            device_serial: s.clone(),
            fingerprint: binding,
        }),
        "recovery access",
        &mut o,
        &mut d,
    ))
    .is_some();
    if !catalog || !icon || !recovery {
        return ProbeReport {
            passed: false,
            api,
            model,
            fingerprint: fp,
            outcomes: o,
            diagnostics: d,
        };
    };
    let baseop = text(exec(
        adb,
        &s,
        DeviceOperation::AppOpGet {
            package: f.clone(),
            user: u,
            app_op: op.clone(),
        },
        "notification baseline",
        &mut o,
        &mut d,
    ))
    .and_then(|x| appop(&x));
    let basehome = text(exec(
        adb,
        &s,
        DeviceOperation::HomeResolve,
        "home baseline",
        &mut o,
        &mut d,
    ))
    .and_then(|x| Component::parse(x.trim()).ok());
    if baseop.is_none() || basehome.is_none() {
        return ProbeReport {
            passed: false,
            api,
            model,
            fingerprint: fp,
            outcomes: o,
            diagnostics: d,
        };
    };
    let (baseop, basehome) = (baseop.unwrap(), basehome.unwrap());
    let launcher_home = Component::parse("org.unscroll.launcher/.MainActivity").unwrap();
    exec(
        adb,
        &s,
        DeviceOperation::HomeChooser,
        "guided chooser",
        &mut o,
        &mut d,
    );
    exec(
        adb,
        &s,
        DeviceOperation::HomeSelect(launcher_home.clone()),
        "shell home selection",
        &mut o,
        &mut d,
    );
    let selected = text(exec(
        adb,
        &s,
        DeviceOperation::HomeResolve,
        "resolved home",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| x.trim() == launcher_home.as_str());
    o.last_mut().unwrap().1 = selected;
    exec(
        adb,
        &s,
        DeviceOperation::Suspend {
            package: f.clone(),
            user: u,
            suspended: true,
        },
        "suspension",
        &mut o,
        &mut d,
    );
    let suspension_verified = text(exec(
        adb,
        &s,
        DeviceOperation::PackageState {
            package: f.clone(),
            user: u,
        },
        "suspension verified",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| suspended(&x, true));
    o.last_mut().unwrap().1 = suspension_verified;
    exec(
        adb,
        &s,
        DeviceOperation::AppOpSet {
            package: f.clone(),
            user: u,
            app_op: op.clone(),
            mode: AppOpMode::Ignore,
        },
        "notification suppression",
        &mut o,
        &mut d,
    );
    let ignored = text(exec(
        adb,
        &s,
        DeviceOperation::AppOpGet {
            package: f.clone(),
            user: u,
            app_op: op.clone(),
        },
        "notification suppression verified",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| appop(&x) == Some(AppOpMode::Ignore));
    o.last_mut().unwrap().1 = ignored;
    let store = PackageId::parse("com.android.vending").unwrap();
    let storeok = text(exec(
        adb,
        &s,
        DeviceOperation::PackageInfo {
            package: store.clone(),
            user: u,
        },
        "store detection",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| installed(&x, &store));
    o.last_mut().unwrap().1 = storeok;
    exec(
        adb,
        &s,
        DeviceOperation::AppOpSet {
            package: f.clone(),
            user: u,
            app_op: op.clone(),
            mode: baseop,
        },
        "notification restored",
        &mut o,
        &mut d,
    );
    let opok = text(exec(
        adb,
        &s,
        DeviceOperation::AppOpGet {
            package: f.clone(),
            user: u,
            app_op: op,
        },
        "notification restoration verified",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| appop(&x) == Some(baseop));
    o.last_mut().unwrap().1 = opok;
    exec(
        adb,
        &s,
        DeviceOperation::Suspend {
            package: f.clone(),
            user: u,
            suspended: false,
        },
        "unsuspension",
        &mut o,
        &mut d,
    );
    let unsuspended = text(exec(
        adb,
        &s,
        DeviceOperation::PackageState {
            package: f,
            user: u,
        },
        "unsuspension verified",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| suspended(&x, false));
    o.last_mut().unwrap().1 = unsuspended;
    exec(
        adb,
        &s,
        DeviceOperation::HomeSelect(basehome.clone()),
        "home restored",
        &mut o,
        &mut d,
    );
    let homeok = text(exec(
        adb,
        &s,
        DeviceOperation::HomeResolve,
        "home restoration verified",
        &mut o,
        &mut d,
    ))
    .is_some_and(|x| x.trim() == basehome.as_str());
    o.last_mut().unwrap().1 = homeok;
    let passed = o.iter().all(|(_, x)| *x);
    ProbeReport {
        passed,
        api,
        model,
        fingerprint: fp,
        outcomes: o,
        diagnostics: d,
    }
}
#[allow(dead_code)]
fn main() {
    let c = std::env::args()
        .any(|a| a == "--disposable-snapshot")
        .then(ProbeConfig::disposable_snapshot)
        .unwrap_or_default();
    let mut adb = match BundledAdb::for_development() {
        Ok(x) => x,
        Err(_) => return,
    };
    let s = match discover(&mut adb) {
        Ok(Discovery::One { serial }) => serial,
        _ => return,
    };
    let r = run(&mut adb, s, c);
    if r.write_local(Path::new("probe-results")).is_err() || !r.passed() {
        std::process::exit(1)
    }
}
