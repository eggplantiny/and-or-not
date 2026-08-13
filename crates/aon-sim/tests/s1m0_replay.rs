use aon_sim::{
    BalanceProfile, Fixed, GateGeometryVariant, HashCheckpoint, InitialWorld, LongWireDesign,
    ModuleContract, NumericProfile, PhysicalScaleMatrix, PhysicalScaleProfile, ProfileBundle,
    Replay, ReplayArtifact, ReplayContractField, ReplayError, Simulation, SimulationContract,
    SimulationPackage, StageFeatureSet, Tick, decode_module_artifact, decode_replay_artifact,
    decode_scenario_manifest, encode_replay_artifact, validate_module_against,
};

fn generated_profiles() -> Vec<PhysicalScaleProfile> {
    let base = PhysicalScaleProfile::stage0_alpha("s1m0-replay-base");
    let geometry = GateGeometryVariant::from_profile(&base);
    let circuit_pitch = base.circuit_routing_pitch;
    let world_pitch = base.world_routing_pitch;
    PhysicalScaleMatrix {
        base_profile: base,
        gate_geometries: vec![geometry],
        circuit_routing_pitches: vec![circuit_pitch],
        world_routing_pitches: vec![world_pitch, aon_sim::Fixed(world_pitch.0 * 2)],
    }
    .resolve()
    .expect("two generated Replay profiles are valid")
    .into_iter()
    .map(|resolved| resolved.profile().clone())
    .collect()
}

fn package(physical_scale: PhysicalScaleProfile) -> SimulationPackage {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-v1"),
        physical_scale,
        balance: BalanceProfile::stage0_alpha("balance-stage0-alpha"),
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles form a contract");
    SimulationPackage::new(
        "s1m0-generated-profile",
        InitialWorld::Empty,
        StageFeatureSet::none(),
        contract,
        profiles,
    )
}

fn record_replay(package: SimulationPackage) -> (Vec<aon_sim::StateHash>, Vec<u8>) {
    let mut simulation = Simulation::new(package).expect("generated package starts");
    let header = simulation.replay_header();
    let mut trace = vec![simulation.state_hash()];
    for _ in 0..4 {
        trace.push(
            simulation
                .step(&[])
                .expect("empty world advances")
                .state_hash,
        );
    }
    let replay = Replay::new_v2(
        header,
        Vec::new(),
        Vec::new(),
        vec![
            HashCheckpoint {
                next_tick: Tick(0),
                state_hash: trace[0],
            },
            HashCheckpoint {
                next_tick: Tick(4),
                state_hash: trace[4],
            },
        ],
    )
    .expect("recorded Replay is valid");
    let artifact = ReplayArtifact::new("scenario.json", replay).expect("portable locator is valid");
    let bytes = encode_replay_artifact(&artifact).expect("Replay encodes");
    (trace, bytes)
}

#[test]
fn generated_profile_replay_round_trips_with_the_identical_full_trace() {
    let profile = generated_profiles().remove(0);
    let expected_physical_hash = profile
        .canonical_hash()
        .expect("generated Physical Profile hashes");
    let replay_package = package(profile.clone());
    assert_eq!(
        replay_package.contract().physical_scale_profile_hash,
        expected_physical_hash
    );
    let (expected_trace, encoded) = record_replay(replay_package.clone());
    let decoded = decode_replay_artifact(&encoded).expect("Replay artifact decodes");
    assert_eq!(
        decoded.replay().header().physical_scale_profile_hash,
        expected_physical_hash
    );
    assert_eq!(
        encode_replay_artifact(&decoded).expect("decoded Replay re-encodes"),
        encoded
    );

    let mut restarted = Simulation::new(replay_package).expect("same generated profile restarts");
    decoded
        .replay()
        .validate_against(&restarted)
        .expect("same semantic profile is Replay-compatible");
    let mut actual_trace = vec![restarted.state_hash()];
    while restarted.next_tick() < decoded.replay().final_next_tick() {
        actual_trace.push(restarted.step(&[]).expect("Replay advances").state_hash);
    }
    assert_eq!(actual_trace, expected_trace);
    decoded
        .replay()
        .verify_trace(&actual_trace)
        .expect("full trace matches retained checkpoints");

    let mut same_semantics = profile;
    same_semantics.profile_id = "different-display-id".to_owned();
    let renamed = Simulation::new(package(same_semantics)).expect("renamed profile starts");
    decoded
        .replay()
        .validate_against(&renamed)
        .expect("profileId is not semantic Replay identity");
}

#[test]
fn different_generated_physical_profile_is_rejected_before_execution() {
    let mut profiles = generated_profiles();
    let first = profiles.remove(0);
    let second = profiles.remove(0);
    assert_ne!(first.canonical_hash(), second.canonical_hash());

    let (_, encoded) = record_replay(package(first));
    let replay = decode_replay_artifact(&encoded).expect("Replay decodes");
    let mismatched = Simulation::new(package(second)).expect("other profile starts");
    assert!(matches!(
        replay.replay().validate_against(&mismatched),
        Err(ReplayError::ContractMismatch {
            field: ReplayContractField::PhysicalScaleProfileHash,
            ..
        })
    ));
    assert_eq!(mismatched.next_tick(), Tick(0));
}

#[test]
fn matrix_generation_does_not_mutate_an_existing_simulation() {
    let profile = PhysicalScaleProfile::stage0_alpha("noninterference-profile");
    let simulation = Simulation::new(package(profile.clone())).expect("simulation starts");
    let before_tick = simulation.next_tick();
    let before_hash = simulation.state_hash();
    let before_contract = *simulation.contract();

    let matrix = PhysicalScaleMatrix {
        base_profile: profile.clone(),
        gate_geometries: vec![GateGeometryVariant::from_profile(&profile)],
        circuit_routing_pitches: vec![profile.circuit_routing_pitch],
        world_routing_pitches: vec![profile.world_routing_pitch],
    };
    let resolved = matrix.resolve().expect("matrix resolves independently");
    let scenario =
        decode_scenario_manifest(include_bytes!("../../../fixtures/scenarios/empty.json"))
            .expect("Scenario artifact decodes");
    let _scenario_hash = scenario.canonical_hash().expect("Scenario artifact hashes");
    let _design_hash = LongWireDesign::try_from_distance(Fixed(profile.world_routing_pitch.0 * 2))
        .expect("positive exact Design distance")
        .canonical_hash();
    let _profile_hashes = simulation
        .profiles()
        .canonical_hashes()
        .expect("Profiles hash without Simulation access");

    let mut module = decode_module_artifact(include_str!(
        "../../../fixtures/modules/s1m0-absolute-geometry-v1.json"
    ))
    .expect("retained Module decodes");
    module.contract = ModuleContract {
        semantics_version: before_contract.semantics_version,
        numeric_profile_hash: before_contract.numeric_profile_hash,
        physical_scale_profile_hash: before_contract.physical_scale_profile_hash,
    };
    let _module_hash = module.semantic_hash().expect("Module artifact hashes");
    validate_module_against(&module, &before_contract, &profile)
        .expect("Module validates without Simulation access");

    assert_eq!(resolved.len(), 1);
    assert_eq!(simulation.next_tick(), before_tick);
    assert_eq!(simulation.state_hash(), before_hash);
    assert_eq!(*simulation.contract(), before_contract);
}
