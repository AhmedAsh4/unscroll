#[path = "../src/recovery/model.rs"]
mod model;

use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/fixtures/recovery-v1");
    fs::read_to_string(root.join(name)).expect("fixture")
}

#[test]
fn accepts_the_shared_valid_contract_fixtures() {
    for name in [
        "valid/new-baseline.json",
        "valid/pending-mutation.json",
        "valid/applied-mutation.json",
        "valid/maintenance-open.json",
        "valid/strict-extension.json",
        "valid/stale-copy.json",
    ] {
        let envelope = model::RecoveryEnvelopeV1::parse(&fixture(name))
            .unwrap_or_else(|error| panic!("{name} should be valid: {error:?}"));
        assert_eq!(envelope.canonical_json(), fixture(name).trim_end());
        assert_eq!(envelope.checksum(), envelope.computed_checksum());
    }
}

#[test]
fn rejects_corrupt_and_ambiguous_shared_fixtures() {
    for (name, classification) in [
        (
            "invalid/checksum-failure.json",
            model::ValidationError::Checksum,
        ),
        (
            "invalid/forked-history.json",
            model::ValidationError::History,
        ),
        (
            "invalid/baseline-mismatch.json",
            model::ValidationError::Baseline,
        ),
        (
            "invalid/unknown-operation.json",
            model::ValidationError::Operation,
        ),
    ] {
        assert_eq!(
            model::RecoveryEnvelopeV1::parse(&fixture(name)).unwrap_err(),
            classification,
            "{name}",
        );
    }
}

#[test]
fn detects_baseline_mutation_between_revisions() {
    let older = model::RecoveryEnvelopeV1::parse(&fixture("valid/pending-mutation.json")).unwrap();
    let newer = model::RecoveryEnvelopeV1::parse(&fixture("valid/applied-mutation.json")).unwrap();
    assert!(older.is_strict_prefix_of(&newer).is_ok());

    let mutated = fixture("invalid/baseline-mismatch.json");
    let mutated = model::RecoveryEnvelopeV1::parse_unchecked_checksum(&mutated).unwrap();
    assert_eq!(
        older.is_strict_prefix_of(&mutated),
        Err(model::ValidationError::Baseline)
    );
}

#[test]
fn rejects_hostile_fields_and_non_reversing_inverses() {
    let valid = fixture("valid/applied-mutation.json");
    for (input, classification) in [
        (
            valid.replace("recovery-v1", "recovery-v2"),
            model::ValidationError::Schema,
        ),
        (
            valid.replace("com.social", "bad package"),
            model::ValidationError::Field,
        ),
        (
            valid.replace("\"user_id\":0", "\"user_id\":1000000"),
            model::ValidationError::Field,
        ),
        (
            valid.replace("\"checksum\":\"", "\"checksum\":\"z"),
            model::ValidationError::Checksum,
        ),
        (
            valid.replace("google/pixel/test", &"x".repeat(513)),
            model::ValidationError::Field,
        ),
        (
            valid.replace("\"suspended\":false", "\"suspended\":true"),
            model::ValidationError::Operation,
        ),
    ] {
        assert_eq!(
            model::RecoveryEnvelopeV1::parse(&input),
            Err(classification)
        );
    }
    assert_eq!(
        model::RecoveryEnvelopeV1::parse(&fixture("valid/strict-extension.json").replace(
            "33333333-3333-3333-3333-333333333333",
            "22222222-2222-2222-2222-222222222222"
        )),
        Err(model::ValidationError::Operation),
    );
    for input in [
        fixture("valid/pending-mutation.json").replace("\"revision\":1", "\"revision\":2"),
        fixture("valid/applied-mutation.json").replace("\"revision\":2", "\"revision\":1"),
    ] {
        assert_eq!(
            model::RecoveryEnvelopeV1::parse(&input),
            Err(model::ValidationError::History),
        );
    }
}

#[test]
fn rejects_a_well_formed_envelope_bound_to_another_device() {
    let input = fixture("valid/new-baseline.json");
    assert!(
        model::RecoveryEnvelopeV1::parse_for_device(&input, "ABC123", "google/pixel/test", 0)
            .is_ok()
    );
    assert_eq!(
        model::RecoveryEnvelopeV1::parse_for_device(&input, "OTHER", "google/pixel/test", 0),
        Err(model::ValidationError::DeviceBinding),
    );
}
