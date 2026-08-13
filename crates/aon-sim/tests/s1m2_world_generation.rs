use aon_sim::{
    ArtifactBytes, BalanceProfile, Command, CommandEnvelope, CommandRejectionReason, DemandId,
    DemandKind, EndpointTarget, Energy, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GatePort,
    GatePortRef, GateType, HostileCollider, InitialWorld, NumericProfile, PackageError,
    PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceWireCommand,
    PowerRatio, PowerSourceId, ProfileBundle, ReplayFormatVersion, RoutingDomain, Seed, Simulation,
    SimulationError, SimulationPackage, StageFeatureSet, StateHashVersion, Tick,
    WorldGeneratorVersion, WorldInputEvent, decode_balance_profile, decode_numeric_profile,
    decode_package, decode_physical_scale_profile, decode_scenario_manifest,
};
use serde_json::{Value, json};

const NUMERIC_ARTIFACT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL_SCALE_ARTIFACT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const CAPACITY_BALANCE_ARTIFACT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/capacity-probe-alpha.json"
));

const QUANTUM: i64 = 1_024;
const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;

fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn required_power_features() -> StageFeatureSet {
    StageFeatureSet {
        capacity: true,
        sensing: true,
        power: true,
        ..StageFeatureSet::none()
    }
}

fn feature_json(features: StageFeatureSet) -> Value {
    json!({
        "signal": features.signal,
        "mobility": features.mobility,
        "capacity": features.capacity,
        "sensing": features.sensing,
        "power": features.power,
        "relay": features.relay,
        "payload": features.payload,
        "radiation": features.radiation
    })
}

fn power_balance_artifact() -> Vec<u8> {
    let mut balance: Value = serde_json::from_slice(CAPACITY_BALANCE_ARTIFACT)
        .expect("the capacity probe fixture is JSON");
    balance["schemaVersion"] = 3.into();
    balance["profileId"] = "balance-s1-m2-world-generation".into();
    balance["powerProbe"] = json!({
        "gateIdleDemand": 1,
        "gateDriveDemand": 1,
        "gateSwitchDemandPerEnergy": { "numerator": 1, "denominator": 1 },
        "wireLeakagePerWU": { "numerator": 1, "denominator": 1 },
        "wireSenseDemandPerWU": { "numerator": 1, "denominator": 1 },
        "movementDemandPerWU": { "numerator": 1, "denominator": 1 },
        "powerLossK": { "numerator": 0, "denominator": 1 },
        "senseNominalDrive": 400,
        "gateStateRetentionTicks": 3
    });
    serde_json::to_vec(&balance).expect("the S1-M2 Balance artifact serializes")
}

fn power_balance_without_capacity_artifact() -> Vec<u8> {
    let bytes = power_balance_artifact();
    let mut balance: Value =
        serde_json::from_slice(&bytes).expect("the S1-M2 Balance artifact is JSON");
    balance
        .as_object_mut()
        .expect("the Balance artifact is an object")
        .remove("capacityProbe");
    serde_json::to_vec(&balance).expect("the no-capacity Balance artifact serializes")
}

fn source(x: i64, y: i64, generation_per_tick: u64) -> Value {
    json!({
        "position": { "x": x, "y": y },
        "generationPerTick": generation_per_tick
    })
}

fn main_core_power_world(core_x: i64, core_y: i64, sources: Vec<Value>) -> Value {
    json!({
        "kind": "main-core-power-v1",
        "mainCore": {
            "position": { "x": core_x, "y": core_y },
            "integrity": 1_000,
            "heatEnergy": 7
        },
        "powerSources": sources
    })
}

fn profiles_from_artifacts(balance_artifact: &[u8]) -> ProfileBundle {
    ProfileBundle {
        numeric: decode_numeric_profile(NUMERIC_ARTIFACT)
            .expect("the reference Numeric Profile decodes"),
        physical_scale: decode_physical_scale_profile(PHYSICAL_SCALE_ARTIFACT)
            .expect("the reference Physical Scale Profile decodes"),
        balance: decode_balance_profile(balance_artifact)
            .expect("the requested Balance Profile decodes"),
    }
}

