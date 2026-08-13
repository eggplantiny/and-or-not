use aon_sim::{
    ArtifactBytes, ArtifactKind, BalanceProfile, JsonErrorCategory, NumericProfile, PackageError,
    PhysicalScaleProfile, ProfileKind, ProfileValidationError, SEMANTICS_VERSION_V1, Simulation,
    SimulationError, decode_balance_profile, decode_numeric_profile, decode_package,
    decode_physical_scale_profile, decode_scenario_manifest, encode_physical_scale_profile,
};

const SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/stage0-alpha.json"
));
const CAPACITY_BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/capacity-probe-alpha.json"
));
const RADIATION_BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/radiation-reference-alpha.json"
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
fn reference_contract_and_profiles_create_a_simulation() {
    let package = decode_package(artifact_bytes(SCENARIO, NUMERIC)).expect("fixtures are valid");

    assert_eq!(package.scenario_id(), "empty");
    assert_eq!(package.semantics_version().as_str(), SEMANTICS_VERSION_V1);
    assert_eq!(package.profiles().numeric().schema_version, 1);
    assert_eq!(package.profiles().numeric().kind, ProfileKind::Numeric);
    assert_eq!(package.profiles().physical_scale().schema_version, 1);
    assert_eq!(
        package.profiles().physical_scale().kind,
        ProfileKind::PhysicalScale
    );
    assert_eq!(package.profiles().balance().schema_version, 2);
    assert_eq!(package.profiles().balance().kind, ProfileKind::Balance);
    assert_eq!(package.profiles().balance().gate_switch_base_energy, 1);
    Simulation::new(package).expect("the declared contract matches the profiles");
}

#[test]
fn balance_schema_v5_is_latest_and_v2_still_requires_switch_energy() {
    let mut old_schema: serde_json::Value =
        serde_json::from_slice(BALANCE).expect("reference balance JSON");
    old_schema["schemaVersion"] = 1.into();
    let old_schema = serde_json::to_vec(&old_schema).expect("test JSON serializes");
    let error = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: &old_schema,
    })
    .expect_err("Balance schema v1 must be rejected");
    assert_eq!(
        error,
        PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Profile(ProfileKind::Balance),
            expected: 5,
            actual: 1,
        }
    );

    let mut missing_energy: serde_json::Value =
        serde_json::from_slice(BALANCE).expect("reference balance JSON");
    missing_energy
        .as_object_mut()
        .expect("balance profile is an object")
        .remove("gateSwitchBaseEnergy");
    let missing_energy = serde_json::to_vec(&missing_energy).expect("test JSON serializes");
    let error = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: &missing_energy,
    })
    .expect_err("gateSwitchBaseEnergy must be required");
    assert!(matches!(
        error,
        PackageError::InvalidJson {
            artifact: ArtifactKind::Profile(ProfileKind::Balance),
            category: JsonErrorCategory::Data,
            ..
        }
    ));
}

#[test]
fn balance_extension_reference_artifacts_match_their_golden_hashes() {
    for (bytes, expected_hash) in [
        (
            CAPACITY_BALANCE,
            "3fb2f3470804e9e95bde625ff615fc74ecff39fe0e8654371cd461178e1f3d8c",
        ),
        (
            RADIATION_BALANCE,
            "86d135f608076ec8c8c1f2702d28cc7c3c4792c4311c503ffa1532239d4589c9",
        ),
    ] {
        let profile: BalanceProfile =
            serde_json::from_slice(bytes).expect("reference balance profile JSON is valid");
        profile
            .validate()
            .expect("reference balance profile is valid");
        assert_eq!(
            profile
                .canonical_hash()
                .expect("reference balance profile hashes")
                .to_string(),
            expected_hash
        );
    }
}

