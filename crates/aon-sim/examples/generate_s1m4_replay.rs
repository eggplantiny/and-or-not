use aon_sim::{
    ArtifactBytes, Capacity, Command, CommandEnvelope, CommandRejection, CommandRejectionReason,
    ConnectionGeneration, ConstructionTarget, DemandId, DemandKind, DestructionKind, DriveStrength,
    DriverId, EndpointTarget, Energy, EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateId,
    GatePort, GatePortRef, GateType, HashCheckpoint, HeatEnergy, InteractionHeatKind, LogicLevel,
    PlaceConstructionSiteCommand, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, PowerRatio, PowerRegionId, PowerSourceId,
    RemoveEntityCommand, Replay, ReplayArtifact, Revision, RoutingDomain, RunEndCause, RunStatus,
    SetExternalDriverCommand, Simulation, SinkId, Tick, WireId, decode_balance_profile,
    decode_numeric_profile, decode_package, decode_physical_scale_profile,
    decode_scenario_manifest, encode_replay_artifact, required_construction_work,
};
use serde_json::json;
use std::path::{Path, PathBuf};

const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] =
    include_bytes!("../../../profiles/balance/s1-m4-construction-contact-damage-alpha.json");

const SCENARIO_ID: &str = "s1-m4-construction-contact-damage-v1";
const SCENARIO_PATH: &str = "fixtures/scenarios/s1-m4-construction-contact-damage-v1.json";
const REPLAY_SCENARIO_PATH: &str = "../../scenarios/s1-m4-construction-contact-damage-v1.json";
const REPLAY_STEM: &str = "fixtures/replays/s1-m4";

const WU: i64 = FIXED_ONE;
const CIRCUIT_PITCH: i64 = 16_384;
const CORE: EntityId = EntityId(1);
const SOURCE_CONSTRUCTION: PowerSourceId = PowerSourceId(EntityId(2));
const SOURCE_CONTACT: PowerSourceId = PowerSourceId(EntityId(3));
const ENEMY_TERMINAL: EntityId = EntityId(4);
const ENEMY_C10_LOW: EntityId = EntityId(5);
const ENEMY_C10_HIGH: EntityId = EntityId(6);
const ENEMY_C09: EntityId = EntityId(7);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * WU, y * WU)
}

fn local_bounds(radius: i64) -> FixedAabb {
    FixedAabb::new(wu(-radius, -radius), wu(radius, radius))
}

fn scenario_bytes() -> Vec<u8> {
    let numeric = decode_numeric_profile(NUMERIC).expect("the retained Numeric Profile decodes");
    let physical = decode_physical_scale_profile(PHYSICAL)
        .expect("the retained Physical Scale Profile decodes");
    let balance = decode_balance_profile(BALANCE).expect("the S1-M4 Balance Profile decodes");
    let enemies = [
        // Sorting by the complete key allocates these as IDs 4..7 in this order.
        (wu(-100, 52), point(0, -WU), 5 * WU),
        (wu(1, 5), point(0, 0), 1024),
        (wu(3, 5), point(0, 0), 1024),
        (wu(10, 46), point(0, -WU), 1024),
    ];
    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 4,
        "scenarioId": SCENARIO_ID,
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-enemy-v1",
            "mainCore": {
                "position": { "x": wu(-100, 0).x.0, "y": wu(-100, 0).y.0 },
                "integrity": 100,
                "heatEnergy": 0
            },
            "powerSources": [
                {
                    "position": { "x": wu(-20, -20).x.0, "y": wu(-20, -20).y.0 },
                    "generationPerTick": 12
                },
                {
                    "position": { "x": wu(-1, 5).x.0, "y": wu(-1, 5).y.0 },
                    "generationPerTick": 500
                }
            ],
            "enemies": enemies.into_iter().map(|(position, velocity, radius)| json!({
                "position": { "x": position.x.0, "y": position.y.0 },
                "velocityPerTick": { "x": velocity.x.0, "y": velocity.y.0 },
                "radius": radius,
                "integrity": 10,
                "heatEnergy": 0
            })).collect::<Vec<_>>()
        },
        "requiredFeatures": {
            "signal": true, "mobility": true, "capacity": true, "sensing": true,
            "power": true, "relay": false, "payload": false, "radiation": false,
            "construction": true, "contact": true, "damage": true
        },
        "profiles": {
            "numeric": {
                "path": "../../profiles/numeric/v1.json",
                "profileId": numeric.profile_id,
                "profileHash": numeric.canonical_hash().expect("Numeric hashes").to_string()
            },
            "physicalScale": {
                "path": "../../profiles/physical-scale/stage0-alpha.json",
                "profileId": physical.profile_id,
                "profileHash": physical.canonical_hash().expect("Physical hashes").to_string()
            },
            "balance": {
                "path": "../../profiles/balance/s1-m4-construction-contact-damage-alpha.json",
                "profileId": balance.profile_id,
                "profileHash": balance.canonical_hash().expect("Balance hashes").to_string()
            }
        }
    }))
    .expect("the S1-M4 Scenario JSON encodes");
    bytes.push(b'\n');
    bytes
}

