use crate::adb::{AppOp, AppOpMode, Component, PackageId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement { Required, UserResolvable, Optional }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    LauncherPolicy { allowed: Vec<PackageId> },
    Suspend { package: PackageId, user: UserId, suspended: bool },
    AppOp { package: PackageId, user: UserId, app_op: AppOp, mode: AppOpMode },
    Home { component: Component },
    Maintenance { open: bool },
    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedOperation {
    pub operation: Operation,
    pub precondition: String,
    pub postcondition: String,
    pub inverse: Operation,
    pub requirement: Requirement,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan { pub operations: Vec<PlannedOperation> }

impl Plan {
    pub(crate) fn push(&mut self, operation: Operation, precondition: impl Into<String>, postcondition: impl Into<String>, inverse: Operation, requirement: Requirement, description: impl Into<String>) {
        self.operations.push(PlannedOperation { operation, precondition: precondition.into(), postcondition: postcondition.into(), inverse, requirement, description: description.into() });
    }
}
