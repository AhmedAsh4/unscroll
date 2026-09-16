//! Reconnection session classification.
//!
//! Both envelope copies are reconciled before any action is offered, and the
//! valid history is compared with the observed device state. Classification is
//! pure: it never mutates the device or either copy. Repairing a stale copy
//! (persisting the returned envelope to both copies) is the caller's job.

use crate::recovery::{
    model::RecoveryEnvelopeV1,
    reconcile::{Outcome, reconcile_checked},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    NewSetup,
    ActivePolicy,
    MaintenanceRecovery,
    ResumableTransaction,
    RollbackOnly,
    RestoreReady,
    CleanupRetry,
    BlockedInconsistency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Resume,
    Rollback,
    Restore,
    RetryCleanup,
    ExportDiagnostics,
    BeginSetup,
}

/// Caller-observed device facts that classification combines with the journal.
///
/// `policy_traces` reports whether the device shows Unscroll policy traces
/// (suspended packages, Unscroll HOME, launcher presence) when both envelope
/// copies are missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedState {
    pub explained: bool,
    pub maintenance_open: bool,
    pub pending_id: Option<String>,
    pub failed_required: bool,
    pub restore_verified: bool,
    pub cleanup_remaining: bool,
    pub policy_traces: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub kind: SessionKind,
    pub envelope: Option<RecoveryEnvelopeV1>,
    pub guidance: String,
}

impl Session {
    /// Only actions proven safe by the journal and observed state.
    pub fn allowed_actions(&self) -> Vec<SessionAction> {
        match self.kind {
            SessionKind::NewSetup => vec![SessionAction::BeginSetup],
            SessionKind::ActivePolicy => vec![SessionAction::Restore, SessionAction::ExportDiagnostics],
            SessionKind::MaintenanceRecovery => vec![SessionAction::ExportDiagnostics],
            SessionKind::ResumableTransaction => {
                vec![SessionAction::Resume, SessionAction::Rollback, SessionAction::ExportDiagnostics]
            }
            SessionKind::RollbackOnly => vec![SessionAction::Rollback, SessionAction::ExportDiagnostics],
            SessionKind::RestoreReady => vec![SessionAction::Restore, SessionAction::ExportDiagnostics],
            SessionKind::CleanupRetry => vec![SessionAction::RetryCleanup, SessionAction::ExportDiagnostics],
            SessionKind::BlockedInconsistency => vec![SessionAction::ExportDiagnostics],
        }
    }
}

