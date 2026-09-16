use crate::{adb::PackageId, policy::{Operation, Plan, PlannedOperation, Requirement}, recovery::{mirror::{persist, MirrorError, MirrorStore}, model::{JournalState, RecoveryEnvelopeV1}}};
use super::{journal, ApplyDevice, ApplyOutcome, ApplyResult, Decision, DeviceFailure};
#[derive(Debug, Clone, PartialEq, Eq)] pub enum ApplyError { Mirror(MirrorError), Journal }

pub fn apply<D: ApplyDevice + MirrorStore>(device: &mut D, mut envelope: RecoveryEnvelopeV1, plan: &Plan, decision: Option<Decision>) -> Result<ApplyResult, ApplyError> {
    let mut completed: Vec<_> = plan.operations.iter().enumerate().filter_map(|(index, step)| (envelope.journal_state(&id(index)) == Some(JournalState::Applied)).then(|| step.clone())).collect();
    if completed.iter().rev().enumerate().any(|(offset, _)| envelope.journal_state(&id(1_000_000 + offset)).is_some()) {
        return rollback(device, envelope, &completed, Vec::new());
    }
    let mut partial_protection = Vec::new();
    for (index, step) in plan.operations.iter().enumerate() {
        let step_id = id(index);
        if envelope.journal_state(&step_id) == Some(JournalState::Applied) { continue; }
        if envelope.journal_state(&step_id) == Some(JournalState::Failed) {
            match (&step.operation, step.requirement, decision) {
                (Operation::Suspend { package, .. }, Requirement::UserResolvable, Some(Decision::Continue)) => match continue_policy(device, envelope, index, package, &mut partial_protection)? { (next, None) => { envelope = next; continue; }, (next, Some(outcome)) => return Ok(result(outcome, next, partial_protection)), },
                (Operation::Suspend { .. }, Requirement::UserResolvable, Some(Decision::Rollback)) => return rollback(device, envelope, &completed, partial_protection),
                (Operation::Suspend { package, .. }, Requirement::UserResolvable, _) => return Ok(result(ApplyOutcome::DecisionRequired { package: package.clone() }, envelope, partial_protection)),
                (_, Requirement::Optional, _) => { partial_protection.push(step.description.clone()); continue; }
                _ => return Ok(result(ApplyOutcome::InconsistentState, envelope, partial_protection)),
            }
        }
        if let Some(pending) = envelope.pending_id() {
            if pending != step_id { return Ok(result(ApplyOutcome::InconsistentState, envelope, partial_protection)); }
            if matches!(step.operation, Operation::Home { .. }) {
                match device.verified(&step.operation) {
                    Ok(true) => { envelope = applied(device, envelope, &pending)?; completed.push(step.clone()); continue; }
                    Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)),
                    _ => match device.chooser() {
                        Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)),
                        Err(_) => return Ok(result(ApplyOutcome::InconsistentState, envelope, partial_protection)),
                        Ok(()) if matches!(decision, Some(Decision::HomeConfirmed)) => match device.verified(&step.operation) {
                            Ok(true) => { envelope = applied(device, envelope, &pending)?; completed.push(step.clone()); continue; }
                            Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)),
                            _ => return Ok(result(ApplyOutcome::ChooserRequired, envelope, partial_protection)),
                        },
                        Ok(()) => return Ok(result(ApplyOutcome::ChooserRequired, envelope, partial_protection)),
                    },
                }
            }
            match mutate_verify(device, &step.operation) {
                Ok(true) => { envelope = applied(device, envelope, &pending)?; completed.push(step.clone()); continue; }
                Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)),
                Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) if step.requirement == Requirement::Optional => { envelope = envelope.mark_failed(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; partial_protection.push(step.description.clone()); continue; }
                Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) if matches!((&step.operation, step.requirement, decision), (Operation::Suspend { .. }, Requirement::UserResolvable, Some(Decision::Continue))) => { envelope = envelope.mark_failed(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; let Operation::Suspend { package, .. } = &step.operation else { unreachable!() }; match continue_policy(device, envelope, index, package, &mut partial_protection)? { (next, None) => { envelope = next; continue; }, (next, Some(outcome)) => return Ok(result(outcome, next, partial_protection)), } }
                Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) => return forward_failed(device, envelope, &pending, index, step, &completed, partial_protection, decision),
            }
        }
        if matches!(step.operation, Operation::Verify) {
            match mutate_verify(device, &step.operation) { Ok(true) => continue, Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)), _ => return rollback(device, envelope, &completed, partial_protection) }
        }
        let pending = if let Operation::LauncherPolicy { allowed } = &step.operation { envelope.append_pending_launcher_policy(&step_id, allowed.iter().map(|p| p.as_str().into()).collect()).map_err(|_| ApplyError::Journal)? } else { journal::pending(&envelope, index, &step.operation, &step.inverse).map_err(|_| ApplyError::Journal)?.1 };
        persist(device, &pending).map_err(ApplyError::Mirror)?; envelope = pending;
        match mutate_verify(device, &step.operation) {
            Ok(true) => { envelope = applied(device, envelope, &step_id)?; completed.push(step.clone()); }
            Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)),
            Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) if step.requirement == Requirement::Optional => { envelope = envelope.mark_failed(&step_id).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; partial_protection.push(step.description.clone()); continue; }
            Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) if matches!(step.operation, Operation::Home { .. }) => {
                match device.chooser() { Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)), Err(_) => return Ok(result(ApplyOutcome::InconsistentState, envelope, partial_protection)), Ok(()) => () }
                if matches!(decision, Some(Decision::HomeConfirmed)) { match device.verified(&step.operation) { Ok(true) => { envelope = applied(device, envelope, &step_id)?; completed.push(step.clone()); continue; }, Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)), _ => () } }
                return Ok(result(ApplyOutcome::ChooserRequired, envelope, partial_protection));
            }
            Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) if matches!((&step.operation, step.requirement, decision), (Operation::Suspend { .. }, Requirement::UserResolvable, Some(Decision::Continue))) => { envelope = envelope.mark_failed(&step_id).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; let Operation::Suspend { package, .. } = &step.operation else { unreachable!() }; match continue_policy(device, envelope, index, package, &mut partial_protection)? { (next, None) => { envelope = next; continue; }, (next, Some(outcome)) => return Ok(result(outcome, next, partial_protection)), } }
            Ok(false) | Err(DeviceFailure::Command | DeviceFailure::Inconsistent) => return forward_failed(device, envelope, &step_id, index, step, &completed, partial_protection, decision),
        }
    }
    Ok(result(ApplyOutcome::Complete, envelope, partial_protection))
}

