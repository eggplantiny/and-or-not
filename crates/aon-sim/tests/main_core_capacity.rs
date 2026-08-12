use aon_sim::{
    ArtifactKind, BalanceProfile, BindPortCommand, Capacity, Command, CommandEnvelope,
    CommandRejectionReason, EndpointTarget, EntityId, Fixed, FixedVec2, HashCheckpoint, HeatEnergy,
    InitialWorld, Integrity, JsonErrorCategory, MainCoreId, NumericProfile, PackageError,
    PhysicalScaleProfile, ProfileBundle, RemoveEntityCommand, RenderSnapshot, Replay,
    ReplayArtifact, RoutingDomain, SCENARIO_SCHEMA_VERSION_V1, SCENARIO_SCHEMA_VERSION_V2,
    Simulation, SimulationContract, SimulationError, SimulationPackage, StageFeatureSet, Tick,
    TopologyNodeId, WireEnd, WireId, WorldGeneratorVersion, decode_replay_artifact,
    decode_scenario_manifest, encode_replay_artifact,
};

const WORLD_PITCH: i64 = 65_536;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn profiles(main_core_capacity: Option<u64>) -> ProfileBundle {
    let balance = match main_core_capacity {
        None => BalanceProfile::stage0_alpha("balance"),
        Some(capacity) => {
            let mut balance = BalanceProfile::capacity_probe_alpha("balance-capacity");
            balance
                .capacity_probe
                .as_mut()
                .expect("capacity profile section exists")
                .main_core_capacity = capacity;
            balance
        }
    };
    ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical"),
        balance,
    }
}

fn capacity_features() -> StageFeatureSet {
    StageFeatureSet {
        capacity: true,
        ..StageFeatureSet::none()
    }
}

fn package(
    initial_world: InitialWorld,
    required_features: StageFeatureSet,
    profiles: ProfileBundle,
) -> SimulationPackage {
    let contract = SimulationContract::from_profiles(&profiles).expect("test profiles are valid");
    SimulationPackage::new(
        "main-core-capacity",
        initial_world,
        required_features,
        contract,
        profiles,
    )
}

fn main_core_package(capacity: u64) -> SimulationPackage {
    package(
        InitialWorld::MainCoreV1 {
            position: point(0, 0),
            integrity: Integrity(1_000),
            heat_energy: HeatEnergy(7),
        },
        capacity_features(),
        profiles(Some(capacity)),
    )
}

