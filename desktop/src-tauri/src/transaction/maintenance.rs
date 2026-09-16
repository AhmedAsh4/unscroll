//! Bounded store maintenance with deliberate friction and forced close.
//!
//! Ordinary ADB cannot reliably allow store updates while forbidding new
//! installations, so V1 suspends detected stores during normal operation and
//! exposes an explicit maintenance window instead.
//!
//! Opening requires the typed acknowledgment [`MAINTENANCE_CONFIRMATION`]
//! (`"OPEN STORE MAINTENANCE"`). A mismatch returns
//! [`MaintenanceError::ConfirmationMismatch`] without touching the device or
//! either mirror. The maintenance-open record (`Operation::Maintenance` with
//! `open:true`) is journaled as pending, mirrored to both copies with
//! readback, verified, then marked applied and mirrored again — strictly
//! BEFORE any recorded store or install-source state is temporarily restored.
//! While open, [`maintenance_state`] reports `Open { prevents_exit: true }`
//! and [`allows_new_installs`] is true: the desktop must prevent ordinary
//! exit and must state that new installations are possible.
//!
//! Closing rescans installed packages from a caller-supplied scan list. Any
//! newly present launchable package not in the recorded universe (baseline
//! packages plus active allowlist plus journaled packages — never a version
//! comparison, so updates and installs are indistinguishable) is appended to
//! the journal BEFORE suspension. Caller-approved additions extend the
//! LauncherPolicy allowlist; every other new package — including new stores
//! even when approved, since stores can never be allowed — is suspended with
//! Required hard-gate semantics. Recorded store suspensions and install-source
//! app-ops are then reapplied, the active policy is verified, and only then
//! is maintenance-close (`open:false`) journaled. A store failure yields
//! `CloseFailed` with the maintenance-open state retained (close is never
//! journaled). Any disconnect at any boundary yields `RecoverableDisconnect`
//! without speculative cleanup.
//!
//! A reconnect that observes maintenance-open must close before any other
//! action: [`requires_forced_close`] reports the gate and
//! [`ensure_no_forced_close`] refuses edit/restore/open while it holds.
//! [`try_exit`] refuses ordinary exit while open.

use crate::{
    adb::{AppOp, AppOpMode, PackageId, UserId},
    policy::Operation,
    recovery::{
        mirror::{MirrorError, MirrorStore},
        model::RecoveryEnvelopeV1,
    },
};
use super::{
    diagnostics::{ParsedEnvelope, ParsedOp, parse_envelope},
    journal::operation_json,
    verify::{ApplyDevice, DeviceFailure},
};

/// Typed warning acknowledgment required before opening maintenance.
///
/// The exact string is a deliberate-friction choice: it names the store
/// exposure so the typed act acknowledges the risk. Documented here and in
/// the maintenance UI contract; a mismatch performs no mutation.
pub const MAINTENANCE_CONFIRMATION: &str = "OPEN STORE MAINTENANCE";

/// Journal id range reserved for maintenance generations. Apply stays below
/// 2_000_000 (plus 500k/1M windows), edit uses 3_000_000..4_000_000, and
/// restore cleanup starts at 5_000_000.
const MAINTENANCE_BASE: u64 = 4_000_000;
const MAINTENANCE_END: u64 = 5_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceState {
    Closed,
    Open { prevents_exit: bool },
}