fn package(scenario: &[u8]) -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the generated S1-M4 package decodes")
}

fn gate_port(gate: GateId, port: GatePort) -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef { gate, port })
}

fn command(target_tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(target_tick),
        ordinal,
        command,
    }
}

fn standard_substrate() -> PlaceFixedSubstrateCommand {
    PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: local_bounds(32),
        footprint: local_bounds(32),
    }
}

fn standard_power_commands() -> Vec<CommandEnvelope> {
    let substrate = EntityId(8);
    let gate = GateId(EntityId(9));
    vec![
        command(0, 0, Command::PlaceFixedSubstrate(standard_substrate())),
        command(
            1,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: RoutingDomain::FixedSubstrate(substrate),
            }),
        ),
        command(
            2,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::FixedSubstrate(substrate),
                points: vec![wu(-1, 5), wu(-1, -1), wu(0, -1), point(0, -CIRCUIT_PITCH)],
                endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE_CONTACT),
                endpoint_b: gate_port(gate, GatePort::Power),
            }),
        ),
    ]
}

fn c10_commands() -> Vec<CommandEnvelope> {
    let mut commands = standard_power_commands();
    commands.push(command(
        8,
        0,
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
            points: vec![
                wu(-1, 5),
                point(-WU, 9 * WU + 2 * CIRCUIT_PITCH),
                point(0, 9 * WU + 2 * CIRCUIT_PITCH),
                wu(0, 5),
                point(3 * WU + CIRCUIT_PITCH, 5 * WU),
                point(CIRCUIT_PITCH, 0),
            ],
            endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE_CONTACT),
            endpoint_b: gate_port(GateId(EntityId(9)), GatePort::Output),
        }),
    ));
    commands
}

fn c09_commands() -> Vec<CommandEnvelope> {
    vec![
        command(0, 0, Command::PlaceFixedSubstrate(standard_substrate())),
        command(
            19,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(0, 0),
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
            }),
        ),
        command(
            20,
            0,
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: point(20 * WU + 2 * CIRCUIT_PITCH, 0),
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
            }),
        ),
        command(
            21,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
                points: vec![point(CIRCUIT_PITCH, 0), point(20 * WU + CIRCUIT_PITCH, 0)],
                endpoint_a: gate_port(GateId(EntityId(9)), GatePort::Output),
                endpoint_b: gate_port(GateId(EntityId(10)), GatePort::InputA),
            }),
        ),
        command(
            38,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
                points: vec![wu(-1, 5), wu(-1, -1), wu(0, -1), point(0, -CIRCUIT_PITCH)],
                endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE_CONTACT),
                endpoint_b: gate_port(GateId(EntityId(9)), GatePort::Power),
            }),
        ),
        command(
            46,
            0,
            Command::PlaceJunction(aon_sim::PlaceJunctionCommand {
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
                position: wu(-10, -10),
            }),
        ),
        command(
            46,
            1,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: wu(10, 0),
                routing_area: local_bounds(4),
                footprint: local_bounds(4),
            }),
        ),
    ]
}