fn scenario_value(schema_version: u32, initial_world: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": schema_version,
        "scenarioId": "scenario-decode-test",
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": initial_world,
        "requiredFeatures": {
            "signal": false,
            "mobility": false,
            "capacity": schema_version == SCENARIO_SCHEMA_VERSION_V2,
            "sensing": false,
            "power": false,
            "relay": false,
            "payload": false,
            "radiation": false
        },
        "profiles": {
            "numeric": {
                "path": "numeric.json",
                "profileId": "numeric",
                "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
            },
            "physicalScale": {
                "path": "physical.json",
                "profileId": "physical",
                "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
            },
            "balance": {
                "path": "balance.json",
                "profileId": "balance",
                "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    })
}

fn main_core_world(position_x: i64, integrity: u64) -> serde_json::Value {
    serde_json::json!({
        "kind": "main-core-v1",
        "position": { "x": position_x, "y": 0 },
        "integrity": integrity,
        "heatEnergy": 7
    })
}

#[test]
fn main_core_initial_state_has_implicit_anchor_capacity_and_read_only_projection() {
    let mut simulation = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let initial_hash = simulation.state_hash();
    let core = *simulation.main_core_state().expect("Main Core is present");

    assert_eq!(core.id(), MainCoreId(EntityId(1)));
    assert_eq!(core.position(), point(0, 0));
    assert_eq!(
        core.anchor_node(),
        TopologyNodeId::MainCoreAnchor(core.id())
    );
    assert_eq!(core.capacity(), Capacity(1_000 * 65_536));
    assert_eq!(core.integrity(), Integrity(1_000));
    assert_eq!(core.heat_energy(), HeatEnergy(7));
    assert_eq!(
        simulation.replay_header().world_generator_version,
        WorldGeneratorVersion::MainCoreV1
    );

    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    assert_eq!(snapshot.primitive_count(), 1);
    let rendered = snapshot.main_core().expect("Main Core is rendered");
    assert_eq!(rendered.id, core.id());
    assert_eq!(rendered.position, core.position());
    assert_eq!(rendered.capacity, core.capacity());
    assert_eq!(rendered.integrity, core.integrity());
    assert_eq!(rendered.heat_energy, core.heat_energy());
    assert_eq!(simulation.state_hash(), initial_hash);

    let first = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer accounting fits")
        .expect("capacity session exposes Analyzer");
    let second = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer accounting fits")
        .expect("capacity session exposes Analyzer");
    assert_eq!(first, second);
    assert_eq!(first.accounting().used(), Capacity(0));
    assert_eq!(first.accounting().supported(), core.capacity());
    assert_eq!(first.main_core_contribution().main_core(), core.id());
    assert_eq!(first.main_core_contribution().capacity(), core.capacity());
    assert!(first.wires().is_empty());
    assert_eq!(simulation.state_hash(), initial_hash);

    let empty_profiles = profiles(Some(u64::MAX));
    let empty_contract =
        SimulationContract::from_profiles(&empty_profiles).expect("Empty profiles are valid");
    let empty = Simulation::new(SimulationPackage::new(
        "empty-analyzer",
        InitialWorld::Empty,
        StageFeatureSet::none(),
        empty_contract,
        empty_profiles,
    ))
    .expect("Empty simulation starts");
    assert_eq!(empty.network_analyzer_snapshot(), Ok(None));

    let report = simulation.step(&[]).expect("empty capacity Tick succeeds");
    assert_eq!(
        report.network_accounting.expect("Phase 4 is active"),
        simulation
            .network_analyzer_snapshot()
            .expect("Analyzer accounting fits")
            .expect("capacity session exposes Analyzer")
            .accounting()
    );
}

#[test]
fn capacity_feature_main_core_and_profile_dependency_triad_is_typed() {
    assert_eq!(
        Simulation::new(package(
            InitialWorld::Empty,
            capacity_features(),
            profiles(Some(1_000)),
        ))
        .err(),
        Some(SimulationError::CapacityRequiresMainCore)
    );
    assert_eq!(
        Simulation::new(package(
            InitialWorld::MainCoreV1 {
                position: point(0, 0),
                integrity: Integrity(1),
                heat_energy: HeatEnergy(0),
            },
            StageFeatureSet::none(),
            profiles(Some(1_000)),
        ))
        .err(),
        Some(SimulationError::MainCoreRequiresCapacity)
    );
    assert_eq!(
        Simulation::new(package(
            InitialWorld::MainCoreV1 {
                position: point(0, 0),
                integrity: Integrity(1),
                heat_energy: HeatEnergy(0),
            },
            capacity_features(),
            profiles(None),
        ))
        .err(),
        Some(SimulationError::CapacityRequiresProfile)
    );
}

#[test]
fn simulation_construction_compound_errors_follow_frozen_precedence() {
    let mut invalid_profiles = profiles(None);
    invalid_profiles.balance.logic_threshold = 0;
    let mut bad_contract =
        SimulationContract::from_profiles(&profiles(Some(1_000))).expect("profiles are valid");
    bad_contract.numeric_profile_hash =
        aon_sim::ProfileHash::from_hex(&"0".repeat(64)).expect("lowercase test hash parses");
    let initial = InitialWorld::MainCoreV1 {
        position: point(1, 0),
        integrity: Integrity(0),
        heat_energy: HeatEnergy(0),
    };

    let mut unsupported = capacity_features();
    unsupported.sensing = true;
    assert_eq!(
        Simulation::new(SimulationPackage::new(
            "precedence",
            initial.clone(),
            unsupported,
            bad_contract,
            invalid_profiles.clone(),
        ))
        .err(),
        Some(SimulationError::UnsupportedStageFeature { feature: "sensing" })
    );

    assert!(matches!(
        Simulation::new(SimulationPackage::new(
            "precedence",
            initial.clone(),
            capacity_features(),
            bad_contract,
            invalid_profiles,
        )),
        Err(SimulationError::InvalidProfile { .. })
    ));

    let valid_profiles = profiles(Some(1_000));
    assert!(matches!(
        Simulation::new(SimulationPackage::new(
            "precedence",
            initial.clone(),
            capacity_features(),
            bad_contract,
            valid_profiles,
        )),
        Err(SimulationError::ProfileHashMismatch { .. })
    ));

    assert_eq!(
        Simulation::new(package(
            InitialWorld::Empty,
            capacity_features(),
            profiles(Some(1_000)),
        ))
        .err(),
        Some(SimulationError::CapacityRequiresMainCore)
    );
}

#[test]
fn main_core_position_requires_geometry_quantum_but_not_world_routing_pitch() {
    let off_world_pitch = InitialWorld::MainCoreV1 {
        position: point(1_024, -1_024),
        integrity: Integrity(1),
        heat_energy: HeatEnergy(0),
    };
    let simulation = Simulation::new(package(
        off_world_pitch,
        capacity_features(),
        profiles(Some(1_000)),
    ))
    .expect("geometry-quantized Main Core need not align to world routing pitch");
    assert_eq!(
        simulation
            .main_core_state()
            .expect("core exists")
            .position(),
        point(1_024, -1_024)
    );

    assert_eq!(
        Simulation::new(package(
            InitialWorld::MainCoreV1 {
                position: point(1, 0),
                integrity: Integrity(1),
                heat_energy: HeatEnergy(0),
            },
            capacity_features(),
            profiles(Some(1_000)),
        ))
        .err(),
        Some(SimulationError::InvalidMainCoreGeometryQuantum)
    );
}

#[test]
fn oversized_capacity_probe_is_inert_for_empty_but_active_main_core_conversion_is_atomic() {
    let oversized = u64::MAX;
    let mut inactive_profiles = profiles(Some(oversized));
    let inactive_probe = inactive_profiles
        .balance
        .capacity_probe
        .as_mut()
        .expect("capacity section exists");
    inactive_probe.relay_capacity = u64::MAX;
    inactive_probe.capacity_denominator_floor = u64::MAX;
    let mut empty = Simulation::new(package(
        InitialWorld::Empty,
        StageFeatureSet::none(),
        inactive_profiles,
    ))
    .expect("capacity-disabled Empty keeps the optional probe inert");
    assert!(empty.main_core_state().is_none());
    assert!(
        empty
            .step(&[])
            .expect("Empty Tick succeeds")
            .network_accounting
            .is_none()
    );

    assert_eq!(
        Simulation::new(main_core_package(oversized)).err(),
        Some(SimulationError::NumericOverflow)
    );
    let mut deferred_large_profiles = profiles(Some(1));
    let deferred_probe = deferred_large_profiles
        .balance
        .capacity_probe
        .as_mut()
        .expect("capacity section exists");
    deferred_probe.relay_capacity = u64::MAX;
    deferred_probe.capacity_denominator_floor = u64::MAX;
    Simulation::new(package(
        InitialWorld::MainCoreV1 {
            position: point(0, 0),
            integrity: Integrity(1),
            heat_energy: HeatEnergy(0),
        },
        capacity_features(),
        deferred_large_profiles,
    ))
    .expect("later-milestone Relay/floor fields stay unconsumed in active S1-M1");
    let recovered =
        Simulation::new(main_core_package(1)).expect("a later valid construction works");
    assert_eq!(
        recovered.main_core_state().expect("core exists").id(),
        MainCoreId(EntityId(1)),
        "failed checked conversion allocated no observable Entity ID"
    );
}

#[test]
fn main_core_removal_is_rejected_without_mutating_the_canonical_world() {
    let mut attempted = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let mut control = Simulation::new(main_core_package(1_000)).expect("control starts");
    let report = attempted
        .step(&[CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::RemoveEntity(RemoveEntityCommand {
                target: EntityId(1),
            }),
        }])
        .expect("command rejection is a successful Tick");
    control.step(&[]).expect("control Tick succeeds");

    assert!(report.command_acceptances.is_empty());
    assert_eq!(report.command_rejections.len(), 1);
    assert_eq!(
        report.command_rejections[0].reason,
        CommandRejectionReason::UnsupportedCommand
    );
    assert!(!report.topology_changed);
    assert_eq!(attempted.state_hash(), control.state_hash());
    assert_eq!(
        attempted.main_core_state().expect("core remains").id(),
        MainCoreId(EntityId(1))
    );
}

#[test]
fn main_core_anchor_requires_exact_core_identity_position_and_open_world() {
    let core = MainCoreId(EntityId(1));
    let valid = Command::PlaceWire(aon_sim::PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
        endpoint_a: EndpointTarget::MainCoreAnchor(core),
        endpoint_b: EndpointTarget::Free,
    });
    let mut simulation = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: valid.clone(),
        }])
        .expect("anchored placement Tick succeeds");
    assert_eq!(report.command_acceptances.len(), 1);
    assert!(report.command_rejections.is_empty());

    let mut wrong_position = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let report = wrong_position
        .step(&[CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceWire(aon_sim::PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(WORLD_PITCH, 0), point(3 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::MainCoreAnchor(core),
                endpoint_b: EndpointTarget::Free,
            }),
        }])
        .expect("invalid endpoint is a command rejection");
    assert_eq!(
        report.command_rejections[0].reason,
        CommandRejectionReason::InvalidEndpoint
    );

    for (endpoint, domain, expected) in [
        (
            EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(2))),
            RoutingDomain::OpenWorld,
            CommandRejectionReason::UnknownEntity,
        ),
        (
            EndpointTarget::MainCoreAnchor(core),
            RoutingDomain::FixedSubstrate(EntityId(1)),
            CommandRejectionReason::InvalidRoutingDomain,
        ),
    ] {
        let mut invalid = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
        let report = invalid
            .step(&[CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: Command::PlaceWire(aon_sim::PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                    endpoint_a: endpoint,
                    endpoint_b: EndpointTarget::Free,
                }),
            }])
            .expect("invalid endpoint is a command rejection");
        assert_eq!(report.command_rejections[0].reason, expected);
    }

    let mut valid_substrate = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let bounds = aon_sim::FixedAabb::new(
        point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
        point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
    );
    let substrate = valid_substrate
        .step(&[CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceFixedSubstrate(aon_sim::PlaceFixedSubstrateCommand {
                origin: point(4 * WORLD_PITCH, 0),
                routing_area: bounds,
                footprint: bounds,
            }),
        }])
        .expect("valid substrate places");
    assert_eq!(
        substrate.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
    let wrong_domain = valid_substrate
        .step(&[CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::PlaceWire(aon_sim::PlaceWireCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(2)),
                points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::MainCoreAnchor(core),
                endpoint_b: EndpointTarget::Free,
            }),
        }])
        .expect("wrong-domain anchor is a command rejection");
    assert_eq!(
        wrong_domain.command_rejections[0].reason,
        CommandRejectionReason::InvalidEndpoint
    );

    let bytes = CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: valid,
    }
    .canonical_bytes()
    .expect("command encodes");
    let mut suffix = vec![4];
    suffix.extend_from_slice(&1_u64.to_le_bytes());
    suffix.push(0);
    assert_eq!(&bytes[bytes.len() - suffix.len()..], suffix);
}

