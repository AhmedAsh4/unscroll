//! Policy apply/edit edge of the boundary.
//!
//! The frontend submits only an allowlist of package-id strings plus a bound
//! device identity. It never submits operation plans: there is no parameter
//! for `DeviceOperation`, `AdbCommand`, or `Plan` anywhere in this module.
//! Cancellation arrives as a domain [`Decision`], never as task abandonment.

use crate::{
    adb::PackageId,
    app_state::UnscrollState,
    policy::PlanError,
    transaction::{ApplyOutcome, Decision, EditError, EditOutcome},
};

use super::{
    map_mirror_error, validate_package_list, CommandError, ProgressEvent, CODE_INVALID_DECISION,
    CODE_INVALID_PACKAGE_ID, CODE_PLAN_REJECTED, CODE_STALE_DEVICE,
};

/// Validate an apply/edit request: payload-shaped strings rejected, package
/// ids typed, device binding verified against the inspected session.
pub fn validate_apply_request(
    state: &UnscrollState,
    serial: &str,
    fingerprint: &str,
    allowed: &[String],
) -> Result<Vec<PackageId>, CommandError> {
    super::device::check_session(state, serial, fingerprint)?;
    validate_package_list(allowed)
}

/// Parse a UI decision string into the domain decision. Unknown strings and
/// anything command-shaped fail with `invalid-decision`.
pub fn validate_decision(value: &str) -> Result<Decision, CommandError> {
    super::reject_command_payload(value)?;
    match value {
        "continue" => Ok(Decision::Continue),
        "rollback" => Ok(Decision::Rollback),
        "home-confirmed" => Ok(Decision::HomeConfirmed),
        "home-cancelled" => Ok(Decision::HomeCancelled),
        _ => Err(CommandError::new(
            CODE_INVALID_DECISION,
            "That answer is not one of the offered choices.",
            "Choose Continue, Roll back, or answer the launcher question again.",
        )),
    }
}

/// True for decisions that cancel the current direction through the domain
/// runner (the only supported cancellation path).
pub fn is_cancellation(decision: Decision) -> bool {
    matches!(decision, Decision::Rollback | Decision::HomeCancelled)
}

pub fn map_plan_error(error: &PlanError) -> CommandError {
    let detail = match error {
        PlanError::UnsupportedDevice => "the phone cannot support this policy",
        PlanError::UnknownPackage => "an app is unknown on this phone",
        PlanError::InconsistentFacts => "the phone facts disagree with the request",
        PlanError::InvalidRecordedOperation => "a recorded operation is invalid",
    };
    CommandError::new(
        CODE_PLAN_REJECTED,
        format!("That selection cannot become a safe plan: {detail}."),
        "Review the app list after reconnecting, then try again.",
    )
}

/// Map an edit outcome to the progress-event tail handlers emit.
pub fn edit_progress(outcome: &EditOutcome) -> Vec<&'static str> {
    let events: &[ProgressEvent] = match outcome {
        EditOutcome::Complete => &[ProgressEvent::Completed],
        EditOutcome::Blocked => &[ProgressEvent::InconsistentState],
        EditOutcome::RecoverableDisconnect => &[ProgressEvent::Disconnected],
    };
    events.iter().map(|event| event.name()).collect()
}

pub fn map_edit_error(error: &EditError) -> CommandError {
    match error {
        EditError::UnknownPackage(_) => CommandError::new(
            CODE_INVALID_PACKAGE_ID,
            "An app in the edited list is unknown on this phone.",
            "Pick apps from the on-screen list after reconnecting.",
        ),
        EditError::Mirror(error) => map_mirror_error(error),
        EditError::Journal => CommandError::new(
            CODE_PLAN_REJECTED,
            "The edited list does not fit the recorded history.",
            "Reconnect and reconcile the session before retrying.",
        ),
        EditError::Blocked(reason) if reason.contains("stale") => CommandError::new(
            CODE_STALE_DEVICE,
            "The edited list is based on outdated recovery data.",
            "Reconnect and reconcile the session before retrying.",
        ),
        EditError::Blocked(_) => CommandError::new(
            CODE_PLAN_REJECTED,
            "The edit cannot proceed safely right now.",
            "Reconnect and reconcile the session before retrying.",
        ),
    }
}
/// Map a runner outcome to the progress-event tail handlers emit. Every name
/// belongs to [`ProgressEvent::name`] so ordering stays journal-ordered.
pub fn apply_progress(outcome: &ApplyOutcome) -> Vec<&'static str> {
    let events: &[ProgressEvent] = match outcome {
        ApplyOutcome::Complete => &[ProgressEvent::Completed],
        ApplyOutcome::DecisionRequired { .. } => &[ProgressEvent::DecisionRequired],
        ApplyOutcome::ChooserRequired => &[ProgressEvent::ChooserRequired],
        ApplyOutcome::RolledBack => &[
            ProgressEvent::RollbackStarted,
            ProgressEvent::RollbackApplied,
            ProgressEvent::Completed,
        ],
        ApplyOutcome::RecoverableDisconnect => &[ProgressEvent::Disconnected],
        ApplyOutcome::InconsistentState => &[ProgressEvent::InconsistentState],
    };
    events.iter().map(|event| event.name()).collect()
}
