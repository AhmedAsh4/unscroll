use crate::policy::Operation;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DeviceFailure { Command, Disconnect, Inconsistent }
pub trait ApplyDevice { fn mutate(&mut self, operation: &Operation) -> Result<(), DeviceFailure>; fn verified(&mut self, operation: &Operation) -> Result<bool, DeviceFailure>; fn chooser(&mut self) -> Result<(), DeviceFailure> { Ok(()) } }

