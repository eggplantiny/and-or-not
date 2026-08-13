use aon_sim::{
    ArtifactBytes, ArtifactKind, BalanceProfile, EnemyInitialState, Fixed, FixedVec2, HeatEnergy,
    InitialWorld, Integrity, JsonErrorCategory, NumericProfile, PackageError, PhysicalScaleProfile,
    ProfileBundle, SCENARIO_SCHEMA_VERSION_V4, StageFeatureSet, decode_balance_profile,
    decode_numeric_profile, decode_package, decode_physical_scale_profile,
    decode_scenario_manifest,
};
use serde_json::{Value, json};

const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const BALANCE_V5: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m4-construction-contact-damage-alpha.json"
));

fn source(x: i64, y: i64, generation_per_tick: u64) -> Value {
    json!({
        "position": { "x": x, "y": y },
        "generationPerTick": generation_per_tick
    })
}

fn enemy(
    x: i64,
    y: i64,
    velocity_x: i64,
    velocity_y: i64,
    radius: i64,
    integrity: u64,
    heat_energy: u64,
) -> Value {
    json!({
        "position": { "x": x, "y": y },
        "velocityPerTick": { "x": velocity_x, "y": velocity_y },
        "radius": radius,
        "integrity": integrity,
        "heatEnergy": heat_energy
    })
}

fn feature_json() -> Value {
    json!({
        "signal": true,
        "mobility": true,
        "capacity": true,
        "sensing": true,
        "power": true,
        "relay": false,
        "payload": false,
        "radiation": false,
        "construction": true,
        "contact": true,
        "damage": true
    })
}

fn profile_bundle(balance_bytes: &[u8]) -> ProfileBundle {
    ProfileBundle {
        numeric: decode_numeric_profile(NUMERIC).expect("Numeric v1 decodes"),
        physical_scale: decode_physical_scale_profile(PHYSICAL).expect("Physical v1 decodes"),
        balance: decode_balance_profile(balance_bytes).expect("Balance decodes"),
    }
}

fn scenario_value(enemies: Vec<Value>, profiles: Option<&ProfileBundle>) -> Value {
    let zero_hash = "0".repeat(64);
    let (numeric_id, numeric_hash, physical_id, physical_hash, balance_id, balance_hash) =
        if let Some(profiles) = profiles {
            (
                profiles.numeric.profile_id.clone(),
                profiles
                    .numeric
                    .canonical_hash()
                    .expect("Numeric hashes")
                    .to_string(),
                profiles.physical_scale.profile_id.clone(),
                profiles
                    .physical_scale
                    .canonical_hash()
                    .expect("Physical hashes")
                    .to_string(),
                profiles.balance.profile_id.clone(),
                profiles
                    .balance
                    .canonical_hash()
                    .expect("Balance hashes")
                    .to_string(),
            )
        } else {
            (
                "n".to_owned(),
                zero_hash.clone(),
                "p".to_owned(),
                zero_hash.clone(),
                "b".to_owned(),
                zero_hash,
            )
        };
    json!({
        "schemaVersion": 4,
        "scenarioId": "s1-m4-scenario-test",
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-enemy-v1",
            "mainCore": {
                "position": { "x": 0, "y": 0 },
                "integrity": 100,
                "heatEnergy": 0
            },
            "powerSources": [source(65_536, 0, 20)],
            "enemies": enemies
        },
        "requiredFeatures": feature_json(),
        "profiles": {
            "numeric": { "path": "n", "profileId": numeric_id, "profileHash": numeric_hash },
            "physicalScale": {
                "path": "p", "profileId": physical_id, "profileHash": physical_hash
            },
            "balance": { "path": "b", "profileId": balance_id, "profileHash": balance_hash }
        }
    })
}

fn decode_value(value: &Value) -> Result<aon_sim::ScenarioManifest, PackageError> {
    decode_scenario_manifest(&serde_json::to_vec(value).expect("Scenario serializes"))
}

fn package(value: &Value, balance: &[u8]) -> Result<aon_sim::SimulationPackage, PackageError> {
    let scenario = serde_json::to_vec(value).expect("Scenario serializes");
    decode_package(ArtifactBytes {
        scenario: &scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: balance,
    })
}

fn canonical_enemy_values(enemy: EnemyInitialState) -> (i64, i64, i64, i64, i64, u64, u64) {
    (
        enemy.position().x.0,
        enemy.position().y.0,
        enemy.velocity_per_tick().x.0,
        enemy.velocity_per_tick().y.0,
        enemy.radius().0,
        enemy.integrity().0,
        enemy.heat_energy().0,
    )
}