fn scenario_artifact(
    initial_world: Value,
    features: StageFeatureSet,
    balance_artifact: &[u8],
) -> Vec<u8> {
    let profiles = profiles_from_artifacts(balance_artifact);
    let numeric_hash = profiles
        .numeric
        .canonical_hash()
        .expect("the Numeric Profile hashes");
    let physical_hash = profiles
        .physical_scale
        .canonical_hash()
        .expect("the Physical Scale Profile hashes");
    let balance_hash = profiles
        .balance
        .canonical_hash()
        .expect("the Balance Profile hashes");

    serde_json::to_vec(&json!({
        "schemaVersion": 3,
        "scenarioId": "s1-m2-world-generation",
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": initial_world,
        "requiredFeatures": feature_json(features),
        "profiles": {
            "numeric": {
                "path": "profiles/numeric/v1.json",
                "profileId": profiles.numeric.profile_id,
                "profileHash": numeric_hash.to_string()
            },
            "physicalScale": {
                "path": "profiles/physical-scale/stage0-alpha.json",
                "profileId": profiles.physical_scale.profile_id,
                "profileHash": physical_hash.to_string()
            },
            "balance": {
                "path": "profiles/balance/s1-m2-world-generation.json",
                "profileId": profiles.balance.profile_id,
                "profileHash": balance_hash.to_string()
            }
        }
    }))
    .expect("the Scenario artifact serializes")
}

fn package_from_artifacts(
    initial_world: Value,
    features: StageFeatureSet,
    balance_artifact: &[u8],
) -> Result<SimulationPackage, PackageError> {
    let scenario = scenario_artifact(initial_world, features, balance_artifact);
    decode_package(ArtifactBytes {
        scenario: &scenario,
        numeric_profile: NUMERIC_ARTIFACT,
        physical_scale_profile: PHYSICAL_SCALE_ARTIFACT,
        balance_profile: balance_artifact,
    })
}

fn reference_world() -> Value {
    main_core_power_world(
        0,
        0,
        vec![
            source(2 * QUANTUM, 0, 20),
            source(-QUANTUM, 2 * QUANTUM, 5),
            source(-QUANTUM, QUANTUM, 7),
        ],
    )
}

fn reference_package() -> SimulationPackage {
    package_from_artifacts(
        reference_world(),
        required_power_features(),
        &power_balance_artifact(),
    )
    .expect("the reference S1-M2 package decodes")
}

fn reference_simulation() -> Simulation {
    Simulation::new(reference_package()).expect("the reference S1-M2 simulation starts")
}

fn simulation_error(package: SimulationPackage) -> SimulationError {
    match Simulation::new(package) {
        Ok(_) => panic!("the invalid package unexpectedly started"),
        Err(error) => error,
    }
}

