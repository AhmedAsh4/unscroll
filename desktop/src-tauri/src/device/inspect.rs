use super::{
    bootstrap::LauncherArtifact,
    capabilities::CapabilityReport,
    catalog::{self, AppCatalogEntry, InstallSourceFact, ProtectedPackageFact, StoreFact},
    discover, Discovery,
};
use crate::adb::{
    require_success, Adb, AdbCommand, AdbError, BridgeOperation, Component, DeviceOperation,
    Fingerprint, PackageId, Property, UserId,
};

const LAUNCHER: &str = "org.unscroll.launcher";
const UNKNOWN_SOURCES: &str = "REQUEST_INSTALL_PACKAGES";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObservation {
    Missing,
    Present,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileFact {
    pub user_id: u32,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XiaomiGuidance {
    InstallViaUsb,
    MiAccount,
    Sim,
    Network,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSnapshot {
    pub serial: String,
    pub api: u32,
    pub model: String,
    pub manufacturer: String,
    pub fingerprint: String,
    pub current_user: u32,
    pub home: Component,
    pub catalog: Vec<AppCatalogEntry>,
    pub protected: Vec<ProtectedPackageFact>,
    pub stores: Vec<StoreFact>,
    pub install_sources: Vec<InstallSourceFact>,
    pub profiles: Vec<ProfileFact>,
    pub private_recovery: RecoveryObservation,
    pub shared_recovery: RecoveryObservation,
    pub xiaomi_guidance: Vec<XiaomiGuidance>,
    pub capabilities: CapabilityReport,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectionError {
    LauncherArtifactUnavailable,
    Discovery,
    UnsupportedApi,
    Bootstrap,
    SignatureMismatch,
    BridgeMismatch,
    RecoveryRequired,
    Preflight,
    Xiaomi(Vec<XiaomiGuidance>),
}

fn command(
    adb: &mut impl Adb,
    serial: &crate::adb::Serial,
    operation: DeviceOperation,
) -> Result<crate::adb::AdbOutput, InspectionError> {
    require_success(
        adb.execute(AdbCommand::Device {
            serial: serial.clone(),
            operation,
        })
        .map_err(|_| InspectionError::Preflight)?,
    )
    .map_err(|_| InspectionError::Preflight)
}
fn read(
    adb: &mut impl Adb,
    serial: &crate::adb::Serial,
    operation: DeviceOperation,
) -> Result<String, InspectionError> {
    command(adb, serial, operation).map(|output| output.stdout().trim().to_owned())
}
fn package() -> PackageId {
    PackageId::parse(LAUNCHER).expect("constant package")
}
fn user() -> UserId {
    UserId::parse(0).expect("primary user")
}
fn missing(error: &AdbError) -> bool {
    matches!(error, AdbError::NonZero(output) if output.stderr().to_ascii_lowercase().contains("no such file") || output.stderr().to_ascii_lowercase().contains("not found"))
}
fn private_recovery_read(
    adb: &mut impl Adb,
    serial: &crate::adb::Serial,
    fingerprint: &Fingerprint,
) -> Result<RecoveryObservation, InspectionError> {
    match adb.execute(AdbCommand::Device {
        serial: serial.clone(),
        operation: DeviceOperation::Bridge(BridgeOperation::ReadEnvelope {
            device_serial: serial.clone(),
            fingerprint: fingerprint.clone(),
        }),
    }) {
        Ok(output) => match require_success(output) {
            Ok(output) => {
                match catalog::envelope(output.stdout()).map_err(|_| InspectionError::Preflight)? {
                    catalog::BridgeReply::Missing => Ok(RecoveryObservation::Missing),
                    catalog::BridgeReply::Value(catalog::Json::Object(result)) => {
                        match result.get("envelope") {
                            Some(catalog::Json::String(value)) if result.len() == 1 => {
                                crate::recovery::model::RecoveryEnvelopeV1::parse_for_device(
                                    value,
                                    serial.as_str(),
                                    fingerprint.as_str(),
                                    0,
                                )
                                .map(|_| RecoveryObservation::Present)
                                .map_err(|_| InspectionError::Preflight)
                            }
                            _ => Err(InspectionError::Preflight),
                        }
                    }
                    catalog::BridgeReply::Value(_) => Err(InspectionError::Preflight),
                }
            }
            Err(error) if missing(&error) => Ok(RecoveryObservation::Missing),
            Err(_) => Err(InspectionError::Preflight),
        },
        Err(_) => Err(InspectionError::Preflight),
    }
}
fn shared_recovery_read(
    adb: &mut impl Adb,
    serial: &crate::adb::Serial,
    fingerprint: &Fingerprint,
) -> Result<RecoveryObservation, InspectionError> {
    match adb.execute(AdbCommand::Device {
        serial: serial.clone(),
        operation: DeviceOperation::ReadDestination(crate::recovery::shared_copy::destination()),
    }) {
        Ok(output) => match require_success(output) {
            Ok(output) => crate::recovery::model::RecoveryEnvelopeV1::parse_for_device(
                std::str::from_utf8(
                    crate::recovery::shared_copy::bounded_read(output.stdout_bytes())
                        .map_err(|_| InspectionError::Preflight)?,
                )
                .map_err(|_| InspectionError::Preflight)?,
                serial.as_str(),
                fingerprint.as_str(),
                0,
            )
            .map(|_| RecoveryObservation::Present)
            .map_err(|_| InspectionError::Preflight),
            Err(error) if missing(&error) => Ok(RecoveryObservation::Missing),
            Err(_) => Err(InspectionError::Preflight),
        },
        Err(_) => Err(InspectionError::Preflight),
    }
}
fn profiles(value: &str) -> Vec<ProfileFact> {
    value
        .lines()
        .filter_map(|line| {
            let rest = line.split("UserInfo{").nth(1)?;
            let (id, rest) = rest.split_once(':')?;
            let id = id.parse().ok()?;
            (id != 0).then(|| ProfileFact {
                user_id: id,
                name: rest.split(':').next().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}
fn handlers(value: &str) -> Vec<PackageId> {
    value
        .lines()
        .filter_map(|line| line.trim().split_once('/').map(|(package, _)| package))
        .filter_map(|package| PackageId::parse(package).ok())
        .collect()
}
fn app_op_allowed(value: &str) -> bool {
    !value.contains(": ignore") && !value.contains(": deny")
}
fn signer_matches(value: &str, expected: &str) -> bool {
    value
        .lines()
        .any(|line| line.trim().strip_prefix("signing_sha256=") == Some(expected))
}
fn xiaomi_guidance() -> Vec<XiaomiGuidance> {
    vec![
        XiaomiGuidance::InstallViaUsb,
        XiaomiGuidance::MiAccount,
        XiaomiGuidance::Sim,
        XiaomiGuidance::Network,
    ]
}

/// Preflight's only permissible pre-baseline mutation is a non-activating launcher install.
pub fn inspect(
    adb: &mut impl Adb,
    artifact: Option<LauncherArtifact>,
) -> Result<DeviceSnapshot, InspectionError> {
    let artifact = artifact.ok_or(InspectionError::LauncherArtifactUnavailable)?;
    artifact
        .validate()
        .map_err(|_| InspectionError::LauncherArtifactUnavailable)?;
    let Discovery::One { serial } = discover(adb).map_err(|_| InspectionError::Discovery)?;
    let api = read(adb, &serial, DeviceOperation::GetProperty(Property::SdkInt))?
        .parse()
        .map_err(|_| InspectionError::UnsupportedApi)?;
    if !(24..=36).contains(&api) {
        return Err(InspectionError::UnsupportedApi);
    }
    let model = read(adb, &serial, DeviceOperation::GetProperty(Property::Model))?;
    let manufacturer = read(
        adb,
        &serial,
        DeviceOperation::GetProperty(Property::Manufacturer),
    )?;
    let fingerprint = Fingerprint::parse(&read(
        adb,
        &serial,
        DeviceOperation::GetProperty(Property::Fingerprint),
    )?)
    .map_err(|_| InspectionError::Preflight)?;
    if read(adb, &serial, DeviceOperation::CurrentUser)?
        .parse::<u32>()
        .ok()
        != Some(0)
    {
        return Err(InspectionError::Preflight);
    }
    let home = Component::parse(&read(adb, &serial, DeviceOperation::HomeResolve)?)
        .map_err(|_| InspectionError::Preflight)?;
    let help = read(adb, &serial, DeviceOperation::PackageHelp)?;
    let shell_suspend = help.contains("suspend") && help.contains("unsuspend");
    let shell_home = help.contains("set-home-activity") && !home.as_str().is_empty();
    let shared_directory = read(adb, &serial, DeviceOperation::SharedRecoveryDirectory).is_ok();
    if !shared_directory {
        return Err(InspectionError::Preflight);
    }
    let existing = read(
        adb,
        &serial,
        DeviceOperation::PackageAnyUser { package: package() },
    )?;
    let installed_here = !existing
        .lines()
        .any(|line| line.trim() == format!("package:{LAUNCHER}"));
    if installed_here {
        match require_success(
            adb.execute(AdbCommand::Install {
                serial: serial.clone(),
                user: user(),
                apk: artifact.path().into(),
            })
            .map_err(|_| InspectionError::Bootstrap)?,
        ) {
            Ok(_) => {}
            Err(error) => {
                return Err(match &error {
                    AdbError::NonZero(output)
                        if output.stderr().contains("INSTALL_FAILED_USER_RESTRICTED") =>
                    {
                        InspectionError::Xiaomi(xiaomi_guidance())
                    }
                    AdbError::NonZero(output) if output.stderr().contains("SecurityException") => {
                        InspectionError::Xiaomi(xiaomi_guidance())
                    }
                    _ => InspectionError::Bootstrap,
                })
            }
        }
    }
    let result = (|| {
        let signature = read(
            adb,
            &serial,
            DeviceOperation::PackageSigning { package: package() },
        )?;
        if !signer_matches(&signature, artifact.signing_sha256()) {
            return Err(InspectionError::SignatureMismatch);
        }
        let health = read(
            adb,
            &serial,
            DeviceOperation::Bridge(BridgeOperation::Health),
        )?;
        catalog::health(&health).map_err(|_| InspectionError::BridgeMismatch)?;
        let facts_text = read(
            adb,
            &serial,
            DeviceOperation::Bridge(BridgeOperation::DeviceFacts {
                device_serial: serial.clone(),
            }),
        )
        .map_err(|_| InspectionError::BridgeMismatch)?;
        let facts = catalog::facts(&facts_text, &serial, &fingerprint)
            .map_err(|_| InspectionError::BridgeMismatch)?;
        let (mut catalog, mut next) = catalog::catalog(&read(
            adb,
            &serial,
            DeviceOperation::Bridge(BridgeOperation::Catalog {
                device_serial: serial.clone(),
                fingerprint: fingerprint.clone(),
            }),
        )?)
        .map_err(|_| InspectionError::BridgeMismatch)?;
        while let Some(cursor) = next {
            let (mut page, continuation) = catalog::catalog(&read(
                adb,
                &serial,
                DeviceOperation::Bridge(BridgeOperation::CatalogNext {
                    device_serial: serial.clone(),
                    fingerprint: fingerprint.clone(),
                    cursor,
                }),
            )?)
            .map_err(|_| InspectionError::BridgeMismatch)?;
            if catalog.len() + page.len() > 1_000 {
                return Err(InspectionError::Preflight);
            }
            catalog.append(&mut page);
            next = continuation;
        }
        let mut apps = Vec::with_capacity(catalog.len());
        let mut protected = Vec::new();
        for (mut app, protected_fact, icon_available) in catalog.drain(..) {
            if icon_available {
                let icon = catalog::icon(
                    &read(
                        adb,
                        &serial,
                        DeviceOperation::Bridge(BridgeOperation::Icon {
                            package: app.package.clone(),
                            component: app.component.clone(),
                        }),
                    )?,
                    app.package.clone(),
                    app.component.clone(),
                )
                .map_err(|_| InspectionError::BridgeMismatch)?;
                let bytes = command(
                    adb,
                    &serial,
                    DeviceOperation::Bridge(BridgeOperation::ReadIcon(icon.stream)),
                )?
                .stdout_bytes()
                .to_vec();
                if bytes.len() != icon.length
                    || crate::recovery::model::sha256_hex(&bytes) != icon.sha256
                    || !bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
                {
                    return Err(InspectionError::BridgeMismatch);
                }
                app.icon = bytes;
            }
            if let Some(fact) = protected_fact {
                protected.push(fact);
            }
            apps.push(app);
        }
        let install_handlers = handlers(&read(adb, &serial, DeviceOperation::InstallHandlers)?);
        let mut install_sources = Vec::new();
        for package in &install_handlers {
            let allowed = app_op_allowed(&read(
                adb,
                &serial,
                DeviceOperation::AppOpGet {
                    package: package.clone(),
                    user: user(),
                    app_op: crate::adb::AppOp::parse(UNKNOWN_SOURCES).expect("constant app-op"),
                },
            )?);
            install_sources.push(InstallSourceFact {
                package: package.clone(),
                allowed,
            });
        }
        let stores = install_handlers
            .into_iter()
            .filter(|handler| apps.iter().any(|app| &app.package == handler))
            .map(|package| StoreFact { package })
            .collect();
        let profile_facts = profiles(&read(adb, &serial, DeviceOperation::UserList)?);
        let shared_recovery = shared_recovery_read(adb, &serial, &fingerprint)?;
        let private_recovery = private_recovery_read(adb, &serial, &fingerprint)?;
        if private_recovery == RecoveryObservation::Present
            || shared_recovery == RecoveryObservation::Present
        {
            return Err(InspectionError::RecoveryRequired);
        }
        let capabilities = CapabilityReport {
            package_suspension: shell_suspend && facts.package_suspension,
            bridge: true,
            recovery_storage: shared_directory && facts.recovery_storage,
            app_op_inspection: facts.app_ops && !install_sources.is_empty(),
            home_path: shell_home && facts.home_selection,
        };
        if !capabilities.ready() {
            return Err(InspectionError::Preflight);
        }
        Ok(DeviceSnapshot {
            serial: serial.as_str().into(),
            api,
            model,
            manufacturer: manufacturer.clone(),
            fingerprint: fingerprint.as_str().into(),
            current_user: 0,
            home,
            catalog: apps,
            protected,
            stores,
            install_sources,
            profiles: profile_facts,
            private_recovery,
            shared_recovery,
            xiaomi_guidance: Vec::new(),
            capabilities,
        })
    })();
    if result.is_err() && installed_here {
        let _ = adb.execute(AdbCommand::Uninstall {
            serial,
            user: user(),
            package: package(),
        });
    }
    result
}