#[test]
fn v4_normalizes_the_complete_enemy_key_and_hashes_the_exact_v4_stream() {
    let later = enemy(65_536, 0, 0, 0, 65_536, 10, 0);
    let earlier = enemy(-65_536, 0, 1_024, 0, 2_048, 10, 7);
    let first = decode_value(&scenario_value(vec![later.clone(), earlier.clone()], None))
        .expect("Scenario v4 decodes");
    let second = decode_value(&scenario_value(vec![earlier, later], None))
        .expect("reordered Scenario v4 decodes");
    assert_eq!(first.canonical_hash(), second.canonical_hash());
    assert_eq!(first.schema_version(), SCENARIO_SCHEMA_VERSION_V4);
    assert_eq!(
        first.required_features(),
        StageFeatureSet {
            signal: true,
            mobility: true,
            capacity: true,
            sensing: true,
            power: true,
            construction: true,
            contact: true,
            damage: true,
            ..StageFeatureSet::none()
        }
    );

    let InitialWorld::MainCorePowerEnemyV1 {
        power_sources,
        enemies,
        ..
    } = first.initial_world()
    else {
        panic!("Scenario v4 selects its frozen initial-world kind")
    };
    assert_eq!(power_sources.len(), 1);
    assert_eq!(
        enemies
            .iter()
            .copied()
            .map(canonical_enemy_values)
            .collect::<Vec<_>>(),
        vec![
            (-65_536, 0, 1_024, 0, 2_048, 10, 7),
            (65_536, 0, 0, 0, 65_536, 10, 0),
        ]
    );

    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"AON\0SCENARIO\0V4\0");
    canonical.extend_from_slice(&4_u16.to_le_bytes());
    canonical.extend_from_slice(&4_u32.to_le_bytes());
    for text in ["s1-m4-scenario-test", "aon-semantics-v1", "blake3-v1"] {
        canonical.extend_from_slice(&(text.len() as u32).to_le_bytes());
        canonical.extend_from_slice(text.as_bytes());
    }
    canonical.push(3);
    for value in [0_i64, 0] {
        canonical.extend_from_slice(&value.to_le_bytes());
    }
    for value in [100_u64, 0] {
        canonical.extend_from_slice(&value.to_le_bytes());
    }
    canonical.extend_from_slice(&1_u32.to_le_bytes());
    for value in [65_536_i64, 0] {
        canonical.extend_from_slice(&value.to_le_bytes());
    }
    canonical.extend_from_slice(&20_u64.to_le_bytes());
    canonical.extend_from_slice(&2_u32.to_le_bytes());
    for values in [
        (
            -65_536_i64,
            0_i64,
            1_024_i64,
            0_i64,
            2_048_i64,
            10_u64,
            7_u64,
        ),
        (65_536, 0, 0, 0, 65_536, 10, 0),
    ] {
        for value in [values.0, values.1, values.2, values.3, values.4] {
            canonical.extend_from_slice(&value.to_le_bytes());
        }
        canonical.extend_from_slice(&values.5.to_le_bytes());
        canonical.extend_from_slice(&values.6.to_le_bytes());
    }
    canonical.extend_from_slice(&[1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1]);
    canonical.extend_from_slice(&[0; 32 * 3]);
    let hash = first.canonical_hash().expect("Scenario v4 hashes");
    assert_eq!(
        hash.to_string(),
        "db98991ccb383894ce79ec14a0071bf3da0e5312fb16b47b36dbd1bdeb9b00e8"
    );
    assert_eq!(hash.as_bytes(), blake3::hash(&canonical).as_bytes());
}

#[test]
fn selected_schema_has_an_exact_feature_and_world_shape() {
    let mut legacy = scenario_value(vec![enemy(0, 0, 0, 0, 1_024, 10, 0)], None);
    legacy["schemaVersion"] = 3.into();
    assert!(matches!(
        decode_value(&legacy),
        Err(PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Data,
            ..
        })
    ));

    let mut missing_v4_feature = scenario_value(vec![enemy(0, 0, 0, 0, 1_024, 10, 0)], None);
    missing_v4_feature["requiredFeatures"]
        .as_object_mut()
        .expect("features object")
        .remove("damage");
    assert!(matches!(
        decode_value(&missing_v4_feature),
        Err(PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Data,
            ..
        })
    ));

    let mut wrong_world = scenario_value(vec![enemy(0, 0, 0, 0, 1_024, 10, 0)], None);
    wrong_world["initialWorld"] = json!({
        "kind": "main-core-power-v1",
        "mainCore": {
            "position": { "x": 0, "y": 0 }, "integrity": 100, "heatEnergy": 0
        },
        "powerSources": []
    });
    assert_eq!(
        decode_value(&wrong_world),
        Err(PackageError::UnsupportedInitialWorld {
            schema_version: 4,
            initial_world: "main-core-power-v1"
        })
    );

    let unsupported = json!({ "schemaVersion": 99, "requiredFeatures": 17 });
    assert_eq!(
        decode_value(&unsupported),
        Err(PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Scenario,
            expected: 4,
            actual: 99
        })
    );
}