#[test]
fn physical_and_balance_hashes_ignore_json_order_whitespace_and_profile_id() {
    let original_physical: PhysicalScaleProfile =
        serde_json::from_slice(PHYSICAL).expect("reference physical profile JSON is valid");
    let mut rewritten_physical: serde_json::Value =
        serde_json::from_slice(PHYSICAL).expect("reference physical profile JSON is valid");
    rewritten_physical["profileId"] = "same-physical-semantics-different-id".into();
    let rewritten_physical =
        serde_json::to_vec(&rewritten_physical).expect("rewritten physical JSON serializes");
    let rewritten_physical_text =
        std::str::from_utf8(&rewritten_physical).expect("rewritten physical JSON is UTF-8");
    assert!(
        rewritten_physical_text
            .find("\"kind\"")
            .expect("kind is present")
            < rewritten_physical_text
                .find("\"schemaVersion\"")
                .expect("schemaVersion is present"),
        "rewritten JSON must use a different top-level key order"
    );
    assert!(!rewritten_physical_text.contains('\n'));
    let rewritten_physical: PhysicalScaleProfile =
        serde_json::from_slice(&rewritten_physical).expect("rewritten physical profile is valid");
    assert_ne!(original_physical.profile_id, rewritten_physical.profile_id);
    assert_eq!(
        original_physical.canonical_hash(),
        rewritten_physical.canonical_hash()
    );

    let original_balance: BalanceProfile =
        serde_json::from_slice(BALANCE).expect("reference balance profile JSON is valid");
    let mut rewritten_balance: serde_json::Value =
        serde_json::from_slice(BALANCE).expect("reference balance profile JSON is valid");
    rewritten_balance["profileId"] = "same-balance-semantics-different-id".into();
    let rewritten_balance =
        serde_json::to_vec(&rewritten_balance).expect("rewritten balance JSON serializes");
    let rewritten_balance_text =
        std::str::from_utf8(&rewritten_balance).expect("rewritten balance JSON is UTF-8");
    assert!(
        rewritten_balance_text
            .find("\"kind\"")
            .expect("kind is present")
            < rewritten_balance_text
                .find("\"schemaVersion\"")
                .expect("schemaVersion is present"),
        "rewritten JSON must use a different top-level key order"
    );
    assert!(!rewritten_balance_text.contains('\n'));
    let rewritten_balance: BalanceProfile =
        serde_json::from_slice(&rewritten_balance).expect("rewritten balance profile is valid");
    assert_ne!(original_balance.profile_id, rewritten_balance.profile_id);
    assert_eq!(
        original_balance.canonical_hash(),
        rewritten_balance.canonical_hash()
    );
}

#[test]
fn standalone_physical_profile_artifact_round_trip_is_canonical_and_hash_stable() {
    let decoded = decode_physical_scale_profile(PHYSICAL).expect("reference profile decodes");
    let expected_hash = decoded.canonical_hash().expect("reference profile hashes");

    let canonical = encode_physical_scale_profile(&decoded).expect("valid profile encodes");
    assert!(canonical.ends_with(b"\n"));
    assert!(!canonical.windows(2).any(|pair| pair == b"\r\n"));

    let reparsed = decode_physical_scale_profile(&canonical).expect("canonical profile decodes");
    assert_eq!(reparsed, decoded);
    assert_eq!(reparsed.canonical_hash(), Ok(expected_hash));
    assert_eq!(
        encode_physical_scale_profile(&reparsed).expect("reparsed profile re-encodes"),
        canonical
    );
}

#[test]
fn standalone_numeric_and_balance_profile_decoders_are_strict_and_validated() {
    assert_eq!(
        decode_numeric_profile(NUMERIC)
            .expect("reference Numeric Profile decodes")
            .canonical_hash(),
        NumericProfile::reference_v1("metadata-only-id").canonical_hash()
    );
    assert_eq!(
        decode_balance_profile(BALANCE)
            .expect("reference Balance Profile decodes")
            .canonical_hash(),
        BalanceProfile::stage0_alpha("metadata-only-id").canonical_hash()
    );
}