#[test]
fn bind_port_accepts_the_exact_main_core_anchor_and_rejects_mismatched_identity() {
    let mut simulation = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let placed = simulation
        .step(&[CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceWire(aon_sim::PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        }])
        .expect("free Wire places");
    assert_eq!(
        placed.command_acceptances[0].created_entity,
        Some(EntityId(2))
    );
    let bound = simulation
        .step(&[CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(2)),
                end: WireEnd::A,
                target: EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
            }),
        }])
        .expect("exact anchor binding succeeds");
    assert_eq!(bound.command_acceptances.len(), 1);
    assert!(bound.command_rejections.is_empty());

    let rejected = simulation
        .step(&[CommandEnvelope {
            target_tick: Tick(2),
            ordinal: 0,
            command: Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(2)),
                end: WireEnd::A,
                target: EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(2))),
            }),
        }])
        .expect("mismatched anchor is a command rejection");
    assert_eq!(
        rejected.command_rejections[0].reason,
        CommandRejectionReason::InvalidEndpoint
    );
}

#[test]
fn scenario_decode_precedence_and_v1_v2_pairing_are_frozen() {
    let mut unsupported = scenario_value(99, serde_json::json!(7));
    unsupported["scenarioId"] = serde_json::Value::Null;
    assert_eq!(
        decode_scenario_manifest(&serde_json::to_vec(&unsupported).expect("JSON serializes")),
        Err(PackageError::UnsupportedSchema {
            artifact: ArtifactKind::Scenario,
            expected: SCENARIO_SCHEMA_VERSION_V2,
            actual: 99,
        })
    );

    let malformed = scenario_value(
        SCENARIO_SCHEMA_VERSION_V2,
        serde_json::json!({
            "kind": "main-core-v1",
            "position": "not-a-point",
            "integrity": 1,
            "heatEnergy": 0,
            "unexpected": true
        }),
    );
    assert!(matches!(
        decode_scenario_manifest(&serde_json::to_vec(&malformed).expect("JSON serializes")),
        Err(PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Data,
            ..
        })
    ));

    let malformed_cross_pair = scenario_value(
        SCENARIO_SCHEMA_VERSION_V1,
        serde_json::json!({
            "kind": "main-core-v1",
            "position": "not-a-point",
            "integrity": 1,
            "heatEnergy": 0,
            "unexpected": true
        }),
    );
    assert!(matches!(
        decode_scenario_manifest(
            &serde_json::to_vec(&malformed_cross_pair).expect("JSON serializes")
        ),
        Err(PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Data,
            ..
        })
    ));

    let v1_main_core = scenario_value(SCENARIO_SCHEMA_VERSION_V1, main_core_world(0, 1));
    assert_eq!(
        decode_scenario_manifest(&serde_json::to_vec(&v1_main_core).expect("JSON serializes")),
        Err(PackageError::UnsupportedInitialWorld {
            schema_version: SCENARIO_SCHEMA_VERSION_V1,
            initial_world: "main-core-v1",
        })
    );
    let v2_empty = scenario_value(
        SCENARIO_SCHEMA_VERSION_V2,
        serde_json::json!({ "kind": "empty" }),
    );
    assert_eq!(
        decode_scenario_manifest(&serde_json::to_vec(&v2_empty).expect("JSON serializes")),
        Err(PackageError::UnsupportedInitialWorld {
            schema_version: SCENARIO_SCHEMA_VERSION_V2,
            initial_world: "empty",
        })
    );

    let zero_integrity = scenario_value(SCENARIO_SCHEMA_VERSION_V2, main_core_world(0, 0));
    assert_eq!(
        decode_scenario_manifest(&serde_json::to_vec(&zero_integrity).expect("JSON serializes")),
        Err(PackageError::NonPositiveInitialWorldField {
            field: "initialWorld.integrity",
        })
    );
}