fn id(index: usize) -> String { format!("00000000-0000-4000-8000-{index:012x}") }
fn result(outcome: ApplyOutcome, envelope: RecoveryEnvelopeV1, partial_protection: Vec<String>) -> ApplyResult { ApplyResult { outcome, envelope, partial_protection } }
fn applied<D: MirrorStore>(device: &mut D, envelope: RecoveryEnvelopeV1, entry: &str) -> Result<RecoveryEnvelopeV1, ApplyError> { let envelope = envelope.mark_applied(entry).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; Ok(envelope) }

fn forward_failed<D: ApplyDevice + MirrorStore>(device: &mut D, envelope: RecoveryEnvelopeV1, entry: &str, index: usize, step: &PlannedOperation, completed: &[PlannedOperation], partial_protection: Vec<String>, decision: Option<Decision>) -> Result<ApplyResult, ApplyError> {
    let envelope = envelope.mark_failed(entry).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?;
    match (&step.operation, step.requirement, decision) {
        (Operation::Suspend { package, .. }, Requirement::UserResolvable, Some(Decision::Continue)) => { let mut partial = partial_protection; match continue_policy(device, envelope, index, package, &mut partial)? { (envelope, None) => Ok(result(ApplyOutcome::Complete, envelope, partial)), (envelope, Some(outcome)) => Ok(result(outcome, envelope, partial)), } }
        (Operation::Suspend { .. }, Requirement::UserResolvable, Some(Decision::Rollback)) => rollback(device, envelope, completed, partial_protection),
        (Operation::Suspend { package, .. }, Requirement::UserResolvable, _) => Ok(result(ApplyOutcome::DecisionRequired { package: package.clone() }, envelope, partial_protection)),
        (_, Requirement::Optional, _) => { let mut partial = partial_protection; partial.push(step.description.clone()); Ok(result(ApplyOutcome::Complete, envelope, partial)) }
        _ => rollback(device, envelope, completed, partial_protection),
    }
}

