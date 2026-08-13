use aon_sim::{
    BalanceProfile, BindPortCommand, Capacity, Command, CommandEnvelope, ConstructionSiteId,
    ConstructionTarget, DemandId, DemandKind, DestructionKind, DriveStrength, DriverId,
    EndpointTarget, EnemyInitialState, Energy, EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2,
    GateId, GatePort, GatePortRef, GateType, HeatEnergy, InitialWorld, Integrity,
    InteractionHeatKind, LogicLevel, MobileId, NumericProfile, PhysicalScaleProfile,
    PlaceConstructionSiteCommand, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, PowerRatio, PowerSourceId,
    PowerSourceInitialState, ProfileBundle, RemoveEntityCommand, RenderSnapshot, RoutingDomain,
    RunEndCause, RunStatus, SetExternalDriverCommand, SignalProbeTarget, Simulation,
    SimulationContract, SimulationError, SimulationPackage, StageFeatureSet, Tick, WireEnd, WireId,
    WorldInputEvent,
};

const WU: i64 = FIXED_ONE;
const CIRCUIT_PITCH: i64 = 16_384;
const QUANTUM: i64 = 1_024;
const WIRE_RADIUS: i64 = 2_048;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * WU, y * WU)
}

fn s1m4_package(
    scenario_id: &'static str,
    source: Option<(FixedVec2, u64)>,
    enemies: Vec<EnemyInitialState>,
    mutate_balance: impl FnOnce(&mut BalanceProfile),
) -> SimulationPackage {
    let mut balance =
        BalanceProfile::construction_contact_damage_alpha(format!("balance-{scenario_id}"));
    mutate_balance(&mut balance);
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1(format!("numeric-{scenario_id}")),
        physical_scale: PhysicalScaleProfile::stage0_alpha(format!("physical-{scenario_id}")),
        balance,
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("S1-M4 profiles validate");
    SimulationPackage::new(
        scenario_id,
        InitialWorld::MainCorePowerEnemyV1 {
            main_core_position: wu(-100, -100),
            main_core_integrity: Integrity(100),
            main_core_heat_energy: HeatEnergy(0),
            power_sources: source
                .into_iter()
                .map(|(position, generation)| {
                    PowerSourceInitialState::new(position, Energy(generation))
                })
                .collect(),
            enemies,
        },
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
        },
        contract,
        profiles,
    )
}

fn enemy(position: FixedVec2) -> EnemyInitialState {
    EnemyInitialState::new(
        position,
        point(0, 0),
        Fixed(QUANTUM),
        Integrity(10),
        HeatEnergy(0),
    )
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn step(simulation: &mut Simulation, commands: Vec<Command>) -> aon_sim::StepReport {
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| {
            envelope(
                simulation,
                u64::try_from(ordinal).expect("test ordinal fits u64"),
                command,
            )
        })
        .collect::<Vec<_>>();
    let report = simulation.step(&envelopes).expect("focused Tick succeeds");
    assert!(
        report.command_rejections.is_empty(),
        "focused commands are accepted: {:?}",
        report.command_rejections
    );
    assert_eq!(report.command_acceptances.len(), envelopes.len());
    report
}

fn render(simulation: &Simulation) -> RenderSnapshot {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
}

fn large_bounds() -> FixedAabb {
    FixedAabb::new(wu(-32, -32), wu(32, 32))
}

fn mobile_bounds() -> FixedAabb {
    FixedAabb::new(wu(-1, -1), wu(1, 1))
}

fn fixed_substrate_command() -> Command {
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: large_bounds(),
        footprint: large_bounds(),
    })
}

fn construction_fixture_build_driver() -> DriverId {
    // The fixture's Track Wire allocates Sense A/B as Drivers 1/2. Balance-v5 then appends the
    // builder's ExternalMobileBuild Driver as Driver 3. The public RenderSnapshot exposes the
    // paired BUILD Sink but intentionally does not expose the external Driver role lookup.
    DriverId(EntityId(3))
}

