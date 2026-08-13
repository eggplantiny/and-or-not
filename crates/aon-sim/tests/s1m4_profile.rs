use aon_sim::{
    ArtifactKind, BALANCE_SCHEMA_VERSION_V4, BALANCE_SCHEMA_VERSION_V5, BalanceProfile,
    JsonErrorCategory, PackageError, ProfileKind, ProfileValidationError, Rational,
    decode_balance_profile,
};
use serde_json::{Number, Value, json};

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
const BALANCE_V5: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m4-construction-contact-damage-alpha.json"
));

fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("fixture is JSON")
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("test value encodes")
}

fn slot_mut<'a>(value: &'a mut Value, path: &[&str]) -> &'a mut Value {
    let mut cursor = value;
    for component in path {
        cursor = cursor
            .get_mut(*component)
            .unwrap_or_else(|| panic!("missing test path component {component}"));
    }
    cursor
}

fn increment(value: &mut Value, path: &[&str]) {
    let slot = slot_mut(value, path);
    let current = slot.as_i64().expect("reference coefficient is signed JSON");
    *slot = Value::Number(Number::from(current + 1));
}

fn assert_data_error(candidate: &[u8]) {
    assert!(matches!(
        decode_balance_profile(candidate),
        Err(PackageError::InvalidJson {
            category: JsonErrorCategory::Data,
            ..
        })
    ));
}

fn positive_scalar_paths() -> Vec<Vec<&'static str>> {
    let mut paths = vec![
        vec!["constructionProbe", "andGateWork"],
        vec!["constructionProbe", "orGateWork"],
        vec!["constructionProbe", "notGateWork"],
        vec!["constructionProbe", "junctionBaseWork"],
        vec!["constructionProbe", "wireEndpointWork"],
        vec!["constructionProbe", "builderWorkPerTick"],
        vec!["contactDamageProbe", "worldLeakWeight"],
        vec!["contactDamageProbe", "enemyConductivity"],
        vec!["contactDamageProbe", "enemyAttackEnergyPerTick"],
    ];
    for section in ["initialIntegrity", "thermalCapacity", "electricalTolerance"] {
        for kind in [
            "mainCore",
            "wire",
            "gate",
            "junction",
            "fixedSubstrate",
            "mobileSubstrate",
            "enemy",
        ] {
            paths.push(vec!["contactDamageProbe", section, kind]);
        }
    }
    paths
}

fn positive_rational_paths() -> Vec<Vec<&'static str>> {
    vec![
        vec!["constructionProbe", "wireWorkPerNCU"],
        vec!["constructionProbe", "substrateWorkPerSquareWU"],
        vec!["constructionProbe", "constructionPowerPerWork"],
        vec!["contactDamageProbe", "liveEnergyPerStrengthWU"],
        vec!["contactDamageProbe", "thermalDamageRate"],
    ]
}

fn unit_fraction_paths() -> Vec<Vec<&'static str>> {
    vec![
        vec!["constructionProbe", "constructionHeatFraction"],
        vec!["contactDamageProbe", "gatePowerHeatFraction"],
        vec!["contactDamageProbe", "movementHeatFraction"],
    ]
}

