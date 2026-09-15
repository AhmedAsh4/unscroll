use super::model::{RecoveryEnvelopeV1, ValidationError};

pub fn envelope(value: &str) -> Result<RecoveryEnvelopeV1, ValidationError> {
    RecoveryEnvelopeV1::parse(value)
}