fn continue_policy<D: ApplyDevice + MirrorStore>(device: &mut D, mut envelope: RecoveryEnvelopeV1, index: usize, package: &PackageId, partial: &mut Vec<String>) -> Result<(RecoveryEnvelopeV1, Option<ApplyOutcome>), ApplyError> {
    let policy_id = id(500_000 + index);
    match envelope.journal_state(&policy_id) {
        Some(JournalState::Applied) => { partial.push(package.as_str().into()); return Ok((envelope, None)); },
        Some(JournalState::Failed) => return Ok((envelope, Some(ApplyOutcome::InconsistentState))),
        None => { let mut allowed = envelope.active_allowed_packages(); allowed.push(package.as_str().into()); envelope = envelope.append_pending_launcher_policy(&policy_id, allowed).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; }
        Some(JournalState::Pending) => (),
    }
    let allowed = envelope.active_allowed_packages().into_iter().map(|value| PackageId::parse(&value).map_err(|_| ApplyError::Journal)).collect::<Result<Vec<_>, _>>()?;
    match device.verified(&Operation::LauncherPolicy { allowed }) {
        Ok(true) => { envelope = applied(device, envelope, &policy_id)?; }
        Err(DeviceFailure::Disconnect) => return Ok((envelope, Some(ApplyOutcome::RecoverableDisconnect))),
        _ => return Ok((envelope, Some(ApplyOutcome::InconsistentState))),
    }
    partial.push(package.as_str().into()); Ok((envelope, None))
}

fn rollback<D: ApplyDevice + MirrorStore>(device: &mut D, mut envelope: RecoveryEnvelopeV1, completed: &[PlannedOperation], partial_protection: Vec<String>) -> Result<ApplyResult, ApplyError> {
    for (offset, step) in completed.iter().rev().enumerate() {
        let inverse_id = id(1_000_000 + offset);
        match envelope.journal_state(&inverse_id) {
            Some(JournalState::Applied) => continue,
            Some(JournalState::Failed) => return Ok(result(ApplyOutcome::InconsistentState, envelope, partial_protection)),
            Some(JournalState::Pending) => (),
            None => { let pending = journal::pending(&envelope, 1_000_000 + offset, &step.inverse, &step.operation).map_err(|_| ApplyError::Journal)?.1; persist(device, &pending).map_err(ApplyError::Mirror)?; envelope = pending; }
        }
        match mutate_verify(device, &step.inverse) {
            Ok(true) => envelope = applied(device, envelope, &inverse_id)?,
            Err(DeviceFailure::Disconnect) => return Ok(result(ApplyOutcome::RecoverableDisconnect, envelope, partial_protection)),
            _ => return Ok(result(ApplyOutcome::InconsistentState, envelope, partial_protection)),
        }
    }
    Ok(result(ApplyOutcome::RolledBack, envelope, partial_protection))
}

fn mutate_verify<D: ApplyDevice>(device: &mut D, operation: &Operation) -> Result<bool, DeviceFailure> { match device.mutate(operation) { Err(DeviceFailure::Disconnect) => Err(DeviceFailure::Disconnect), _ => device.verified(operation) } }