#[test]
fn v5_fixture_is_strict_valid_and_matches_the_exact_reference_constructor() {
    let decoded = decode_balance_profile(BALANCE_V5).expect("Balance v5 fixture decodes");
    assert_eq!(decoded.schema_version, BALANCE_SCHEMA_VERSION_V5);
    assert_eq!(
        decoded,
        BalanceProfile::construction_contact_damage_alpha(
            "balance-s1-m4-construction-contact-damage-alpha"
        )
    );

    let construction = decoded.construction_probe.expect("construction probe");
    assert_eq!(construction.and_gate_work, 8);
    assert_eq!(construction.or_gate_work, 8);
    assert_eq!(construction.not_gate_work, 6);
    assert_eq!(construction.junction_base_work, 4);
    assert_eq!(construction.wire_endpoint_work, 2);
    assert_eq!(construction.wire_work_per_ncu, Rational::new(1, 1).unwrap());
    assert_eq!(construction.builder_work_per_tick, 8);
    assert_eq!(
        construction.construction_heat_fraction,
        Rational::new(1, 4).unwrap()
    );

    let contact = decoded.contact_damage_probe.expect("contact/damage probe");
    assert_eq!(
        contact.live_energy_per_strength_wu,
        Rational::new(1, 400).unwrap()
    );
    assert_eq!(contact.world_leak_weight, 2);
    assert_eq!(contact.enemy_conductivity, 1);
    assert_eq!(contact.initial_integrity.main_core, 100);
    assert_eq!(contact.initial_integrity.enemy, 10);
    assert_eq!(contact.thermal_capacity.mobile_substrate, 10);
    assert_eq!(contact.electrical_tolerance.fixed_substrate, 1);
    assert_eq!(contact.safe_temperature.0, 65_536);
    assert_eq!(contact.enemy_attack_energy_per_tick, 10);
    assert_eq!(
        contact.gate_power_heat_fraction,
        Rational::new(1, 4).unwrap()
    );
    assert_eq!(contact.movement_heat_fraction, Rational::new(1, 4).unwrap());
    assert_eq!(
        decoded
            .canonical_hash()
            .expect("v5 fixture hashes")
            .to_string(),
        "88b8fdc40dae59563699a0f611adae21c40d770d3d1c9076f8262a756107311a"
    );
}

#[test]
fn retained_v2_v3_v4_semantic_hashes_remain_exact() {
    for (fixture, expected) in [
        (
            BALANCE_V2,
            "b1540d6ad19c616ce60e96523108264355311168c51a0b92de2fdf596e2646fd",
        ),
        (
            BALANCE_V3,
            "96d89224a7edc9b2bbd82b092891465d42b0c8e3954ebed6f9693af216cdcc63",
        ),
        (
            BALANCE_V4,
            "a0a8974aebc87e30d602ffa019340e59c908912c0b36e0e0634e51214afc45ef",
        ),
    ] {
        assert_eq!(
            decode_balance_profile(fixture)
                .expect("retained Balance fixture decodes")
                .canonical_hash()
                .expect("retained Balance fixture hashes")
                .to_string(),
            expected
        );
    }
}

#[test]
fn schema_matrix_forbids_new_sections_before_v5_and_requires_all_v5_sections() {
    let v5 = value(BALANCE_V5);
    for (fixture, schema_version) in [(BALANCE_V2, 2), (BALANCE_V3, 3), (BALANCE_V4, 4)] {
        for section in ["constructionProbe", "contactDamageProbe"] {
            let mut candidate = value(fixture);
            candidate[section] = v5[section].clone();
            assert_eq!(
                decode_balance_profile(&bytes(&candidate)),
                Err(PackageError::InvalidProfile {
                    profile: ProfileKind::Balance,
                    error: ProfileValidationError::FieldForbiddenForSchema {
                        field: section,
                        schema_version,
                    },
                })
            );
        }
    }

    for section in [
        "capacityProbe",
        "powerProbe",
        "capacitySupportProbe",
        "constructionProbe",
        "contactDamageProbe",
    ] {
        let mut candidate = v5.clone();
        candidate.as_object_mut().unwrap().remove(section);
        assert_eq!(
            decode_balance_profile(&bytes(&candidate)),
            Err(PackageError::InvalidProfile {
                profile: ProfileKind::Balance,
                error: ProfileValidationError::FieldRequiredForSchema {
                    field: section,
                    schema_version: BALANCE_SCHEMA_VERSION_V5,
                },
            }),
            "missing {section}"
        );
    }
}

