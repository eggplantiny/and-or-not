use aon_sim::{
    ArtifactBytes, ArtifactKind, JsonErrorCategory, PackageError, ProfileKind, decode_package,
    decode_scenario_manifest,
};

const SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/bootstrap-empty-v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/bootstrap-empty-v1.json"
));
const BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/bootstrap-empty-v1.json"
));

fn artifact_bytes<'a>(scenario: &'a [u8], numeric: &'a [u8]) -> ArtifactBytes<'a> {
    ArtifactBytes {
        scenario,
        numeric_profile: numeric,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    }
}

#[test]
fn empty_artifacts_decode() {
    let package = decode_package(artifact_bytes(SCENARIO, NUMERIC)).expect("fixtures are valid");

    assert_eq!(package.scenario_id(), "empty");
    assert_eq!(package.semantics_version(), "bootstrap-v0");
    assert_eq!(package.profiles().numeric().kind(), ProfileKind::Numeric);
    assert_eq!(
        package.profiles().physical_scale().kind(),
        ProfileKind::PhysicalScale
    );
    assert_eq!(package.profiles().balance().kind(), ProfileKind::Balance);
}

#[test]
fn malformed_json_is_a_typed_error() {
    let error = decode_scenario_manifest(b"{").expect_err("truncated JSON must fail");

    assert!(matches!(
        error,
        PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Eof,
            ..
        }
    ));
}

#[test]
fn unknown_field_is_rejected() {
    let scenario = br#"{
        "schemaVersion": 0,
        "scenarioId": "empty",
        "semanticsVersion": "bootstrap-v0",
        "initialWorld": { "kind": "empty" },
        "profiles": {
            "numeric": { "path": "n", "profileId": "bootstrap-empty-numeric-v1" },
            "physicalScale": { "path": "p", "profileId": "bootstrap-empty-physical-scale-v1" },
            "balance": { "path": "b", "profileId": "bootstrap-empty-balance-v1" }
        },
        "unexpected": true
    }"#;

    let error = decode_scenario_manifest(scenario).expect_err("unknown fields must fail");
    assert!(matches!(
        error,
        PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Data,
            ..
        }
    ));
}

#[test]
fn unsupported_schema_is_rejected_before_package_creation() {
    let scenario = br#"{
        "schemaVersion": 1,
        "scenarioId": "empty",
        "semanticsVersion": "bootstrap-v0",
        "initialWorld": { "kind": "empty" },
        "profiles": {
            "numeric": { "path": "n", "profileId": "bootstrap-empty-numeric-v1" },
            "physicalScale": { "path": "p", "profileId": "bootstrap-empty-physical-scale-v1" },
            "balance": { "path": "b", "profileId": "bootstrap-empty-balance-v1" }
        }
    }"#;

    assert_eq!(
        decode_scenario_manifest(scenario),
        Err(PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Scenario,
            expected: 0,
            actual: 1,
        })
    );
}

#[test]
fn unsupported_semantics_version_is_rejected() {
    let scenario = br#"{
        "schemaVersion": 0,
        "scenarioId": "empty",
        "semanticsVersion": "stage2-v999",
        "initialWorld": { "kind": "empty" },
        "profiles": {
            "numeric": { "path": "n", "profileId": "bootstrap-empty-numeric-v1" },
            "physicalScale": { "path": "p", "profileId": "bootstrap-empty-physical-scale-v1" },
            "balance": { "path": "b", "profileId": "bootstrap-empty-balance-v1" }
        }
    }"#;

    assert_eq!(
        decode_scenario_manifest(scenario),
        Err(PackageError::UnsupportedSemanticsVersion {
            expected: "bootstrap-v0",
            actual: "stage2-v999".to_owned(),
        })
    );
}

#[test]
fn wrong_profile_kind_is_rejected() {
    let wrong_numeric = br#"{
        "schemaVersion": 0,
        "profileId": "wrong",
        "kind": "balance"
    }"#;

    assert_eq!(
        decode_package(artifact_bytes(SCENARIO, wrong_numeric)),
        Err(PackageError::ProfileKindMismatch {
            expected: ProfileKind::Numeric,
            actual: ProfileKind::Balance,
        })
    );
}

#[test]
fn profile_bytes_must_match_the_scenario_reference() {
    let wrong_numeric = br#"{"schemaVersion":0,"profileId":"different-numeric","kind":"numeric"}"#;

    assert_eq!(
        decode_package(artifact_bytes(SCENARIO, wrong_numeric)),
        Err(PackageError::ProfileReferenceMismatch {
            profile: ProfileKind::Numeric,
            expected_id: "bootstrap-empty-numeric-v1".to_owned(),
            actual_id: "different-numeric".to_owned(),
        })
    );
}