fn construction_commands() -> Vec<CommandEnvelope> {
    let substrate = EntityId(8);
    let mut commands = vec![command(
        0,
        0,
        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: wu(-24, -20),
            routing_area: FixedAabb::new(wu(-8, -8), wu(8, 8)),
            footprint: FixedAabb::new(wu(-8, -8), wu(8, 8)),
        }),
    )];
    let targets = [
        ConstructionTarget::Gate {
            gate_type: GateType::And,
            origin: point(-20 * WU + 2 * CIRCUIT_PITCH, -18 * WU),
            routing_domain: RoutingDomain::FixedSubstrate(substrate),
        },
        ConstructionTarget::Junction {
            routing_domain: RoutingDomain::FixedSubstrate(substrate),
            position: wu(-18, -20),
        },
        ConstructionTarget::Wire {
            routing_domain: RoutingDomain::FixedSubstrate(substrate),
            points: vec![wu(-20, -22), wu(-19, -22)],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        },
        ConstructionTarget::FixedSubstrate {
            origin: wu(-16, -20),
            routing_area: FixedAabb::new(wu(0, 0), wu(1, 1)),
            footprint: FixedAabb::new(wu(0, 0), wu(1, 1)),
        },
    ];
    let cycles = [
        (1, EntityId(9), EntityId(10), DriverId(EntityId(3))),
        (6, EntityId(13), EntityId(14), DriverId(EntityId(9))),
        (11, EntityId(17), EntityId(18), DriverId(EntityId(12))),
        (16, EntityId(21), EntityId(22), DriverId(EntityId(17))),
    ];
    for ((start, track, mobile, build), target) in cycles.into_iter().zip(targets) {
        commands.extend([
            command(
                start,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: vec![wu(-20, -20), wu(-19, -20)],
                    endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE_CONSTRUCTION),
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
            command(
                start + 1,
                0,
                Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                    origin: point(-20 * WU + CIRCUIT_PITCH, -20 * WU),
                    routing_area: local_bounds(4),
                    footprint: local_bounds(4),
                }),
            ),
            command(
                start + 2,
                0,
                Command::PlaceConstructionSite(PlaceConstructionSiteCommand { target }),
            ),
            command(
                start + 2,
                1,
                Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: build,
                    level: LogicLevel::High,
                    strength: DriveStrength(400),
                }),
            ),
            command(
                start + 4,
                0,
                Command::RemoveEntity(RemoveEntityCommand { target: mobile }),
            ),
            command(
                start + 4,
                1,
                Command::RemoveEntity(RemoveEntityCommand { target: track }),
            ),
        ]);
    }
    commands
}

fn construction_partial_commands() -> Vec<CommandEnvelope> {
    vec![
        command(
            0,
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: FixedAabb::new(wu(-32, -32), wu(32, 32)),
                footprint: FixedAabb::new(wu(-32, -32), wu(32, 32)),
            }),
        ),
        command(
            1,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(-20, -20), wu(-19, -20)],
                endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE_CONSTRUCTION),
                endpoint_b: EndpointTarget::Free,
            }),
        ),
        command(
            2,
            0,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(-20 * WU + CIRCUIT_PITCH, -20 * WU),
                routing_area: local_bounds(4),
                footprint: local_bounds(4),
            }),
        ),
        command(
            2,
            1,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(-19 * WU - CIRCUIT_PITCH, -20 * WU),
                routing_area: local_bounds(4),
                footprint: local_bounds(4),
            }),
        ),
        command(
            3,
            0,
            Command::PlaceConstructionSite(PlaceConstructionSiteCommand {
                target: ConstructionTarget::Gate {
                    gate_type: GateType::And,
                    origin: point(-20 * WU + 2 * CIRCUIT_PITCH, -20 * WU + 2 * CIRCUIT_PITCH),
                    routing_domain: RoutingDomain::FixedSubstrate(EntityId(8)),
                },
            }),
        ),
        command(
            3,
            1,
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: DriverId(EntityId(3)),
                level: LogicLevel::High,
                strength: DriveStrength(400),
            }),
        ),
        command(
            3,
            2,
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: DriverId(EntityId(4)),
                level: LogicLevel::High,
                strength: DriveStrength(400),
            }),
        ),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TraceKind {
    ConstructionPartial,
    ConstructionFourTargets,
    C10,
    C09,
    Terminal,
}

impl TraceKind {
    const fn file_name(self) -> &'static str {
        match self {
            Self::ConstructionPartial => "construction-partial-multibuilder-v1.json",
            Self::ConstructionFourTargets => "construction-four-targets-v1.json",
            Self::C10 => "c10-contact-v1.json",
            Self::C09 => "c09-wire-break-v1.json",
            Self::Terminal => "terminal-v1.json",
        }
    }

    const fn final_next_tick(self) -> Tick {
        match self {
            Self::ConstructionPartial => Tick(5),
            Self::ConstructionFourTargets => Tick(21),
            Self::C10 => Tick(9),
            Self::C09 => Tick(52),
            Self::Terminal => Tick(56),
        }
    }

    fn commands(self) -> Vec<CommandEnvelope> {
        match self {
            Self::ConstructionPartial => construction_partial_commands(),
            Self::ConstructionFourTargets => construction_commands(),
            Self::C10 => c10_commands(),
            Self::C09 => c09_commands(),
            Self::Terminal => Vec::new(),
        }
    }
}

