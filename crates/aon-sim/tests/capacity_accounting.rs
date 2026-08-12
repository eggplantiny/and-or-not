use aon_sim::{
    BalanceProfile, Capacity, Command, CommandEnvelope, EndpointTarget, EntityId, Fixed, FixedAabb,
    FixedVec2, HeatEnergy, InitialWorld, Integrity, MainCoreId, NumericProfile,
    PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceMobileSubstrateCommand,
    PlaceWireCommand, ProfileBundle, RemoveEntityCommand, RoutingDomain, Simulation,
    SimulationContract, SimulationPackage, StageFeatureSet, StepReport, WireId,
};

const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn package(main_core_capacity: u64) -> SimulationPackage {
    let mut balance = BalanceProfile::capacity_probe_alpha("balance-capacity");
    balance
        .capacity_probe
        .as_mut()
        .expect("capacity section exists")
        .main_core_capacity = main_core_capacity;
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical"),
        balance,
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("test profiles are valid");
    SimulationPackage::new(
        "capacity-accounting",
        InitialWorld::MainCoreV1 {
            position: point(0, 0),
            integrity: Integrity(1_000),
            heat_energy: HeatEnergy(0),
        },
        StageFeatureSet {
            capacity: true,
            ..StageFeatureSet::none()
        },
        contract,
        profiles,
    )
}

fn wire(
    points: Vec<FixedVec2>,
    routing_domain: RoutingDomain,
    endpoint_a: EndpointTarget,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain,
        points,
        endpoint_a,
        endpoint_b: EndpointTarget::Free,
    })
}

fn step_commands(simulation: &mut Simulation, commands: Vec<Command>) -> StepReport {
    let target_tick = simulation.next_tick();
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick,
            ordinal: u64::try_from(ordinal).expect("test ordinal fits"),
            command,
        })
        .collect::<Vec<_>>();
    let report = simulation.step(&envelopes).expect("capacity Tick succeeds");
    assert!(
        report.command_rejections.is_empty(),
        "fixture commands must be accepted: {:?}",
        report.command_rejections
    );
    assert_eq!(report.command_acceptances.len(), envelopes.len());
    report
}

#[test]
fn phase4_counts_each_wire_body_once_in_raw_fixed_units_across_routing_domains() {
    let mut simulation = Simulation::new(package(1_000)).expect("capacity simulation starts");
    let core = MainCoreId(EntityId(1));
    let open_report = step_commands(
        &mut simulation,
        vec![wire(
            vec![point(0, 0), point(10 * WORLD_PITCH, 0)],
            RoutingDomain::OpenWorld,
            EndpointTarget::MainCoreAnchor(core),
        )],
    );
    let open = open_report
        .network_accounting
        .expect("Phase 4 accounting is active");
    assert_eq!(open.used(), Capacity(10 * WORLD_PITCH as u64));
    assert_eq!(open.supported(), Capacity(1_000 * WORLD_PITCH as u64));

    let substrate_origin = point(0, 20 * WORLD_PITCH);
    step_commands(
        &mut simulation,
        vec![Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: substrate_origin,
            routing_area: FixedAabb::new(
                point(-8 * CIRCUIT_PITCH, -8 * CIRCUIT_PITCH),
                point(8 * CIRCUIT_PITCH, 8 * CIRCUIT_PITCH),
            ),
            footprint: FixedAabb::new(
                point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
                point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
            ),
        })],
    );
    let internal_report = step_commands(
        &mut simulation,
        vec![wire(
            vec![substrate_origin, point(2 * WORLD_PITCH, 20 * WORLD_PITCH)],
            RoutingDomain::FixedSubstrate(EntityId(3)),
            EndpointTarget::Free,
        )],
    );
    let accounting = internal_report
        .network_accounting
        .expect("Phase 4 accounting is active");
    assert_eq!(accounting.used(), Capacity(12 * WORLD_PITCH as u64));

    let before = simulation.state_hash();
    let analyzer = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer arithmetic fits")
        .expect("capacity session exposes Analyzer");
    assert_eq!(analyzer.accounting(), accounting);
    assert_eq!(
        analyzer
            .wires()
            .iter()
            .map(|row| row.wire())
            .collect::<Vec<_>>(),
        vec![WireId(EntityId(2)), WireId(EntityId(4))]
    );
    assert_eq!(
        analyzer
            .wires()
            .iter()
            .map(|row| row.length().0)
            .sum::<u64>(),
        accounting.used().0
    );
    assert_eq!(simulation.state_hash(), before, "Analyzer is read-only");
}

