use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, DriveStrength, EntityId, FIXED_ONE, Fixed, FixedAabb,
    FixedVec2, GateId, GateType, InitialWorld, LogicLevel, NumericProfile, PhysicalScaleProfile,
    PlaceFixedSubstrateCommand, PlaceGateCommand, ProfileBundle, RoutingDomain,
    SetExternalDriverCommand, SignalArrivalKind, Simulation, SimulationContract, SimulationPackage,
    StageFeatureSet, StateHash, StepReport,
};

const CIRCUIT_PITCH: i64 = 16_384;
const SUBSTRATE_HALF_EXTENT: i64 = 32 * FIXED_ONE;
const EXTERNAL_STRENGTH: DriveStrength = DriveStrength(100);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn simulation() -> Simulation {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("signal-arrival-observations"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("signal-arrival-observations"),
        balance: BalanceProfile::stage0_alpha("signal-arrival-observations"),
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let mut required_features = StageFeatureSet::none();
    required_features.signal = true;
    Simulation::new(SimulationPackage::new(
        "signal-arrival-observations",
        InitialWorld::Empty,
        required_features,
        contract,
        profiles,
    ))
    .expect("the observation fixture starts")
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn expect_created(simulation: &mut Simulation, command: Command) -> EntityId {
    let report = simulation
        .step(&[envelope(simulation, 0, command)])
        .expect("the fixture command succeeds");
    assert!(report.command_rejections.is_empty());
    report.command_acceptances[0]
        .created_entity
        .expect("the fixture command creates one entity")
}

fn place_substrate(simulation: &mut Simulation) -> RoutingDomain {
    let bounds = FixedAabb::new(
        point(-SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT),
        point(SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT),
    );
    RoutingDomain::FixedSubstrate(expect_created(
        simulation,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: point(0, 0),
            routing_area: bounds,
            footprint: bounds,
        }),
    ))
}

fn run_observation_sequence() -> (StepReport, StepReport, StateHash) {
    let mut simulation = simulation();
    let domain = place_substrate(&mut simulation);
    let placement = simulation
        .step(&[
            envelope(
                &simulation,
                20,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(34 * CIRCUIT_PITCH, 0),
                    routing_domain: domain,
                }),
            ),
            envelope(
                &simulation,
                10,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(0, 0),
                    routing_domain: domain,
                }),
            ),
        ])
        .expect("both NOT Gates are placed atomically");
    assert!(placement.command_rejections.is_empty());

    let gates = placement
        .command_acceptances
        .iter()
        .map(|acceptance| {
            GateId(
                acceptance
                    .created_entity
                    .expect("each placement creates one Gate"),
            )
        })
        .collect::<Vec<_>>();
    let ports = gates
        .iter()
        .map(|&gate| {
            simulation
                .gate_signal_ports(gate)
                .expect("the placed Gate exposes signal ports")
        })
        .collect::<Vec<_>>();

    assert_eq!(placement.signal_arrivals.len(), 2);
    assert!(placement.signal_arrivals.iter().all(|arrival| {
        arrival.due_tick == placement.completed_tick
            && arrival.kind == SignalArrivalKind::TopologySync
    }));
    let expected_sync_order = ports
        .iter()
        .map(|ports| (ports.input_a.sink, ports.input_a.external_driver))
        .collect::<Vec<_>>();
    let observed_sync_order = placement
        .signal_arrivals
        .iter()
        .map(|arrival| (arrival.sink, arrival.source_driver))
        .collect::<Vec<_>>();
    assert_eq!(observed_sync_order, expected_sync_order);

    let propagation = simulation
        .step(&[
            envelope(
                &simulation,
                20,
                Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: ports[1].input_a.external_driver,
                    level: LogicLevel::High,
                    strength: EXTERNAL_STRENGTH,
                }),
            ),
            envelope(
                &simulation,
                10,
                Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: ports[0].input_a.external_driver,
                    level: LogicLevel::High,
                    strength: EXTERNAL_STRENGTH,
                }),
            ),
        ])
        .expect("both external Drivers transition atomically");

    assert_eq!(propagation.signal_arrivals.len(), 2);
    assert!(propagation.signal_arrivals.iter().all(|arrival| {
        arrival.due_tick == propagation.completed_tick
            && arrival.kind == SignalArrivalKind::Propagation
            && arrival.sample.level == LogicLevel::High
            && arrival.sample.driver_id == arrival.source_driver
    }));
    let observed_propagation_order = propagation
        .signal_arrivals
        .iter()
        .map(|arrival| (arrival.sink, arrival.source_driver))
        .collect::<Vec<_>>();
    assert_eq!(observed_propagation_order, expected_sync_order);
    assert_eq!(propagation.state_hash, simulation.state_hash());

    (placement, propagation, simulation.state_hash())
}

#[test]
fn due_arrivals_expose_kind_tick_and_full_key_order_without_affecting_determinism() {
    let first = run_observation_sequence();
    let second = run_observation_sequence();

    assert_eq!(first, second);
}
