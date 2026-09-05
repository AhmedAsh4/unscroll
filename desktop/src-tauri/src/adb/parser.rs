use super::{AdbError, Serial};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceStatus {
    Device,
    Unauthorized,
    Offline,
    Reconnecting,
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    pub serial: Serial,
    pub status: DeviceStatus,
    pub transport_id: Option<u32>,
}

pub fn parse_devices(output: &str) -> Result<Vec<DeviceRecord>, AdbError> {
    let normalized = output.replace('\r', "");
    let Some((_, body)) = normalized.split_once("List of devices attached\n") else {
        return Err(AdbError::UnexpectedOutput);
    };
    body.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split_whitespace();
            let serial_text = fields.next().ok_or(AdbError::UnexpectedOutput)?;
            if network_serial(serial_text) {
                return Err(AdbError::NetworkTransport);
            }
            let serial = Serial::parse(serial_text).map_err(|_| AdbError::UnexpectedOutput)?;
            let status = match fields.next().ok_or(AdbError::UnexpectedOutput)? {
                "device" => DeviceStatus::Device,
                "unauthorized" => DeviceStatus::Unauthorized,
                "offline" => DeviceStatus::Offline,
                "reconnecting" => DeviceStatus::Reconnecting,
                state => DeviceStatus::Unknown(state.into()),
            };
            let mut transport_id = None;
            for field in fields {
                if let Some(value) = field.strip_prefix("transport_id:") {
                    transport_id = Some(value.parse().map_err(|_| AdbError::UnexpectedOutput)?);
                }
            }
            Ok(DeviceRecord {
                serial,
                status,
                transport_id,
            })
        })
        .collect()
}

fn network_serial(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains(':')
        || value.ends_with("._adb-tls-connect._tcp")
        || value.ends_with("._adb-tls-pairing._tcp")
        || value.ends_with("._adb._tcp")
}