#[test]
fn scenario_semantic_hash_includes_identity_but_excludes_paths_and_profile_ids() {
    let original = decode_scenario_manifest(SCENARIO).expect("reference Scenario decodes");
    let original_hash = original
        .canonical_hash()
        .expect("reference Scenario hashes");
    assert_eq!(
        original_hash.to_string(),
        "46a41702ea9dd3f404aa50f0c4952e5d773472c9a7f3410e8cacc8d68bde9ddd"
    );

    let mut relocated: serde_json::Value =
        serde_json::from_slice(SCENARIO).expect("reference Scenario JSON");
    for profile in ["numeric", "physicalScale", "balance"] {
        relocated["profiles"][profile]["path"] = format!("relocated/{profile}.json").into();
        relocated["profiles"][profile]["profileId"] = format!("display-{profile}").into();
    }
    let relocated = serde_json::to_vec(&relocated).expect("relocated Scenario serializes");
    assert_eq!(
        decode_scenario_manifest(&relocated)
            .expect("relocated Scenario decodes")
            .canonical_hash(),
        Ok(original_hash)
    );

    let mut renamed: serde_json::Value =
        serde_json::from_slice(SCENARIO).expect("reference Scenario JSON");
    renamed["scenarioId"] = "different-logical-scenario".into();
    let renamed = serde_json::to_vec(&renamed).expect("renamed Scenario serializes");
    assert_ne!(
        decode_scenario_manifest(&renamed)
            .expect("renamed Scenario decodes")
            .canonical_hash(),
        Ok(original_hash)
    );
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
fn unknown_scenario_field_is_rejected() {
    let mut scenario: serde_json::Value = serde_json::from_slice(SCENARIO).expect("fixture JSON");
    scenario["unexpected"] = serde_json::Value::Bool(true);
    let scenario = serde_json::to_vec(&scenario).expect("test JSON serializes");

    let error = decode_scenario_manifest(&scenario).expect_err("unknown fields must fail");
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
fn unsupported_schema_semantics_and_hash_algorithm_are_rejected() {
    let mut scenario: serde_json::Value = serde_json::from_slice(SCENARIO).expect("fixture JSON");
    scenario["schemaVersion"] = 5.into();
    let bytes = serde_json::to_vec(&scenario).expect("test JSON serializes");
    assert_eq!(
        decode_scenario_manifest(&bytes),
        Err(PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Scenario,
            expected: 4,
            actual: 5,
        })
    );

    scenario["schemaVersion"] = 1.into();
    scenario["semanticsVersion"] = "stage2-v999".into();
    let bytes = serde_json::to_vec(&scenario).expect("test JSON serializes");
    assert_eq!(
        decode_scenario_manifest(&bytes),
        Err(PackageError::UnsupportedSemanticsVersion {
            expected: "aon-semantics-v1",
            actual: "stage2-v999".to_owned(),
        })
    );

    scenario["semanticsVersion"] = "aon-semantics-v1".into();
    scenario["hashAlgorithm"] = "sha256".into();
    let bytes = serde_json::to_vec(&scenario).expect("test JSON serializes");
    assert_eq!(
        decode_scenario_manifest(&bytes),
        Err(PackageError::UnsupportedHashAlgorithm {
            expected: "blake3-v1",
            actual: "sha256".to_owned(),
        })
    );
}

#[test]
fn malformed_or_wrong_kind_profile_is_rejected_deterministically() {
    let invalid_fixed_one = br#"{
        "schemaVersion": 1,
        "profileId": "numeric-v1",
        "kind": "numeric",
        "fixedOne": 1,
        "overflow": "deterministic-error",
        "division": "floor-ceil-nearest-even",
        "geometryLength": "ceil-integer-euclidean-sqrt"
    }"#;
    assert!(matches!(
        decode_package(artifact_bytes(SCENARIO, invalid_fixed_one)),
        Err(PackageError::InvalidProfile {
            profile: ProfileKind::Numeric,
            error: ProfileValidationError::FixedOneMismatch { .. }
        })
    ));

    let wrong_kind = br#"{
        "schemaVersion": 1,
        "profileId": "numeric-v1",
        "kind": "balance",
        "fixedOne": 65536,
        "overflow": "deterministic-error",
        "division": "floor-ceil-nearest-even",
        "geometryLength": "ceil-integer-euclidean-sqrt"
    }"#;
    assert!(matches!(
        decode_package(artifact_bytes(SCENARIO, wrong_kind)),
        Err(PackageError::InvalidProfile {
            profile: ProfileKind::Numeric,
            error: ProfileValidationError::ProfileKindMismatch { .. }
        })
    ));
}

#[test]
fn profile_id_reference_is_bound_before_simulation_creation() {
    let different_id = String::from_utf8(NUMERIC.to_vec())
        .expect("fixture is UTF-8")
        .replace("numeric-v1", "different-numeric");

    assert_eq!(
        decode_package(artifact_bytes(SCENARIO, different_id.as_bytes())),
        Err(PackageError::ProfileReferenceMismatch {
            profile: ProfileKind::Numeric,
            expected_id: "numeric-v1".to_owned(),
            actual_id: "different-numeric".to_owned(),
        })
    );
}

#[test]
fn declared_contract_hash_mismatch_is_rejected_by_simulation_new() {
    let scenario = String::from_utf8(SCENARIO.to_vec())
        .expect("fixture is UTF-8")
        .replace(
            "fe92f0c723660040a3200254890c8a34ec3ed9e65fc242de1c0951e4ecd00469",
            &"0".repeat(64),
        );
    let package = decode_package(artifact_bytes(scenario.as_bytes(), NUMERIC))
        .expect("artifact shape and profile IDs are valid");

    assert!(matches!(
        Simulation::new(package),
        Err(SimulationError::ProfileHashMismatch {
            profile: ProfileKind::Numeric,
            ..
        })
    ));
}

#[test]
fn declared_physical_hash_mismatch_is_rejected_by_simulation_new() {
    let mut scenario: serde_json::Value =
        serde_json::from_slice(SCENARIO).expect("fixture JSON is valid");
    scenario["profiles"]["physicalScale"]["profileHash"] = "0".repeat(64).into();
    let scenario = serde_json::to_vec(&scenario).expect("test JSON serializes");
    let package = decode_package(artifact_bytes(&scenario, NUMERIC))
        .expect("artifact shape and profile IDs are valid");

    assert!(matches!(
        Simulation::new(package),
        Err(SimulationError::ProfileHashMismatch {
            profile: ProfileKind::PhysicalScale,
            ..
        })
    ));
}
