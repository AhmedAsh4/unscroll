//! Narrow Tauri boundary between Svelte and the Rust services.
//!
//! Rules (see Task 15 invariants):
//!
//! 1. Commands are thin: validate UI strings with the existing typed parsers
//!    (`Serial`/`PackageId`/`Fingerprint`/`Destination`), call the
//!    responsible service, map outcomes to [`CommandError`] DTOs, and report
//!    progress through [`ProgressEvent`] names. No business logic lives here.
//! 2. No command accepts raw ADB arguments, shell strings, `DeviceOperation`
//!    / `AdbCommand` JSON, or caller-authored operation plans. There is
//!    deliberately no parameter of those types anywhere in this module, and
//!    [`reject_command_payload`] fails any string shaped like a shell
//!    fragment before typed parsing runs.
//! 3. Every active operation binds inspected serial + fingerprint
//!    (`app_state::UnscrollState`); mismatches fail with `stale-device`.
//! 4. One mutating transaction at a time via the session mutex; the second
//!    caller gets `busy-transaction`. Cancellation is a domain `Decision`
//!    (`policy::validate_decision`), never task abandonment.
//! 5. Icons travel as bounded bytes and are released on catalog change
//!    (`UnscrollState::clear_icons` via `device::note_inspection`).
//! 6. Filesystem access is limited to explicit user-selected diagnostic
//!    export destinations (`diagnostics::validate_export_path`) plus
//!    packaged resources. No shell/process/arbitrary-FS surface exists here,
//!    and the capability file grants none to the webview.
//! 7. Errors are stable codes plus a human message and recovery action; raw
//!    commands and unredacted device data never cross this boundary.
//! 8. The frontend calls only through `desktop/src/lib/api/invoke.ts` using
//!    the command names below; it never constructs device commands.
//!
//! ## Tauri command-name contract (wired by the UI tasks)
//!
//! `discover_devices`, `inspect_device`, `get_session`, `start_apply`,
//! `respond_to_decision`, `start_edit`, `open_maintenance`,
//! `close_maintenance`, `start_restore`, `retry_cleanup`,
//! `preview_diagnostics`, `export_diagnostics`, `load_app_icon`. Each handler validates with
//! the helpers in this module, gates on the session, delegates to
//! `device` / `transaction` / `policy` / `recovery`, and emits
//! [`ProgressEvent::name`] events in [`progress_sequence`] order.

pub mod device;
pub mod diagnostics;
pub mod handlers;
pub mod maintenance;
pub mod policy;
pub mod recovery;

use crate::{
    adb::{Destination, Fingerprint, PackageId, Serial},
    recovery::mirror::MirrorError,
};

/// Stable, user-actionable error codes. `error-codes.json` (shared with the
/// TypeScript side) must contain every entry of [`ERROR_CODES`].
pub const CODE_INVALID_PACKAGE_ID: &str = "invalid-package-id";
pub const CODE_INVALID_SERIAL: &str = "invalid-serial";
pub const CODE_INVALID_FINGERPRINT: &str = "invalid-fingerprint";
pub const CODE_INVALID_DESTINATION: &str = "invalid-destination";
pub const CODE_STALE_DEVICE: &str = "stale-device";
pub const CODE_BUSY_TRANSACTION: &str = "busy-transaction";
pub const CODE_INVALID_CONFIRMATION: &str = "invalid-confirmation";
pub const CODE_INVALID_DECISION: &str = "invalid-decision";
pub const CODE_EXPORT_PATH_REQUIRED: &str = "export-path-required";
pub const CODE_REJECTED_COMMAND_PAYLOAD: &str = "rejected-command-payload";
pub const CODE_NO_DEVICE: &str = "no-device";
pub const CODE_MULTIPLE_DEVICES: &str = "multiple-devices";
pub const CODE_UNAUTHORIZED_DEVICE: &str = "unauthorized-device";
pub const CODE_DEVICE_UNAVAILABLE: &str = "device-unavailable";
pub const CODE_MISSING_DRIVER: &str = "missing-driver";
pub const CODE_UNSUPPORTED_DEVICE: &str = "unsupported-device";
pub const CODE_PREFLIGHT_FAILED: &str = "preflight-failed";
pub const CODE_RECOVERY_REQUIRED: &str = "recovery-required";
pub const CODE_INCOMPATIBLE_LAUNCHER: &str = "incompatible-launcher";
pub const CODE_ICON_TOO_LARGE: &str = "icon-too-large";
pub const CODE_ICON_CACHE_FULL: &str = "icon-cache-full";
pub const CODE_PLAN_REJECTED: &str = "plan-rejected";
pub const CODE_MAINTENANCE_BLOCKED: &str = "maintenance-blocked";
pub const CODE_RESTORE_BLOCKED: &str = "restore-blocked";
pub const CODE_EXPORT_FAILED: &str = "export-failed";

