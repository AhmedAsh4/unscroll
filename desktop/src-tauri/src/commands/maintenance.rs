//! Store-maintenance edge of the boundary.
//!
//! Opening requires the exact typed acknowledgment
//! `OPEN STORE MAINTENANCE`; anything else fails with
//! `invalid-confirmation` and mutates nothing.

use crate::{
    adb::PackageId,
    transaction::{CloseOutcome, MaintenanceError, OpenOutcome, MAINTENANCE_CONFIRMATION},
};

use super::{
    map_mirror_error, validate_package_list, CommandError, ProgressEvent,
    CODE_INVALID_CONFIRMATION, CODE_INVALID_PACKAGE_ID, CODE_MAINTENANCE_BLOCKED,
    CODE_STALE_DEVICE,
};

/// Validate the typed warning acknowledgment for opening maintenance.
pub fn validate_open_confirmation(value: &str) -> Result<(), CommandError> {
    super::reject_command_payload(value)?;
    if value == MAINTENANCE_CONFIRMATION {
        Ok(())
    } else {
        Err(CommandError::new(
            CODE_INVALID_CONFIRMATION,
            "The maintenance phrase was not typed exactly.",
            "Type OPEN STORE MAINTENANCE exactly to accept the warning.",
        ))
    }
}

/// Validate the rescan package list submitted when closing maintenance.
pub fn validate_scan(packages: &[String]) -> Result<Vec<PackageId>, CommandError> {
    validate_package_list(packages)
}

pub fn map_maintenance_error(error: &MaintenanceError) -> CommandError {
    match error {
        MaintenanceError::ConfirmationMismatch => CommandError::new(
            CODE_INVALID_CONFIRMATION,
            "The maintenance phrase was not typed exactly.",
            "Type OPEN STORE MAINTENANCE exactly to accept the warning.",
        ),
        MaintenanceError::UnknownPackage(_) => CommandError::new(
            CODE_INVALID_PACKAGE_ID,
            "A maintenance package is unknown on this phone.",
            "Reconnect and let maintenance rescan before retrying.",
        ),
        MaintenanceError::Stale(_) => CommandError::new(
            CODE_STALE_DEVICE,
            "Maintenance is based on outdated recovery data.",
            "Reconnect and reconcile the session before retrying.",
        ),
        MaintenanceError::Mirror(error) => map_mirror_error(error),
        _ => CommandError::new(
            CODE_MAINTENANCE_BLOCKED,
            "Store maintenance cannot proceed safely right now.",
            "Close maintenance before other actions, or reconnect and retry.",
        ),
    }
}

/// Map maintenance outcomes to the progress-event tails handlers emit.
pub fn open_progress(outcome: &OpenOutcome) -> Vec<&'static str> {
    let events: &[ProgressEvent] = match outcome {
        OpenOutcome::Opened => &[ProgressEvent::MaintenanceOpened, ProgressEvent::Completed],
        OpenOutcome::RecoverableDisconnect => &[ProgressEvent::Disconnected],
    };
    events.iter().map(|event| event.name()).collect()
}

pub fn close_progress(outcome: &CloseOutcome) -> Vec<&'static str> {
    let events: &[ProgressEvent] = match outcome {
        CloseOutcome::Closed => &[ProgressEvent::MaintenanceClosed, ProgressEvent::Completed],
        CloseOutcome::CloseFailed => &[ProgressEvent::InconsistentState],
        CloseOutcome::RecoverableDisconnect => &[ProgressEvent::Disconnected],
    };
    events.iter().map(|event| event.name()).collect()
}
