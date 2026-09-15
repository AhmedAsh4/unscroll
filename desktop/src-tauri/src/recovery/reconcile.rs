use super::model::{RecoveryEnvelopeV1, ValidationError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Consistent,
    RepairPrivate(String),
    RepairShared(String),
    MissingShared,
    BothMissing,
    Corruption,
    Fork,
    BaselineMismatch,
    DeviceBindingMismatch,
    UnexplainedDeviceState,
}


pub fn reconcile_checked(
    private: Option<&str>, shared: Option<&str>, serial: &str, fingerprint: &str,
    expected_baseline_hash: &str, device_state_explained: bool,
) -> Outcome {
    if !device_state_explained { return Outcome::UnexplainedDeviceState; }
    match (private, shared) {
        (None, None) => Outcome::BothMissing,
        (Some(private), None) => match parse_checked(private, serial, fingerprint, expected_baseline_hash) { Ok(_) => Outcome::MissingShared, Err(ValidationError::Baseline) => Outcome::BaselineMismatch, Err(ValidationError::DeviceBinding) => Outcome::DeviceBindingMismatch, Err(_) => Outcome::Corruption },
        (None, Some(shared)) => match parse_checked(shared, serial, fingerprint, expected_baseline_hash) {
            Ok(envelope) => Outcome::RepairPrivate(envelope.canonical_json()),
            Err(ValidationError::Baseline) => Outcome::BaselineMismatch,
            Err(ValidationError::DeviceBinding) => Outcome::DeviceBindingMismatch,
            Err(_) => Outcome::Corruption,
        },
        (Some(private), Some(shared)) => {
            let private = match parse_checked(private, serial, fingerprint, expected_baseline_hash) { Ok(v) => v, Err(ValidationError::Baseline) => return Outcome::BaselineMismatch, Err(ValidationError::DeviceBinding) => return Outcome::DeviceBindingMismatch, Err(_) => return Outcome::Corruption };
            let shared = match parse_checked(shared, serial, fingerprint, expected_baseline_hash) { Ok(v) => v, Err(ValidationError::Baseline) => return Outcome::BaselineMismatch, Err(ValidationError::DeviceBinding) => return Outcome::DeviceBindingMismatch, Err(_) => return Outcome::Corruption };
            if private.canonical_json() == shared.canonical_json() { return Outcome::Consistent; }
            if private.is_strict_prefix_of(&shared).is_ok() { Outcome::RepairPrivate(shared.canonical_json()) }
            else if shared.is_strict_prefix_of(&private).is_ok() { Outcome::RepairShared(private.canonical_json()) }
            else { Outcome::Fork }

        }
    }
}

fn parse_checked(value: &str, serial: &str, fingerprint: &str, baseline: &str) -> Result<RecoveryEnvelopeV1, ValidationError> {
    let envelope = RecoveryEnvelopeV1::parse_for_device(value, serial, fingerprint, 0)?;
    if envelope.baseline_hash() != baseline { return Err(ValidationError::Baseline); }
    Ok(envelope)
}
