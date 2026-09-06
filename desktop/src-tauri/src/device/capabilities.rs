#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapabilityReport {
    pub package_suspension: bool,
    pub bridge: bool,
    pub recovery_storage: bool,
    pub app_op_inspection: bool,
    pub home_path: bool,
}

impl CapabilityReport {
    pub fn ready(&self) -> bool {
        self.package_suspension
            && self.bridge
            && self.recovery_storage
            && self.app_op_inspection
            && self.home_path
    }
}
