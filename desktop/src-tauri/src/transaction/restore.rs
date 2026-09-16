//! Full restore of the recorded Unscroll changes.
//!
//! Recorded inverses execute in reverse journal order, grouped app-ops and
//! settings first, then unsuspends, then HOME. Only packages Unscroll recorded
//! as suspended are unsuspended: a package already suspended before the
//! baseline is never touched. HOME restores through the shell command with a
//! guided-chooser fallback and re-verification. After full verification the
//! launcher and private copy are removed, and the shared envelope is deleted
//! last. If only final cleanup fails, the remaining data is retained and
//! `retry_cleanup` removes it without repeating restored mutations.

use std::collections::BTreeSet;

use crate::{
    adb::{AppOp, AppOpMode, Component, PackageId, UserId},
    policy::Operation,
    recovery::{
        mirror::{MirrorError, MirrorStore, persist},
        model::RecoveryEnvelopeV1,
    },
};
use super::{
    diagnostics::{ParsedEnvelope, ParsedOp, parse_envelope},
    verify::{ApplyDevice, DeviceFailure},
};

pub const RESTORE_CONFIRMATION: &str = "RESTORE MY PHONE";

const PRIVATE_CLEANUP_OP: &str = r#"{"kind":"cleanup","removed":true,"target":"private_envelope"}"#;
const PRIVATE_CLEANUP_INV: &str = r#"{"kind":"cleanup","removed":false,"target":"private_envelope"}"#;