fn construction_fixture(with_base_substrate: bool) -> (Simulation, Option<EntityId>, MobileId) {
    let mut simulation = Simulation::new(s1m4_package(
        "s1m4-construction-runtime",
        Some((wu(-2, 0), 500)),
        vec![enemy(wu(100, 100))],
        |_| {},
    ))
    .expect("Construction fixture starts");
    let source = simulation
        .power_sources()
        .next()
        .expect("Construction fixture has one Source")
        .id();

    let track = step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![wu(-2, 0), wu(6, 0)],
            endpoint_a: EndpointTarget::PowerSourceAnchor(source),
            endpoint_b: EndpointTarget::Free,
        })],
    );
    assert!(track.command_acceptances[0].created_entity.is_some());

    let base = with_base_substrate.then(|| {
        step(&mut simulation, vec![fixed_substrate_command()]).command_acceptances[0]
            .created_entity
            .expect("base Fixed Substrate has an ID")
    });
    let placed_mobile = step(
        &mut simulation,
        vec![Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: point(0, 0),
            routing_area: mobile_bounds(),
            footprint: mobile_bounds(),
        })],
    );
    let mobile = MobileId(
        placed_mobile.command_acceptances[0]
            .created_entity
            .expect("builder Mobile has an ID"),
    );
    let snapshot = render(&simulation);
    let record = snapshot
        .mobiles()
        .iter()
        .find(|record| record.id == mobile)
        .expect("builder is rendered");
    assert_eq!(record.build, Some(LogicLevel::Low));
    assert!(record.ports.build.is_some());
    (simulation, base, mobile)
}

#[derive(Clone, Copy, Debug)]
enum BuiltKind {
    Gate,
    Wire,
    Junction,
    FixedSubstrate,
}

fn construction_target(
    kind: BuiltKind,
    mobile_position: FixedVec2,
    base: Option<EntityId>,
) -> ConstructionTarget {
    match kind {
        BuiltKind::Gate => ConstructionTarget::Gate {
            gate_type: GateType::And,
            origin: point(mobile_position.x.0, mobile_position.y.0 + WU),
            routing_domain: RoutingDomain::FixedSubstrate(
                base.expect("Gate target has a base Substrate"),
            ),
        },
        BuiltKind::Wire => ConstructionTarget::Wire {
            routing_domain: RoutingDomain::FixedSubstrate(
                base.expect("Wire target has a base Substrate"),
            ),
            points: vec![
                point(mobile_position.x.0 - WU, mobile_position.y.0 + WU),
                point(mobile_position.x.0 + WU, mobile_position.y.0 + WU),
            ],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        },
        BuiltKind::Junction => ConstructionTarget::Junction {
            routing_domain: RoutingDomain::FixedSubstrate(
                base.expect("Junction target has a base Substrate"),
            ),
            position: point(mobile_position.x.0, mobile_position.y.0 + WU),
        },
        BuiltKind::FixedSubstrate => ConstructionTarget::FixedSubstrate {
            origin: point(mobile_position.x.0, mobile_position.y.0 + 2 * WU),
            routing_area: mobile_bounds(),
            footprint: mobile_bounds(),
        },
    }
}

fn active_ids(snapshot: &RenderSnapshot, kind: BuiltKind) -> Vec<EntityId> {
    match kind {
        BuiltKind::Gate => snapshot
            .gates()
            .iter()
            .map(|record| record.id.entity_id())
            .collect(),
        BuiltKind::Wire => snapshot
            .wires()
            .iter()
            .map(|record| record.id.entity_id())
            .collect(),
        BuiltKind::Junction => snapshot
            .junctions()
            .iter()
            .map(|record| record.id.entity_id())
            .collect(),
        BuiltKind::FixedSubstrate => snapshot
            .fixed_substrates()
            .iter()
            .map(|record| record.id)
            .collect(),
    }
}