#[test]
fn scenario_v2_main_core_payload_is_strict_for_unknown_duplicate_float_and_overflow_fields() {
    let unknown = scenario_value(
        SCENARIO_SCHEMA_VERSION_V2,
        serde_json::json!({
            "kind": "main-core-v1",
            "position": { "x": 0, "y": 0 },
            "integrity": 1,
            "heatEnergy": 0,
            "unexpected": true
        }),
    );
    let floating = scenario_value(
        SCENARIO_SCHEMA_VERSION_V2,
        serde_json::json!({
            "kind": "main-core-v1",
            "position": { "x": 0.5, "y": 0 },
            "integrity": 1,
            "heatEnergy": 0
        }),
    );
    let overflow = scenario_value(
        SCENARIO_SCHEMA_VERSION_V2,
        serde_json::json!({
            "kind": "main-core-v1",
            "position": { "x": 9223372036854775808_u64, "y": 0 },
            "integrity": 1,
            "heatEnergy": 0
        }),
    );
    for input in [unknown, floating, overflow] {
        assert!(matches!(
            decode_scenario_manifest(&serde_json::to_vec(&input).expect("JSON serializes")),
            Err(PackageError::InvalidJson {
                artifact: ArtifactKind::Scenario,
                category: JsonErrorCategory::Data,
                ..
            })
        ));
    }

    let duplicate = br#"{
        "schemaVersion":2,
        "scenarioId":"duplicate-main-core",
        "semanticsVersion":"aon-semantics-v1",
        "hashAlgorithm":"blake3-v1",
        "initialWorld":{
            "kind":"main-core-v1",
            "position":{"x":0,"x":1024,"y":0},
            "integrity":1,
            "heatEnergy":0
        },
        "requiredFeatures":{"signal":false,"mobility":false,"capacity":true,"sensing":false,"power":false,"relay":false,"payload":false,"radiation":false},
        "profiles":{
            "numeric":{"path":"n","profileId":"n","profileHash":"0000000000000000000000000000000000000000000000000000000000000000"},
            "physicalScale":{"path":"p","profileId":"p","profileHash":"0000000000000000000000000000000000000000000000000000000000000000"},
            "balance":{"path":"b","profileId":"b","profileHash":"0000000000000000000000000000000000000000000000000000000000000000"}
        }
    }"#;
    assert!(matches!(
        decode_scenario_manifest(duplicate),
        Err(PackageError::InvalidJson {
            artifact: ArtifactKind::Scenario,
            category: JsonErrorCategory::Data,
            ..
        })
    ));
}

