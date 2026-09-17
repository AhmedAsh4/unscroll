//! Session-reconciliation and restore edge of the boundary.
//!
//! Restore requires the exact typed confirmation `RESTORE MY PHONE`;
//! anything else fails with `invalid-confirmation` and mutates nothing.

use crate::{
    recovery::model::RecoveryEnvelopeV1,
    transaction::{
        classify, is_maintenance_open, preview, ObservedState, RestoreError, Session,
        SessionAction, SessionKind, RestoreOutcome, RESTORE_CONFIRMATION,
    },
};

use super::{
    CommandError, ProgressEvent, CODE_INVALID_CONFIRMATION, CODE_RESTORE_BLOCKED, SESSION_KINDS,
};

/// Validate the typed restore confirmation.
pub fn validate_restore_confirmation(value: &str) -> Result<(), CommandError> {
    super::reject_command_payload(value)?;
    if value == RESTORE_CONFIRMATION {
        Ok(())
    } else {
        Err(CommandError::new(
            CODE_INVALID_CONFIRMATION,
            "The restore phrase was not typed exactly.",
            "Type RESTORE MY PHONE exactly to begin restoring.",
        ))
    }
}

/// Stable session-action names shared with the TypeScript side.
/// `allowed_action_names` indexes into this table so the two cannot drift.
pub const SESSION_ACTIONS: &[&str] = &[
    "resume",
    "rollback",
    "restore",
    "retry-cleanup",
    "export-diagnostics",
    "begin-setup",
];

/// Stable session-kind name shared with the TypeScript side.
pub fn session_kind_name(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::NewSetup => SESSION_KINDS[0],
        SessionKind::ActivePolicy => SESSION_KINDS[1],
        SessionKind::MaintenanceRecovery => SESSION_KINDS[2],
        SessionKind::ResumableTransaction => SESSION_KINDS[3],
        SessionKind::RollbackOnly => SESSION_KINDS[4],
        SessionKind::RestoreReady => SESSION_KINDS[5],
        SessionKind::CleanupRetry => SESSION_KINDS[6],
        SessionKind::BlockedInconsistency => SESSION_KINDS[7],
    }
}

/// Stable action names offered for a session kind.
pub fn allowed_action_names(kind: SessionKind) -> Vec<&'static str> {
    let actions: &[SessionAction] = match kind {
        SessionKind::NewSetup => &[SessionAction::BeginSetup],
        SessionKind::ActivePolicy => &[SessionAction::Restore, SessionAction::ExportDiagnostics],
        SessionKind::MaintenanceRecovery => &[SessionAction::ExportDiagnostics],
        SessionKind::ResumableTransaction => &[
            SessionAction::Resume,
            SessionAction::Rollback,
            SessionAction::ExportDiagnostics,
        ],
        SessionKind::RollbackOnly => &[SessionAction::Rollback, SessionAction::ExportDiagnostics],
        SessionKind::RestoreReady => &[SessionAction::Restore, SessionAction::ExportDiagnostics],
        SessionKind::CleanupRetry => &[SessionAction::RetryCleanup, SessionAction::ExportDiagnostics],
        SessionKind::BlockedInconsistency => &[SessionAction::ExportDiagnostics],
    };
    actions
        .iter()
        .map(|action| match action {
            SessionAction::Resume => SESSION_ACTIONS[0],
            SessionAction::Rollback => SESSION_ACTIONS[1],
            SessionAction::Restore => SESSION_ACTIONS[2],
            SessionAction::RetryCleanup => SESSION_ACTIONS[3],
            SessionAction::ExportDiagnostics => SESSION_ACTIONS[4],
            SessionAction::BeginSetup => SESSION_ACTIONS[5],
        })
        .collect()
}

pub fn map_restore_error(error: &RestoreError) -> CommandError {
    use super::CODE_RECOVERY_REQUIRED;
    match error {
        RestoreError::ConfirmationMismatch => CommandError::new(
            CODE_INVALID_CONFIRMATION,
            "The restore phrase was not typed exactly.",
            "Type RESTORE MY PHONE exactly to begin restoring.",
        ),
        RestoreError::MissingCopies => CommandError::new(
            CODE_RECOVERY_REQUIRED,
            "The phone holds no Unscroll recovery data.",
            "Inspect the phone first, or start a new setup.",
        ),
        _ => CommandError::new(
            CODE_RESTORE_BLOCKED,
            "Restore cannot proceed safely right now.",
            "Reconnect, reconcile the session, then retry restore.",
        ),
    }
}

/// Map a restore outcome to the progress-event tail handlers emit.
pub fn restore_progress(outcome: &RestoreOutcome) -> Vec<&'static str> {
    let events: &[ProgressEvent] = match outcome {
        RestoreOutcome::Complete => &[ProgressEvent::Completed],
        RestoreOutcome::ChooserRequired => &[ProgressEvent::ChooserRequired],
        RestoreOutcome::CleanupRetry => &[ProgressEvent::RestoreCompleted],
        RestoreOutcome::Blocked => &[ProgressEvent::InconsistentState],
        RestoreOutcome::RecoverableDisconnect => &[ProgressEvent::Disconnected],
    };
    events.iter().map(|event| event.name()).collect()
}

/// Classify reconciled mirror copies into the session the UI may offer.
///
/// Pure and unit-testable: mirror texts in, session out. Mirror-derivable
/// facts (maintenance state, pending id, journal failures, cleanup marks)
/// come from the agreed envelope; the two device-observed inputs arrive as
/// parameters so tests and handlers share one code path:
/// - `policy_traces`: whether the device shows Unscroll traces (used only
///   when both copies are missing; handlers resolve Unscroll HOME live).
/// - `explained`: whether device state is explained by the valid history.
///   Handlers pass true once mirrors agree, because every later mutation is
///   still verified per operation by the runner and restore re-verifies
///   before cleanup; a false value fails closed to `blocked-inconsistency`
///   (covered by test).
/// - `failed_required`: any failed journal entry conservatively forces the
///   rollback-only session. Optional failures already resolved inline by the
///   runner stay safe under rollback (inverses are verified), while a paused
///   required failure must never offer edit.
pub fn classify_mirrors(
    private: Option<&str>,
    shared: Option<&str>,
    serial: &str,
    fingerprint: &str,
    policy_traces: bool,
    explained: bool,
) -> Session {
    let working = private
        .and_then(|text| RecoveryEnvelopeV1::parse(text).ok())
        .or_else(|| shared.and_then(|text| RecoveryEnvelopeV1::parse(text).ok()));
    let baseline_hash = working.as_ref().map(|envelope| envelope.baseline_hash()).unwrap_or_default();
    let (maintenance_open, pending_id, failed_required, restore_verified, cleanup_remaining) =
        match &working {
            Some(envelope) => {
                let failures = preview(envelope, "", fingerprint).errors;
                let cleaned = envelope.has_applied_private_cleanup();
                (
                    is_maintenance_open(envelope),
                    envelope.pending_id(),
                    !failures.is_empty(),
                    cleaned,
                    cleaned,
                )
            }
            None => (false, None, false, false, false),
        };
    classify(
        private,
        shared,
        serial,
        fingerprint,
        &baseline_hash,
        &ObservedState {
            explained,
            maintenance_open,
            pending_id,
            failed_required,
            restore_verified,
            cleanup_remaining,
            policy_traces,
        },
    )
}