#[test]
fn all_four_sites_complete_in_phase11_and_activate_with_fresh_ids_next_phase0() {
    for kind in [
        BuiltKind::Gate,
        BuiltKind::Wire,
        BuiltKind::Junction,
        BuiltKind::FixedSubstrate,
    ] {
        let needs_base = !matches!(kind, BuiltKind::FixedSubstrate);
        let (mut simulation, base, mobile) = construction_fixture(needs_base);
        let before = render(&simulation);
        let mobile_position = before
            .mobiles()
            .iter()
            .find(|record| record.id == mobile)
            .expect("builder is live")
            .world_position;
        let target = construction_target(kind, mobile_position, base);
        let active_before = active_ids(&before, kind);
        let capacity_before = simulation
            .network_analyzer_snapshot()
            .expect("Capacity analyzer succeeds")
            .expect("Capacity is active")
            .accounting()
            .used();

        // The only prior Driver identities are the Track Wire's two Sense Drivers. Balance-v5
        // then appends this Mobile's BUILD Driver as DriverId 3.
        let build_driver = construction_fixture_build_driver();
        let completed = step(
            &mut simulation,
            vec![
                Command::PlaceConstructionSite(PlaceConstructionSiteCommand { target }),
                Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: build_driver,
                    level: LogicLevel::High,
                    strength: DriveStrength(400),
                }),
            ],
        );
        let site = ConstructionSiteId(
            completed.command_acceptances[0]
                .created_entity
                .expect("placing a Site allocates its own ID"),
        );
        assert_eq!(completed.construction_work.len(), 1, "kind={kind:?}");
        let work = completed.construction_work[0];
        assert_eq!((work.site, work.builder), (site, mobile));
        assert_eq!(work.requested, work.nominal_power);
        assert_eq!(work.granted_work, work.requested);
        assert_eq!(work.applied_work, work.requested);
        assert_eq!(work.completed_work, work.requested);
        assert_eq!(
            completed
                .power
                .as_ref()
                .expect("Power report exists")
                .load(DemandId::new(mobile.entity_id(), DemandKind::Construction))
                .expect("BUILD contributes the tag-12 load")
                .ratio,
            PowerRatio::ONE
        );
        assert_eq!(
            completed
                .network_accounting
                .expect("Capacity report exists")
                .used(),
            capacity_before,
            "a completed Site is not active during its Phase-11 completion Tick"
        );
        let after_completion = render(&simulation);
        assert_eq!(active_ids(&after_completion, kind), active_before);
        let ready = simulation
            .construction_sites()
            .get(site)
            .expect("completed Site remains canonical through Phase 11");
        assert_eq!(ready.completed_work, ready.required_work);
        assert!(ready.activation_ready);

        let activated = step(&mut simulation, vec![]);
        assert!(activated.construction_work.is_empty());
        assert!(simulation.construction_sites().get(site).is_none());
        let after_activation = render(&simulation);
        let active_after = active_ids(&after_activation, kind);
        let created = active_after
            .iter()
            .copied()
            .find(|id| !active_before.contains(id))
            .expect("next Phase 0 activates exactly one target");
        assert!(
            created > site.entity_id(),
            "active target receives a fresh ID"
        );

        let capacity_after = activated
            .network_accounting
            .expect("Capacity remains observable")
            .used();
        if matches!(kind, BuiltKind::Wire) {
            assert_eq!(
                capacity_after,
                Capacity(capacity_before.0 + u64::try_from(2 * WU).unwrap()),
                "the complete Wire length enters Capacity on its activation Tick"
            );
        } else {
            assert_eq!(capacity_after, capacity_before);
        }
    }
}

