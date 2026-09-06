use crate::adb::{require_success, Adb, AdbCommand, Destination, DeviceOperation, Serial};
use std::{io, path::Path};

pub const SHARED_RECOVERY_PATH: &str = "/sdcard/Documents/Unscroll/recovery-v1.json";
pub const MAX_SHARED_COPY_BYTES: usize = 65_536;

pub fn bounded_read(value: &[u8]) -> io::Result<&[u8]> {
    (value.len() <= MAX_SHARED_COPY_BYTES)
        .then_some(value)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "shared recovery copy exceeds limit",
            )
        })
}

pub fn destination() -> Destination {
    Destination::parse(SHARED_RECOVERY_PATH).expect("fixed shared recovery path")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedCopyError {
    Source,
    Transport,
}

/// Writes only the contract's fixed destination through a fixed remote temporary name.
pub fn write_from(
    adb: &mut impl Adb,
    serial: &Serial,
    source: &Path,
) -> Result<(), SharedCopyError> {
    let source_ok = source.is_file()
        && source
            .metadata()
            .is_ok_and(|metadata| metadata.len() <= MAX_SHARED_COPY_BYTES as u64);
    if !source_ok {
        return Err(SharedCopyError::Source);
    }
    require_success(
        adb.execute(AdbCommand::PushSharedRecovery {
            serial: serial.clone(),
            source: source.into(),
        })
        .map_err(|_| SharedCopyError::Transport)?,
    )
    .map_err(|_| SharedCopyError::Transport)?;
    require_success(
        adb.execute(AdbCommand::Device {
            serial: serial.clone(),
            operation: DeviceOperation::CommitSharedRecovery,
        })
        .map_err(|_| SharedCopyError::Transport)?,
    )
    .map_err(|_| SharedCopyError::Transport)?;
    Ok(())
}
