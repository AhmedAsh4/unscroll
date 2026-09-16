use crate::adb::PackageId;
use crate::recovery::model::RecoveryEnvelopeV1;
#[derive(Debug, Clone, PartialEq, Eq)] pub enum ApplyOutcome { Complete, DecisionRequired { package: PackageId }, ChooserRequired, RolledBack, RecoverableDisconnect, InconsistentState }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ApplyResult { pub outcome: ApplyOutcome, pub envelope: RecoveryEnvelopeV1, pub partial_protection: Vec<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Decision { Continue, Rollback, HomeConfirmed, HomeCancelled }

