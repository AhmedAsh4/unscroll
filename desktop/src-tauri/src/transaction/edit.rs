//! Allowlist edits as mirrored deltas against the active policy.
//!
//! The baseline is never replaced: the updated allowlist and the required
//! suspend/unsuspend operations are appended to the same mirrored journal
//! history so a later full restore can undo them too.
//!
//! `policy::planner::edit` is intentionally not reused here: it requires a live
//! `DeviceSnapshot`, while edits apply to a reconciled envelope from any
//! compatible installation. Unknown-package refusal is therefore enforced
//! against the envelope's recorded universe (baseline packages plus the active
//! allowlist plus any package already journaled as a suspension/app-op or in
//! the journaled LauncherPolicy allowlist union (op plus inverse))
//! instead of the device catalog. `ApplyDevice` is reused directly;
//! no separate edit-device trait is needed.

use crate::{
    adb::{PackageId, UserId},
    policy::Operation,
    recovery::{
        mirror::{MirrorError, MirrorStore},
        model::{JournalState, RecoveryEnvelopeV1},
    },
};
use super::{
    diagnostics::{ParsedEnvelope, ParsedOp, parse_envelope},
    journal::operation_json,
    verify::{ApplyDevice, DeviceFailure},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOutcome {
    Complete,
    RecoverableDisconnect,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditResult {
    pub outcome: EditOutcome,
    pub envelope: RecoveryEnvelopeV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    UnknownPackage(String),
    Mirror(MirrorError),
    Journal,
    Blocked(&'static str),
}

/// Journal id range reserved for edit generations. The apply runner stays
/// below 2_000_000 and restore cleanup starts at 5_000_000.
const EDIT_BASE: u64 = 3_000_000;

/// Edit the allowlist against a reconciled envelope.
///
/// Mirrors are read before any mutation: both copies must parse and agree (a
/// single missing copy is tolerated and healed by the subsequent persist),
/// and the passed envelope must equal that reconciled canonical copy. A stale
/// envelope is refused without mutating the device or either mirror; when the
/// session reports a stale/missing copy the caller is expected to persist the
/// repair first and re-check before editing.
///
/// Retrying a pending edit requires identical `before`/`after`/`protected`:
/// the retry reuses the pending generation's entry ids, so a mismatched
/// `before` would build a shorter policy-only window and report `Complete`
/// without executing the original suspend steps. Resuming retries are
/// therefore refused unless `want` equals the active allowlist and `have`
/// equals the pending generation's recorded prior allowlist (its
/// launcher-policy inverse). Applied steps are re-verified on resume before
/// skipping.
///
/// `protected` carries the caller-supplied protection set (the session/planner
/// layer owns protection facts; the transaction stays decoupled). Protected
/// packages are never suspended or unsuspended by the edit — they stay
/// available — while the launcher-policy write still records `after` as given.
pub fn edit<D: MirrorStore + ApplyDevice>(
    device: &mut D,
    envelope: RecoveryEnvelopeV1,
    before: &[PackageId],
    after: &[PackageId],
    protected: &[PackageId],
) -> Result<EditResult, EditError>
where
    D::Error: std::fmt::Debug,
{
    let private = match device.read_private() {
        Ok(value) => value,
        Err(error) if is_disconnect_error(&error) => {
            return Ok(EditResult { outcome: EditOutcome::RecoverableDisconnect, envelope });
        }
        Err(_) => return Err(EditError::Mirror(MirrorError::PrivateReadback)),
    };
    let shared = match device.read_shared() {
        Ok(value) => value,
        Err(error) if is_disconnect_error(&error) => {
            return Ok(EditResult { outcome: EditOutcome::RecoverableDisconnect, envelope });
        }
        Err(_) => return Err(EditError::Mirror(MirrorError::SharedReadback)),
    };
    let mut valid = Vec::new();
    for copy in [private.as_deref(), shared.as_deref()].into_iter().flatten() {
        let Ok(parsed) = RecoveryEnvelopeV1::parse(copy) else {
            return Err(EditError::Blocked("recovery copies disagree; reconcile first"));
        };
        valid.push(parsed.canonical_json());
    }
    if valid.is_empty() {
        return Err(EditError::Blocked("envelope is stale; re-read before editing"));
    }
    if valid.iter().any(|copy| copy != &valid[0]) {
        return Err(EditError::Blocked("recovery copies disagree; reconcile first"));
    }
    if valid[0] != envelope.canonical_json() {
        return Err(EditError::Blocked("envelope is stale; re-read before editing"));
    }
    let view = parse_envelope(&envelope.canonical_json()).ok_or(EditError::Journal)?;
    let mut known = view.initial_packages.clone();
    known.extend(view.active_allowed.iter().cloned());
    for entry in &view.entries {
        for operation in [&entry.op, &entry.inverse] {
            match operation {
                ParsedOp::Suspend { package, .. } | ParsedOp::AppOp { package, .. } => {
                    known.push(package.clone());
                }
                ParsedOp::LauncherPolicy { allowed } => {
                    known.extend(allowed.iter().cloned());
                }
                _ => (),
            }
        }
    }
    known.sort();
    known.dedup();
    for package in before.iter().chain(after.iter()) {
        if !known.iter().any(|item| item == package.as_str()) {
            return Err(EditError::UnknownPackage(package.as_str().into()));
        }
    }
    let mut have: Vec<&str> = before.iter().map(PackageId::as_str).collect();
    have.sort_unstable();
    let mut want: Vec<&str> = after.iter().map(PackageId::as_str).collect();
    want.sort_unstable();
    let mut active: Vec<&str> = view.active_allowed.iter().map(String::as_str).collect();
    active.sort_unstable();
    // A pending edit already updated the active allowlist optimistically via
    // its launcher-policy persist, so a retry with the same before/after sees
    // `want == active` (not `have == active`). Fresh edits require `have`.
    let resuming_edit = envelope.pending_id().as_deref().and_then(edit_sequence).is_some();
    if resuming_edit {
        if want != active {
            return Err(EditError::Blocked("the allowlist changed since it was read; re-read before editing"));
        }
        if let Some(pending_id) = envelope.pending_id() {
            if let Some(pending_seq) = edit_sequence(&pending_id) {
                match pending_generation_prior(&view, pending_seq) {
                    Some((_, prior_allowed)) => {
                        let mut expected = prior_allowed.clone();
                        expected.sort();
                        let mut seen: Vec<String> = have.iter().map(|item| (*item).to_owned()).collect();
                        seen.sort();
                        if seen != expected {
                            return Err(EditError::Blocked(
                                "the pending edit entry does not match this edit; re-read before editing",
                            ));
                        }
                    }
                    None => {
                        return Err(EditError::Blocked(
                            "the pending edit entry does not match this edit; re-read before editing",
                        ));
                    }
                }
            }
        }
    } else if have != active {
        return Err(EditError::Blocked("the allowlist changed since it was read; re-read before editing"));
    }
    if have == want && !resuming_edit {
        return Ok(EditResult { outcome: EditOutcome::Complete, envelope });
    }
    let user = UserId::parse(0).map_err(|_| EditError::Journal)?;
    let is_protected = |package: &PackageId| protected.iter().any(|item| item.as_str() == package.as_str());
    let mut prior: Vec<PackageId> = before.to_vec();
    prior.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    let mut allowed: Vec<PackageId> = after.to_vec();
    allowed.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    let mut steps = vec![(
        Operation::LauncherPolicy { allowed: allowed.clone() },
        Operation::LauncherPolicy { allowed: prior.clone() },
    )];
    for package in prior.iter().filter(|item| !allowed.iter().any(|keep| keep.as_str() == item.as_str())) {
        if is_protected(package) {
            continue;
        }
        steps.push((
            Operation::Suspend { package: package.clone(), user, suspended: true },
            Operation::Suspend { package: package.clone(), user, suspended: false },
        ));
    }
    for package in allowed.iter().filter(|item| !prior.iter().any(|keep| keep.as_str() == item.as_str())) {
        if is_protected(package) {
            continue;
        }
        steps.push((
            Operation::Suspend { package: package.clone(), user, suspended: false },
            Operation::Suspend { package: package.clone(), user, suspended: true },
        ));
    }
    let existing: Vec<String> = view.entries.iter().map(|entry| entry.id.clone()).collect();
    let ids = resolve_ids(&envelope, &view, &existing, &steps)?;
    let mut envelope = envelope;
    for (offset, ((operation, inverse), step_id)) in steps.iter().zip(ids.iter()).enumerate() {
        match envelope.journal_state(step_id) {
            Some(JournalState::Applied) => match device.verified(operation) {
                Ok(true) => continue,
                Err(DeviceFailure::Disconnect) => {
                    return Ok(EditResult { outcome: EditOutcome::RecoverableDisconnect, envelope });
                }
                _ => {
                    return Ok(EditResult { outcome: EditOutcome::Blocked, envelope });
                }
            },
            Some(JournalState::Failed) => {
                return Ok(EditResult { outcome: EditOutcome::Blocked, envelope });
            }
            Some(JournalState::Pending) => match finish(device, envelope, step_id, operation)? {
                StepResult::Continue(next) => {
                    envelope = next;
                    continue;
                }
                StepResult::Stop(outcome, next) => {
                    return Ok(EditResult { outcome, envelope: next });
                }
            },
            None => (),
        }
        envelope = if offset == 0 {
            let Operation::LauncherPolicy { allowed } = operation else {
                return Err(EditError::Journal);
            };
            envelope
                .append_pending_launcher_policy(step_id, allowed.iter().map(|item| item.as_str().into()).collect())
                .map_err(|_| EditError::Journal)?
        } else {
            envelope
                .append_pending(step_id, &operation_json(operation), &operation_json(inverse))
                .map_err(|_| EditError::Journal)?
        };
        match persist_edit(device, &envelope) {
            Ok(()) => (),
            Err(PersistFail::Disconnect) => {
                return Ok(EditResult { outcome: EditOutcome::RecoverableDisconnect, envelope });
            }
            Err(PersistFail::Mirror(error)) => return Err(EditError::Mirror(error)),
        }
        match finish(device, envelope, step_id, operation)? {
            StepResult::Continue(next) => envelope = next,
            StepResult::Stop(outcome, next) => {
                return Ok(EditResult { outcome, envelope: next });
            }
        }
    }
    Ok(EditResult { outcome: EditOutcome::Complete, envelope })
}

enum StepResult {
    Continue(RecoveryEnvelopeV1),
    Stop(EditOutcome, RecoveryEnvelopeV1),
}

fn finish<D: MirrorStore + ApplyDevice>(
    device: &mut D,
    envelope: RecoveryEnvelopeV1,
    step_id: &str,
    operation: &Operation,
) -> Result<StepResult, EditError>
where
    D::Error: std::fmt::Debug,
{
    match mutate_verify(device, operation) {
        Ok(true) => {
            let envelope = envelope.mark_applied(step_id).map_err(|_| EditError::Journal)?;
            match persist_edit(device, &envelope) {
                Ok(()) => Ok(StepResult::Continue(envelope)),
                Err(PersistFail::Disconnect) => {
                    Ok(StepResult::Stop(EditOutcome::RecoverableDisconnect, envelope))
                }
                Err(PersistFail::Mirror(error)) => Err(EditError::Mirror(error)),
            }
        }
        Err(DeviceFailure::Disconnect) => Ok(StepResult::Stop(EditOutcome::RecoverableDisconnect, envelope)),
        _ => {
            let envelope = envelope.mark_failed(step_id).map_err(|_| EditError::Journal)?;
            match persist_edit(device, &envelope) {
                Ok(()) => Ok(StepResult::Stop(EditOutcome::Blocked, envelope)),
                Err(PersistFail::Disconnect) => {
                    Ok(StepResult::Stop(EditOutcome::RecoverableDisconnect, envelope))
                }
                Err(PersistFail::Mirror(error)) => Err(EditError::Mirror(error)),
            }
        }
    }
}

fn edit_id(sequence: u64) -> String {
    format!("00000000-0000-4000-8000-{sequence:012x}")
}

fn edit_sequence(id: &str) -> Option<u64> {
    let suffix = id.strip_prefix("00000000-0000-4000-8000-")?;
    u64::from_str_radix(suffix, 16)
        .ok()
        .filter(|sequence| (EDIT_BASE..EDIT_BASE + 1_000_000).contains(sequence))
}

/// Deterministic step ids: a fresh generation continues after the highest
/// recorded edit id, while a retried generation reuses its own entry ids so a
/// pending edit step resumes instead of duplicating. The resume window is
/// anchored to the generation's own launcher-policy entry (the seq already
/// located by `pending_generation_prior`): step-0 must equal that policy seq,
/// so a retry can never reach back into a previous generation's applied tail
/// on journal states alone. Every reused slot (applied prefix plus the pending
/// entry) is additionally verified to record the recomputed operation —
/// semantic compare against `ParsedOp` for both op and inverse — which also
/// enforces the documented identical-before/after/protected contract: a
/// retried `protected` set that would change the suspend plan mismatches and
/// is refused instead of silently completing.
fn resolve_ids(
    envelope: &RecoveryEnvelopeV1,
    view: &ParsedEnvelope,
    existing: &[String],
    steps: &[(Operation, Operation)],
) -> Result<Vec<String>, EditError> {
    if let Some(pending) = envelope.pending_id() {
        let Some(sequence) = edit_sequence(&pending) else {
            return Err(EditError::Blocked(
                "an unfinished non-edit transaction is pending; resume or roll it back first",
            ));
        };
        let Some((anchor, _)) = pending_generation_prior(view, sequence) else {
            return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
        };
        let start = anchor;
        if start < EDIT_BASE {
            return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
        }
        let Some(position) = sequence.checked_sub(start).and_then(|offset| usize::try_from(offset).ok()) else {
            return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
        };
        if position >= steps.len() {
            return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
        }
        let earlier = (0..position).all(|before| {
            envelope.journal_state(&edit_id(start + before as u64)) == Some(JournalState::Applied)
        });
        let pending_holds = envelope.journal_state(&edit_id(start + position as u64)) == Some(JournalState::Pending);
        let later = (position + 1..steps.len())
            .all(|after| envelope.journal_state(&edit_id(start + after as u64)).is_none());
        if !(earlier && pending_holds && later) {
            return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
        }
        for offset in 0..=position {
            let step_id = edit_id(start + offset as u64);
            let Some(recorded) = view.entries.iter().find(|entry| entry.id == step_id) else {
                return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
            };
            let (expected_op, expected_inverse) = &steps[offset];
            if !operation_matches(expected_op, &recorded.op)
                || !operation_matches(expected_inverse, &recorded.inverse)
            {
                return Err(EditError::Blocked("the pending edit entry does not match this edit; re-read before editing"));
            }
        }
        return Ok((0..steps.len()).map(|offset| edit_id(start + offset as u64)).collect());
    }
    let start = existing.iter().filter_map(|id| edit_sequence(id)).max().map(|max| max + 1).unwrap_or(EDIT_BASE);
    (0..steps.len())
        .map(|offset| start.checked_add(offset as u64).map(edit_id))
        .collect::<Option<Vec<_>>>()
        .ok_or(EditError::Journal)
}

/// Semantic compare of a recomputed `Operation` against a journaled `ParsedOp`.
/// Edit generations only contain launcher-policy and suspension steps; any
/// other recorded kind mismatches and refuses the resume.
fn operation_matches(expected: &Operation, recorded: &ParsedOp) -> bool {
    match (expected, recorded) {
        (Operation::LauncherPolicy { allowed }, ParsedOp::LauncherPolicy { allowed: journaled }) => {
            let mut want: Vec<&str> = allowed.iter().map(PackageId::as_str).collect();
            want.sort_unstable();
            let mut have: Vec<&str> = journaled.iter().map(String::as_str).collect();
            have.sort_unstable();
            want == have
        }
        (
            Operation::Suspend { package, user, suspended },
            ParsedOp::Suspend { package: journaled_package, user: journaled_user, suspended: journaled_suspended },
        ) => {
            package.as_str() == journaled_package.as_str()
                && u64::from(user.get()) == *journaled_user
                && suspended == journaled_suspended
        }
        _ => false,
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

fn persist_edit<D: MirrorStore>(device: &mut D, envelope: &RecoveryEnvelopeV1) -> Result<(), PersistFail>
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

fn pending_generation_prior(view: &ParsedEnvelope, pending_seq: u64) -> Option<(u64, Vec<String>)> {
    let mut best: Option<(u64, Vec<String>)> = None;
    for entry in &view.entries {
        let Some(seq) = edit_sequence(&entry.id) else {
            continue;
        };
        if seq > pending_seq {
            continue;
        }
        let ParsedOp::LauncherPolicy { .. } = &entry.op else {
            continue;
        };
        let ParsedOp::LauncherPolicy { allowed } = &entry.inverse else {
            continue;
        };
        let replace = match &best {
            None => true,
            Some((seen, _)) => seq > *seen,
        };
        if replace {
            best = Some((seq, allowed.clone()));
        }
    }
    best
}
