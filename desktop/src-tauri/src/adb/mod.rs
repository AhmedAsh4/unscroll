mod command;
mod parser;
mod process;

pub use command::{
    AdbCommand, AppOp, AppOpMode, BridgeOperation, Component, Destination, DeviceOperation,
    Fingerprint, PackageId, Property, Serial, StreamId, UserId, ValidationError,
};
pub use parser::{parse_devices, DeviceRecord, DeviceStatus};
pub use process::{
    redacted_diagnostic, require_success, AdbError, AdbOutput, AdbResponse, BundledAdb, Diagnostic,
    FakeAdb,
};

pub trait Adb {
    fn execute(&mut self, command: AdbCommand) -> Result<AdbOutput, AdbError>;
}

pub fn stop_server(adb: &mut impl Adb) -> Result<AdbOutput, AdbError> {
    require_success(adb.execute(AdbCommand::StopServer)?)
}