/// Journal id range reserved for restore cleanup entries.
const CLEANUP_BASE: u64 = 5_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    Complete,
    ChooserRequired,
    CleanupRetry,
    Blocked,
    RecoverableDisconnect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreResult {
    pub outcome: RestoreOutcome,
    pub envelope: RecoveryEnvelopeV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    ConfirmationMismatch,
    MissingCopies,
    NotReady(&'static str),
    Store(String),
    Mirror(MirrorError),
}

impl From<()> for RestoreError {
    fn from(_: ()) -> Self {
        Self::Store("mirror store failed".into())
    }
}

impl From<MirrorError> for RestoreError {
    fn from(error: MirrorError) -> Self {
        Self::Mirror(error)
    }
}

impl From<DeviceFailure> for RestoreError {
    fn from(error: DeviceFailure) -> Self {
        Self::Store(format!("device failed: {error:?}"))
    }
}

/// Device operations needed for restore beyond verified mutation: launcher
/// removal. The default refuses, so a device that cannot remove the launcher
/// keeps its recovery evidence instead of claiming success.
pub trait RestoreDevice: ApplyDevice {
    fn uninstall_launcher(&mut self) -> Result<(), DeviceFailure> {
        Err(DeviceFailure::Command)
    }
}

pub fn restore<D>(device: &mut D, envelope: RecoveryEnvelopeV1, confirmation: &str) -> Result<RestoreResult, RestoreError>
where
    D: MirrorStore + RestoreDevice,
    D::Error: Into<RestoreError>,
{
    if confirmation != RESTORE_CONFIRMATION {
        return Err(RestoreError::ConfirmationMismatch);
    }
    let expected = envelope.baseline_hash();
    let private = match device.read_private().map_err(Into::into) {
        Ok(value) => value,
        Err(error) if is_disconnect_store(&error) => return disconnected(envelope),
        Err(error) => return Err(error),
    };
    let shared = match device.read_shared().map_err(Into::into) {
        Ok(value) => value,
        Err(error) if is_disconnect_store(&error) => return disconnected(envelope),
        Err(error) => return Err(error),
    };
    if private.is_none() && shared.is_none() {
        return Err(RestoreError::MissingCopies);
    }
    let mut valid = Vec::new();
    for copy in [private.as_deref(), shared.as_deref()].into_iter().flatten() {
        let Ok(parsed) = RecoveryEnvelopeV1::parse(copy) else {
            continue;
        };
        if parsed.baseline_hash() != expected {
            return Err(RestoreError::NotReady("a recovery copy does not match this baseline"));
        }
        valid.push(parsed.canonical_json());
    }
    if valid.is_empty() {
        return Err(RestoreError::NotReady("no readable recovery copy"));
    }
    if valid.iter().any(|copy| copy != &valid[0]) {
        return Err(RestoreError::NotReady("recovery copies disagree; reconcile first"));
    }
    if valid[0] != envelope.canonical_json() {
        return Err(RestoreError::NotReady("the envelope is stale; re-read before restoring"));
    }
    if envelope.pending_id().is_some() {
        return Err(RestoreError::NotReady(
            "an unfinished transaction is pending; resume or roll it back first",
        ));
    }
    if envelope.has_applied_private_cleanup() {
        return Err(RestoreError::NotReady(
            "restoration is verified; use retry_cleanup to remove remaining copies",
        ));
    }
    let view = parse_envelope(&envelope.canonical_json()).ok_or(RestoreError::NotReady("the journal cannot be read"))?;
    let mut appops = Vec::new();
    let mut suspends = Vec::new();
    let mut homes = Vec::new();
    for entry in view.entries.iter().filter(|entry| entry.state == "applied").rev() {
        match inverse_operation(&entry.inverse)? {
            Some(operation @ Operation::AppOp { .. }) => appops.push(operation),
            Some(operation @ Operation::Suspend { .. }) => suspends.push(operation),
            Some(operation @ Operation::Home { .. }) => homes.push(operation),
            Some(_) | None => (),
        }
    }
    let baseline_suspended: BTreeSet<&str> = view.initial_suspended.iter().map(String::as_str).collect();
    suspends.retain(|operation| match operation {
        Operation::Suspend { package, suspended: false, .. } => !baseline_suspended.contains(package.as_str()),
        _ => true,
    });
    let plan: Vec<Operation> = appops.into_iter().chain(suspends).chain(homes).collect();
    for operation in &plan {
        match device.verified(operation) {
            Ok(true) => continue,
            Err(DeviceFailure::Disconnect) => return disconnected(envelope),
            _ => (),
        }
        let restored = match device.mutate(operation) {
            Ok(()) => match device.verified(operation) {
                Ok(true) => true,
                Err(DeviceFailure::Disconnect) => return disconnected(envelope),
                _ => false,
            },
            Err(DeviceFailure::Disconnect) => return disconnected(envelope),
            Err(_) => false,
        };
        if !restored {
            if matches!(operation, Operation::Home { .. }) {
                return home_fallback(device, envelope);
            }
            return blocked(envelope);
        }
    }
    for operation in &plan {
        match device.verified(operation) {
            Ok(true) => (),
            Err(DeviceFailure::Disconnect) => return disconnected(envelope),
            _ => return blocked(envelope),
        }
    }
    match device.uninstall_launcher() {
        Ok(()) => (),
        Err(DeviceFailure::Disconnect) => return disconnected(envelope),
        Err(_) => return blocked(envelope),
    }
    let mut envelope = envelope;
    let cleanup_id = if envelope.has_applied_private_cleanup() {
        None
    } else {
        let id = next_cleanup_id(&view)?;
        envelope = envelope
            .append_pending(&id, PRIVATE_CLEANUP_OP, PRIVATE_CLEANUP_INV)
            .map_err(|_| RestoreError::NotReady("cannot journal cleanup"))?;
        persist(device, &envelope).map_err(RestoreError::Mirror)?;
        Some(id)
    };
    if device.delete_private().is_err() {
        if let Some(id) = &cleanup_id {
            if let Ok(marked) = envelope.mark_applied(id) {
                envelope = marked;
                let _ = persist(device, &envelope);
            }
        }
        return cleanup_retry(envelope);
    }
    if let Some(id) = &cleanup_id {
        envelope = envelope.mark_applied(id).map_err(|_| RestoreError::NotReady("cannot record cleanup"))?;
        if write_shared_verified(device, &envelope).is_err() {
            return cleanup_retry(envelope);
        }
    }
    if device.delete_shared().is_err() {
        return cleanup_retry(envelope);
    }
    Ok(RestoreResult { outcome: RestoreOutcome::Complete, envelope })
}

/// Remove remaining recovery copies after a verified restore without repeating
/// any restored device mutation. Only deletions and shared-copy repair happen
/// here; suspend, app-op, HOME, and launcher operations are never retried.
pub fn retry_cleanup<D>(store: &mut D, envelope: RecoveryEnvelopeV1) -> Result<(), RestoreError>
where
    D: MirrorStore,
    D::Error: Into<RestoreError>,
{
    let private = store.read_private().map_err(Into::into)?;
    let shared = store.read_shared().map_err(Into::into)?;
    if private.is_none() && shared.is_none() {
        return Ok(());
    }
    if !envelope.has_applied_private_cleanup() {
        return Err(RestoreError::NotReady(
            "restoration is not verified; cleanup retry refuses to delete evidence",
        ));
    }
    if let Some(text) = shared.as_deref() {
        let matches = RecoveryEnvelopeV1::parse(text).ok().is_some_and(|read| read.canonical_json() == envelope.canonical_json());
        if !matches {
            write_shared_verified(store, &envelope)?;
        }
    }
    if private.is_some() {
        store.delete_private().map_err(Into::into)?;
    }
    if store.read_shared().map_err(Into::into)?.is_some() {
        store.delete_shared().map_err(Into::into)?;
    }
    Ok(())
}

/// Convert a recorded inverse into a device operation. Policy, maintenance,
/// and cleanup inverses need no device mutation during restore; anything else
/// unknown refuses the restore instead of guessing.
fn inverse_operation(inverse: &ParsedOp) -> Result<Option<Operation>, RestoreError> {
    match inverse {
        ParsedOp::Suspend { package, user, suspended } => Ok(Some(Operation::Suspend {
            package: PackageId::parse(package).map_err(|_| RestoreError::NotReady("recorded package is invalid"))?,
            user: restore_user(*user)?,
            suspended: *suspended,
        })),
        ParsedOp::AppOp { package, user, op, mode } => Ok(Some(Operation::AppOp {
            package: PackageId::parse(package).map_err(|_| RestoreError::NotReady("recorded package is invalid"))?,
            user: restore_user(*user)?,
            app_op: AppOp::parse(op).map_err(|_| RestoreError::NotReady("recorded app-op is invalid"))?,
            mode: AppOpMode::parse(mode).map_err(|_| RestoreError::NotReady("recorded app-op mode is invalid"))?,
        })),
        ParsedOp::Home { component } => Ok(Some(Operation::Home {
            component: Component::parse(component).map_err(|_| RestoreError::NotReady("recorded HOME is invalid"))?,
        })),
        ParsedOp::LauncherPolicy { .. } | ParsedOp::Maintenance { .. } | ParsedOp::Cleanup { .. } => Ok(None),
        ParsedOp::Other { .. } => Err(RestoreError::NotReady("recorded operation needs no known restore")),
    }
}

fn restore_user(user: u64) -> Result<UserId, RestoreError> {
    UserId::parse(u32::try_from(user).map_err(|_| RestoreError::NotReady("recorded user is invalid"))?)
        .map_err(|_| RestoreError::NotReady("recorded user is invalid"))
}

fn next_cleanup_id(view: &ParsedEnvelope) -> Result<String, RestoreError> {
    let used = view.entries.iter().filter(|entry| entry.op_kind == "cleanup").count() as u64;
    let sequence = CLEANUP_BASE.checked_add(used).ok_or(RestoreError::NotReady("cleanup history exhausted"))?;
    Ok(format!("00000000-0000-4000-8000-{sequence:012x}"))
}

fn write_shared_verified<D>(device: &mut D, envelope: &RecoveryEnvelopeV1) -> Result<(), RestoreError>
where
    D: MirrorStore,
    D::Error: Into<RestoreError>,
{
    let expected = envelope.canonical_json();
    device.write_shared(&expected).map_err(Into::into)?;
    let back = device.read_shared().map_err(Into::into)?;
    match back.and_then(|text| RecoveryEnvelopeV1::parse(&text).ok()) {
        Some(read) if read.canonical_json() == expected => Ok(()),
        _ => Err(RestoreError::Store("shared copy readback mismatch".into())),
    }
}

fn home_fallback<D: RestoreDevice>(device: &mut D, envelope: RecoveryEnvelopeV1) -> Result<RestoreResult, RestoreError> {
    match device.chooser() {
        Err(DeviceFailure::Disconnect) => disconnected(envelope),
        Err(_) => blocked(envelope),
        Ok(()) => Ok(RestoreResult { outcome: RestoreOutcome::ChooserRequired, envelope }),
    }
}

fn disconnected(envelope: RecoveryEnvelopeV1) -> Result<RestoreResult, RestoreError> {
    Ok(RestoreResult { outcome: RestoreOutcome::RecoverableDisconnect, envelope })
}

/// A pre-mutation mirror read that failed with a disconnect surfaces as a
/// recoverable disconnect (no mutations ran), not a store error. Post-mutation
/// disconnects already map to `disconnected` at each call site.
fn is_disconnect_store(error: &RestoreError) -> bool {
    matches!(error, RestoreError::Store(message) if message.contains("Disconnect"))
}

fn blocked(envelope: RecoveryEnvelopeV1) -> Result<RestoreResult, RestoreError> {
    Ok(RestoreResult { outcome: RestoreOutcome::Blocked, envelope })
}

fn cleanup_retry(envelope: RecoveryEnvelopeV1) -> Result<RestoreResult, RestoreError> {
    Ok(RestoreResult { outcome: RestoreOutcome::CleanupRetry, envelope })
}