#[test]
fn source_less_construction_reports_positive_nominal_zero_grant_and_no_progress() {
    let mut simulation = Simulation::new(s1m4_package(
        "s1m4-source-less-construction",
        None,
        vec![enemy(wu(100, 100))],
        |_| {},
    ))
    .expect("source-less Construction fixture starts");
    step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![wu(-2, 0), wu(6, 0)],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        })],
    );
    let base = Some(
        step(&mut simulation, vec![fixed_substrate_command()]).command_acceptances[0]
            .created_entity
            .expect("base Fixed Substrate has an ID"),
    );
    let mobile = MobileId(
        step(
            &mut simulation,
            vec![Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(0, 0),
                routing_area: mobile_bounds(),
                footprint: mobile_bounds(),
            })],
        )
        .command_acceptances[0]
            .created_entity
            .expect("source-less builder Mobile has an ID"),
    );
    let before = render(&simulation);
    let mobile_position = before
        .mobiles()
        .iter()
        .find(|record| record.id == mobile)
        .expect("builder is live")
        .world_position;
    let placed = step(
        &mut simulation,
        vec![Command::PlaceConstructionSite(
            PlaceConstructionSiteCommand {
                target: construction_target(BuiltKind::Junction, mobile_position, base),
            },
        )],
    );
    let site = ConstructionSiteId(
        placed.command_acceptances[0]
            .created_entity
            .expect("placing a Site allocates its own ID"),
    );
    assert!(placed.construction_work.is_empty());
    let unchanged = simulation
        .construction_sites()
        .get(site)
        .expect("the live Site exists before BUILD")
        .clone();
    assert_eq!(unchanged.completed_work, Energy(0));

    let report = step(
        &mut simulation,
        vec![Command::SetExternalDriver(SetExternalDriverCommand {
            driver: construction_fixture_build_driver(),
            level: LogicLevel::High,
            strength: DriveStrength(400),
        })],
    );
    let load = report
        .power
        .as_ref()
        .expect("Power report exists")
        .load(DemandId::new(mobile.entity_id(), DemandKind::Construction))
        .expect("BUILD contributes the tag-12 load on its source-less Track");
    assert!(load.nominal.0 > 0);
    assert_eq!(load.granted, Energy(0));
    assert_eq!(load.ratio, PowerRatio::ZERO);

    assert_eq!(report.construction_work.len(), 1);
    let work = report.construction_work[0];
    assert_eq!((work.site, work.builder), (site, mobile));
    assert!(work.requested.0 > 0);
    assert_eq!(work.nominal_power, load.nominal);
    assert_eq!(work.granted_work, Energy(0));
    assert_eq!(work.applied_work, Energy(0));
    assert_eq!(work.completed_work, Energy(0));
    assert_eq!(
        simulation
            .construction_sites()
            .get(site)
            .expect("zero grant leaves the Site live"),
        &unchanged
    );
}

fn wire_site_contact_case(outside_rounded_corner: bool) -> aon_sim::StepReport {
    let (mut simulation, base, _mobile) = construction_fixture(true);
    let mobile_position = render(&simulation).mobiles()[0].world_position;
    let corner = point(mobile_position.x.0 + WU, mobile_position.y.0 + WU);
    let p0 = if outside_rounded_corner {
        point(corner.x.0 + WIRE_RADIUS, corner.y.0 + WIRE_RADIUS)
    } else {
        point(corner.x.0 + WIRE_RADIUS, corner.y.0)
    };
    let p1 = point(p0.x.0 + CIRCUIT_PITCH, p0.y.0);
    if outside_rounded_corner {
        assert_eq!(
            (p0.x.0 - corner.x.0, p0.y.0 - corner.y.0),
            (WIRE_RADIUS, WIRE_RADIUS)
        );
    }
    step(
        &mut simulation,
        vec![
            Command::PlaceConstructionSite(PlaceConstructionSiteCommand {
                target: ConstructionTarget::Wire {
                    routing_domain: RoutingDomain::FixedSubstrate(base.expect("base exists")),
                    points: vec![p0, p1],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                },
            }),
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: construction_fixture_build_driver(),
                level: LogicLevel::High,
                strength: DriveStrength(400),
            }),
        ],
    )
}

#[test]
fn wire_site_uses_a_round_capsule_at_the_mobile_aabb_corner() {
    let tangent = wire_site_contact_case(false);
    assert_eq!(
        tangent.construction_work.len(),
        1,
        "closed corner tangency selects the Wire Site"
    );

    let outside = wire_site_contact_case(true);
    assert!(
        outside.construction_work.is_empty(),
        "a centerline offset by (radius,radius) is inside a square expansion but outside the rounded capsule"
    );
}