#[test]
fn every_new_v5_field_is_independently_semantic_hash_sensitive() {
    let original = decode_balance_profile(BALANCE_V5).expect("v5 fixture decodes");
    let baseline = original.canonical_hash().expect("v5 fixture hashes");
    let mut paths = positive_scalar_paths();
    paths.push(vec!["contactDamageProbe", "safeTemperature"]);
    for mut path in positive_rational_paths() {
        path.push("denominator");
        paths.push(path);
    }
    for mut path in unit_fraction_paths() {
        path.push("denominator");
        paths.push(path);
    }
    assert_eq!(paths.len(), 39, "every one of the 39 new fields is listed");

    for path in paths {
        let mut candidate = value(BALANCE_V5);
        increment(&mut candidate, &path);
        let changed = decode_balance_profile(&bytes(&candidate))
            .unwrap_or_else(|error| panic!("valid mutation {path:?} rejected: {error:?}"));
        assert_ne!(
            changed.canonical_hash().expect("changed v5 profile hashes"),
            baseline,
            "field {path:?} must be independently hash-sensitive"
        );
    }
}

#[test]
fn every_positive_v5_scalar_and_nested_kind_rejects_zero() {
    for path in positive_scalar_paths() {
        let mut candidate = value(BALANCE_V5);
        *slot_mut(&mut candidate, &path) = 0.into();
        assert!(
            matches!(
                decode_balance_profile(&bytes(&candidate)),
                Err(PackageError::InvalidProfile {
                    error: ProfileValidationError::NonPositiveField { .. },
                    ..
                })
            ),
            "zero field {path:?} must fail closed"
        );
    }
}

#[test]
fn rational_sign_unit_interval_and_temperature_boundaries_are_exact() {
    for path in positive_rational_paths() {
        for invalid_numerator in [0, -1] {
            let mut candidate = value(BALANCE_V5);
            let mut numerator_path = path.clone();
            numerator_path.push("numerator");
            *slot_mut(&mut candidate, &numerator_path) = invalid_numerator.into();
            assert!(matches!(
                decode_balance_profile(&bytes(&candidate)),
                Err(PackageError::InvalidProfile {
                    error: ProfileValidationError::NonPositiveField { .. },
                    ..
                })
            ));
        }
    }

    for path in unit_fraction_paths() {
        for (numerator, denominator, valid) in [(0, 1, false), (2, 1, false), (1, 1, true)] {
            let mut candidate = value(BALANCE_V5);
            let mut numerator_path = path.clone();
            numerator_path.push("numerator");
            let mut denominator_path = path.clone();
            denominator_path.push("denominator");
            *slot_mut(&mut candidate, &numerator_path) = numerator.into();
            *slot_mut(&mut candidate, &denominator_path) = denominator.into();
            if valid {
                decode_balance_profile(&bytes(&candidate)).expect("unit boundary is accepted");
            } else {
                assert!(matches!(
                    decode_balance_profile(&bytes(&candidate)),
                    Err(PackageError::InvalidProfile {
                        error: ProfileValidationError::OutsideUnitInterval { .. },
                        ..
                    })
                ));
            }
        }
    }

    let mut zero_temperature = value(BALANCE_V5);
    zero_temperature["contactDamageProbe"]["safeTemperature"] = 0.into();
    decode_balance_profile(&bytes(&zero_temperature)).expect("zero temperature is allowed");

    let mut negative_temperature = value(BALANCE_V5);
    negative_temperature["contactDamageProbe"]["safeTemperature"] = (-1).into();
    assert_eq!(
        decode_balance_profile(&bytes(&negative_temperature)),
        Err(PackageError::InvalidProfile {
            profile: ProfileKind::Balance,
            error: ProfileValidationError::NegativeField {
                profile: ProfileKind::Balance,
                field: "contactDamageProbe.safeTemperature",
            },
        })
    );
}