#[test]
fn scenario_v3_package_generation_sorts_sources_and_assigns_stable_ids() {
    let balance = power_balance_artifact();
    let scenario = scenario_artifact(reference_world(), required_power_features(), &balance);
    let manifest = decode_scenario_manifest(&scenario).expect("the Scenario v3 manifest decodes");

    assert_eq!(manifest.schema_version(), 3);
    assert_eq!(manifest.required_features(), required_power_features());
    let InitialWorld::MainCorePowerV1 {
        main_core_position,
        main_core_integrity,
        main_core_heat_energy,
        power_sources,
    } = manifest.initial_world()
    else {
        panic!("Scenario v3 retained the wrong initial-world variant");
    };
    assert_eq!(*main_core_position, point(0, 0));
    assert_eq!(main_core_integrity.0, 1_000);
    assert_eq!(main_core_heat_energy.0, 7);
    assert_eq!(
        power_sources
            .iter()
            .map(|source| (source.position(), source.generation_per_tick()))
            .collect::<Vec<_>>(),
        vec![
            (point(-QUANTUM, QUANTUM), Energy(7)),
            (point(-QUANTUM, 2 * QUANTUM), Energy(5)),
            (point(2 * QUANTUM, 0), Energy(20)),
        ]
    );

    let package = decode_package(ArtifactBytes {
        scenario: &scenario,
        numeric_profile: NUMERIC_ARTIFACT,
        physical_scale_profile: PHYSICAL_SCALE_ARTIFACT,
        balance_profile: &balance,
    })
    .expect("the Scenario v3 package decodes");
    let simulation = Simulation::new(package).expect("the Scenario v3 package starts");

    let core = simulation
        .main_core_state()
        .copied()
        .expect("main-core-power-v1 creates one Main Core");
    assert_eq!(core.id().entity_id(), EntityId(1));
    assert_eq!(core.position(), point(0, 0));

    let generated_sources = simulation
        .power_sources()
        .copied()
        .map(|state| (state.id(), state.position(), state.generation_per_tick()))
        .collect::<Vec<_>>();
    assert_eq!(
        generated_sources,
        vec![
            (
                PowerSourceId(EntityId(2)),
                point(-QUANTUM, QUANTUM),
                Energy(7)
            ),
            (
                PowerSourceId(EntityId(3)),
                point(-QUANTUM, 2 * QUANTUM),
                Energy(5)
            ),
            (
                PowerSourceId(EntityId(4)),
                point(2 * QUANTUM, 0),
                Energy(20)
            ),
        ]
    );
    for (id, position, generation) in generated_sources {
        let state = simulation
            .power_source_state(id)
            .copied()
            .expect("every generated Power Source is addressable by ID");
        assert_eq!(state.position(), position);
        assert_eq!(state.generation_per_tick(), generation);
    }
    assert!(
        simulation
            .power_source_state(PowerSourceId(EntityId(5)))
            .is_none()
    );
}

#[test]
fn scenario_v3_rejects_ambiguous_or_non_generating_sources_before_package_construction() {
    let balance = power_balance_artifact();
    let duplicate_world =
        main_core_power_world(0, 0, vec![source(QUANTUM, 0, 1), source(QUANTUM, 0, 2)]);
    assert_eq!(
        package_from_artifacts(duplicate_world, required_power_features(), &balance),
        Err(PackageError::DuplicateInitialPowerSourcePosition {
            position: point(QUANTUM, 0)
        })
    );

    let zero_generation = main_core_power_world(0, 0, vec![source(QUANTUM, 0, 0)]);
    assert_eq!(
        package_from_artifacts(zero_generation, required_power_features(), &balance),
        Err(PackageError::NonPositiveInitialWorldField {
            field: "initialWorld.powerSources[].generationPerTick"
        })
    );
}

#[test]
fn scenario_v3_allows_source_less_worlds_and_a_source_at_the_core_position() {
    let balance = power_balance_artifact();
    let source_less = package_from_artifacts(
        main_core_power_world(0, 0, Vec::new()),
        required_power_features(),
        &balance,
    )
    .expect("source-less MainCorePower is valid rho-zero evidence");
    let source_less = Simulation::new(source_less).expect("the source-less world starts");
    assert_eq!(
        source_less
            .main_core_state()
            .expect("the Main Core exists")
            .id()
            .entity_id(),
        EntityId(1)
    );
    assert_eq!(source_less.power_sources().len(), 0);

    let colocated = package_from_artifacts(
        main_core_power_world(0, 0, vec![source(0, 0, 9)]),
        required_power_features(),
        &balance,
    )
    .expect("a Source may share the Main Core point");
    let colocated = Simulation::new(colocated).expect("the colocated Source world starts");
    let source = colocated
        .power_source_state(PowerSourceId(EntityId(2)))
        .copied()
        .expect("the colocated Source receives the first post-Core ID");
    assert_eq!(source.position(), point(0, 0));
    assert_eq!(source.generation_per_tick(), Energy(9));
}

fn bridge_simulation() -> Simulation {
    let balance = power_balance_artifact();
    let package = package_from_artifacts(
        main_core_power_world(-2 * WORLD_PITCH, 0, vec![source(0, 0, 100)]),
        required_power_features(),
        &balance,
    )
    .expect("the routing-domain bridge fixture decodes");
    Simulation::new(package).expect("the routing-domain bridge fixture starts")
}