struct LiveWireFixture {
    simulation: Simulation,
    substrate: EntityId,
    gate: GateId,
    source: PowerSourceId,
}

fn gate_output(gate: GateId) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate,
        port: GatePort::Output,
    })
}

fn gate_input(gate: GateId) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate,
        port: GatePort::InputA,
    })
}

fn gate_power(gate: GateId) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate,
        port: GatePort::Power,
    })
}

fn powered_not_fixture(
    scenario_id: &'static str,
    generation: u64,
    enemies: Vec<EnemyInitialState>,
) -> LiveWireFixture {
    let source_position = wu(-1, 5);
    let mut simulation = Simulation::new(s1m4_package(
        scenario_id,
        Some((source_position, generation)),
        enemies,
        |_| {},
    ))
    .expect("powered NOT fixture starts");
    let source = simulation
        .power_sources()
        .next()
        .expect("powered fixture has one Source")
        .id();
    let substrate = step(&mut simulation, vec![fixed_substrate_command()]).command_acceptances[0]
        .created_entity
        .expect("substrate ID");
    let gate = GateId(
        step(
            &mut simulation,
            vec![Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: RoutingDomain::FixedSubstrate(substrate),
            })],
        )
        .command_acceptances[0]
            .created_entity
            .expect("gate ID"),
    );
    step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::FixedSubstrate(substrate),
            points: vec![
                source_position,
                wu(-1, -1),
                wu(0, -1),
                point(0, -CIRCUIT_PITCH),
            ],
            endpoint_a: EndpointTarget::PowerSourceAnchor(source),
            endpoint_b: gate_power(gate),
        })],
    );
    for _ in 0..16 {
        if simulation
            .gate_signal_state(gate)
            .is_some_and(|state| state.current_output == LogicLevel::High)
        {
            return LiveWireFixture {
                simulation,
                substrate,
                gate,
                source,
            };
        }
        step(&mut simulation, vec![]);
    }
    panic!("powered NOT did not settle HIGH")
}

fn c10_live_wire_command(substrate: EntityId, gate: GateId, source: PowerSourceId) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::FixedSubstrate(substrate),
        points: vec![
            wu(-1, 5),
            point(-WU, 9 * WU + 2 * CIRCUIT_PITCH),
            point(0, 9 * WU + 2 * CIRCUIT_PITCH),
            wu(0, 5),
            point(3 * WU + CIRCUIT_PITCH, 5 * WU),
            point(CIRCUIT_PITCH, 0),
        ],
        endpoint_a: EndpointTarget::PowerSourceAnchor(source),
        endpoint_b: gate_output(gate),
    })
}

fn live_load(report: &aon_sim::StepReport, wire: WireId) -> aon_sim::PowerLoadReport {
    report
        .power
        .as_ref()
        .expect("Power report exists")
        .load(DemandId::new(wire.entity_id(), DemandKind::LiveWire))
        .expect("HIGH Wire has a LiveWire load")
        .clone()
}