/// Every stable code, in one place for fixture-agreement tests.
pub const ERROR_CODES: &[&str] = &[
    CODE_INVALID_PACKAGE_ID,
    CODE_INVALID_SERIAL,
    CODE_INVALID_FINGERPRINT,
    CODE_INVALID_DESTINATION,
    CODE_STALE_DEVICE,
    CODE_BUSY_TRANSACTION,
    CODE_INVALID_CONFIRMATION,
    CODE_INVALID_DECISION,
    CODE_EXPORT_PATH_REQUIRED,
    CODE_REJECTED_COMMAND_PAYLOAD,
    CODE_NO_DEVICE,
    CODE_MULTIPLE_DEVICES,
    CODE_UNAUTHORIZED_DEVICE,
    CODE_DEVICE_UNAVAILABLE,
    CODE_MISSING_DRIVER,
    CODE_UNSUPPORTED_DEVICE,
    CODE_PREFLIGHT_FAILED,
    CODE_RECOVERY_REQUIRED,
    CODE_INCOMPATIBLE_LAUNCHER,
    CODE_ICON_TOO_LARGE,
    CODE_ICON_CACHE_FULL,
    CODE_PLAN_REJECTED,
    CODE_MAINTENANCE_BLOCKED,
    CODE_RESTORE_BLOCKED,
    CODE_EXPORT_FAILED,
];

/// Error DTO crossing to the UI: a stable code, a human message, and a
/// recovery action. Raw commands, payloads, and unredacted device data are
/// never included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
    pub action: String,
}

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>, action: impl Into<String>) -> Self {
        Self { code, message: message.into(), action: action.into() }
    }

    pub fn stale_device() -> Self {
        Self::new(
            CODE_STALE_DEVICE,
            "This phone is not the one that was inspected.",
            "Reconnect the inspected phone, or start a new inspection.",
        )
    }

    pub fn busy() -> Self {
        Self::new(
            CODE_BUSY_TRANSACTION,
            "Another phone operation is still running.",
            "Wait for it to finish. To stop it, answer its on-screen decision instead of disconnecting.",
        )
    }

    pub fn icon_too_large() -> Self {
        Self::new(
            CODE_ICON_TOO_LARGE,
            "An app icon exceeded the display bound.",
            "Reconnect the phone to reload the app list.",
        )
    }

    pub fn icon_cache_full() -> Self {
        Self::new(
            CODE_ICON_CACHE_FULL,
            "The app list holds more icons than the display cache keeps.",
            "Reconnect the phone to reload the app list.",
        )
    }

    /// Minimal JSON encoding of this DTO for the Tauri string transport.
    /// Handlers return `Result<String, String>` so no serialization
    /// dependency crosses the boundary; this escaping keeps quotes,
    /// backslashes, and control characters intact.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"code\":{},\"message\":{},\"action\":{}}}",
            json_quote(self.code),
            json_quote(&self.message),
            json_quote(&self.action)
        )
    }
}

/// JSON string quoting for hand-built DTO payloads (see `to_json`).
pub fn json_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Reject shell-shaped input before typed parsing. There is no legitimate
/// serial, package id, fingerprint, destination, confirmation, or path that
/// contains `;`, `&&`, `|`, `$(`, backticks, or newlines, or that starts
/// with `-` (an option-injection attempt).
pub fn reject_command_payload(value: &str) -> Result<(), CommandError> {
    let shaped = value.contains(';')
        || value.contains("&&")
        || value.contains('|')
        || value.contains("$(")
        || value.contains('`')
        || value.contains('\n')
        || value.contains('\r')
        || value.trim_start().starts_with('-');
    if shaped {
        return Err(CommandError::new(
            CODE_REJECTED_COMMAND_PAYLOAD,
            "That value looks like a command and was rejected.",
            "Retype the value, or pick it from the on-screen list.",
        ));
    }
    Ok(())
}

pub fn validate_serial(value: &str) -> Result<Serial, CommandError> {
    reject_command_payload(value)?;
    Serial::parse(value).map_err(|_| {
        CommandError::new(
            CODE_INVALID_SERIAL,
            "That device serial is not valid.",
            "Reconnect the phone and inspect it again.",
        )
    })
}

pub fn validate_package(value: &str) -> Result<PackageId, CommandError> {
    reject_command_payload(value)?;
    PackageId::parse(value).map_err(|_| {
        CommandError::new(
            CODE_INVALID_PACKAGE_ID,
            "That app identifier is not valid.",
            "Pick the app from the on-screen list instead of typing it.",
        )
    })
}

pub fn validate_package_list(values: &[String]) -> Result<Vec<PackageId>, CommandError> {
    values.iter().map(|value| validate_package(value)).collect()
}

