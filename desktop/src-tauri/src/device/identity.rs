use crate::adb::Serial;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub serial: Serial,
    pub fingerprint: String,
    pub user_id: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiSupport {
    Supported(u32),
    UnverifiedNewer(u32),
    Unsupported(u32),
}
pub fn classify_api(api: u32) -> ApiSupport {
    match api {
        24..=36 => ApiSupport::Supported(api),
        37.. => ApiSupport::UnverifiedNewer(api),
        _ => ApiSupport::Unsupported(api),
    }
}
