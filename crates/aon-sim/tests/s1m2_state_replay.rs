use aon_sim::{
    BalanceProfile, CompiledPowerTopology, Fixed, FixedVec2, HashCheckpoint, HeatEnergy,
    HostileCollider, InitialWorld, Integrity, JsonErrorCategory, NominalPowerDemandSet,
    NumericProfile, PhysicalScaleProfile, PowerNodeKey, PowerSourceAttachment, PowerSourceStore,
    PowerTopologyInput, ProfileBundle, Replay, ReplayArtifact, ReplayError, ReplayFormatVersion,
    STATE_HASH_VERSION_V3, STATE_HASH_VERSION_V4, STATE_HASH_VERSION_V5, STATE_HASH_VERSION_V6,
    Simulation, SimulationContract, SimulationPackage, StageFeatureSet, StateHashVersion, Tick,
    WorldGeneratorVersion, WorldInputEvent, decode_replay_artifact, decode_scenario_manifest,
    encode_replay_artifact, solve_power_step,
};

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn hostile(id: u64, x: i64, radius: i64) -> HostileCollider {
    HostileCollider {
        id,
        center: point(x, 0),
        radius: Fixed(radius),
    }
}

fn profiles(balance: BalanceProfile) -> ProfileBundle {
    ProfileBundle {
        numeric: NumericProfile::reference_v1("state-replay-numeric"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("state-replay-physical"),
        balance,
    }
}

fn simulation(
    scenario_id: &str,
    initial_world: InitialWorld,
    required_features: StageFeatureSet,
    profiles: ProfileBundle,
) -> Simulation {
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    Simulation::new(SimulationPackage::new(
        scenario_id,
        initial_world,
        required_features,
        contract,
        profiles,
    ))
    .expect("test Simulation starts")
}

fn empty_simulation() -> Simulation {
    simulation(
        "state-v6-empty",
        InitialWorld::Empty,
        StageFeatureSet::none(),
        profiles(BalanceProfile::stage0_alpha("state-replay-balance")),
    )
}

fn main_core_simulation() -> Simulation {
    simulation(
        "state-v6-main-core",
        InitialWorld::MainCoreV1 {
            position: point(0, 0),
            integrity: Integrity(1_000),
            heat_energy: HeatEnergy(7),
        },
        StageFeatureSet {
            capacity: true,
            ..StageFeatureSet::none()
        },
        profiles(BalanceProfile::capacity_probe_alpha(
            "state-replay-capacity",
        )),
    )
}

fn main_core_power_world() -> InitialWorld {
    let scenario = serde_json::json!({
        "schemaVersion": 3,
        "scenarioId": "state-v6-main-core-power",
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": 0, "y": 0 },
                "integrity": 1000,
                "heatEnergy": 7
            },
            "powerSources": [{
                "position": { "x": 65536, "y": 0 },
                "generationPerTick": 16
            }]
        },
        "requiredFeatures": {
            "signal": false,
            "mobility": false,
            "capacity": true,
            "sensing": true,
            "power": true,
            "relay": false,
            "payload": false,
            "radiation": false
        },
        "profiles": {
            "numeric": {
                "path": "numeric.json",
                "profileId": "numeric",
                "profileHash": ZERO_HASH
            },
            "physicalScale": {
                "path": "physical.json",
                "profileId": "physical",
                "profileHash": ZERO_HASH
            },
            "balance": {
                "path": "balance.json",
                "profileId": "balance",
                "profileHash": ZERO_HASH
            }
        }
    });
    decode_scenario_manifest(&serde_json::to_vec(&scenario).expect("Scenario JSON encodes"))
        .expect("Scenario v3 decodes")
        .initial_world()
        .clone()
}

fn main_core_power_simulation() -> Simulation {
    simulation(
        "state-v6-main-core-power",
        main_core_power_world(),
        StageFeatureSet {
            capacity: true,
            sensing: true,
            power: true,
            ..StageFeatureSet::none()
        },
        profiles(BalanceProfile::power_probe_alpha("state-replay-power")),
    )
}

fn initial_checkpoint(simulation: &Simulation) -> HashCheckpoint {
    HashCheckpoint {
        next_tick: Tick(0),
        state_hash: simulation.state_hash(),
    }
}