#[derive(Default)]
struct TraceFacts {
    saw_partial_construction: bool,
    saw_multi_builder: bool,
    saw_c09_stale: bool,
    completed_four_targets: usize,
    activated_four_targets: usize,
    c09_pending_revision: Option<Revision>,
    c09_pending_generation: Option<ConnectionGeneration>,
    c09_pending_capacity: Option<Capacity>,
    c09_pending_region: Option<PowerRegionId>,
    c09_pending_region_count: Option<usize>,
    c09_source_driver: Option<DriverId>,
    c09_sink: Option<SinkId>,
    terminal_attack_ticks: Vec<Tick>,
}

fn assert_tick(
    kind: TraceKind,
    report: &aon_sim::StepReport,
    simulation: &Simulation,
    facts: &mut TraceFacts,
) {
    if kind == TraceKind::C09 && report.completed_tick == Tick(46) {
        assert_eq!(
            report.command_rejections,
            [CommandRejection {
                target_tick: Tick(46),
                ordinal: 1,
                reason: CommandRejectionReason::UnsupportedPlacement,
            }]
        );
    } else {
        assert!(
            report.command_rejections.is_empty(),
            "Tick {:?} rejected {:?}",
            report.completed_tick,
            report.command_rejections
        );
    }
    match kind {
        TraceKind::ConstructionPartial | TraceKind::ConstructionFourTargets => {
            for work in &report.construction_work {
                let load = report
                    .power
                    .as_ref()
                    .expect("Construction reports Power")
                    .load(DemandId::new(
                        work.builder.entity_id(),
                        DemandKind::Construction,
                    ))
                    .expect("each builder owns one Construction load");
                if PowerRatio::ZERO < load.ratio && load.ratio < PowerRatio::ONE {
                    facts.saw_partial_construction = true;
                }
            }
            if report.construction_work.len() == 2 {
                facts.saw_multi_builder = true;
                assert!(report.construction_work[0].builder < report.construction_work[1].builder);
                assert_eq!(
                    report.construction_work[0].site,
                    report.construction_work[1].site
                );
            }
            if matches!(kind, TraceKind::ConstructionPartial) && report.completed_tick == Tick(3) {
                assert_eq!(report.construction_work.len(), 2);
                assert_eq!(report.construction_work[0].site.entity_id(), EntityId(12));
                assert_eq!(report.construction_work[1].site.entity_id(), EntityId(12));
                assert_eq!(
                    report.construction_work[0].builder.entity_id(),
                    EntityId(10)
                );
                assert_eq!(
                    report.construction_work[1].builder.entity_id(),
                    EntityId(11)
                );
                assert_eq!(
                    (
                        report.construction_work[0].nominal_power,
                        report.construction_work[0].granted_work,
                        report.construction_work[0].applied_work,
                        report.construction_work[0].completed_work,
                    ),
                    (Energy(8), Energy(4), Energy(4), Energy(4))
                );
                assert_eq!(
                    (
                        report.construction_work[1].nominal_power,
                        report.construction_work[1].granted_work,
                        report.construction_work[1].applied_work,
                        report.construction_work[1].completed_work,
                    ),
                    (Energy(8), Energy(4), Energy(4), Energy(8))
                );
                let site = simulation
                    .construction_sites()
                    .get(aon_sim::ConstructionSiteId(EntityId(12)))
                    .expect("partial Site remains through its completion Tick");
                assert!(site.activation_ready);
                let mut snapshot = aon_sim::RenderSnapshot::default();
                simulation.write_render_snapshot(&mut snapshot);
                assert!(
                    snapshot
                        .gates()
                        .iter()
                        .all(|row| row.id.entity_id() != EntityId(13))
                );
            }
            if matches!(kind, TraceKind::ConstructionPartial) && report.completed_tick == Tick(4) {
                assert!(simulation.construction_sites().is_empty());
                let mut snapshot = aon_sim::RenderSnapshot::default();
                simulation.write_render_snapshot(&mut snapshot);
                assert!(
                    snapshot
                        .gates()
                        .iter()
                        .any(|row| row.id.entity_id() == EntityId(13))
                );
            }
            if matches!(kind, TraceKind::ConstructionFourTargets) {
                if report.completed_tick == Tick(13) {
                    assert_eq!(
                        report
                            .network_accounting
                            .expect("Wire completion reports Capacity")
                            .used(),
                        Capacity(u64::try_from(WU).expect("WU is positive"))
                    );
                }
                if report.completed_tick == Tick(14) {
                    assert_eq!(
                        report
                            .network_accounting
                            .expect("Wire activation reports Capacity")
                            .used(),
                        Capacity(2 * u64::try_from(WU).expect("WU is positive"))
                    );
                }
                for (completion_tick, site, required) in [
                    (Tick(3), EntityId(11), Energy(8)),
                    (Tick(8), EntityId(15), Energy(4)),
                    (Tick(13), EntityId(19), Energy(3)),
                    (Tick(18), EntityId(23), Energy(1)),
                ] {
                    if report.completed_tick == completion_tick {
                        assert_eq!(report.construction_work.len(), 1);
                        let work = report.construction_work[0];
                        assert_eq!(work.site.entity_id(), site);
                        assert_eq!(work.requested, required);
                        assert_eq!(work.granted_work, required);
                        assert_eq!(work.applied_work, required);
                        assert_eq!(work.completed_work, required);
                        assert!(
                            simulation
                                .construction_sites()
                                .get(work.site)
                                .is_some_and(|row| row.activation_ready)
                        );
                        facts.completed_four_targets += 1;
                    }
                }
                for (activation_tick, site, target) in [
                    (Tick(4), EntityId(11), EntityId(12)),
                    (Tick(9), EntityId(15), EntityId(16)),
                    (Tick(14), EntityId(19), EntityId(20)),
                    (Tick(19), EntityId(23), EntityId(24)),
                ] {
                    if report.completed_tick == activation_tick {
                        assert!(
                            simulation
                                .construction_sites()
                                .iter()
                                .all(|row| row.id.entity_id() != site)
                        );
                        assert!(target > site);
                        let mut snapshot = aon_sim::RenderSnapshot::default();
                        simulation.write_render_snapshot(&mut snapshot);
                        match target {
                            EntityId(12) => assert!(
                                snapshot
                                    .gates()
                                    .iter()
                                    .any(|row| row.id.entity_id() == target)
                            ),
                            EntityId(16) => assert!(
                                snapshot
                                    .junctions()
                                    .iter()
                                    .any(|row| row.id.entity_id() == target)
                            ),
                            EntityId(20) => assert!(
                                snapshot
                                    .wires()
                                    .iter()
                                    .any(|row| row.id.entity_id() == target)
                            ),
                            EntityId(24) => assert!(
                                snapshot
                                    .fixed_substrates()
                                    .iter()
                                    .any(|row| row.id == target)
                            ),
                            _ => unreachable!("the four activation IDs are frozen"),
                        }
                        facts.activated_four_targets += 1;
                    }
                }
            }
        }
        TraceKind::C10 if report.completed_tick == Tick(8) => {
            let wire = WireId(EntityId(11));
            let live = report
                .power
                .as_ref()
                .expect("C-10 reports Power")
                .load(DemandId::new(wire.entity_id(), DemandKind::LiveWire))
                .expect("C-10 reports its Live load");
            assert_eq!(
                (live.nominal, live.granted, live.ratio),
                (Energy(20), Energy(20), PowerRatio::ONE)
            );
            assert_eq!(
                report
                    .contacts
                    .iter()
                    .map(|row| (row.target.entity_id(), row.absorbed))
                    .collect::<Vec<_>>(),
                [(ENEMY_C10_LOW, Energy(5)), (ENEMY_C10_HIGH, Energy(5))]
            );
            assert_eq!(
                report
                    .interaction_heat
                    .iter()
                    .find(|row| {
                        row.owner == wire.entity_id()
                            && row.kind == InteractionHeatKind::LiveWireRemainder
                    })
                    .map(|row| row.energy),
                Some(HeatEnergy(10))
            );
        }
        TraceKind::C09 if report.completed_tick == Tick(45) => {
            let victim = WireId(EntityId(11));
            let damage = report
                .damage
                .iter()
                .find(|row| row.target == victim.entity_id())
                .expect("C-09 victim is damaged");
            assert_eq!(
                (
                    damage.electrical_exposure,
                    damage.integrity_after,
                    damage.pending_destruction
                ),
                (Energy(10), aon_sim::Integrity(0), true)
            );
            assert!(report.signal_arrivals.is_empty());
            let mut snapshot = aon_sim::RenderSnapshot::default();
            simulation.write_render_snapshot(&mut snapshot);
            let wire = snapshot
                .wires()
                .iter()
                .find(|row| row.id == victim)
                .expect("the pending C-09 Wire remains on the Track surface");
            assert_eq!(wire.connection_generation, ConnectionGeneration::INITIAL);
            assert!(simulation.wire_signal_state(victim).is_some());
            assert!(simulation.wire_sense_state(victim).is_some());
            let accounting = report
                .network_accounting
                .expect("the pending C-09 Tick reports Capacity");
            assert!(
                simulation
                    .network_analyzer_snapshot()
                    .expect("the pending Capacity analyzer succeeds")
                    .expect("Capacity is enabled")
                    .wires()
                    .iter()
                    .any(|row| {
                        row.wire() == victim
                            && row.length()
                                == Capacity(u64::try_from(20 * WU).expect("20 WU is positive"))
                    })
            );
            let power = report
                .power
                .as_ref()
                .expect("the pending Tick reports Power");
            let victim_load = power
                .load(DemandId::new(victim.entity_id(), DemandKind::WireLeakage))
                .expect("the pending Wire remains a Power load");
            assert_eq!(victim_load.granted, Energy(0));
            let source_ports = simulation
                .gate_signal_ports(GateId(EntityId(9)))
                .expect("the C-09 source Gate exists");
            let downstream_ports = simulation
                .gate_signal_ports(GateId(EntityId(10)))
                .expect("the C-09 downstream Gate exists");
            facts.c09_pending_revision = Some(simulation.topology_revision());
            facts.c09_pending_generation = Some(wire.connection_generation);
            facts.c09_pending_capacity = Some(accounting.used());
            facts.c09_pending_region = Some(victim_load.region);
            facts.c09_pending_region_count = Some(power.regions.len());
            facts.c09_source_driver = Some(source_ports.output);
            facts.c09_sink = Some(downstream_ports.input_a.sink);
            assert!(
                simulation
                    .sink_driver_sample(downstream_ports.input_a.sink, source_ports.output,)
                    .is_some(),
                "the pending C-09 Tick retains the pre-break Sink/Driver slot"
            );
        }
        TraceKind::C09 if report.completed_tick == Tick(46) => {
            let victim = WireId(EntityId(11));
            assert_eq!(
                report.destructions,
                [aon_sim::DestructionReport {
                    target: victim.entity_id(),
                    kind: DestructionKind::Damage,
                }]
            );
            assert!(report.topology_changed);
            let pending_revision = facts
                .c09_pending_revision
                .expect("the pending revision was retained");
            assert_eq!(
                simulation.topology_revision(),
                Revision(pending_revision.0 + 1)
            );
            assert_eq!(
                facts.c09_pending_generation,
                Some(ConnectionGeneration::INITIAL)
            );
            assert_eq!(simulation.wire_signal_state(victim), None);
            assert_eq!(simulation.wire_sense_state(victim), None);
            let mut snapshot = aon_sim::RenderSnapshot::default();
            simulation.write_render_snapshot(&mut snapshot);
            assert!(snapshot.wires().iter().all(|row| row.id != victim));
            assert!(snapshot.mobiles().is_empty());
            assert_eq!(
                report.command_acceptances,
                [aon_sim::CommandAcceptance {
                    target_tick: Tick(46),
                    ordinal: 0,
                    created_entity: Some(EntityId(13)),
                }]
            );
            assert!(
                snapshot
                    .junctions()
                    .iter()
                    .any(|row| row.id.entity_id() == EntityId(13))
            );
            assert_ne!(EntityId(13), victim.entity_id());
            let pending_capacity = facts
                .c09_pending_capacity
                .expect("the pending Capacity was retained");
            assert_eq!(
                report
                    .network_accounting
                    .expect("the removal Tick reports Capacity")
                    .used(),
                Capacity(pending_capacity.0 - u64::try_from(20 * WU).expect("20 WU is positive"))
            );
            let power = report
                .power
                .as_ref()
                .expect("the removal Tick reports Power");
            assert!(
                power
                    .loads
                    .iter()
                    .all(|load| load.demand.owner() != victim.entity_id())
            );
            assert_eq!(
                power.regions.len().checked_add(1),
                facts.c09_pending_region_count,
                "removing the source-less victim removes its complete Power region",
            );
            assert!(
                power
                    .regions
                    .iter()
                    .all(|region| { Some(region.region) != facts.c09_pending_region })
            );
            assert!(simulation.construction_sites().is_empty());
            assert_eq!(
                simulation.sink_driver_sample(
                    facts.c09_sink.expect("the downstream Sink was retained"),
                    facts
                        .c09_source_driver
                        .expect("the source Driver was retained"),
                ),
                None,
                "the broken route's Sink/Driver slot is absent"
            );
        }
        TraceKind::C09 => {
            if report.signal_counters.invalid_path_arrivals > 0 {
                assert_eq!(report.completed_tick, Tick(51));
                assert_eq!(report.signal_counters.invalid_path_arrivals, 1);
                assert_eq!(report.signal_counters.signal_arrivals_applied, 0);
                assert!(report.signal_arrivals.iter().any(|arrival| {
                    arrival.due_tick == Tick(51)
                        && Some(arrival.source_driver) == facts.c09_source_driver
                        && Some(arrival.sink) == facts.c09_sink
                }));
                facts.saw_c09_stale = true;
            }
        }
        TraceKind::Terminal => {
            if report.damage.iter().any(|row| row.target == CORE) {
                let damage = report
                    .damage
                    .iter()
                    .find(|row| row.target == CORE)
                    .expect("the terminal Core damage row exists");
                assert_eq!(damage.electrical_exposure, Energy(10));
                facts.terminal_attack_ticks.push(report.completed_tick);
            }
            if !matches!(report.run_status, RunStatus::Ended { .. }) {
                return;
            }
            assert_eq!(report.completed_tick, Tick(55));
            assert_eq!(
                report.run_status,
                RunStatus::Ended {
                    completed_tick: Tick(55),
                    cause: RunEndCause::MainCoreDestroyed,
                }
            );
            assert_eq!(simulation.run_status(), report.run_status);
        }
        _ => {}
    }
}

