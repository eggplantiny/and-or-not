use aon_sim::{
    BALANCE_SCHEMA_VERSION_V4, BalanceProfile, JsonErrorCategory, PackageError, ProfileKind,
    ProfileValidationError, Rational, decode_balance_profile,
};

const BALANCE_V2: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/stage0-alpha.json"
));
const BALANCE_V3: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m2-power-probe-alpha.json"
));
const BALANCE_V4: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m3-capacity-support-alpha.json"
));

fn encode_json(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("test JSON encodes")
}

fn assert_data_error(bytes: &[u8]) {
    assert!(matches!(
        decode_balance_profile(bytes),
        Err(PackageError::InvalidJson {
            category: JsonErrorCategory::Data,
            ..
        })
    ));
}

#[test]
fn v4_reference_fixture_is_strict_valid_and_matches_the_constructor() {
    let decoded = decode_balance_profile(BALANCE_V4).expect("Balance v4 fixture decodes");
    let reencoded = encode_json(
        &serde_json::from_slice(BALANCE_V4).expect("Balance v4 fixture is strict JSON"),
    );
    assert_eq!(
        decode_balance_profile(&reencoded).expect("re-encoded Balance v4 decodes"),
        decoded
    );
    assert_eq!(decoded.schema_version, BALANCE_SCHEMA_VERSION_V4);
    assert_eq!(
        decoded,
        BalanceProfile::capacity_support_probe_alpha("balance-s1-m3-capacity-support-alpha")
    );
    assert_eq!(
        decoded
            .capacity_support_probe
            .expect("v4 support probe")
            .support_power_per_ncu,
        Rational::new(1, 1).expect("unit rational")
    );
    assert_eq!(
        decoded
            .capacity_probe
            .expect("v4 capacity probe")
            .main_core_capacity,
        100
    );
    assert_eq!(
        decoded.canonical_hash().expect("v4 hashes").to_string(),
        "a0a8974aebc87e30d602ffa019340e59c908912c0b36e0e0634e51214afc45ef"
    );
}

#[test]
fn v2_and_v3_hashes_are_retained_exactly() {
    for (bytes, expected) in [
        (
            BALANCE_V2,
            "b1540d6ad19c616ce60e96523108264355311168c51a0b92de2fdf596e2646fd",
        ),
        (
            BALANCE_V3,
            "96d89224a7edc9b2bbd82b092891465d42b0c8e3954ebed6f9693af216cdcc63",
        ),
    ] {
        assert_eq!(
            decode_balance_profile(bytes)
                .expect("retained profile decodes")
                .canonical_hash()
                .expect("retained profile hashes")
                .to_string(),
            expected
        );
    }
}

#[test]
fn v4_requires_all_three_probe_sections() {
    for missing in ["capacityProbe", "powerProbe", "capacitySupportProbe"] {
        let mut value: serde_json::Value =
            serde_json::from_slice(BALANCE_V4).expect("v4 fixture JSON");
        value
            .as_object_mut()
            .expect("profile object")
            .remove(missing);
        assert_eq!(
            decode_balance_profile(&encode_json(&value)),
            Err(PackageError::InvalidProfile {
                profile: ProfileKind::Balance,
                error: ProfileValidationError::FieldRequiredForSchema {
                    field: missing,
                    schema_version: BALANCE_SCHEMA_VERSION_V4,
                },
            })
        );
    }
}

#[test]
fn v2_and_v3_forbid_the_v4_support_section() {
    let support = serde_json::json!({
        "supportPowerPerNCU": { "numerator": 1, "denominator": 1 }
    });
    for (bytes, schema_version) in [(BALANCE_V2, 2), (BALANCE_V3, 3)] {
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).expect("retained profile JSON");
        value["capacitySupportProbe"] = support.clone();
        assert_eq!(
            decode_balance_profile(&encode_json(&value)),
            Err(PackageError::InvalidProfile {
                profile: ProfileKind::Balance,
                error: ProfileValidationError::FieldForbiddenForSchema {
                    field: "capacitySupportProbe",
                    schema_version,
                },
            })
        );
    }
}