#[test]
fn every_current_initial_world_advertises_state_v6_in_a_v2_header() {
    let cases = [
        (empty_simulation(), WorldGeneratorVersion::EmptyV1),
        (main_core_simulation(), WorldGeneratorVersion::MainCoreV1),
        (
            main_core_power_simulation(),
            WorldGeneratorVersion::MainCorePowerV1,
        ),
    ];
    let mut initial_hashes = Vec::new();
    for (simulation, expected_generator) in cases {
        let header = simulation.replay_header();
        assert_eq!(header.format_version, ReplayFormatVersion::V2);
        assert_eq!(header.state_hash_version, StateHashVersion::V6);
        assert_eq!(header.state_hash_version.as_str(), STATE_HASH_VERSION_V6);
        assert_eq!(header.world_generator_version, expected_generator);
        assert_eq!(header.initial_state_hash, simulation.state_hash());
        initial_hashes.push(header.initial_state_hash);
    }
    assert_ne!(initial_hashes[0], initial_hashes[1]);
    assert_ne!(initial_hashes[1], initial_hashes[2]);
    assert_ne!(initial_hashes[0], initial_hashes[2]);
}

#[test]
fn retained_state_v3_v4_and_v5_headers_are_typed_but_execution_rejected() {
    let simulation = empty_simulation();
    for (version, actual) in [
        (StateHashVersion::V3, STATE_HASH_VERSION_V3),
        (StateHashVersion::V4, STATE_HASH_VERSION_V4),
        (StateHashVersion::V5, STATE_HASH_VERSION_V5),
    ] {
        let mut header = simulation.replay_header();
        header.state_hash_version = version;
        let replay = Replay::new_v2(
            header,
            Vec::new(),
            Vec::new(),
            vec![initial_checkpoint(&simulation)],
        )
        .expect("retained version remains a typed Replay value");
        assert_eq!(
            replay.validate_against(&simulation),
            Err(ReplayError::UnsupportedStateHashVersion {
                expected: STATE_HASH_VERSION_V6,
                actual: actual.to_owned(),
            })
        );
    }
}

#[test]
fn replay_v2_hostile_frames_are_strict_normalized_and_round_trip_exactly() {
    let mut execution = empty_simulation();
    let header = execution.replay_header();
    let initial = initial_checkpoint(&execution);
    let unsorted_frame_zero = WorldInputEvent::HostileFrame {
        target_tick: Tick(0),
        hostiles: vec![hostile(9, 90, 3), hostile(2, 20, 1)],
    };
    let normalized_frame_zero = WorldInputEvent::HostileFrame {
        target_tick: Tick(0),
        hostiles: vec![hostile(2, 20, 1), hostile(9, 90, 3)],
    };
    let frame_one = WorldInputEvent::HostileFrame {
        target_tick: Tick(1),
        hostiles: Vec::new(),
    };
    let first = execution
        .step_with_world_inputs(&[], std::slice::from_ref(&normalized_frame_zero))
        .expect("Tick 0 frame executes");
    let second = execution
        .step_with_world_inputs(&[], std::slice::from_ref(&frame_one))
        .expect("Tick 1 empty frame executes");
    let trace = [initial.state_hash, first.state_hash, second.state_hash];

    let replay = Replay::new_v2(
        header,
        Vec::new(),
        vec![frame_one, unsorted_frame_zero],
        vec![
            initial,
            HashCheckpoint {
                next_tick: Tick(2),
                state_hash: second.state_hash,
            },
        ],
    )
    .expect("valid v2 Replay normalizes");
    assert_eq!(
        replay
            .world_inputs()
            .iter()
            .map(WorldInputEvent::target_tick)
            .collect::<Vec<_>>(),
        vec![Tick(0), Tick(1)]
    );
    assert_eq!(
        replay.world_inputs()[0]
            .hostiles()
            .iter()
            .map(|collider| collider.id)
            .collect::<Vec<_>>(),
        vec![2, 9]
    );
    assert!(replay.world_inputs()[1].hostiles().is_empty());
    replay
        .verify_trace(&trace)
        .expect("recorded trace matches checkpoints");
    replay
        .validate_against(&empty_simulation())
        .expect("fresh matching Simulation accepts v2 Replay");

    let artifact = ReplayArtifact::new("scenarios/state-v6-empty.json", replay)
        .expect("portable Replay artifact constructs");
    let encoded = encode_replay_artifact(&artifact).expect("Replay encodes");
    let decoded = decode_replay_artifact(&encoded).expect("Replay decodes");
    assert_eq!(decoded, artifact);
    assert_eq!(
        encode_replay_artifact(&decoded).expect("decoded Replay re-encodes"),
        encoded
    );

    let mut unknown: serde_json::Value =
        serde_json::from_slice(&encoded).expect("encoded Replay is JSON");
    unknown["worldInputs"][0]["unexpected"] = true.into();
    assert!(matches!(
        decode_replay_artifact(&serde_json::to_vec(&unknown).expect("mutated JSON encodes")),
        Err(ReplayError::InvalidJson {
            category: JsonErrorCategory::Data,
            ..
        })
    ));
}