#[test]
fn scenario_v1_hash_is_preserved_and_v2_hash_is_main_core_sensitive() {
    const EMPTY_SCENARIO: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/scenarios/empty.json"
    ));
    assert_eq!(
        decode_scenario_manifest(EMPTY_SCENARIO)
            .expect("retained v1 Scenario decodes")
            .canonical_hash()
            .expect("v1 Scenario hashes")
            .to_string(),
        "46a41702ea9dd3f404aa50f0c4952e5d773472c9a7f3410e8cacc8d68bde9ddd"
    );

    let first = scenario_value(SCENARIO_SCHEMA_VERSION_V2, main_core_world(0, 1));
    let second = scenario_value(SCENARIO_SCHEMA_VERSION_V2, main_core_world(WORLD_PITCH, 1));
    let first = decode_scenario_manifest(&serde_json::to_vec(&first).expect("JSON serializes"))
        .expect("v2 Scenario decodes");
    let second = decode_scenario_manifest(&serde_json::to_vec(&second).expect("JSON serializes"))
        .expect("v2 Scenario decodes");
    let mut independently_encoded = Vec::new();
    independently_encoded.extend_from_slice(b"AON\0SCENARIO\0V2\0");
    independently_encoded.extend_from_slice(&2_u16.to_le_bytes());
    independently_encoded.extend_from_slice(&SCENARIO_SCHEMA_VERSION_V2.to_le_bytes());
    for text in ["scenario-decode-test", "aon-semantics-v1", "blake3-v1"] {
        independently_encoded.extend_from_slice(
            &u32::try_from(text.len())
                .expect("retained text length fits u32")
                .to_le_bytes(),
        );
        independently_encoded.extend_from_slice(text.as_bytes());
    }
    independently_encoded.push(1); // MainCoreV1
    independently_encoded.extend_from_slice(&0_i64.to_le_bytes()); // position.x
    independently_encoded.extend_from_slice(&0_i64.to_le_bytes()); // position.y
    independently_encoded.extend_from_slice(&1_u64.to_le_bytes()); // integrity
    independently_encoded.extend_from_slice(&7_u64.to_le_bytes()); // heatEnergy
    independently_encoded.extend_from_slice(&[0, 0, 1, 0, 0, 0, 0, 0]);
    independently_encoded.extend_from_slice(&[0; 32 * 3]);
    let independent = blake3::hash(&independently_encoded).to_hex().to_string();
    assert_eq!(
        independent,
        "f65b3f6b416b9704a8337e883873023d8b3bd61188b821c06974cf66d94a6d41"
    );
    assert_eq!(
        first
            .canonical_hash()
            .expect("v2 Scenario hashes")
            .to_string(),
        independent,
        "the production v2 hash must match an independently assembled byte stream"
    );
    assert_ne!(first.canonical_hash(), second.canonical_hash());

    for changed in [
        scenario_value(
            SCENARIO_SCHEMA_VERSION_V2,
            serde_json::json!({
                "kind": "main-core-v1",
                "position": { "x": 0, "y": WORLD_PITCH },
                "integrity": 1,
                "heatEnergy": 7
            }),
        ),
        scenario_value(
            SCENARIO_SCHEMA_VERSION_V2,
            serde_json::json!({
                "kind": "main-core-v1",
                "position": { "x": 0, "y": 0 },
                "integrity": 2,
                "heatEnergy": 7
            }),
        ),
        scenario_value(
            SCENARIO_SCHEMA_VERSION_V2,
            serde_json::json!({
                "kind": "main-core-v1",
                "position": { "x": 0, "y": 0 },
                "integrity": 1,
                "heatEnergy": 8
            }),
        ),
    ] {
        let changed = decode_scenario_manifest(
            &serde_json::to_vec(&changed).expect("changed Scenario serializes"),
        )
        .expect("changed v2 Scenario decodes");
        assert_ne!(first.canonical_hash(), changed.canonical_hash());
    }
    let mut changed_feature = scenario_value(SCENARIO_SCHEMA_VERSION_V2, main_core_world(0, 1));
    changed_feature["requiredFeatures"]["signal"] = true.into();
    let changed_feature = decode_scenario_manifest(
        &serde_json::to_vec(&changed_feature).expect("changed Scenario serializes"),
    )
    .expect("feature-changed v2 Scenario decodes");
    assert_ne!(first.canonical_hash(), changed_feature.canonical_hash());
    let mut changed_profile = scenario_value(SCENARIO_SCHEMA_VERSION_V2, main_core_world(0, 1));
    changed_profile["profiles"]["balance"]["profileHash"] =
        "0100000000000000000000000000000000000000000000000000000000000000".into();
    let changed_profile = decode_scenario_manifest(
        &serde_json::to_vec(&changed_profile).expect("changed Scenario serializes"),
    )
    .expect("profile-changed v2 Scenario decodes");
    assert_ne!(first.canonical_hash(), changed_profile.canonical_hash());
    assert!(matches!(
        first.initial_world(),
        InitialWorld::MainCoreV1 { .. }
    ));
}

