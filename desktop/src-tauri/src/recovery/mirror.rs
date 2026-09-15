use super::model::{BaselineInput, DeviceBinding, InitialPackageSuspension, RecoveryEnvelopeV1};

use crate::adb::{require_success, Adb, AdbCommand, BridgeOperation, DeviceOperation, Fingerprint, PackageId, RecoveryEnvelope, Serial};
use crate::device::DeviceSnapshot;
use std::path::Path;

pub trait MirrorStore {
    type Error;

    fn write_private(&mut self, envelope: &str) -> Result<(), Self::Error>;
    fn write_shared(&mut self, envelope: &str) -> Result<(), Self::Error>;
    fn read_private(&mut self) -> Result<Option<String>, Self::Error>;
    fn read_shared(&mut self) -> Result<Option<String>, Self::Error>;
    fn delete_private(&mut self) -> Result<(), Self::Error>;
    fn delete_shared(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorError {
    PrivateWrite,
    PrivateReadback,
    SharedWrite,
    SharedReadback,
    PrivateDelete,
    SharedDelete,
    NotRestored,
    SourceMismatch,
    Baseline,
    SourceWrite,
}

pub fn persist_adb(adb: &mut impl Adb, serial: &Serial, fingerprint: &Fingerprint, envelope: &RecoveryEnvelopeV1, source: &Path) -> Result<(), MirrorError> {
    let expected = envelope.canonical_json();
    let source_value = std::fs::read_to_string(source).map_err(|_| MirrorError::SourceMismatch)?;
    let source_value = RecoveryEnvelopeV1::parse(&source_value).map_err(|_| MirrorError::SourceMismatch)?;
    if source_value.canonical_json() != expected { return Err(MirrorError::SourceMismatch); }
    let private = RecoveryEnvelope::parse(&expected).map_err(|_| MirrorError::PrivateWrite)?;
    require_success(adb.execute(AdbCommand::Device { serial: serial.clone(), operation: DeviceOperation::Bridge(BridgeOperation::WriteEnvelope { device_serial: serial.clone(), fingerprint: fingerprint.clone(), envelope: private }) }).map_err(|_| MirrorError::PrivateWrite)?).map_err(|_| MirrorError::PrivateWrite)?;
    let private = require_success(adb.execute(AdbCommand::Device { serial: serial.clone(), operation: DeviceOperation::Bridge(BridgeOperation::ReadEnvelope { device_serial: serial.clone(), fingerprint: fingerprint.clone() }) }).map_err(|_| MirrorError::PrivateReadback)?).map_err(|_| MirrorError::PrivateReadback)?;
    let private = crate::device::catalog::recovery_envelope(private.stdout()).map_err(|_| MirrorError::PrivateReadback)?;
    if !matches_copy(private, &expected) { return Err(MirrorError::PrivateReadback); }
    super::shared_copy::write_from(adb, serial, source).map_err(|_| MirrorError::SharedWrite)?;
    let shared = require_success(adb.execute(AdbCommand::Device { serial: serial.clone(), operation: DeviceOperation::ReadDestination(super::shared_copy::destination()) }).map_err(|_| MirrorError::SharedReadback)?).map_err(|_| MirrorError::SharedReadback)?;
    let shared = std::str::from_utf8(super::shared_copy::bounded_read(shared.stdout_bytes()).map_err(|_| MirrorError::SharedReadback)?).ok().map(str::to_owned);
    if !matches_copy(shared, &expected) { return Err(MirrorError::SharedReadback); }
    Ok(())
}

pub fn initialize_adb(adb: &mut impl Adb, snapshot: &DeviceSnapshot, baseline_id: &str, baseline_launcher: PackageId, allowed_packages: Vec<PackageId>, source: &Path) -> Result<(), MirrorError> {
    let serial = Serial::parse(&snapshot.serial).map_err(|_| MirrorError::Baseline)?;
    let fingerprint = Fingerprint::parse(&snapshot.fingerprint).map_err(|_| MirrorError::Baseline)?;
    if snapshot.current_user != 0 { return Err(MirrorError::Baseline); }
    let input = BaselineInput { binding: DeviceBinding { serial: serial.as_str().into(), fingerprint: fingerprint.as_str().into(), user_id: 0 }, baseline_id: baseline_id.into(), baseline_launcher: baseline_launcher.as_str().into(), initial_home: snapshot.home.as_str().into(), initial_packages: snapshot.catalog.iter().map(|app| InitialPackageSuspension { package: app.package.as_str().into(), suspended: app.suspended, user_id: 0 }).collect(), allowed_packages: allowed_packages.into_iter().map(|package| package.as_str().into()).collect() };
    let baseline = RecoveryEnvelopeV1::new_baseline(input).map_err(|_| MirrorError::Baseline)?;
    std::fs::write(source, baseline.canonical_json()).map_err(|_| MirrorError::SourceWrite)?;
    persist_adb(adb, &serial, &fingerprint, &baseline, source)
}
pub fn persist(store: &mut impl MirrorStore, envelope: &RecoveryEnvelopeV1) -> Result<(), MirrorError> {
    let expected = envelope.canonical_json();
    store.write_private(&expected).map_err(|_| MirrorError::PrivateWrite)?;
    if !matches_copy(store.read_private().map_err(|_| MirrorError::PrivateReadback)?, &expected) {
        return Err(MirrorError::PrivateReadback);
    }
    store.write_shared(&expected).map_err(|_| MirrorError::SharedWrite)?;
    if !matches_copy(store.read_shared().map_err(|_| MirrorError::SharedReadback)?, &expected) {
        return Err(MirrorError::SharedReadback);
    }
    Ok(())
}

pub fn cleanup(store: &mut impl MirrorStore, restoration_verified: bool, expected: &RecoveryEnvelopeV1) -> Result<(), MirrorError> {
    if !restoration_verified || !expected.has_applied_private_cleanup() { return Err(MirrorError::NotRestored); }
    let private = store.read_private().map_err(|_| MirrorError::PrivateReadback)?;
    let shared = store.read_shared().map_err(|_| MirrorError::SharedReadback)?;
    let shared_ok = matches_copy(shared, &expected.canonical_json());
    if private.is_none() {
        if !shared_ok { return Err(MirrorError::NotRestored); }
        return store.delete_shared().map_err(|_| MirrorError::SharedDelete);
    }
    if !matches_copy(private, &expected.canonical_json()) || !shared_ok { return Err(MirrorError::NotRestored); }
    store.delete_private().map_err(|_| MirrorError::PrivateDelete)?;
    store.delete_shared().map_err(|_| MirrorError::SharedDelete)
}
fn matches_copy(value: Option<String>, expected: &str) -> bool {
    value
        .and_then(|value| RecoveryEnvelopeV1::parse(&value).ok())
        .is_some_and(|value| value.canonical_json() == expected)
}
