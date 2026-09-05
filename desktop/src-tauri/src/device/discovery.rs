use super::DiscoveryError;
use crate::adb::{parse_devices, require_success, Adb, AdbCommand, DeviceStatus};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    One { serial: crate::adb::Serial },
}
pub fn discover(adb: &mut impl Adb) -> Result<Discovery, DiscoveryError> {
    require_success(adb.execute(AdbCommand::StartServer)?)?;
    let output = require_success(adb.execute(AdbCommand::Devices)?)?;
    let devices = parse_devices(output.stdout())?;
    if devices.is_empty() {
        return Err(DiscoveryError::NoDevice);
    }
    if devices
        .iter()
        .any(|device| matches!(device.status, DeviceStatus::Unauthorized))
    {
        return Err(DiscoveryError::Unauthorized);
    }
    if devices
        .iter()
        .any(|device| matches!(device.status, DeviceStatus::Offline))
    {
        return Err(DiscoveryError::Offline);
    }
    if devices
        .iter()
        .any(|device| matches!(device.status, DeviceStatus::Reconnecting))
    {
        return Err(DiscoveryError::Reconnecting);
    }
    if devices
        .iter()
        .any(|device| matches!(device.status, DeviceStatus::Unknown(_)))
    {
        return Err(DiscoveryError::UnexpectedOutput);
    }
    if devices.len() != 1 {
        return Err(DiscoveryError::MultipleDevices);
    }
    Ok(Discovery::One {
        serial: devices.into_iter().next().expect("one device").serial,
    })
}