#[test]
fn splitting_a_centerline_across_wire_entities_does_not_change_additive_usage() {
    let core = MainCoreId(EntityId(1));
    let mut whole = Simulation::new(package(1_000)).expect("whole simulation starts");
    let whole_report = step_commands(
        &mut whole,
        vec![wire(
            vec![point(0, 0), point(10 * WORLD_PITCH, 0)],
            RoutingDomain::OpenWorld,
            EndpointTarget::MainCoreAnchor(core),
        )],
    );

    let mut split = Simulation::new(package(1_000)).expect("split simulation starts");
    let split_report = step_commands(
        &mut split,
        vec![
            wire(
                vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                RoutingDomain::OpenWorld,
                EndpointTarget::MainCoreAnchor(core),
            ),
            wire(
                vec![point(2 * WORLD_PITCH, 0), point(5 * WORLD_PITCH, 0)],
                RoutingDomain::OpenWorld,
                EndpointTarget::Free,
            ),
            wire(
                vec![point(5 * WORLD_PITCH, 0), point(7 * WORLD_PITCH, 0)],
                RoutingDomain::OpenWorld,
                EndpointTarget::Free,
            ),
            wire(
                vec![point(7 * WORLD_PITCH, 0), point(10 * WORLD_PITCH, 0)],
                RoutingDomain::OpenWorld,
                EndpointTarget::Free,
            ),
        ],
    );

    let whole_accounting = whole_report.network_accounting.expect("whole accounting");
    let split_accounting = split_report.network_accounting.expect("split accounting");
    assert_eq!(whole_accounting.used(), Capacity(10 * WORLD_PITCH as u64));
    assert_eq!(split_accounting.used(), whole_accounting.used());
    let rows = split
        .network_analyzer_snapshot()
        .expect("Analyzer arithmetic fits")
        .expect("capacity Analyzer exists");
    assert_eq!(rows.wires().len(), 4);
    assert!(
        rows.wires()
            .windows(2)
            .all(|pair| pair[0].wire() < pair[1].wire())
    );
}

#[test]
fn over_capacity_build_is_accepted_and_reported_without_structural_side_effects() {
    let mut simulation = Simulation::new(package(1)).expect("capacity simulation starts");
    let report = step_commands(
        &mut simulation,
        vec![wire(
            vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
            RoutingDomain::OpenWorld,
            EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
        )],
    );
    let accounting = report
        .network_accounting
        .expect("Phase 4 accounting exists");
    assert_eq!(accounting.used(), Capacity(2 * WORLD_PITCH as u64));
    assert_eq!(accounting.supported(), Capacity(WORLD_PITCH as u64));
    assert!(accounting.used().0 > accounting.supported().0);
    assert!(report.command_rejections.is_empty());
    assert_eq!(report.command_acceptances.len(), 1);
}

#[test]
fn same_tick_removal_is_reflected_in_phase4_and_derived_accounting_is_not_hashed() {
    let mut simulation = Simulation::new(package(1_000)).expect("capacity simulation starts");
    let placed = step_commands(
        &mut simulation,
        vec![wire(
            vec![point(0, 0), point(6 * WORLD_PITCH, 0)],
            RoutingDomain::OpenWorld,
            EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
        )],
    );
    assert_eq!(
        placed.network_accounting.expect("accounting").used(),
        Capacity(6 * WORLD_PITCH as u64)
    );

    let removed = step_commands(
        &mut simulation,
        vec![Command::RemoveEntity(RemoveEntityCommand {
            target: EntityId(2),
        })],
    );
    assert_eq!(
        removed.network_accounting.expect("accounting").used(),
        Capacity(0)
    );
    let before = simulation.state_hash();
    for _ in 0..3 {
        let analyzer = simulation
            .network_analyzer_snapshot()
            .expect("Analyzer arithmetic fits")
            .expect("capacity Analyzer exists");
        assert_eq!(analyzer.accounting().used(), Capacity(0));
        assert!(analyzer.wires().is_empty());
        assert_eq!(simulation.state_hash(), before);
    }
}

#[test]
fn equivalent_command_input_order_has_identical_accounting_and_state() {
    let commands = vec![
        CommandEnvelope {
            target_tick: aon_sim::Tick(0),
            ordinal: 0,
            command: wire(
                vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                RoutingDomain::OpenWorld,
                EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
            ),
        },
        CommandEnvelope {
            target_tick: aon_sim::Tick(0),
            ordinal: 1,
            command: wire(
                vec![
                    point(0, 4 * WORLD_PITCH),
                    point(3 * WORLD_PITCH, 4 * WORLD_PITCH),
                ],
                RoutingDomain::OpenWorld,
                EndpointTarget::Free,
            ),
        },
    ];
    let mut forward = Simulation::new(package(1_000)).expect("forward starts");
    let mut reversed = Simulation::new(package(1_000)).expect("reversed starts");
    let forward_report = forward.step(&commands).expect("forward Tick succeeds");
    let reversed_report = reversed
        .step(&commands.iter().rev().cloned().collect::<Vec<_>>())
        .expect("reversed Tick succeeds");
    assert_eq!(forward_report, reversed_report);
    assert_eq!(forward.state_hash(), reversed.state_hash());
    assert_eq!(
        forward
            .network_analyzer_snapshot()
            .expect("Analyzer arithmetic fits"),
        reversed
            .network_analyzer_snapshot()
            .expect("Analyzer arithmetic fits")
    );
}