#[test]
fn v4_rejects_empty_invalid_overflowing_and_duplicate_enemies() {
    let empty = scenario_value(Vec::new(), None);
    assert_eq!(
        decode_value(&empty),
        Err(PackageError::EmptyInitialEnemySet)
    );

    let zero_radius = scenario_value(vec![enemy(0, 0, 0, 0, 0, 10, 0)], None);
    assert_eq!(
        decode_value(&zero_radius),
        Err(PackageError::NonPositiveInitialWorldField {
            field: "initialWorld.enemies[].radius"
        })
    );

    let zero_integrity = scenario_value(vec![enemy(0, 0, 0, 0, 1_024, 0, 0)], None);
    assert_eq!(
        decode_value(&zero_integrity),
        Err(PackageError::NonPositiveInitialWorldField {
            field: "initialWorld.enemies[].integrity"
        })
    );

    let overflow = scenario_value(vec![enemy(i64::MAX, 0, 1, 0, 1_024, 10, 0)], None);
    assert_eq!(
        decode_value(&overflow),
        Err(PackageError::InitialEnemyTrajectoryOverflow {
            position: FixedVec2::new(Fixed(i64::MAX), Fixed::ZERO),
            velocity_per_tick: FixedVec2::new(Fixed(1), Fixed::ZERO)
        })
    );

    let duplicate_enemy = enemy(0, 0, 0, 0, 1_024, 10, 7);
    let duplicate = scenario_value(vec![duplicate_enemy.clone(), duplicate_enemy.clone()], None);
    assert_eq!(
        decode_value(&duplicate),
        Err(PackageError::DuplicateInitialEnemy {
            position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            velocity_per_tick: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            radius: Fixed(1_024),
            integrity: Integrity(10),
            heat_energy: HeatEnergy(7)
        })
    );

    let same_position_different_velocity = scenario_value(
        vec![
            enemy(0, 0, 0, 0, 1_024, 10, 0),
            enemy(0, 0, 1_024, 0, 1_024, 10, 0),
        ],
        None,
    );
    decode_value(&same_position_different_velocity)
        .expect("only an exact complete-key duplicate is rejected");
}

#[test]
fn package_decode_enforces_v5_integrity_and_physical_quantum_coherence() {
    let profiles = profile_bundle(BALANCE_V5);
    let valid = scenario_value(
        vec![enemy(65_536, 0, -1_024, 0, 2_048, 10, 0)],
        Some(&profiles),
    );
    package(&valid, BALANCE_V5).expect("coherent Scenario v4 package decodes");

    let mut misaligned = valid.clone();
    misaligned["initialWorld"]["enemies"][0]["velocityPerTick"]["x"] = (-1_025).into();
    assert_eq!(
        package(&misaligned, BALANCE_V5),
        Err(PackageError::InitialWorldFieldNotQuantumAligned {
            field: "initialWorld.enemies[].velocityPerTick.x"
        })
    );

    let mut core_mismatch = valid.clone();
    core_mismatch["initialWorld"]["mainCore"]["integrity"] = 99.into();
    assert_eq!(
        package(&core_mismatch, BALANCE_V5),
        Err(PackageError::InitialIntegrityProfileMismatch {
            entity_kind: "main-core",
            expected: Integrity(100),
            actual: Integrity(99)
        })
    );

    let mut enemy_mismatch = valid;
    enemy_mismatch["initialWorld"]["enemies"][0]["integrity"] = 9.into();
    assert_eq!(
        package(&enemy_mismatch, BALANCE_V5),
        Err(PackageError::InitialIntegrityProfileMismatch {
            entity_kind: "enemy",
            expected: Integrity(10),
            actual: Integrity(9)
        })
    );
}

#[test]
fn public_reference_v5_matches_the_artifact_used_by_scenario_v4() {
    let decoded = decode_balance_profile(BALANCE_V5).expect("v5 Balance artifact decodes");
    assert_eq!(
        decoded,
        BalanceProfile::construction_contact_damage_alpha(
            "balance-s1-m4-construction-contact-damage-alpha"
        )
    );
    assert_eq!(NumericProfile::reference_v1("n").schema_version, 1);
    assert_eq!(
        PhysicalScaleProfile::stage0_alpha("p").wire_geometry_quantum,
        Fixed(1_024)
    );
}