impl MaintenanceState {
    pub fn prevents_exit(&self) -> bool {
        matches!(self, Self::Open { prevents_exit: true })
    }
    pub fn allows_new_installs(&self) -> bool {
        matches!(self, Self::Open { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    Opened,
    RecoverableDisconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenResult {
    pub outcome: OpenOutcome,
    pub envelope: RecoveryEnvelopeV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    Closed,
    CloseFailed,
    RecoverableDisconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseResult {
    pub outcome: CloseOutcome,
    pub envelope: RecoveryEnvelopeV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceError {
    ConfirmationMismatch,
    AlreadyOpen,
    NotOpen,
    UnknownPackage(String),
    Stale(&'static str),
    Blocked(&'static str),
    Mirror(MirrorError),
    Journal,
}

/// Caller-supplied rescan entry. Only `launchable` packages participate; no
/// version field exists by design — any newly present package not in the
/// recorded universe is treated as a new install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedPackage {
    pub package: PackageId,
    pub launchable: bool,
    pub is_store: bool,
}

impl ScannedPackage {
    pub fn new(package: PackageId, launchable: bool, is_store: bool) -> Self {
        Self { package, launchable, is_store }
    }
}

/// True while a maintenance window is open: either the top-level envelope
/// state reads `open` (launcher-written fixtures) or the last journaled
/// maintenance entry has not been closed with an applied `open:false`.
/// A pending close still counts as open — the window ends only when close is
/// applied and mirrored.
///
/// Desktop/session callers map this into
/// `ObservedState.maintenance_open` for [`crate::transaction::classify`]
/// (which yields `MaintenanceRecovery`) and gate direct edit/restore/open
/// calls with [`ensure_no_forced_close`].
pub fn is_maintenance_open(envelope: &RecoveryEnvelopeV1) -> bool {
    let canonical = envelope.canonical_json();
    let top_open = canonical.contains(r#""maintenance":{"state":"open"}"#);
    let Some(view) = parse_envelope(&canonical) else {
        return top_open;
    };
    let mut last: Option<(bool, &str)> = None;
    for entry in &view.entries {
        match (&entry.op, entry.state.as_str()) {
            (ParsedOp::Maintenance { open }, "applied" | "pending") => {
                last = Some((*open, entry.state.as_str()));
            }
            _ => (),
        }
    }
    match last {
        // Open:true (pending or applied) is open; open:false pending is a
        // close in progress, still open; only applied close ends the window.
        Some((true, _)) => true,
        Some((false, "applied")) => false,
        Some((false, _)) => true,
        None => top_open,
    }
}

pub fn requires_forced_close(envelope: &RecoveryEnvelopeV1) -> bool {
    is_maintenance_open(envelope)
}

pub fn maintenance_state(envelope: &RecoveryEnvelopeV1) -> MaintenanceState {
    if is_maintenance_open(envelope) {
        MaintenanceState::Open { prevents_exit: true }
    } else {
        MaintenanceState::Closed
    }
}

pub fn allows_new_installs(envelope: &RecoveryEnvelopeV1) -> bool {
    is_maintenance_open(envelope)
}

pub fn can_exit(envelope: &RecoveryEnvelopeV1) -> bool {
    !is_maintenance_open(envelope)
}

/// Ordinary desktop exit while maintenance is open is refused without
/// mutation. The caller must run a verified [`close`] first.
pub fn try_exit(envelope: &RecoveryEnvelopeV1) -> Result<(), MaintenanceError> {
    if is_maintenance_open(envelope) {
        return Err(MaintenanceError::Blocked(
            "maintenance is open; verified close is required before exit",
        ));
    }
    Ok(())
}

/// Refuse edit/restore/open while a forced close is required. Session
/// classification already maps maintenance-open to `MaintenanceRecovery`
/// (diagnostics/export only); this helper wires the same gate for direct
/// transaction callers.
pub fn ensure_no_forced_close(envelope: &RecoveryEnvelopeV1) -> Result<(), MaintenanceError> {
    if is_maintenance_open(envelope) {
        return Err(MaintenanceError::Blocked(
            "maintenance is open; close maintenance before other actions",
        ));
    }
    Ok(())
}

/// Open a bounded maintenance window.
///
/// `stores` carries the recorded user-facing store packages to temporarily
/// unsuspend. All recorded `AppOp` restrictions are treated as install
/// sources and restored to their recorded prior modes. Blocked apps are never
/// unsuspended here: only packages in `stores` plus recorded app-ops are
/// touched. The maintenance-open journal entry is persisted BEFORE any store
/// restore, so an interruption always records the exposure.
///
/// Contract: when this returns `Err(Blocked)` after the open entry was
/// journaled (a recorded store or install source could not be restored), the
/// mirrors already hold the applied-open envelope. The in-memory envelope is
/// discarded with the error; callers must re-read the mirrors before
/// [`close`]. The window stays open and fail-closed (stores remain
/// suspended) until a verified close completes.
pub fn open<D: MirrorStore + ApplyDevice>(
    device: &mut D,
    envelope: RecoveryEnvelopeV1,
    confirmation: &str,
    stores: &[PackageId],
) -> Result<OpenResult, MaintenanceError>
where
    D::Error: std::fmt::Debug,
{
    if confirmation != MAINTENANCE_CONFIRMATION {
        return Err(MaintenanceError::ConfirmationMismatch);
    }
    let reconciled = read_reconciled(device, &envelope)?;
    let envelope = match reconciled {
        Reconciled::Disconnect => {
            return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
        }
        Reconciled::Envelope(next) => next,
    };
    let view = parse_envelope(&envelope.canonical_json()).ok_or(MaintenanceError::Journal)?;
    if let Some(pending) = envelope.pending_id() {
        if maintenance_sequence(&pending).is_none() {
            return Err(MaintenanceError::Blocked(
                "an unfinished non-maintenance transaction is pending; resume or roll it back first",
            ));
        }
    }
    if is_maintenance_open(&envelope) {
        // A pending open resumes below; an applied open refuses a second
        // window. An applied-open retry never re-enters via open: close the
        // window first, then open a fresh one.
        if envelope.pending_id().is_none() {
            return Err(MaintenanceError::AlreadyOpen);
        }
        if let Some(pending) = envelope.pending_id() {
            if let Some(recorded) = view.entries.iter().find(|entry| entry.id == pending) {
                if !matches!(&recorded.op, ParsedOp::Maintenance { open: true }) {
                    return Err(MaintenanceError::AlreadyOpen);
                }
            }
        }
    }
    let mut envelope = envelope;
    // Journal maintenance-open first (pending -> applied with mirrored
    // persist before any store restore).
    let open_id = match envelope.pending_id() {
        Some(pending) if maintenance_sequence(&pending).is_some() => pending,
        Some(_) => {
            return Err(MaintenanceError::Blocked(
                "an unfinished non-maintenance transaction is pending; resume or roll it back first",
            ));
        }
        None => next_maintenance_id(&view)?,
    };
    if envelope.journal_state(&open_id).is_none() {
        let operation = Operation::Maintenance { open: true };
        let inverse = Operation::Maintenance { open: false };
        envelope = envelope
            .append_pending(&open_id, &operation_json(&operation), &operation_json(&inverse))
            .map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
        match mutate_verify(device, &operation) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            _ => {
                let envelope = envelope.mark_failed(&open_id).map_err(|_| MaintenanceError::Journal)?;
                match persist_maintenance(device, &envelope) {
                    Ok(()) => (),
                    Err(PersistFail::Disconnect) => {
                        return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
                }
                return Err(MaintenanceError::Blocked("maintenance open could not be verified"));
            }
        }
        envelope = envelope.mark_applied(&open_id).map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
    } else {
        // Pending open from a prior attempt: finish the mutation then mark
        // applied.
        let operation = Operation::Maintenance { open: true };
        match mutate_verify(device, &operation) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            _ => {
                let envelope = envelope.mark_failed(&open_id).map_err(|_| MaintenanceError::Journal)?;
                match persist_maintenance(device, &envelope) {
                    Ok(()) => (),
                    Err(PersistFail::Disconnect) => {
                        return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
                }
                return Err(MaintenanceError::Blocked("maintenance open could not be verified"));
            }
        }
        envelope = envelope.mark_applied(&open_id).map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
    }
    // Temporarily restore recorded store + install-source state. These are
    // device mutations only — no new journal entries — so a later restore
    // still reverses exactly the recorded policy.
    let view = parse_envelope(&envelope.canonical_json()).ok_or(MaintenanceError::Journal)?;
    let user = UserId::parse(0).map_err(|_| MaintenanceError::Journal)?;
    for store in stores {
        let recorded = view.entries.iter().any(|entry| {
            entry.state == "applied"
                && matches!(&entry.op, ParsedOp::Suspend { package, suspended: true, .. } if package == store.as_str())
        });
        if !recorded {
            continue;
        }
        let unsuspend = Operation::Suspend { package: store.clone(), user, suspended: false };
        match device.verified(&unsuspend) {
            Ok(true) => continue,
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            _ => (),
        }
        match device.mutate(&unsuspend) {
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            Err(_) => return Err(MaintenanceError::Blocked("a recorded store could not be restored")),
            Ok(()) => (),
        }
        match device.verified(&unsuspend) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            _ => return Err(MaintenanceError::Blocked("a recorded store could not be restored")),
        }
    }
    for entry in view.entries.iter().filter(|entry| entry.state == "applied") {
        let (package, op_name, prior_mode) = match (&entry.op, &entry.inverse) {
            (
                ParsedOp::AppOp { package, op, .. },
                ParsedOp::AppOp { mode: inverse_mode, .. },
            ) => (package.clone(), op.clone(), inverse_mode.clone()),
            _ => continue,
        };
        let restore = appop_operation(&package, &op_name, &prior_mode)?;
        // Skip when already at the prior value.
        match device.verified(&restore) {
            Ok(true) => continue,
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            _ => (),
        }
        match device.mutate(&restore) {
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            Err(_) => return Err(MaintenanceError::Blocked("a recorded install source could not be restored")),
            Ok(()) => (),
        }
        match device.verified(&restore) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(OpenResult { outcome: OpenOutcome::RecoverableDisconnect, envelope });
            }
            _ => return Err(MaintenanceError::Blocked("a recorded install source could not be restored")),
        }
    }
    Ok(OpenResult { outcome: OpenOutcome::Opened, envelope })
}

/// Close a maintenance window: rescan, journal new installs before
/// suspension, reapply restrictions, verify, then journal close.
///
/// `scan` is the caller-supplied installed-package rescan; `approved` lists
/// the new packages the user chose to keep. Stores are never allowlisted —
/// a new store is always suspended even when approved.
pub fn close<D: MirrorStore + ApplyDevice>(
    device: &mut D,
    envelope: RecoveryEnvelopeV1,
    scan: &[ScannedPackage],
    approved: &[PackageId],
) -> Result<CloseResult, MaintenanceError>
where
    D::Error: std::fmt::Debug,
{
    let reconciled = read_reconciled(device, &envelope)?;
    let mut envelope = match reconciled {
        Reconciled::Disconnect => {
            return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
        }
        Reconciled::Envelope(next) => next,
    };
    if !is_maintenance_open(&envelope) {
        return Err(MaintenanceError::NotOpen);
    }
    if let Some(pending) = envelope.pending_id() {
        if maintenance_sequence(&pending).is_none() {
            return Err(MaintenanceError::Blocked(
                "an unfinished non-maintenance transaction is pending; resume or roll it back first",
            ));
        }
        // Pending maintenance entries resume inline below.
    }
    let mut view = parse_envelope(&envelope.canonical_json()).ok_or(MaintenanceError::Journal)?;
    // Resume a pending maintenance step first so a disconnect retry never
    // duplicates ids: finish the recorded operation, then continue.
    if let Some(pending) = envelope.pending_id() {
        let recorded = view
            .entries
            .iter()
            .find(|entry| entry.id == pending)
            .cloned()
            .ok_or(MaintenanceError::Journal)?;
        let operation = parsed_to_operation(&recorded.op)?;
        match mutate_verify(device, &operation) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            _ => {
                envelope = envelope.mark_failed(&pending).map_err(|_| MaintenanceError::Journal)?;
                match persist_maintenance(device, &envelope) {
                    Ok(()) => (),
                    Err(PersistFail::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
                }
                return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
            }
        }
        envelope = envelope.mark_applied(&pending).map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
        view = parse_envelope(&envelope.canonical_json()).ok_or(MaintenanceError::Journal)?;
        if !is_maintenance_open(&envelope) {
            // The pending step was the close itself.
            return Ok(CloseResult { outcome: CloseOutcome::Closed, envelope });
        }
    }
    // Recorded universe: baseline packages plus active allowlist plus any
    // package already journaled (suspension/app-op or LauncherPolicy union,
    // op plus inverse) — the same known-universe rule as edits.
    // BL-01: failed entries must not count as known for freshness. A failed
    // new-store suspend stays failed as history; the retry must see the
    // package as fresh again, journal a new suspend under a fresh id, and
    // only then journal close. `seen` (all states) still backs the
    // UnknownPackage check so a previously-failed approval is not refused.
    let mut known = view.initial_packages.clone();
    known.extend(view.active_allowed.iter().cloned());
    let mut seen = known.clone();
    for entry in &view.entries {
        for operation in [&entry.op, &entry.inverse] {
            match operation {
                ParsedOp::Suspend { package, .. } | ParsedOp::AppOp { package, .. } => {
                    seen.push(package.clone());
                    if entry.state != "failed" {
                        known.push(package.clone());
                    }
                }
                ParsedOp::LauncherPolicy { allowed } => {
                    seen.extend(allowed.iter().cloned());
                    if entry.state != "failed" {
                        known.extend(allowed.iter().cloned());
                    }
                }
                _ => (),
            }
        }
    }
    known.sort();
    known.dedup();
    seen.sort();
    seen.dedup();
    for package in approved {
        let name = package.as_str().to_owned();
        let in_scan = scan.iter().any(|item| item.package.as_str() == name && item.launchable);
        if !seen.iter().any(|item| item == &name) && !in_scan {
            return Err(MaintenanceError::UnknownPackage(name));
        }
    }
    // Newly present launchable packages: in scan, launchable, not recorded.
    // No version comparison — updates and installs are indistinguishable.
    let mut fresh: Vec<&ScannedPackage> = scan
        .iter()
        .filter(|item| item.launchable && !known.iter().any(|name| name == item.package.as_str()))
        .collect();
    fresh.sort_by(|left, right| left.package.as_str().cmp(right.package.as_str()));
    let approved_set: Vec<&str> = approved.iter().map(PackageId::as_str).collect();
    let mut allowed_additions: Vec<PackageId> = Vec::new();
    let mut to_suspend: Vec<&ScannedPackage> = Vec::new();
    for item in fresh {
        if approved_set.iter().any(|name| *name == item.package.as_str()) && !item.is_store {
            allowed_additions.push(item.package.clone());
        } else {
            to_suspend.push(item);
        }
    }
    let user = UserId::parse(0).map_err(|_| MaintenanceError::Journal)?;
    // Preserve allowed additions first: a single allowlist extension.
    if !allowed_additions.is_empty() {
        let mut next_allowed: Vec<String> = view.active_allowed.clone();
        next_allowed.extend(allowed_additions.iter().map(|package| package.as_str().to_owned()));
        next_allowed.sort();
        next_allowed.dedup();
        // Stores can never be allowed; the suspend branch above already
        // withheld them, so reaching here with a store is a caller bug.
        let id = next_maintenance_id(&view)?;
        envelope = envelope
            .append_pending_launcher_policy(&id, next_allowed)
            .map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
        let allowed = envelope
            .active_allowed_packages()
            .into_iter()
            .map(|name| PackageId::parse(&name).map_err(|_| MaintenanceError::Journal))
            .collect::<Result<Vec<_>, _>>()?;
        match mutate_verify(device, &Operation::LauncherPolicy { allowed }) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            _ => {
                envelope = envelope.mark_failed(&id).map_err(|_| MaintenanceError::Journal)?;
                match persist_maintenance(device, &envelope) {
                    Ok(()) => (),
                    Err(PersistFail::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
                }
                return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
            }
        }
        envelope = envelope.mark_applied(&id).map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
        view = parse_envelope(&envelope.canonical_json()).ok_or(MaintenanceError::Journal)?;
    }
    // Journal each newly present unapproved package BEFORE suspension so a
    // later full restore can undo exactly the Unscroll changes.
    for item in &to_suspend {
        let operation = Operation::Suspend { package: item.package.clone(), user, suspended: true };
        let inverse = Operation::Suspend { package: item.package.clone(), user, suspended: false };
        let id = next_maintenance_id(&view)?;
        envelope = envelope
            .append_pending(&id, &operation_json(&operation), &operation_json(&inverse))
            .map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
        match mutate_verify(device, &operation) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            _ => {
                // Stores are hard-gated: any new-suspend failure retains
                // recovery data and the maintenance-open state.
                envelope = envelope.mark_failed(&id).map_err(|_| MaintenanceError::Journal)?;
                match persist_maintenance(device, &envelope) {
                    Ok(()) => (),
                    Err(PersistFail::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
                }
                return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
            }
        }
        envelope = envelope.mark_applied(&id).map_err(|_| MaintenanceError::Journal)?;
        match persist_maintenance(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
        }
        view = parse_envelope(&envelope.canonical_json()).ok_or(MaintenanceError::Journal)?;
    }
    // Reapply recorded store + install-source restrictions (device mutations
    // only; the journal already records them). Anything currently at odds
    // with the recorded policy — including stores left open and blocked apps
    // the user touched during the window — is driven back.
    let view_snapshot = view.clone();
    for entry in view_snapshot.entries.iter().filter(|entry| entry.state == "applied") {
        match &entry.op {
            ParsedOp::Suspend { package, user: journal_user, suspended: true } => {
                let operation = Operation::Suspend {
                    package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
                    user: parsed_user(*journal_user)?,
                    suspended: true,
                };
                match device.verified(&operation) {
                    Ok(true) => continue,
                    Err(DeviceFailure::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    _ => (),
                }
                match device.mutate(&operation) {
                    Err(DeviceFailure::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(_) => {
                        return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
                    }
                    Ok(()) => (),
                }
                match device.verified(&operation) {
                    Ok(true) => (),
                    Err(DeviceFailure::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    _ => {
                        return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
                    }
                }
            }
            ParsedOp::AppOp { package, user: journal_user, op, mode } => {
                let operation = Operation::AppOp {
                    package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
                    user: parsed_user(*journal_user)?,
                    app_op: AppOp::parse(op).map_err(|_| MaintenanceError::Journal)?,
                    mode: AppOpMode::parse(mode).map_err(|_| MaintenanceError::Journal)?,
                };
                match device.verified(&operation) {
                    Ok(true) => continue,
                    Err(DeviceFailure::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    _ => (),
                }
                match device.mutate(&operation) {
                    Err(DeviceFailure::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    Err(_) => {
                        return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
                    }
                    Ok(()) => (),
                }
                match device.verified(&operation) {
                    Ok(true) => (),
                    Err(DeviceFailure::Disconnect) => {
                        return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                    }
                    _ => {
                        return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
                    }
                }
            }
            _ => (),
        }
    }
    // Verify the active policy before journaling close.
    for entry in view_snapshot.entries.iter().filter(|entry| entry.state == "applied") {
        let operation = match &entry.op {
            ParsedOp::Suspend { package, user: journal_user, suspended } => Some(Operation::Suspend {
                package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
                user: parsed_user(*journal_user)?,
                suspended: *suspended,
            }),
            ParsedOp::AppOp { package, user: journal_user, op, mode } => Some(Operation::AppOp {
                package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
                user: parsed_user(*journal_user)?,
                app_op: AppOp::parse(op).map_err(|_| MaintenanceError::Journal)?,
                mode: AppOpMode::parse(mode).map_err(|_| MaintenanceError::Journal)?,
            }),
            ParsedOp::LauncherPolicy { allowed } => Some(Operation::LauncherPolicy {
                allowed: allowed
                    .iter()
                    .map(|name| PackageId::parse(name).map_err(|_| MaintenanceError::Journal))
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            ParsedOp::Maintenance { open } => Some(Operation::Maintenance { open: *open }),
            _ => None,
        };
        let Some(operation) = operation else { continue };
        // The open record itself verifies trivially; every other applied
        // step must verify or the close fails closed.
        if matches!(operation, Operation::Maintenance { open: true }) {
            continue;
        }
        match device.verified(&operation) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => {
                return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
            }
            _ => {
                return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
            }
        }
    }
    // Journal maintenance-close last; only an applied close ends the window.
    let id = next_maintenance_id(&view)?;
    let operation = Operation::Maintenance { open: false };
    let inverse = Operation::Maintenance { open: true };
    envelope = envelope
        .append_pending(&id, &operation_json(&operation), &operation_json(&inverse))
        .map_err(|_| MaintenanceError::Journal)?;
    match persist_maintenance(device, &envelope) {
        Ok(()) => (),
        Err(PersistFail::Disconnect) => {
            return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
        }
        Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
    }
    match mutate_verify(device, &operation) {
        Ok(true) => (),
        Err(DeviceFailure::Disconnect) => {
            return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
        }
        _ => {
            envelope = envelope.mark_failed(&id).map_err(|_| MaintenanceError::Journal)?;
            match persist_maintenance(device, &envelope) {
                Ok(()) => (),
                Err(PersistFail::Disconnect) => {
                    return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
                }
                Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
            }
            return Ok(CloseResult { outcome: CloseOutcome::CloseFailed, envelope });
        }
    }
    envelope = envelope.mark_applied(&id).map_err(|_| MaintenanceError::Journal)?;
    match persist_maintenance(device, &envelope) {
        Ok(()) => (),
        Err(PersistFail::Disconnect) => {
            return Ok(CloseResult { outcome: CloseOutcome::RecoverableDisconnect, envelope });
        }
        Err(PersistFail::Mirror(error)) => return Err(MaintenanceError::Mirror(error)),
    }
    Ok(CloseResult { outcome: CloseOutcome::Closed, envelope })
}

enum Reconciled {
    Disconnect,
    Envelope(RecoveryEnvelopeV1),
}

/// Mirror read-before-mutate: both copies must parse and agree (a single
/// missing copy is tolerated and healed by the subsequent persist), and the
/// passed envelope must equal that reconciled canonical copy. A stale
/// envelope is refused without mutating the device or either mirror. A
/// disconnect while reading surfaces as recoverable without mutation.
fn read_reconciled<D: MirrorStore>(
    device: &mut D,
    envelope: &RecoveryEnvelopeV1,
) -> Result<Reconciled, MaintenanceError>
where
    D::Error: std::fmt::Debug,
{
    let private = match device.read_private() {
        Ok(value) => value,
        Err(error) if is_disconnect_error(&error) => return Ok(Reconciled::Disconnect),
        Err(_) => return Err(MaintenanceError::Mirror(MirrorError::PrivateReadback)),
    };
    let shared = match device.read_shared() {
        Ok(value) => value,
        Err(error) if is_disconnect_error(&error) => return Ok(Reconciled::Disconnect),
        Err(_) => return Err(MaintenanceError::Mirror(MirrorError::SharedReadback)),
    };
    let mut valid = Vec::new();
    for copy in [private.as_deref(), shared.as_deref()].into_iter().flatten() {
        let Ok(parsed) = RecoveryEnvelopeV1::parse(copy) else {
            return Err(MaintenanceError::Blocked("recovery copies disagree; reconcile first"));
        };
        valid.push(parsed.canonical_json());
    }
    if valid.is_empty() {
        return Err(MaintenanceError::Blocked("envelope is stale; re-read before maintenance"));
    }
    if valid.iter().any(|copy| copy != &valid[0]) {
        return Err(MaintenanceError::Blocked("recovery copies disagree; reconcile first"));
    }
    if valid[0] != envelope.canonical_json() {
        return Err(MaintenanceError::Stale("envelope is stale; re-read before maintenance"));
    }
    Ok(Reconciled::Envelope(envelope.clone()))
}

fn maintenance_id(sequence: u64) -> String {
    format!("00000000-0000-4000-8000-{sequence:012x}")
}

fn maintenance_sequence(id: &str) -> Option<u64> {
    let suffix = id.strip_prefix("00000000-0000-4000-8000-")?;
    u64::from_str_radix(suffix, 16)
        .ok()
        .filter(|sequence| (MAINTENANCE_BASE..MAINTENANCE_END).contains(sequence))
}

fn next_maintenance_id(view: &ParsedEnvelope) -> Result<String, MaintenanceError> {
    let max = view.entries.iter().filter_map(|entry| maintenance_sequence(&entry.id)).max();
    let start = max.map(|value| value + 1).unwrap_or(MAINTENANCE_BASE);
    if start >= MAINTENANCE_END {
        return Err(MaintenanceError::Journal);
    }
    Ok(maintenance_id(start))
}

fn parsed_user(user: u64) -> Result<UserId, MaintenanceError> {
    UserId::parse(u32::try_from(user).map_err(|_| MaintenanceError::Journal)?)
        .map_err(|_| MaintenanceError::Journal)
}

fn appop_operation(package: &str, op: &str, mode: &str) -> Result<Operation, MaintenanceError> {
    Ok(Operation::AppOp {
        package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
        user: UserId::parse(0).map_err(|_| MaintenanceError::Journal)?,
        app_op: AppOp::parse(op).map_err(|_| MaintenanceError::Journal)?,
        mode: AppOpMode::parse(mode).map_err(|_| MaintenanceError::Journal)?,
    })
}

fn parsed_to_operation(parsed: &ParsedOp) -> Result<Operation, MaintenanceError> {
    match parsed {
        ParsedOp::Suspend { package, user, suspended } => Ok(Operation::Suspend {
            package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
            user: parsed_user(*user)?,
            suspended: *suspended,
        }),
        ParsedOp::AppOp { package, user, op, mode } => Ok(Operation::AppOp {
            package: PackageId::parse(package).map_err(|_| MaintenanceError::Journal)?,
            user: parsed_user(*user)?,
            app_op: AppOp::parse(op).map_err(|_| MaintenanceError::Journal)?,
            mode: AppOpMode::parse(mode).map_err(|_| MaintenanceError::Journal)?,
        }),
        ParsedOp::LauncherPolicy { allowed } => Ok(Operation::LauncherPolicy {
            allowed: allowed
                .iter()
                .map(|name| PackageId::parse(name).map_err(|_| MaintenanceError::Journal))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        ParsedOp::Maintenance { open } => Ok(Operation::Maintenance { open: *open }),
        ParsedOp::Home { .. } | ParsedOp::Cleanup { .. } | ParsedOp::Other { .. } => {
            Err(MaintenanceError::Blocked("the pending maintenance entry cannot be resumed"))
        }
    }
}

fn mutate_verify<D: ApplyDevice>(device: &mut D, operation: &Operation) -> Result<bool, DeviceFailure> {
    match device.mutate(operation) {
        Err(DeviceFailure::Disconnect) => Err(DeviceFailure::Disconnect),
        _ => device.verified(operation),
    }
}

fn is_disconnect_error<E: std::fmt::Debug>(error: &E) -> bool {
    format!("{error:?}").contains("Disconnect")
}

enum PersistFail {
    Disconnect,
    Mirror(MirrorError),
}

fn matches_copy(value: &Option<String>, expected: &str) -> bool {
    value
        .as_ref()
        .and_then(|value| RecoveryEnvelopeV1::parse(value).ok())
        .is_some_and(|value| value.canonical_json() == expected)
}

fn persist_maintenance<D: MirrorStore>(
    device: &mut D,
    envelope: &RecoveryEnvelopeV1,
) -> Result<(), PersistFail>
where
    D::Error: std::fmt::Debug,
{
    let expected = envelope.canonical_json();
    if let Err(error) = device.write_private(&expected) {
        if is_disconnect_error(&error) {
            return Err(PersistFail::Disconnect);
        }
        return Err(PersistFail::Mirror(MirrorError::PrivateWrite));
    }
    match device.read_private() {
        Err(error) if is_disconnect_error(&error) => return Err(PersistFail::Disconnect),
        Err(_) => return Err(PersistFail::Mirror(MirrorError::PrivateReadback)),
        Ok(value) if !matches_copy(&value, &expected) => {
            return Err(PersistFail::Mirror(MirrorError::PrivateReadback))
        }
        _ => (),
    }
    if let Err(error) = device.write_shared(&expected) {
        if is_disconnect_error(&error) {
            return Err(PersistFail::Disconnect);
        }
        return Err(PersistFail::Mirror(MirrorError::SharedWrite));
    }
    match device.read_shared() {
        Err(error) if is_disconnect_error(&error) => return Err(PersistFail::Disconnect),
        Err(_) => return Err(PersistFail::Mirror(MirrorError::SharedReadback)),
        Ok(value) if !matches_copy(&value, &expected) => {
            return Err(PersistFail::Mirror(MirrorError::SharedReadback))
        }
        _ => (),
    }
    Ok(())
}