#[test]
fn v5_json_rejects_unknown_duplicate_float_and_zero_denominator() {
    let mut unknown = value(BALANCE_V5);
    unknown["contactDamageProbe"]["initialIntegrity"]["unknown"] = 1.into();
    assert_data_error(&bytes(&unknown));

    let mut float = value(BALANCE_V5);
    float["constructionProbe"]["andGateWork"] = json!(8.0);
    assert_data_error(&bytes(&float));

    for path in positive_rational_paths()
        .into_iter()
        .chain(unit_fraction_paths())
    {
        let mut zero_denominator = value(BALANCE_V5);
        let mut denominator_path = path;
        denominator_path.push("denominator");
        *slot_mut(&mut zero_denominator, &denominator_path) = 0.into();
        assert_data_error(&bytes(&zero_denominator));
    }

    let text = std::str::from_utf8(BALANCE_V5).expect("fixture is UTF-8");
    let duplicate = text.replacen(
        "\"andGateWork\": 8,",
        "\"andGateWork\": 8,\n    \"andGateWork\": 8,",
        1,
    );
    assert_ne!(duplicate, text, "duplicate mutation applies");
    assert_data_error(duplicate.as_bytes());
}

#[test]
fn unsupported_schema_and_supported_body_validation_precedence_are_exact() {
    let mut unsupported = value(BALANCE_V5);
    unsupported["schemaVersion"] = 99.into();
    unsupported["constructionProbe"]["andGateWork"] = "bad".into();
    unsupported["contactDamageProbe"]["unknown"] = true.into();
    assert_eq!(
        decode_balance_profile(&bytes(&unsupported)),
        Err(PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Profile(ProfileKind::Balance),
            expected: BALANCE_SCHEMA_VERSION_V5,
            actual: 99,
        })
    );

    let mut retained_first =
        BalanceProfile::construction_contact_damage_alpha("precedence-retained");
    retained_first
        .power_probe
        .as_mut()
        .unwrap()
        .gate_idle_demand = 0;
    retained_first
        .construction_probe
        .as_mut()
        .unwrap()
        .and_gate_work = 0;
    retained_first
        .contact_damage_probe
        .as_mut()
        .unwrap()
        .world_leak_weight = 0;
    assert_eq!(
        retained_first.validate(),
        Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "powerProbe.gateIdleDemand",
        })
    );

    let mut construction_first =
        BalanceProfile::construction_contact_damage_alpha("precedence-construction");
    construction_first
        .construction_probe
        .as_mut()
        .unwrap()
        .and_gate_work = 0;
    construction_first
        .contact_damage_probe
        .as_mut()
        .unwrap()
        .world_leak_weight = 0;
    assert_eq!(
        construction_first.validate(),
        Err(ProfileValidationError::NonPositiveField {
            profile: ProfileKind::Balance,
            field: "constructionProbe.andGateWork",
        })
    );

    let mut missing_construction =
        BalanceProfile::construction_contact_damage_alpha("precedence-missing");
    missing_construction.construction_probe = None;
    missing_construction
        .contact_damage_probe
        .as_mut()
        .unwrap()
        .world_leak_weight = 0;
    assert_eq!(
        missing_construction.validate(),
        Err(ProfileValidationError::FieldRequiredForSchema {
            field: "constructionProbe",
            schema_version: BALANCE_SCHEMA_VERSION_V5,
        })
    );
}

#[test]
fn v4_is_still_currently_valid_and_forbids_v5_sections() {
    let mut retained = BalanceProfile::capacity_support_probe_alpha("retained-v4");
    assert_eq!(retained.schema_version, BALANCE_SCHEMA_VERSION_V4);
    retained.construction_probe =
        BalanceProfile::construction_contact_damage_alpha("source").construction_probe;
    assert_eq!(
        retained.validate(),
        Err(ProfileValidationError::FieldForbiddenForSchema {
            field: "constructionProbe",
            schema_version: BALANCE_SCHEMA_VERSION_V4,
        })
    );
}