fn record(kind: TraceKind, scenario: &[u8]) -> ReplayArtifact {
    let mut simulation = Simulation::new(package(scenario)).expect("the S1-M4 Simulation starts");
    assert_eq!(
        simulation
            .main_core_state()
            .expect("the Main Core exists")
            .id()
            .entity_id(),
        CORE
    );
    assert_eq!(
        simulation
            .enemies()
            .iter()
            .map(|enemy| enemy.id().entity_id())
            .collect::<Vec<_>>(),
        [ENEMY_TERMINAL, ENEMY_C10_LOW, ENEMY_C10_HIGH, ENEMY_C09]
    );
    let header = simulation.replay_header();
    let commands = kind.commands();
    let mut facts = TraceFacts::default();
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: simulation.state_hash(),
    }];
    while simulation.next_tick() < kind.final_next_tick() {
        let tick = simulation.next_tick();
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step(&batch)
            .expect("the retained S1-M4 Tick succeeds");
        assert_tick(kind, &report, &simulation, &mut facts);
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
    }
    match kind {
        TraceKind::ConstructionPartial => {
            assert!(facts.saw_partial_construction);
            assert!(facts.saw_multi_builder);
        }
        TraceKind::ConstructionFourTargets => {
            assert!(simulation.construction_sites().is_empty());
            assert_eq!(facts.completed_four_targets, 4);
            assert_eq!(facts.activated_four_targets, 4);
        }
        TraceKind::C09 => assert!(facts.saw_c09_stale),
        TraceKind::Terminal => {
            assert_eq!(
                facts.terminal_attack_ticks,
                (46..=55).map(Tick).collect::<Vec<_>>()
            );
            assert_eq!(
                simulation.run_status(),
                RunStatus::Ended {
                    completed_tick: Tick(55),
                    cause: RunEndCause::MainCoreDestroyed,
                }
            );
        }
        TraceKind::C10 => {}
    }
    ReplayArtifact::new(
        REPLAY_SCENARIO_PATH,
        Replay::new_v2(header, commands, Vec::new(), checkpoints)
            .expect("the S1-M4 Replay is valid"),
    )
    .expect("the S1-M4 Scenario locator is portable")
}

