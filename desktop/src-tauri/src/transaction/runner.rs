use crate::{policy::{Operation, Plan, PlannedOperation, Requirement}, recovery::{mirror::{persist, MirrorError, MirrorStore}, model::{JournalState, RecoveryEnvelopeV1}}};
use super::{journal, ApplyDevice, ApplyOutcome, ApplyResult, Decision, DeviceFailure};
#[derive(Debug, Clone, PartialEq, Eq)] pub enum ApplyError { Mirror(MirrorError), Journal }

pub fn apply<D: ApplyDevice + MirrorStore>(device: &mut D, mut envelope: RecoveryEnvelopeV1, plan: &Plan, decision: Option<Decision>) -> Result<ApplyResult, ApplyError> {
    let mut completed = Vec::new(); let mut partial_protection = Vec::new();
    for (index, step) in plan.operations.iter().enumerate() {
        let expected_id = format!("00000000-0000-4000-8000-{index:012x}");
        if envelope.journal_state(&expected_id) == Some(JournalState::Applied) { completed.push(step.clone()); continue; }        if envelope.journal_state(&expected_id) == Some(JournalState::Failed) {
            match (&step.operation, step.requirement, decision) {
                (Operation::Suspend { package, .. }, Requirement::UserResolvable, Some(Decision::Continue)) => {
                    let mut allowed = envelope.active_allowed_packages(); allowed.push(package.as_str().into());
                    let policy_id = format!("00000000-0000-4000-8000-{:012x}", 500000 + index);
                    if envelope.journal_state(&policy_id) != Some(JournalState::Applied) {
                        let pending = envelope.append_pending_launcher_policy(&policy_id, allowed).map_err(|_| ApplyError::Journal)?;
                        persist(device, &pending).map_err(ApplyError::Mirror)?;
                        if !device.verified(&Operation::LauncherPolicy { allowed: vec![package.clone()] }).unwrap_or(false) { return Ok(ApplyResult { outcome: ApplyOutcome::InconsistentState, envelope: pending, partial_protection }); }
                        envelope = pending.mark_applied(&policy_id).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?;
                    }
                    partial_protection.push(package.as_str().into()); continue;
                }
                (Operation::Suspend { package: _, .. }, Requirement::UserResolvable, Some(Decision::Rollback)) => return rollback(device, envelope, &completed, partial_protection),
                (Operation::Suspend { package, .. }, Requirement::UserResolvable, _) => return Ok(ApplyResult { outcome: ApplyOutcome::DecisionRequired { package: package.clone() }, envelope, partial_protection }),
                _ => return Ok(ApplyResult { outcome: ApplyOutcome::InconsistentState, envelope, partial_protection }),
            }
        }
        if let Some(pending) = envelope.pending_id() {
            if pending != expected_id { return Ok(ApplyResult { outcome: ApplyOutcome::InconsistentState, envelope, partial_protection }); }
            if matches!(step.operation, Operation::Home { .. }) {
                match device.verified(&step.operation) {
                    Ok(true) => { envelope = envelope.mark_applied(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; completed.push(step.clone()); continue; }
                    Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }),
                    _ => match device.chooser() { Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }), Err(_) => return Ok(ApplyResult { outcome: ApplyOutcome::InconsistentState, envelope, partial_protection }), Ok(()) => { if matches!(decision, Some(Decision::HomeConfirmed)) { match device.verified(&step.operation) { Ok(true) => { envelope = envelope.mark_applied(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; completed.push(step.clone()); continue; }, Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }), _ => () } } return Ok(ApplyResult { outcome: ApplyOutcome::ChooserRequired, envelope, partial_protection }) } },
                }
            }            match mutate_verify(device, &step.operation) {
                Ok(true) => { envelope = envelope.mark_applied(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; completed.push(step.clone()); continue; }
                Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }),
                _ => return Ok(ApplyResult { outcome: ApplyOutcome::InconsistentState, envelope, partial_protection }),
            }
        }        if matches!(step.operation, Operation::Verify) {
            if matches!(step.operation, Operation::Home { .. }) {
                match device.verified(&step.operation) {
                    Ok(true) => { envelope = envelope.mark_applied(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; completed.push(step.clone()); continue; }
                    Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }),
                    _ => match device.chooser() { Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }), Err(_) => return Ok(ApplyResult { outcome: ApplyOutcome::InconsistentState, envelope, partial_protection }), Ok(()) => { if matches!(decision, Some(Decision::HomeConfirmed)) { match device.verified(&step.operation) { Ok(true) => { envelope = envelope.mark_applied(&pending).map_err(|_| ApplyError::Journal)?; persist(device, &envelope).map_err(ApplyError::Mirror)?; completed.push(step.clone()); continue; }, Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }), _ => () } } return Ok(ApplyResult { outcome: ApplyOutcome::ChooserRequired, envelope, partial_protection }) } },
                }
            }            match mutate_verify(device, &step.operation) {
                Ok(true) => continue,
                Err(DeviceFailure::Disconnect) => return Ok(ApplyResult { outcome: ApplyOutcome::RecoverableDisconnect, envelope, partial_protection }),
                _ => return rollback(device, envelope, &completed, partial_protection),
            }
        }
        let (id, pending) = if matches!(step.operation, Operation::LauncherPolicy { .. }) { let id=format!("00000000-0000-4000-8000-{index:012x}"); let allowed=match &step.operation { Operation::LauncherPolicy { allowed } => allowed.iter().map(|p| p.as_str().into()).collect(), _ => unreachable!() }; (id.clone(), envelope.append_pending_launcher_policy(&id, allowed).map_err(|_| ApplyError::Journal)?) } else { journal::pending(&envelope,index,&step.operation,&step.inverse).map_err(|_| ApplyError::Journal)? };
        persist(device,&pending).map_err(ApplyError::Mirror)?; envelope=pending;
        let actual = mutate_verify(device, &step.operation); if matches!(actual, Ok(true)) { completed.push(step.clone()); }
        match actual {
            Ok(true)=>{ envelope=envelope.mark_applied(&id).map_err(|_|ApplyError::Journal)?; persist(device,&envelope).map_err(ApplyError::Mirror)?; }
            Err(DeviceFailure::Disconnect)=>return Ok(ApplyResult{outcome:ApplyOutcome::RecoverableDisconnect,envelope,partial_protection}),
            Err(DeviceFailure::Inconsistent)=>return Ok(ApplyResult{outcome:ApplyOutcome::InconsistentState,envelope,partial_protection}),
            Ok(false)|Err(DeviceFailure::Command)=>match (&step.operation,step.requirement) {
                (Operation::Home{..},_)=>{ match device.chooser() { Err(DeviceFailure::Disconnect)=>return Ok(ApplyResult{outcome:ApplyOutcome::RecoverableDisconnect,envelope,partial_protection}), Err(_)=>return Ok(ApplyResult{outcome:ApplyOutcome::InconsistentState,envelope,partial_protection}), Ok(())=>() }; if matches!(decision, Some(Decision::HomeConfirmed)) { match mutate_verify(device, &step.operation) { Ok(true)=>{ envelope=envelope.mark_applied(&id).map_err(|_|ApplyError::Journal)?; persist(device,&envelope).map_err(ApplyError::Mirror)?; continue }, Err(DeviceFailure::Disconnect)=>return Ok(ApplyResult{outcome:ApplyOutcome::RecoverableDisconnect,envelope,partial_protection}), _=>() } } return Ok(ApplyResult{outcome:ApplyOutcome::ChooserRequired,envelope,partial_protection}) },
                (Operation::Suspend{package,..},Requirement::UserResolvable)=> {
                    envelope=envelope.mark_failed(&id).map_err(|_|ApplyError::Journal)?; persist(device,&envelope).map_err(ApplyError::Mirror)?;
                    match decision { Some(Decision::Continue)=>{ let mut allowed=envelope.active_allowed_packages(); allowed.push(package.as_str().into()); let policy_id=format!("00000000-0000-4000-8000-{:012x}",500000+index); let pending=envelope.append_pending_launcher_policy(&policy_id,allowed).map_err(|_|ApplyError::Journal)?; persist(device,&pending).map_err(ApplyError::Mirror)?; if !device.verified(&Operation::LauncherPolicy { allowed: vec![package.clone()] }).unwrap_or(false) { return Ok(ApplyResult{outcome:ApplyOutcome::InconsistentState,envelope:pending,partial_protection}); } envelope=pending.mark_applied(&policy_id).map_err(|_|ApplyError::Journal)?; persist(device,&envelope).map_err(ApplyError::Mirror)?; partial_protection.push(package.as_str().into()); continue }, Some(Decision::Rollback)=>return rollback(device,envelope,&completed,partial_protection), Some(Decision::HomeConfirmed)|Some(Decision::HomeCancelled)|None=>return Ok(ApplyResult{outcome:ApplyOutcome::DecisionRequired{package:package.clone()},envelope,partial_protection}) }
                }
                (_,Requirement::Optional)=>{ envelope=envelope.mark_failed(&id).map_err(|_|ApplyError::Journal)?; persist(device,&envelope).map_err(ApplyError::Mirror)?; partial_protection.push(step.description.clone()); }
                _=>{ envelope=envelope.mark_failed(&id).map_err(|_|ApplyError::Journal)?; persist(device,&envelope).map_err(ApplyError::Mirror)?; return rollback(device,envelope,&completed,partial_protection) }
            }
        }
    }
    Ok(ApplyResult{outcome:ApplyOutcome::Complete,envelope,partial_protection})
}
fn rollback<D: ApplyDevice + MirrorStore>(device:&mut D,mut envelope:RecoveryEnvelopeV1,completed:&[PlannedOperation],partial_protection:Vec<String>)->Result<ApplyResult,ApplyError>{for(offset,step)in completed.iter().rev().enumerate(){let(id,pending)=journal::pending(&envelope,1_000_000+offset,&step.inverse,&step.operation).map_err(|_|ApplyError::Journal)?;persist(device,&pending).map_err(ApplyError::Mirror)?;envelope=pending;match mutate_verify(device, &step.inverse){Ok(true)=>{envelope=envelope.mark_applied(&id).map_err(|_|ApplyError::Journal)?;persist(device,&envelope).map_err(ApplyError::Mirror)?},Ok(false)|Err(DeviceFailure::Command|DeviceFailure::Inconsistent)=>return Ok(ApplyResult{outcome:ApplyOutcome::InconsistentState,envelope,partial_protection}),Err(DeviceFailure::Disconnect)=>return Ok(ApplyResult{outcome:ApplyOutcome::RecoverableDisconnect,envelope,partial_protection})}}Ok(ApplyResult{outcome:ApplyOutcome::RolledBack,envelope,partial_protection})}






fn mutate_verify<D: ApplyDevice>(device: &mut D, operation: &Operation) -> Result<bool, DeviceFailure> {
    match device.mutate(operation) { Err(DeviceFailure::Disconnect) => Err(DeviceFailure::Disconnect), _ => device.verified(operation) }
}