#[test]
fn c10_runtime_conserves_exact_grant_and_orders_equal_enemy_contacts() {
    let low_enemy = enemy(wu(1, 5));
    let high_enemy = enemy(wu(3, 5));
    let mut fixture = powered_not_fixture("s1m4-c10-runtime", 500, vec![low_enemy, high_enemy]);
    let enemy_ids = fixture
        .simulation
        .enemies()
        .iter()
        .map(|state| state.id())
        .collect::<Vec<_>>();
    assert_eq!(enemy_ids.len(), 2);
    let report = step(
        &mut fixture.simulation,
        vec![c10_live_wire_command(
            fixture.substrate,
            fixture.gate,
            fixture.source,
        )],
    );
    let wire = WireId(
        report.command_acceptances[0]
            .created_entity
            .expect("C-10 Wire has an ID"),
    );
    let live = live_load(&report, wire);
    assert_eq!((live.nominal, live.granted), (Energy(20), Energy(20)));
    assert_eq!(live.ratio, PowerRatio::ONE);
    assert_eq!(
        report
            .contacts
            .iter()
            .map(|row| (row.wire, row.target, row.weight, row.absorbed))
            .collect::<Vec<_>>(),
        vec![
            (wire, enemy_ids[0], 1, Energy(5)),
            (wire, enemy_ids[1], 1, Energy(5)),
        ]
    );
    let heat = report
        .interaction_heat
        .iter()
        .find(|row| {
            row.owner == wire.entity_id() && row.kind == InteractionHeatKind::LiveWireRemainder
        })
        .expect("the world-leak remainder becomes Wire Heat");
    assert_eq!(heat.energy, HeatEnergy(10));
    assert_eq!(
        report
            .contacts
            .iter()
            .map(|row| row.absorbed.0)
            .sum::<u64>()
            + heat.energy.0,
        live.granted.0
    );
    assert_eq!(
        report
            .damage
            .iter()
            .filter(|row| enemy_ids
                .iter()
                .any(|enemy| enemy.entity_id() == row.target))
            .map(|row| (row.target, row.electrical_exposure, row.integrity_after))
            .collect::<Vec<_>>(),
        vec![
            (enemy_ids[0].entity_id(), Energy(5), Integrity(5)),
            (enemy_ids[1].entity_id(), Energy(5), Integrity(5)),
        ]
    );
}

#[test]
fn live_wire_report_exposes_partial_and_source_less_actual_grants() {
    let mut partial = powered_not_fixture("s1m4-live-partial", 60, vec![enemy(wu(100, 100))]);
    let partial_report = step(
        &mut partial.simulation,
        vec![c10_live_wire_command(
            partial.substrate,
            partial.gate,
            partial.source,
        )],
    );
    let partial_wire = WireId(
        partial_report.command_acceptances[0]
            .created_entity
            .expect("partial Wire ID"),
    );
    let partial_load = live_load(&partial_report, partial_wire);
    assert_eq!(partial_load.nominal, Energy(20));
    assert!(
        Energy(0) < partial_load.granted && partial_load.granted < partial_load.nominal,
        "the complete region solve supplies a genuinely partial actual grant: {partial_load:?}"
    );
    assert!(partial_report.contacts.is_empty());

    let mut source_less =
        powered_not_fixture("s1m4-live-source-less", 500, vec![enemy(wu(100, 100))]);
    let armed = step(
        &mut source_less.simulation,
        vec![c10_live_wire_command(
            source_less.substrate,
            source_less.gate,
            source_less.source,
        )],
    );
    let wire = WireId(
        armed.command_acceptances[0]
            .created_entity
            .expect("source-less candidate Wire ID"),
    );
    assert_eq!(live_load(&armed, wire).granted, Energy(20));
    let disconnected = step(
        &mut source_less.simulation,
        vec![Command::BindPort(BindPortCommand {
            wire,
            end: WireEnd::A,
            target: EndpointTarget::Free,
        })],
    );
    let source_less_load = live_load(&disconnected, wire);
    assert_eq!(source_less_load.nominal, Energy(20));
    assert_eq!(source_less_load.granted, Energy(0));
    assert_eq!(source_less_load.ratio, PowerRatio::ZERO);
    assert!(disconnected.contacts.is_empty());
    assert!(disconnected.interaction_heat.iter().all(|row| {
        !(row.owner == wire.entity_id() && row.kind == InteractionHeatKind::LiveWireRemainder)
    }));
}