fn build_bytes() -> (Vec<u8>, Vec<(TraceKind, Vec<u8>)>) {
    assert_retained_wire_work_growth();
    let scenario = scenario_bytes();
    let replays = [
        TraceKind::ConstructionPartial,
        TraceKind::ConstructionFourTargets,
        TraceKind::C10,
        TraceKind::C09,
        TraceKind::Terminal,
    ]
    .into_iter()
    .map(|kind| {
        (
            kind,
            encode_replay_artifact(&record(kind, &scenario)).expect("S1-M4 Replay encodes"),
        )
    })
    .collect();
    (scenario, replays)
}

fn assert_retained_wire_work_growth() {
    let balance = decode_balance_profile(BALANCE).expect("the S1-M4 Balance Profile decodes");
    let probe = balance
        .construction_probe
        .expect("the S1-M4 Construction probe exists");
    let wire = |points| ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points,
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    let short = wire(vec![point(0, 0), point(WU, 0)]);
    let long = wire(vec![point(0, 0), point(WU + 1, 0)]);
    let redundant = wire(vec![point(0, 0), point(WU / 2, 0), point(WU + 1, 0)]);
    assert_eq!(
        (
            required_construction_work(&short, &probe),
            required_construction_work(&long, &probe),
            required_construction_work(&redundant, &probe),
        ),
        (Ok(Energy(3)), Ok(Energy(4)), Ok(Energy(4)))
    );
}