#[test]
fn support_power_is_positive_and_independently_hash_sensitive() {
    let original = decode_balance_profile(BALANCE_V4).expect("v4 fixture decodes");
    let original_hash = original.canonical_hash().expect("v4 hashes");

    let mut changed = original.clone();
    changed
        .capacity_support_probe
        .as_mut()
        .expect("v4 support probe")
        .support_power_per_ncu = Rational::new(2, 1).expect("positive rational");
    assert!(changed.validate().is_ok());
    assert_ne!(changed.canonical_hash(), Ok(original_hash));

    let mut zero = original;
    zero.capacity_support_probe
        .as_mut()
        .expect("v4 support probe")
        .support_power_per_ncu = Rational::new(0, 1).expect("zero rational parses");
    assert_eq!(
        zero.validate(),
        Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "capacitySupportProbe.supportPowerPerNCU",
        })
    );
}

#[test]
fn v4_strengthens_quadratic_capacity_cost_without_changing_v3_acceptance() {
    let mut retained_v3 = decode_balance_profile(BALANCE_V3).expect("v3 fixture decodes");
    retained_v3
        .capacity_probe
        .as_mut()
        .expect("v3 capacity probe")
        .overcap_quadratic_k = Rational::new(0, 1).expect("zero rational parses");
    assert!(retained_v3.validate().is_ok());

    let v4 = decode_balance_profile(BALANCE_V4).expect("v4 fixture decodes");
    let v4_hash = v4.canonical_hash().expect("v4 hashes");
    let mut changed_v4 = v4.clone();
    changed_v4
        .capacity_probe
        .as_mut()
        .expect("v4 capacity probe")
        .overcap_quadratic_k = Rational::new(3, 1).expect("positive rational parses");
    assert!(changed_v4.validate().is_ok());
    assert_ne!(changed_v4.canonical_hash(), Ok(v4_hash));

    let mut invalid_v4 = v4;
    invalid_v4
        .capacity_probe
        .as_mut()
        .expect("v4 capacity probe")
        .overcap_quadratic_k = Rational::new(0, 1).expect("zero rational parses");
    assert_eq!(
        invalid_v4.validate(),
        Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "capacityProbe.overcapQuadraticK",
        })
    );
}

#[test]
fn v4_json_rejects_unknown_duplicate_zero_denominator_and_wrong_schema() {
    let mut unknown: serde_json::Value =
        serde_json::from_slice(BALANCE_V4).expect("v4 fixture JSON");
    unknown["capacitySupportProbe"]["extra"] = 1.into();
    assert_data_error(&encode_json(&unknown));

    let text = std::str::from_utf8(BALANCE_V4).expect("fixture UTF-8");
    let duplicate = text.replacen(
        "\"supportPowerPerNCU\": { \"numerator\": 1, \"denominator\": 1 }",
        "\"supportPowerPerNCU\": { \"numerator\": 1, \"denominator\": 1 },\n    \"supportPowerPerNCU\": { \"numerator\": 1, \"denominator\": 1 }",
        1,
    );
    assert_ne!(duplicate, text, "duplicate mutation must apply");
    assert_data_error(duplicate.as_bytes());

    let mut zero_denominator: serde_json::Value =
        serde_json::from_slice(BALANCE_V4).expect("v4 fixture JSON");
    zero_denominator["capacitySupportProbe"]["supportPowerPerNCU"]["denominator"] = 0.into();
    assert_data_error(&encode_json(&zero_denominator));

    let mut wrong_schema: serde_json::Value =
        serde_json::from_slice(BALANCE_V4).expect("v4 fixture JSON");
    wrong_schema["schemaVersion"] = 5.into();
    assert_eq!(
        decode_balance_profile(&encode_json(&wrong_schema)),
        Err(PackageError::UnsupportedSchema {
            artifact: aon_sim::ArtifactKind::Profile(ProfileKind::Balance),
            expected: BALANCE_SCHEMA_VERSION_V4,
            actual: 5,
        })
    );
}

#[test]
fn unsupported_balance_schema_precedes_strict_version_body_faults() {
    let mut compound: serde_json::Value =
        serde_json::from_slice(BALANCE_V4).expect("v4 fixture JSON");
    compound["schemaVersion"] = 99.into();
    compound["capacitySupportProbe"]["supportPowerPerNCU"] = "not-a-rational".into();
    compound["capacitySupportProbe"]["unknownForSupportedVersion"] = true.into();

    assert_eq!(
        decode_balance_profile(&encode_json(&compound)),
        Err(PackageError::UnsupportedSchema {
            artifact: aon_sim::ArtifactKind::Profile(ProfileKind::Balance),
            expected: BALANCE_SCHEMA_VERSION_V4,
            actual: 99,
        })
    );
}