pub fn validate_fingerprint(value: &str) -> Result<Fingerprint, CommandError> {
    reject_command_payload(value)?;
    Fingerprint::parse(value).map_err(|_| {
        CommandError::new(
            CODE_INVALID_FINGERPRINT,
            "That device fingerprint is not valid.",
            "Reconnect the phone and inspect it again.",
        )
    })
}

pub fn validate_destination(value: &str) -> Result<Destination, CommandError> {
    reject_command_payload(value)?;
    Destination::parse(value).map_err(|_| {
        CommandError::new(
            CODE_INVALID_DESTINATION,
            "That on-device destination is not allowed.",
            "Use the suggested Unscroll folder on the phone.",
        )
    })
}

/// Map mirror-layer failures to stable codes. Transport/readback problems
/// mean the phone or cable dropped; baseline problems mean the request does
/// not fit the recorded history; deletion problems surface during restore
/// cleanup only.
pub fn map_mirror_error(error: &MirrorError) -> CommandError {
    match error {
        MirrorError::Baseline => CommandError::new(
            CODE_PLAN_REJECTED,
            "The request does not fit the recorded recovery baseline.",
            "Reconnect and reconcile the session before retrying.",
        ),
        MirrorError::NotRestored | MirrorError::PrivateDelete | MirrorError::SharedDelete => {
            CommandError::new(
                CODE_RESTORE_BLOCKED,
                "Restore cleanup cannot proceed safely right now.",
                "Reconnect, reconcile the session, then retry cleanup.",
            )
        }
        _ => CommandError::new(
            CODE_DEVICE_UNAVAILABLE,
            "The phone stopped responding while saving recovery data.",
            "Check the USB cable, then try again.",
        ),
    }
}
/// Typed progress events. Names are stable across Rust and TypeScript
/// (`progress-events.json`) and are emitted in journal order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressEvent {
    Started,
    OperationApplied,
    DecisionRequired,
    ChooserRequired,
    RollbackStarted,
    RollbackApplied,
    Completed,
    Disconnected,
    MaintenanceOpened,
    MaintenanceClosed,
    RestoreStarted,
    RestoreCompleted,
    InconsistentState,
}

impl ProgressEvent {
    pub fn name(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::OperationApplied => "operation-applied",
            Self::DecisionRequired => "decision-required",
            Self::ChooserRequired => "chooser-required",
            Self::RollbackStarted => "rollback-started",
            Self::RollbackApplied => "rollback-applied",
            Self::Completed => "completed",
            Self::Disconnected => "disconnected",
            Self::MaintenanceOpened => "maintenance-opened",
            Self::MaintenanceClosed => "maintenance-closed",
            Self::RestoreStarted => "restore-started",
            Self::RestoreCompleted => "restore-completed",
            Self::InconsistentState => "inconsistent-state",
        }
    }
}

/// Every stable progress-event name, for fixture-agreement tests.
pub const PROGRESS_EVENTS: &[&str] = &[
    "started",
    "operation-applied",
    "decision-required",
    "chooser-required",
    "rollback-started",
    "rollback-applied",
    "completed",
    "disconnected",
    "maintenance-opened",
    "maintenance-closed",
    "restore-started",
    "restore-completed",
    "inconsistent-state",
];

/// Canonical event ordering per scenario, matching journal ordering: pending
/// entries are applied in order, pauses/rollbacks/disconnects interrupt the
/// same sequence, and maintenance/restore have their own windows. Used by
/// command handlers when emitting progress and asserted by boundary tests.
pub fn progress_sequence(scenario: &str) -> Vec<&'static str> {
    use ProgressEvent as E;
    let events: &[ProgressEvent] = match scenario {
        "success" => &[E::Started, E::OperationApplied, E::OperationApplied, E::Completed],
        "pause" => &[E::Started, E::OperationApplied, E::DecisionRequired],
        "resume" => &[E::DecisionRequired, E::OperationApplied, E::Completed],
        "rollback" => &[E::Started, E::OperationApplied, E::RollbackStarted, E::RollbackApplied, E::Completed],
        "disconnect" => &[E::Started, E::OperationApplied, E::Disconnected],
        "maintenance" => &[E::MaintenanceOpened, E::MaintenanceClosed, E::Completed],
        "restore" => &[E::RestoreStarted, E::RestoreCompleted],
        _ => &[],
    };
    events.iter().map(|event| event.name()).collect()
}

/// Stable session-kind names shared with the TypeScript side
/// (`session-kinds.json`).
pub const SESSION_KINDS: &[&str] = &[
    "new-setup",
    "active-policy",
    "maintenance-recovery",
    "resumable-transaction",
    "rollback-only",
    "restore-ready",
    "cleanup-retry",
    "blocked-inconsistency",
];