#[test]
fn c09_pending_wire_is_usable_then_all_surfaces_leave_together_and_arrival_stales() {
    let mut fixture = powered_not_fixture("s1m4-c09-runtime", 100, vec![enemy(wu(10, 0))]);
    let substrate = fixture.substrate;
    let source_gate = fixture.gate;
    let downstream = GateId(
        step(
            &mut fixture.simulation,
            vec![Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(20 * WU + 2 * CIRCUIT_PITCH, 0),
                routing_domain: RoutingDomain::FixedSubstrate(substrate),
            })],
        )
        .command_acceptances[0]
            .created_entity
            .expect("downstream Gate ID"),
    );
    let source_ports = fixture
        .simulation
        .gate_signal_ports(source_gate)
        .expect("source ports");
    let downstream_ports = fixture
        .simulation
        .gate_signal_ports(downstream)
        .expect("downstream ports");
    let baseline_capacity = fixture
        .simulation
        .network_analyzer_snapshot()
        .expect("Capacity analyzer succeeds")
        .expect("Capacity is active")
        .accounting()
        .used();
    let attacked = step(
        &mut fixture.simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::FixedSubstrate(substrate),
            points: vec![point(CIRCUIT_PITCH, 0), point(20 * WU + CIRCUIT_PITCH, 0)],
            endpoint_a: gate_output(source_gate),
            endpoint_b: gate_input(downstream),
        })],
    );
    let victim = WireId(
        attacked.command_acceptances[0]
            .created_entity
            .expect("victim Wire ID"),
    );
    assert_eq!(attacked.signal_counters.topology_sync_arrivals_staged, 1);
    assert!(attacked.signal_arrivals.is_empty());
    assert_eq!(
        fixture
            .simulation
            .sink_driver_sample(downstream_ports.input_a.sink, source_ports.output,),
        None,
        "the staged Arrival is still in flight"
    );
    let victim_damage = attacked
        .damage
        .iter()
        .find(|row| row.target == victim.entity_id())
        .expect("Enemy attack damages victim");
    assert_eq!(victim_damage.electrical_exposure, Energy(10));
    assert_eq!(
        (
            victim_damage.integrity_before,
            victim_damage.integrity_after
        ),
        (Integrity(10), Integrity(0))
    );
    assert!(victim_damage.pending_destruction);
    let live = live_load(&attacked, victim);
    assert_eq!(live.nominal, Energy(20));
    assert_eq!(live.granted, Energy(0));
    assert_eq!(live.ratio, PowerRatio::ZERO);
    assert!(fixture.simulation.wire_signal_state(victim).is_some());
    assert!(fixture.simulation.wire_sense_state(victim).is_some());
    assert!(
        fixture
            .simulation
            .construction_contact_damage_analyzer_snapshot()
            .expect("pending analyzer succeeds")
            .expect("S1-M4 analyzer is enabled")
            .armed_wires
            .iter()
            .any(|row| row.wire == victim)
    );
    assert_eq!(
        attacked
            .network_accounting
            .expect("Capacity is reported")
            .used(),
        Capacity(baseline_capacity.0 + u64::try_from(20 * WU).unwrap()),
        "the pending-destroyed Wire completes its current Tick"
    );

    let pending_revision = fixture.simulation.topology_revision();
    let removed = step(&mut fixture.simulation, vec![]);
    assert_eq!(
        removed.destructions,
        vec![aon_sim::DestructionReport {
            target: victim.entity_id(),
            kind: DestructionKind::Damage,
        }]
    );
    assert!(removed.topology_changed);
    assert!(fixture.simulation.topology_revision() > pending_revision);
    assert_eq!(fixture.simulation.wire_signal_state(victim), None);
    assert_eq!(fixture.simulation.wire_sense_state(victim), None);
    assert_eq!(
        fixture
            .simulation
            .signal_probe(SignalProbeTarget::Wire(victim)),
        None
    );
    assert_eq!(
        removed
            .network_accounting
            .expect("Capacity is reported after removal")
            .used(),
        baseline_capacity
    );
    assert!(
        removed
            .power
            .as_ref()
            .expect("Power report exists")
            .loads
            .iter()
            .all(|load| load.demand.owner() != victim.entity_id())
    );
    assert!(fixture.simulation.construction_sites().is_empty());

    let track_probe_command = envelope(
        &fixture.simulation,
        0,
        Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: wu(10, 0),
            routing_area: mobile_bounds(),
            footprint: mobile_bounds(),
        }),
    );
    let track_probe = fixture
        .simulation
        .step(&[track_probe_command])
        .expect("off-Track placement is an ordinary rejection");
    assert!(track_probe.command_acceptances.is_empty());
    assert_eq!(
        track_probe.command_rejections[0].reason,
        aon_sim::CommandRejectionReason::UnsupportedPlacement,
        "the destroyed Wire no longer exists on the Track surface"
    );

    let mut stale = None;
    for _ in 0..32 {
        let report = step(&mut fixture.simulation, vec![]);
        if report.signal_counters.invalid_path_arrivals > 0 {
            stale = Some(report);
            break;
        }
    }
    let stale = stale.expect("the retained in-flight Arrival eventually becomes due");
    assert_eq!(stale.signal_counters.invalid_path_arrivals, 1);
    assert_eq!(stale.signal_counters.signal_arrivals_applied, 0);
    assert!(stale.signal_arrivals.iter().any(|arrival| {
        arrival.source_driver == source_ports.output
            && arrival.sink == downstream_ports.input_a.sink
    }));
}