#[test]
fn redundant_vertices_preserve_length_while_bends_sum_maximal_runs() {
    let mut simulation = Simulation::new(package(1_000)).expect("capacity simulation starts");
    let report = step_commands(
        &mut simulation,
        vec![
            wire(
                vec![
                    point(0, 0),
                    point(WORLD_PITCH, 0),
                    point(3 * WORLD_PITCH, 0),
                ],
                RoutingDomain::OpenWorld,
                EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
            ),
            wire(
                vec![
                    point(0, 4 * WORLD_PITCH),
                    point(2 * WORLD_PITCH, 4 * WORLD_PITCH),
                    point(2 * WORLD_PITCH, 6 * WORLD_PITCH),
                ],
                RoutingDomain::OpenWorld,
                EndpointTarget::Free,
            ),
        ],
    );
    assert_eq!(
        report.network_accounting.expect("accounting").used(),
        Capacity(7 * WORLD_PITCH as u64)
    );
    let analyzer = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer arithmetic fits")
        .expect("capacity Analyzer exists");
    assert_eq!(
        analyzer
            .wires()
            .iter()
            .map(|row| row.length())
            .collect::<Vec<_>>(),
        vec![
            Capacity(3 * WORLD_PITCH as u64),
            Capacity(4 * WORLD_PITCH as u64)
        ]
    );
}

#[test]
fn diagonal_rounding_is_per_wire_without_cross_wire_remainder_redistribution() {
    let mut direct = Simulation::new(package(1_000)).expect("direct starts");
    let direct_report = step_commands(
        &mut direct,
        vec![wire(
            vec![point(0, 0), point(32_768, 32_768)],
            RoutingDomain::OpenWorld,
            EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
        )],
    );
    assert_eq!(
        direct_report.network_accounting.expect("accounting").used(),
        Capacity(46_341)
    );

    let mut split = Simulation::new(package(1_000)).expect("split starts");
    let split_report = step_commands(
        &mut split,
        vec![
            wire(
                vec![point(0, 0), point(16_384, 16_384)],
                RoutingDomain::OpenWorld,
                EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
            ),
            wire(
                vec![point(16_384, 16_384), point(32_768, 32_768)],
                RoutingDomain::OpenWorld,
                EndpointTarget::Free,
            ),
        ],
    );
    assert_eq!(
        split_report.network_accounting.expect("accounting").used(),
        Capacity(46_342)
    );
    let rows = split
        .network_analyzer_snapshot()
        .expect("Analyzer arithmetic fits")
        .expect("capacity Analyzer exists");
    assert_eq!(
        rows.wires()
            .iter()
            .map(|row| row.length())
            .collect::<Vec<_>>(),
        vec![Capacity(23_171), Capacity(23_171)]
    );
}

#[test]
fn mobile_substrate_wire_body_contributes_once_to_network_usage() {
    let mut simulation = Simulation::new(package(1_000)).expect("capacity simulation starts");
    step_commands(
        &mut simulation,
        vec![wire(
            vec![point(0, 0), point(4 * WORLD_PITCH, 0)],
            RoutingDomain::OpenWorld,
            EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
        )],
    );
    let bounds = FixedAabb::new(
        point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    step_commands(
        &mut simulation,
        vec![Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: point(WORLD_PITCH, 0),
            routing_area: bounds,
            footprint: bounds,
        })],
    );
    let report = step_commands(
        &mut simulation,
        vec![wire(
            vec![point(-2 * CIRCUIT_PITCH, 0), point(2 * CIRCUIT_PITCH, 0)],
            RoutingDomain::MobileSubstrate(EntityId(3)),
            EndpointTarget::Free,
        )],
    );
    assert_eq!(
        report.network_accounting.expect("accounting").used(),
        Capacity(5 * WORLD_PITCH as u64)
    );
    let analyzer = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer arithmetic fits")
        .expect("capacity Analyzer exists");
    assert_eq!(
        analyzer
            .wires()
            .iter()
            .map(|row| (row.wire(), row.length()))
            .collect::<Vec<_>>(),
        vec![
            (WireId(EntityId(2)), Capacity(4 * WORLD_PITCH as u64)),
            (WireId(EntityId(4)), Capacity(WORLD_PITCH as u64)),
        ]
    );
}
