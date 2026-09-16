mod adapter; mod journal; mod outcome; mod runner; mod verify;
pub use adapter::AdbTransaction; pub use outcome::{ApplyOutcome, ApplyResult, Decision}; pub use runner::{apply, ApplyError}; pub use verify::{ApplyDevice, DeviceFailure};