#[test]
fn fatal_core_tick_commits_terminal_hash_and_later_steps_are_strictly_read_only() {
    let mut simulation = Simulation::new(s1m4_package(
        "s1m4-core-terminal-runtime",
        None,
        vec![enemy(wu(-100, -100))],
        |balance| {
            balance
                .contact_damage_probe
                .as_mut()
                .expect("v5 damage probe")
                .enemy_attack_energy_per_tick = 100;
        },
    ))
    .expect("terminal fixture starts");
    let core = simulation
        .main_core_state()
        .expect("terminal fixture has a Core")
        .id()
        .entity_id();
    let initial_hash = simulation.state_hash();
    let terminal = simulation.step(&[]).expect("fatal Tick completes");
    assert_eq!(terminal.completed_tick, Tick(0));
    assert_eq!(terminal.next_tick, Tick(1));
    assert_eq!(
        terminal.run_status,
        RunStatus::Ended {
            completed_tick: Tick(0),
            cause: RunEndCause::MainCoreDestroyed,
        }
    );
    assert_eq!(terminal.state_hash, simulation.state_hash());
    assert_ne!(terminal.state_hash, initial_hash);
    assert_eq!(
        terminal
            .damage
            .iter()
            .find(|row| row.target == core)
            .map(|row| (row.electrical_exposure, row.integrity_after)),
        Some((Energy(100), Integrity(0)))
    );
    assert_eq!(
        simulation
            .main_core_state()
            .expect("terminal Core remains canonical")
            .integrity(),
        Integrity(0)
    );

    let next_tick = simulation.next_tick();
    let terminal_hash = simulation.state_hash();
    let analyzer = simulation
        .construction_contact_damage_analyzer_snapshot()
        .expect("terminal analyzer reads")
        .expect("S1-M4 analyzer is enabled");
    let invalid_command = CommandEnvelope {
        target_tick: Tick(u64::MAX),
        ordinal: 0,
        command: Command::RemoveEntity(RemoveEntityCommand {
            target: EntityId(0),
        }),
    };
    let invalid_input = WorldInputEvent::HostileFrame {
        target_tick: Tick(u64::MAX),
        hostiles: vec![],
    };
    assert_eq!(
        simulation.step_with_world_inputs(&[invalid_command], &[invalid_input]),
        Err(SimulationError::RunEnded),
        "RunEnded precedes command and World-input validation"
    );
    assert_eq!(simulation.step(&[]), Err(SimulationError::RunEnded));
    assert_eq!(simulation.next_tick(), next_tick);
    assert_eq!(simulation.state_hash(), terminal_hash);
    assert_eq!(simulation.run_status(), terminal.run_status);
    assert_eq!(
        simulation
            .construction_contact_damage_analyzer_snapshot()
            .expect("terminal analyzer rereads")
            .expect("S1-M4 analyzer stays enabled"),
        analyzer
    );
}