fn envelope(simulation: &Simulation, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal: 0,
        command,
    }
}

fn place_bridge_substrate(simulation: &mut Simulation, origin: FixedVec2) -> EntityId {
    let bounds = FixedAabb::new(
        point(-2 * WORLD_PITCH, -2 * WORLD_PITCH),
        point(2 * WORLD_PITCH, 2 * WORLD_PITCH),
    );
    let report = simulation
        .step(&[envelope(
            simulation,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin,
                routing_area: bounds,
                footprint: bounds,
            }),
        )])
        .expect("the bridge substrate places");
    assert!(report.command_rejections.is_empty());
    report.command_acceptances[0]
        .created_entity
        .expect("the substrate creates one Entity")
}

#[test]
fn source_anchor_is_the_explicit_bridge_into_a_fixed_substrate_power_region() {
    let mut simulation = bridge_simulation();
    let substrate = place_bridge_substrate(&mut simulation, point(0, 0));
    assert_eq!(substrate, EntityId(3));
    let domain = RoutingDomain::FixedSubstrate(substrate);

    let gate = GateId(EntityId(4));
    let gate_origin = point(2 * CIRCUIT_PITCH, CIRCUIT_PITCH);
    let gate_report = simulation
        .step(&[envelope(
            &simulation,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: gate_origin,
                routing_domain: domain,
            }),
        )])
        .expect("the bridge fixture Gate places");
    assert!(gate_report.command_rejections.is_empty());
    assert_eq!(
        gate_report.command_acceptances[0].created_entity,
        Some(gate.entity_id())
    );

    let source = PowerSourceId(EntityId(2));
    let power_port = point(2 * CIRCUIT_PITCH, 0);
    let report = simulation
        .step(&[envelope(
            &simulation,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![
                    point(0, 0),
                    point(0, -CIRCUIT_PITCH),
                    point(2 * CIRCUIT_PITCH, -CIRCUIT_PITCH),
                    power_port,
                ],
                endpoint_a: EndpointTarget::PowerSourceAnchor(source),
                endpoint_b: EndpointTarget::GatePort(GatePortRef {
                    gate,
                    port: GatePort::Power,
                }),
            }),
        )])
        .expect("the Source-to-Gate bridge Wire places and solves");
    assert!(
        report.command_rejections.is_empty(),
        "bridge Wire rejected: {:?}",
        report.command_rejections
    );
    assert_eq!(
        report.command_acceptances[0].created_entity,
        Some(EntityId(5))
    );

    let power = report.power.expect("Power-enabled worlds report the solve");
    let gate_idle = power
        .load(DemandId::new(gate.entity_id(), DemandKind::GateIdle))
        .expect("the actual Gate contributes its idle demand");
    assert_eq!(gate_idle.ratio, PowerRatio::ONE);
    assert_eq!(
        gate_idle
            .source_route
            .as_ref()
            .expect("the bridged Gate has a canonical Source route")
            .source(),
        source
    );
    assert_eq!(
        power
            .region(gate_idle.region)
            .expect("the Gate load region is reported")
            .sources,
        vec![source]
    );
}