#[test]
fn replay_main_core_anchor_json_is_camel_case_and_round_trips() {
    let mut simulation = Simulation::new(main_core_package(1_000)).expect("Main Core starts");
    let initial = simulation.state_hash();
    let command = CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: Command::PlaceWire(aon_sim::PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
            endpoint_a: EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
            endpoint_b: EndpointTarget::Free,
        }),
    };
    let report = simulation
        .step(std::slice::from_ref(&command))
        .expect("anchored placement succeeds");
    let replay = Replay::new(
        simulation.replay_header(),
        vec![command],
        vec![
            HashCheckpoint {
                next_tick: Tick(0),
                state_hash: initial,
            },
            HashCheckpoint {
                next_tick: Tick(1),
                state_hash: report.state_hash,
            },
        ],
    )
    .expect("Replay shape is valid");
    let artifact = ReplayArtifact::new("scenario.json", replay).expect("path is portable");
    let encoded = encode_replay_artifact(&artifact).expect("Replay encodes");
    let json: serde_json::Value = serde_json::from_slice(&encoded).expect("encoded Replay is JSON");
    assert_eq!(
        json["commands"][0]["command"]["endpointA"],
        serde_json::json!({ "kind": "main-core-anchor", "mainCore": 1 })
    );
    assert_eq!(
        decode_replay_artifact(&encoded).expect("encoded Replay decodes"),
        artifact
    );
}