fn replay_path(kind: TraceKind) -> PathBuf {
    Path::new(REPLAY_STEM).join(kind.file_name())
}

fn main() {
    let write = std::env::args().any(|argument| argument == "--write");
    let first = build_bytes();
    let second = build_bytes();
    assert_eq!(first.0, second.0, "the Scenario is byte-stable");
    for ((first_kind, first_bytes), (second_kind, second_bytes)) in first.1.iter().zip(&second.1) {
        assert_eq!(first_kind.file_name(), second_kind.file_name());
        assert_eq!(first_bytes, second_bytes, "Replay is byte-stable");
    }
    let scenario_hash = decode_scenario_manifest(&first.0)
        .expect("Scenario decodes")
        .canonical_hash()
        .expect("Scenario hashes");
    println!("scenarioHash={scenario_hash}");
    if write {
        std::fs::write(SCENARIO_PATH, &first.0).expect("the Scenario fixture writes");
        for (kind, bytes) in &first.1 {
            let path = replay_path(*kind);
            std::fs::create_dir_all(path.parent().expect("Replay path has a parent"))
                .expect("the Replay fixture directory exists");
            std::fs::write(&path, bytes).expect("the Replay fixture writes");
        }
    }

    let checked_scenario = std::fs::read(SCENARIO_PATH).expect("the checked-in Scenario reads");
    assert_eq!(
        checked_scenario, first.0,
        "the checked-in Scenario must exactly match the deterministic generator"
    );
    decode_scenario_manifest(&checked_scenario).expect("the checked-in Scenario strictly decodes");
    let canonical_scenario = Path::new(SCENARIO_PATH)
        .canonicalize()
        .expect("the checked-in Scenario canonicalizes");

    for (kind, generated_bytes) in &first.1 {
        let path = replay_path(*kind);
        let checked_bytes = std::fs::read(&path).expect("the checked-in Replay reads");
        assert_eq!(
            checked_bytes, *generated_bytes,
            "the checked-in Replay must exactly match the deterministic generator"
        );
        let artifact = aon_sim::decode_replay_artifact(&checked_bytes)
            .expect("the checked-in Replay strictly decodes");
        assert_eq!(
            aon_sim::encode_replay_artifact(&artifact)
                .expect("the checked-in Replay canonically re-encodes"),
            checked_bytes
        );
        let resolved_scenario = path
            .parent()
            .expect("Replay path has a parent")
            .join(artifact.scenario_path());
        assert!(
            resolved_scenario.is_file(),
            "Replay Scenario locator must resolve: {}",
            resolved_scenario.display()
        );
        assert_eq!(
            resolved_scenario
                .canonicalize()
                .expect("the referenced Scenario canonicalizes"),
            canonical_scenario
        );
    }
}