pub fn classify(
    private: Option<&str>,
    shared: Option<&str>,
    serial: &str,
    fingerprint: &str,
    baseline_hash: &str,
    state: &ObservedState,
) -> Session {
    let outcome = reconcile_checked(private, shared, serial, fingerprint, baseline_hash, state.explained);
    let repaired = match &outcome {
        Outcome::RepairPrivate(canonical) | Outcome::RepairShared(canonical) => Some(canonical.clone()),
        _ => None,
    };
    match outcome {
        Outcome::BothMissing if !state.policy_traces => Session {
            kind: SessionKind::NewSetup,
            envelope: None,
            guidance: "No Unscroll recovery data was found on this device. You may start a new setup. No changes were made.".into(),
        },
        Outcome::BothMissing => blocked(None, "No recovery copies exist, but the device shows Unscroll policy traces. Automatic restoration cannot be proven safe. Use manual ADB inspection or a factory reset as described in the Unscroll recovery guide."),
        Outcome::UnexplainedDeviceState => blocked(
            valid_copy(private, shared),
            "The phone's actual state is not explained by either recovery copy, so no action can be proven safe.",
        ),
        Outcome::Corruption => blocked(
            valid_copy(private, shared),
            "A recovery copy failed checksum or schema validation, so no action can be proven safe. Export diagnostics, then repair from the valid copy before retrying.",
        ),
        Outcome::Fork => blocked(
            valid_copy(private, shared),
            "The two recovery copies contain forked histories that cannot be merged automatically.",
        ),
        Outcome::BaselineMismatch => blocked(
            valid_copy(private, shared),
            "A recovery copy does not match the expected baseline, so it must not be trusted for mutations.",
        ),
        Outcome::DeviceBindingMismatch => blocked(
            valid_copy(private, shared),
            "A recovery copy is bound to a different device, so it must not be trusted for mutations.",
        ),
        Outcome::Consistent | Outcome::RepairPrivate(_) | Outcome::RepairShared(_) | Outcome::MissingShared => {
            let raw = repaired.as_deref().or(private).or(shared);
            let envelope = raw.and_then(|text| RecoveryEnvelopeV1::parse(text).ok());
            let Some(envelope) = envelope else {
                return blocked(None, "A recovery copy passed reconciliation but cannot be parsed.");
            };
            let repair_note = if repaired.is_some() || matches!(outcome, Outcome::MissingShared) {
                " One recovery copy is stale or missing; persist this envelope to both copies and re-check before editing or restoring."
            } else {
                ""
            };
            if state.maintenance_open {
                return Session {
                    kind: SessionKind::MaintenanceRecovery,
                    envelope: Some(envelope),
                    guidance: format!(
                        "Store maintenance was left open. Close maintenance before editing, restoring, or opening another window. Only diagnostic export is available until maintenance is closed.{repair_note}"
                    ),
                };
            }
            if state.cleanup_remaining {
                if state.restore_verified {
                    return Session {
                        kind: SessionKind::CleanupRetry,
                        envelope: Some(envelope),
                        guidance: format!(
                            "Restore is verified but final cleanup did not finish. Call retry_cleanup to remove the remaining copies; do not call restore again (it refuses verified envelopes). Restored settings will not be repeated.{repair_note}"
                        ),
                    };
                }
                return blocked(
                    Some(envelope),
                    "Cleanup remains but restoration was not verified, so the remaining data must be kept.",
                );
            }
            if state.failed_required {
                return Session {
                    kind: SessionKind::RollbackOnly,
                    envelope: Some(envelope),
                    guidance: format!(
                        "A required operation failed. Only rolling back the verified changes is safe.{repair_note}"
                    ),
                };
            }
            match (envelope.pending_id(), &state.pending_id) {
                (Some(actual), Some(expected)) if actual == *expected => {
                    return Session {
                        kind: SessionKind::ResumableTransaction,
                        envelope: Some(envelope),
                        guidance: format!(
                            "An Unscroll transaction was interrupted. You may resume it or roll back the verified changes.{repair_note}"
                        ),
                    };
                }
                (None, None) => (),
                _ => {
                    return blocked(
                        Some(envelope),
                        "The journal pending entry does not match the observed device state.",
                    );
                }
            }
            if state.restore_verified {
                return Session {
                    kind: SessionKind::RestoreReady,
                    envelope: Some(envelope),
                    guidance: format!(
                        "The device state is verified against the recovery history. Type RESTORE MY PHONE to restore.{repair_note}"
                    ),
                };
            }
            Session {
                kind: SessionKind::ActivePolicy,
                envelope: Some(envelope),
                guidance: format!(
                    "An active Unscroll policy was found and verified. You may edit the allowlist, open store maintenance, restore the phone, or export diagnostics.{repair_note}"
                ),
            }
        }
    }
}

fn blocked(envelope: Option<RecoveryEnvelopeV1>, guidance: &str) -> Session {
    Session {
        kind: SessionKind::BlockedInconsistency,
        envelope,
        guidance: format!("{guidance} No automatic changes were made."),
    }
}

fn valid_copy(private: Option<&str>, shared: Option<&str>) -> Option<RecoveryEnvelopeV1> {
    private
        .and_then(|text| RecoveryEnvelopeV1::parse(text).ok())
        .or_else(|| shared.and_then(|text| RecoveryEnvelopeV1::parse(text).ok()))
}