#[test]
fn replay_v2_rejects_duplicate_frames_and_invalid_hostile_identity_or_radius() {
    let simulation = empty_simulation();
    let header = simulation.replay_header();
    let checkpoints = || {
        vec![
            initial_checkpoint(&simulation),
            HashCheckpoint {
                next_tick: Tick(2),
                state_hash: simulation.state_hash(),
            },
        ]
    };
    let frame = |target_tick, hostiles| WorldInputEvent::HostileFrame {
        target_tick: Tick(target_tick),
        hostiles,
    };

    assert_eq!(
        Replay::new_v2(
            header,
            Vec::new(),
            vec![frame(0, Vec::new()), frame(0, Vec::new())],
            checkpoints(),
        ),
        Err(ReplayError::DuplicateWorldInputTick {
            target_tick: Tick(0),
        })
    );
    assert_eq!(
        Replay::new_v2(
            header,
            Vec::new(),
            vec![frame(0, vec![hostile(0, 0, 0)])],
            checkpoints(),
        ),
        Err(ReplayError::ZeroHostileId {
            target_tick: Tick(0),
        })
    );
    assert_eq!(
        Replay::new_v2(
            header,
            Vec::new(),
            vec![frame(0, vec![hostile(4, 0, 0), hostile(4, 1, 0)],)],
            checkpoints(),
        ),
        Err(ReplayError::DuplicateHostileId {
            target_tick: Tick(0),
            hostile_id: 4,
        })
    );
    assert_eq!(
        Replay::new_v2(
            header,
            Vec::new(),
            vec![frame(0, vec![hostile(5, 0, -1)])],
            checkpoints(),
        ),
        Err(ReplayError::NegativeHostileRadius {
            target_tick: Tick(0),
            hostile_id: 5,
        })
    );
}

#[test]
fn explicit_empty_hostile_frame_and_omission_are_simulation_equivalent() {
    let mut omitted = main_core_power_simulation();
    let mut explicit = main_core_power_simulation();
    let omitted_report = omitted.step(&[]).expect("omitted frame Tick succeeds");
    let explicit_report = explicit
        .step_with_world_inputs(
            &[],
            &[WorldInputEvent::HostileFrame {
                target_tick: Tick(0),
                hostiles: Vec::new(),
            }],
        )
        .expect("explicit empty frame Tick succeeds");
    assert_eq!(explicit_report, omitted_report);
    assert_eq!(explicit.state_hash(), omitted.state_hash());
}

#[test]
fn derived_power_and_network_analyzer_reads_do_not_interfere_with_state_hash() {
    let simulation = main_core_power_simulation();
    let before = simulation.state_hash();

    let first_analyzer = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer read succeeds")
        .expect("Main Core exposes Analyzer");
    let second_analyzer = simulation
        .network_analyzer_snapshot()
        .expect("repeated Analyzer read succeeds")
        .expect("Main Core exposes Analyzer");
    assert_eq!(first_analyzer, second_analyzer);

    let source_states = simulation.power_sources().copied().collect::<Vec<_>>();
    let source_store =
        PowerSourceStore::new(source_states.clone()).expect("copied Source view is valid");
    for source in &source_states {
        assert_eq!(simulation.power_source_state(source.id()), Some(source));
    }
    let topology = CompiledPowerTopology::compile(&PowerTopologyInput {
        bodies: Vec::new(),
        sources: source_states
            .iter()
            .map(|source| PowerSourceAttachment {
                source: source.id(),
                node: PowerNodeKey::SourceAnchor(source.id()),
            })
            .collect(),
        loads: Vec::new(),
    })
    .expect("derived isolated-Source topology compiles");
    let nominal = NominalPowerDemandSet::default();
    let probe = simulation
        .profiles()
        .balance
        .power_probe
        .expect("M2 profile has Power probe");
    let first_power = solve_power_step(&topology, &source_store, &nominal, probe)
        .expect("derived Power read solves");
    let second_power = solve_power_step(&topology, &source_store, &nominal, probe)
        .expect("repeated derived Power read solves");
    assert_eq!(first_power, second_power);
    assert!(first_power.loads.is_empty());
    assert!(first_power.heat_contributions.is_empty());
    assert_eq!(simulation.state_hash(), before);
}
