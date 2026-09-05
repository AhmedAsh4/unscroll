use crate::adb::AdbError;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    NoDevice,
    MultipleDevices,
    Unauthorized,
    Offline,
    Reconnecting,
    NetworkTransport,
    MissingDriver,
    UnsupportedApi,
    Timeout,
    Disconnected,
    Transport,
    UnexpectedOutput,
}
impl From<AdbError> for DiscoveryError {
    fn from(value: AdbError) -> Self {
        match value {
            AdbError::Timeout(_) => Self::Timeout,
            AdbError::OutputOverflow(_)
            | AdbError::UnexpectedOutput
            | AdbError::InconsistentState
            | AdbError::LaunchFailed => Self::UnexpectedOutput,
            AdbError::NetworkTransport => Self::NetworkTransport,
            AdbError::NonZero(output) => classify_nonzero(output.stderr()),
        }
    }
}
pub fn classify_nonzero(stderr: &str) -> DiscoveryError {
    let output = stderr.to_ascii_lowercase();
    if output.contains("device disconnected") {
        DiscoveryError::Disconnected
    } else {
        classify_transport(&output).unwrap_or(DiscoveryError::Transport)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapError {
    MiuiUsbInstallRestricted,
    SecurityException,
    Other,
}
pub fn classify_bootstrap(output: &str) -> BootstrapError {
    if output.contains("INSTALL_FAILED_USER_RESTRICTED") {
        BootstrapError::MiuiUsbInstallRestricted
    } else if output.contains("SecurityException") {
        BootstrapError::SecurityException
    } else {
        BootstrapError::Other
    }
}
pub fn classify_transport(output: &str) -> Option<DiscoveryError> {
    let output = output.to_ascii_lowercase();
    (output.contains("driver missing")
        || output.contains("driver not installed")
        || output.contains("no permissions"))
    .then_some(DiscoveryError::MissingDriver)
}