#[test]
fn substrate_source_bridge_still_requires_the_source_point_inside_the_routing_area() {
    let mut attempted = bridge_simulation();
    let mut control = bridge_simulation();
    let far_origin = point(4 * WORLD_PITCH, 0);
    let attempted_substrate = place_bridge_substrate(&mut attempted, far_origin);
    let control_substrate = place_bridge_substrate(&mut control, far_origin);
    assert_eq!(attempted_substrate, control_substrate);

    let domain = RoutingDomain::FixedSubstrate(attempted_substrate);
    let report = attempted
        .step(&[envelope(
            &attempted,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(0, 0), far_origin],
                endpoint_a: EndpointTarget::PowerSourceAnchor(PowerSourceId(EntityId(2))),
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("an out-of-area Source binding is an ordinary command rejection");
    control.step(&[]).expect("the control Tick succeeds");

    assert!(report.command_acceptances.is_empty());
    assert_eq!(report.command_rejections.len(), 1);
    assert_eq!(
        report.command_rejections[0].reason,
        CommandRejectionReason::SubstrateBoundsViolation
    );
    assert!(!report.topology_changed);
    assert_eq!(attempted.state_hash(), control.state_hash());
}

#[test]
fn main_core_power_world_requires_the_complete_feature_and_profile_triad() {
    let balance = power_balance_artifact();
    for features in [
        StageFeatureSet {
            capacity: false,
            ..required_power_features()
        },
        StageFeatureSet {
            sensing: false,
            ..required_power_features()
        },
        StageFeatureSet {
            power: false,
            ..required_power_features()
        },
    ] {
        let package = package_from_artifacts(reference_world(), features, &balance)
            .expect("feature declarations decode before engine validation");
        assert_eq!(
            simulation_error(package),
            SimulationError::MainCorePowerRequiresFeatures
        );
    }

    let missing_power = package_from_artifacts(
        reference_world(),
        required_power_features(),
        CAPACITY_BALANCE_ARTIFACT,
    )
    .expect("the valid schema-v2 Balance Profile decodes");
    assert_eq!(
        simulation_error(missing_power),
        SimulationError::MainCorePowerRequiresProfiles
    );

    let no_capacity_balance = power_balance_without_capacity_artifact();
    let missing_capacity = package_from_artifacts(
        reference_world(),
        required_power_features(),
        &no_capacity_balance,
    )
    .expect("the valid schema-v3 Balance Profile may omit capacityProbe");
    assert_eq!(
        simulation_error(missing_capacity),
        SimulationError::MainCorePowerRequiresProfiles
    );
}

#[test]
fn world_generation_validates_main_core_and_power_source_quantization() {
    let balance = power_balance_artifact();
    let off_quantum_core = package_from_artifacts(
        main_core_power_world(1, 0, vec![source(QUANTUM, 0, 1)]),
        required_power_features(),
        &balance,
    )
    .expect("artifact decoding does not own runtime geometry quantization");
    assert_eq!(
        simulation_error(off_quantum_core),
        SimulationError::InvalidMainCoreGeometryQuantum
    );

    let off_quantum_source = package_from_artifacts(
        main_core_power_world(0, 0, vec![source(1, 0, 1)]),
        required_power_features(),
        &balance,
    )
    .expect("artifact decoding does not own runtime geometry quantization");
    assert_eq!(
        simulation_error(off_quantum_source),
        SimulationError::InvalidPowerSourceGeometryQuantum
    );
}

#[test]
fn replay_header_identifies_v2_v7_main_core_power_generation() {
    let simulation = reference_simulation();
    let header = simulation.replay_header();

    assert_eq!(header.format_version, ReplayFormatVersion::V2);
    assert_eq!(header.state_hash_version, StateHashVersion::V7);
    assert_eq!(
        header.world_generator_version,
        WorldGeneratorVersion::MainCorePowerV1
    );
    assert_eq!(header.seed, Seed::ZERO);
    assert_eq!(header.initial_state_hash, simulation.state_hash());
}

fn hostile(id: u64, radius: i64) -> HostileCollider {
    HostileCollider {
        id,
        center: point(id as i64 * QUANTUM, 0),
        radius: Fixed(radius),
    }
}

fn assert_world_inputs_rejected_atomically(
    simulation: &mut Simulation,
    inputs: &[WorldInputEvent],
    expected: SimulationError,
) {
    let before_tick = simulation.next_tick();
    let before_hash = simulation.state_hash();
    let before_core = simulation.main_core_state().copied();
    let before_sources = simulation.power_sources().copied().collect::<Vec<_>>();

    assert_eq!(
        simulation
            .step_with_world_inputs(&[], inputs)
            .expect_err("the invalid WorldInput stream must be rejected"),
        expected
    );
    assert_eq!(simulation.next_tick(), before_tick);
    assert_eq!(simulation.state_hash(), before_hash);
    assert_eq!(simulation.main_core_state().copied(), before_core);
    assert_eq!(
        simulation.power_sources().copied().collect::<Vec<_>>(),
        before_sources
    );
}

#[test]
fn world_input_tick_frame_and_hostile_validation_is_atomic() {
    let mut simulation = reference_simulation();

    assert_world_inputs_rejected_atomically(
        &mut simulation,
        &[WorldInputEvent::HostileFrame {
            target_tick: Tick(1),
            hostiles: Vec::new(),
        }],
        SimulationError::WorldInputTickMismatch,
    );
    assert_world_inputs_rejected_atomically(
        &mut simulation,
        &[
            WorldInputEvent::HostileFrame {
                target_tick: Tick(0),
                hostiles: Vec::new(),
            },
            WorldInputEvent::HostileFrame {
                target_tick: Tick(0),
                hostiles: Vec::new(),
            },
        ],
        SimulationError::DuplicateWorldInputFrame,
    );
    assert_world_inputs_rejected_atomically(
        &mut simulation,
        &[WorldInputEvent::HostileFrame {
            target_tick: Tick(0),
            hostiles: vec![hostile(0, 0)],
        }],
        SimulationError::InvalidHostileId,
    );
    assert_world_inputs_rejected_atomically(
        &mut simulation,
        &[WorldInputEvent::HostileFrame {
            target_tick: Tick(0),
            hostiles: vec![hostile(7, 0), hostile(7, QUANTUM)],
        }],
        SimulationError::DuplicateHostileId { id: 7 },
    );
    assert_world_inputs_rejected_atomically(
        &mut simulation,
        &[WorldInputEvent::HostileFrame {
            target_tick: Tick(0),
            hostiles: vec![hostile(9, -1)],
        }],
        SimulationError::NegativeHostileRadius { id: 9 },
    );
    assert_world_inputs_rejected_atomically(
        &mut simulation,
        &[WorldInputEvent::HostileFrame {
            target_tick: Tick(0),
            hostiles: vec![hostile(2, 0), hostile(1, 0)],
        }],
        SimulationError::InvalidCanonicalState,
    );
}

#[test]
fn step_is_exactly_equivalent_to_an_empty_world_input_slice() {
    let mut via_step = reference_simulation();
    let mut via_explicit_empty_inputs = reference_simulation();
    let mut via_empty_frame = reference_simulation();

    let shorthand = via_step.step(&[]).expect("step succeeds");
    let explicit = via_explicit_empty_inputs
        .step_with_world_inputs(&[], &[])
        .expect("an explicit empty WorldInput slice succeeds");
    let empty_frame = via_empty_frame
        .step_with_world_inputs(
            &[],
            &[WorldInputEvent::HostileFrame {
                target_tick: Tick(0),
                hostiles: Vec::new(),
            }],
        )
        .expect("an empty current-Tick HostileFrame succeeds");

    assert_eq!(shorthand, explicit);
    assert_eq!(shorthand, empty_frame);
    assert_eq!(via_step.next_tick(), via_explicit_empty_inputs.next_tick());
    assert_eq!(via_step.next_tick(), via_empty_frame.next_tick());
    assert_eq!(
        via_step.state_hash(),
        via_explicit_empty_inputs.state_hash()
    );
    assert_eq!(via_step.state_hash(), via_empty_frame.state_hash());
    assert_eq!(
        via_step.power_sources().copied().collect::<Vec<_>>(),
        via_explicit_empty_inputs
            .power_sources()
            .copied()
            .collect::<Vec<_>>()
    );
}

// Keep the constructors used above tied to the public profile API as well as JSON artifacts.
#[test]
fn generated_power_profile_matches_the_public_probe_constructor() {
    let generated = decode_balance_profile(&power_balance_artifact())
        .expect("the generated test artifact decodes");
    let expected = BalanceProfile::power_probe_alpha("balance-s1-m2-world-generation");
    assert_eq!(generated, expected);

    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-v1"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical-scale-stage0-alpha"),
        balance: expected,
    };
    profiles
        .validate()
        .expect("the public S1-M2 profiles validate");
}
