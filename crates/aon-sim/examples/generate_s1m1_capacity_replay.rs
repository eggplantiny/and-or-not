use aon_sim::{
    ArtifactBytes, Capacity, Command, CommandEnvelope, EndpointTarget, EntityId, FIXED_ONE, Fixed,
    FixedAabb, FixedVec2, HashCheckpoint, JunctionId, MainCoreId, PlaceFixedSubstrateCommand,
    PlaceJunctionCommand, PlaceWireCommand, RemoveEntityCommand, Replay, ReplayArtifact,
    RoutingDomain, Simulation, Tick, TopologyNodeId, WireId, decode_package,
    encode_replay_artifact,
};
use std::path::Path;

const SCENARIO: &[u8] =
    include_bytes!("../../../fixtures/scenarios/s1-m1-capacity-accounting-v1.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/capacity-probe-alpha.json");

const MULTI_ROLE_WIRE: WireId = WireId(EntityId(2));
const FIXED_SUBSTRATE: EntityId = EntityId(3);
const SPLIT_A: JunctionId = JunctionId(EntityId(4));
const SPLIT_B: JunctionId = JunctionId(EntityId(5));
const SPLIT_C: JunctionId = JunctionId(EntityId(6));
const WORLD_WIRES: [WireId; 4] = [
    WireId(EntityId(7)),
    WireId(EntityId(8)),
    WireId(EntityId(9)),
    WireId(EntityId(10)),
];
const INTERNAL_WIRE: WireId = WireId(EntityId(11));
const FINAL_NEXT_TICK: Tick = Tick(3);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * FIXED_ONE, y * FIXED_ONE)
}

fn package() -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the retained S1-M1 package is valid")
}

fn commands() -> Vec<CommandEnvelope> {
    let substrate_bounds = FixedAabb::new(wu(-8, -8), wu(8, 8));
    vec![
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(0, 0), wu(4, 0), wu(7, 0), wu(10, 0)],
                endpoint_a: EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
                endpoint_b: EndpointTarget::Free,
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::RemoveEntity(RemoveEntityCommand {
                target: MULTI_ROLE_WIRE.entity_id(),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 1,
            command: Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: wu(0, 0),
                routing_area: substrate_bounds,
                footprint: substrate_bounds,
            }),
        },
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 2,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: wu(2, 16),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 3,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: wu(5, 16),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 4,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: wu(8, 16),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 5,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                // Redundant same-direction vertices freeze the within-Wire canonicalization rule.
                points: vec![wu(0, 16), wu(1, 16), wu(2, 16)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Junction(SPLIT_A),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 6,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(2, 16), wu(5, 16)],
                endpoint_a: EndpointTarget::Junction(SPLIT_A),
                endpoint_b: EndpointTarget::Junction(SPLIT_B),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 7,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(5, 16), wu(8, 16)],
                endpoint_a: EndpointTarget::Junction(SPLIT_B),
                endpoint_b: EndpointTarget::Junction(SPLIT_C),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 8,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(8, 16), wu(10, 16)],
                endpoint_a: EndpointTarget::Junction(SPLIT_C),
                endpoint_b: EndpointTarget::Free,
            }),
        },
        CommandEnvelope {
            target_tick: Tick(2),
            ordinal: 0,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::FixedSubstrate(FIXED_SUBSTRATE),
                points: vec![wu(0, 4), wu(2, 4)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        },
    ]
}

fn record() -> ReplayArtifact {
    let mut simulation = Simulation::new(package()).expect("the S1-M1 simulation starts");
    let main_core = *simulation
        .main_core_state()
        .expect("the S1-M1 world generator creates one Main Core");
    assert_eq!(main_core.id(), MainCoreId(EntityId(1)));
    assert_eq!(main_core.position(), wu(0, 0));
    assert_eq!(
        main_core.anchor_node(),
        TopologyNodeId::MainCoreAnchor(MainCoreId(EntityId(1)))
    );
    assert_eq!(main_core.capacity(), Capacity(1_000 * FIXED_ONE as u64));
    assert_eq!(main_core.integrity().0, 1_000);
    assert_eq!(main_core.heat_energy().0, 0);

    let commands = commands();
    let mut checkpoints = vec![HashCheckpoint {
        next_tick: Tick(0),
        state_hash: simulation.state_hash(),
    }];
    while simulation.next_tick() < FINAL_NEXT_TICK {
        let target_tick = simulation.next_tick();
        let batch = commands
            .iter()
            .filter(|command| command.target_tick == target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step(&batch)
            .expect("the retained S1-M1 Tick succeeds");
        assert!(
            report.command_rejections.is_empty(),
            "retained commands are accepted at {target_tick}: {:?}",
            report.command_rejections
        );
        let accounting = report
            .network_accounting
            .expect("capacity-enabled Ticks report Network accounting");
        assert_eq!(accounting.supported(), Capacity(1_000 * FIXED_ONE as u64));
        let expected_used = match report.next_tick {
            Tick(1) | Tick(2) => Capacity(10 * FIXED_ONE as u64),
            Tick(3) => Capacity(12 * FIXED_ONE as u64),
            _ => unreachable!("the retained Replay has exactly three Ticks"),
        };
        assert_eq!(accounting.used(), expected_used);
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
    }

    let analyzer = simulation
        .network_analyzer_snapshot()
        .expect("the Network Analyzer succeeds")
        .expect("the capacity-enabled world exposes a Network Analyzer snapshot");
    assert_eq!(analyzer.next_tick(), FINAL_NEXT_TICK);
    assert_eq!(
        analyzer.accounting().used(),
        Capacity(12 * FIXED_ONE as u64)
    );
    assert_eq!(
        analyzer.accounting().supported(),
        Capacity(1_000 * FIXED_ONE as u64)
    );
    assert_eq!(
        analyzer
            .wires()
            .iter()
            .map(|row| (row.wire(), row.length()))
            .collect::<Vec<_>>(),
        [
            (WORLD_WIRES[0], Capacity(2 * FIXED_ONE as u64)),
            (WORLD_WIRES[1], Capacity(3 * FIXED_ONE as u64)),
            (WORLD_WIRES[2], Capacity(3 * FIXED_ONE as u64)),
            (WORLD_WIRES[3], Capacity(2 * FIXED_ONE as u64)),
            (INTERNAL_WIRE, Capacity(2 * FIXED_ONE as u64)),
        ]
    );

    ReplayArtifact::new(
        "../scenarios/s1-m1-capacity-accounting-v1.json",
        Replay::new_v2(
            simulation.replay_header(),
            commands,
            Vec::new(),
            checkpoints,
        )
        .expect("the retained S1-M1 Replay is valid"),
    )
    .expect("the retained Scenario locator is portable")
}

fn main() {
    let write = std::env::args().any(|argument| argument == "--write");
    let artifact = record();
    let bytes = encode_replay_artifact(&artifact).expect("the retained S1-M1 Replay encodes");
    let path = Path::new("fixtures/replays/s1-m1-capacity-accounting-v1.json");
    if write {
        std::fs::write(path, bytes).expect("the retained S1-M1 Replay fixture writes");
        println!("wrote {}", path.display());
    } else {
        println!(
            "{}",
            String::from_utf8(bytes).expect("Replay JSON is UTF-8")
        );
    }
}
