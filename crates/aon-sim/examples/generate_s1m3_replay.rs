use aon_sim::{
    ArtifactBytes, Capacity, Command, CommandEnvelope, DemandId, DemandKind, EndpointTarget,
    Energy, EntityId, FIXED_ONE, Fixed, FixedVec2, HashCheckpoint, HeatEnergy, JunctionId,
    MainCoreId, PlaceJunctionCommand, PlaceWireCommand, PowerHeatKind, PowerRatio, PowerSourceId,
    Replay, ReplayArtifact, RoutingDomain, Simulation, Tick, WireId, decode_balance_profile,
    decode_numeric_profile, decode_package, decode_physical_scale_profile,
    decode_scenario_manifest, encode_replay_artifact,
};
use serde_json::json;
use std::path::Path;

const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] =
    include_bytes!("../../../profiles/balance/s1-m3-capacity-support-alpha.json");

const SCENARIO_ID: &str = "s1-m3-c22-capacity-support-v1";
const SCENARIO_PATH: &str = "fixtures/scenarios/s1-m3-c22-capacity-support-v1.json";
const REPLAY_PATH: &str = "fixtures/replays/s1-m3-c22-capacity-support-v1.json";
const REPLAY_SCENARIO_PATH: &str = "../scenarios/s1-m3-c22-capacity-support-v1.json";

const WU: i64 = FIXED_ONE;
const CORE: MainCoreId = MainCoreId(EntityId(1));
const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));
const JUNCTION: JunctionId = JunctionId(EntityId(3));
const WIRE_LOW_ID: WireId = WireId(EntityId(4));
const WIRE_HIGH_ID: WireId = WireId(EntityId(5));
const FINAL_NEXT_TICK: Tick = Tick(3);

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point(x * WU, y * WU)
}

fn scenario_bytes() -> Vec<u8> {
    let numeric = decode_numeric_profile(NUMERIC).expect("the retained Numeric Profile decodes");
    let physical = decode_physical_scale_profile(PHYSICAL)
        .expect("the retained Physical Scale Profile decodes");
    let balance =
        decode_balance_profile(BALANCE).expect("the S1-M3 Capacity Support Profile decodes");
    assert_eq!(
        balance
            .canonical_hash()
            .expect("the S1-M3 Balance Profile hashes")
            .to_string(),
        "a0a8974aebc87e30d602ffa019340e59c908912c0b36e0e0634e51214afc45ef"
    );

    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 3,
        "scenarioId": SCENARIO_ID,
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": wu(-8, -8).x.0, "y": wu(-8, -8).y.0 },
                "integrity": 1000,
                "heatEnergy": 0
            },
            "powerSources": [{
                "position": { "x": 0, "y": 0 },
                "generationPerTick": 268
            }]
        },
        "requiredFeatures": {
            "signal": true,
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
                "path": "../../profiles/numeric/v1.json",
                "profileId": numeric.profile_id,
                "profileHash": numeric
                    .canonical_hash()
                    .expect("the Numeric Profile hashes")
                    .to_string()
            },
            "physicalScale": {
                "path": "../../profiles/physical-scale/stage0-alpha.json",
                "profileId": physical.profile_id,
                "profileHash": physical
                    .canonical_hash()
                    .expect("the Physical Scale Profile hashes")
                    .to_string()
            },
            "balance": {
                "path": "../../profiles/balance/s1-m3-capacity-support-alpha.json",
                "profileId": balance.profile_id,
                "profileHash": balance
                    .canonical_hash()
                    .expect("the S1-M3 Balance Profile hashes")
                    .to_string()
            }
        }
    }))
    .expect("the C-22 Scenario JSON encodes");
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
    .expect("the generated S1-M3 package decodes")
}

fn commands() -> Vec<CommandEnvelope> {
    vec![
        CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: wu(70, 0),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(0, 0), wu(70, 0)],
                endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
                endpoint_b: EndpointTarget::Junction(JUNCTION),
            }),
        },
        CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 1,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![wu(70, 0), wu(120, 0)],
                endpoint_a: EndpointTarget::Junction(JUNCTION),
                endpoint_b: EndpointTarget::Free,
            }),
        },
    ]
}

fn assert_exact_power(report: &aon_sim::PowerStepReport) {
    assert_eq!(report.regions.len(), 1);
    assert_eq!(report.regions[0].generation, Energy(268));
    assert_eq!(report.regions[0].total_nominal_demand, Energy(268));
    assert_eq!(report.regions[0].ratio, PowerRatio::ONE);
    assert_eq!(report.loads.len(), 6);

    for (wire, leakage, sensing, support) in [(WIRE_LOW_ID, 70, 70, 17), (WIRE_HIGH_ID, 50, 50, 11)]
    {
        for (kind, expected) in [
            (DemandKind::WireLeakage, leakage),
            (DemandKind::WireSensing, sensing),
            (DemandKind::OvercapacitySupport, support),
        ] {
            let load = report
                .load(DemandId::new(wire.entity_id(), kind))
                .expect("every C-22 intrinsic load is reported");
            assert_eq!(load.nominal, Energy(expected));
            assert_eq!(load.granted, Energy(expected));
            assert_eq!(load.ratio, PowerRatio::ONE);
            assert_eq!(load.transmission_loss, Energy(0));
        }
    }

    let support_heat = report
        .heat_contributions
        .iter()
        .filter(|heat| heat.kind == PowerHeatKind::OvercapacitySupport)
        .map(|heat| (heat.owner, heat.demand, heat.energy))
        .collect::<Vec<_>>();
    assert_eq!(
        support_heat,
        [
            (
                WIRE_LOW_ID,
                DemandId::new(WIRE_LOW_ID.entity_id(), DemandKind::OvercapacitySupport),
                HeatEnergy(4),
            ),
            (
                WIRE_HIGH_ID,
                DemandId::new(WIRE_HIGH_ID.entity_id(), DemandKind::OvercapacitySupport),
                HeatEnergy(3),
            ),
        ]
    );
    assert_eq!(
        support_heat
            .iter()
            .map(|(_, _, energy)| energy.0)
            .sum::<u64>(),
        7
    );
}

fn assert_canonical_core_is_unchanged(simulation: &Simulation) {
    let core = simulation
        .main_core_state()
        .expect("the C-22 Main Core remains alive");
    assert_eq!(core.id(), CORE);
    assert_eq!(core.capacity(), Capacity(100 * FIXED_ONE as u64));
    assert_eq!(core.integrity().0, 1000);
    assert_eq!(core.heat_energy(), HeatEnergy(0));
}

fn record(scenario: &[u8]) -> ReplayArtifact {
    let mut simulation = Simulation::new(package(scenario)).expect("the C-22 Simulation starts");
    assert_canonical_core_is_unchanged(&simulation);
    assert_eq!(
        simulation
            .power_source_state(SOURCE)
            .expect("the C-22 Source exists")
            .generation_per_tick(),
        Energy(268)
    );

    let header = simulation.replay_header();
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
            .expect("the retained C-22 Tick succeeds");
        assert!(
            report.command_rejections.is_empty(),
            "retained commands are accepted at {target_tick}: {:?}",
            report.command_rejections
        );
        match target_tick {
            Tick(0) => assert_eq!(
                report
                    .command_acceptances
                    .iter()
                    .map(|accepted| accepted.created_entity)
                    .collect::<Vec<_>>(),
                [Some(JUNCTION.entity_id())]
            ),
            Tick(1) => assert_eq!(
                report
                    .command_acceptances
                    .iter()
                    .map(|accepted| accepted.created_entity)
                    .collect::<Vec<_>>(),
                [
                    Some(WIRE_LOW_ID.entity_id()),
                    Some(WIRE_HIGH_ID.entity_id())
                ]
            ),
            Tick(2) => assert!(report.command_acceptances.is_empty()),
            _ => unreachable!("the C-22 Replay records exactly three Ticks"),
        }

        let accounting = report
            .network_accounting
            .expect("C-22 reports global Network accounting");
        assert_eq!(accounting.supported(), Capacity(100 * FIXED_ONE as u64));
        if target_tick == Tick(0) {
            assert_eq!(accounting.used(), Capacity(0));
            assert_eq!(accounting.excess(), Some(Capacity(0)));
            assert_eq!(accounting.total_support_demand(), Some(Energy(0)));
        } else {
            assert_eq!(accounting.used(), Capacity(120 * FIXED_ONE as u64));
            assert_eq!(accounting.excess(), Some(Capacity(20 * FIXED_ONE as u64)));
            assert_eq!(accounting.total_support_demand(), Some(Energy(28)));
            assert_exact_power(report.power.as_ref().expect("C-22 emits a Power report"));
        }
        assert_canonical_core_is_unchanged(&simulation);
        checkpoints.push(HashCheckpoint {
            next_tick: report.next_tick,
            state_hash: report.state_hash,
        });
    }

    let before_analysis = simulation.state_hash();
    let network = simulation
        .network_analyzer_snapshot()
        .expect("the C-22 Network Analyzer succeeds")
        .expect("capacity exposes the C-22 Network Analyzer");
    assert_eq!(network.next_tick(), FINAL_NEXT_TICK);
    assert_eq!(
        network.accounting().used(),
        Capacity(120 * FIXED_ONE as u64)
    );
    assert_eq!(
        network.accounting().excess(),
        Some(Capacity(20 * FIXED_ONE as u64))
    );
    assert_eq!(
        network.accounting().total_support_demand(),
        Some(Energy(28))
    );
    assert_eq!(
        network
            .wires()
            .iter()
            .map(|wire| (wire.wire(), wire.length(), wire.support_demand()))
            .collect::<Vec<_>>(),
        [
            (
                WIRE_LOW_ID,
                Capacity(70 * FIXED_ONE as u64),
                Some(Energy(17)),
            ),
            (
                WIRE_HIGH_ID,
                Capacity(50 * FIXED_ONE as u64),
                Some(Energy(11)),
            ),
        ]
    );
    let power = simulation
        .power_sense_analyzer_snapshot()
        .expect("the C-22 Power Analyzer succeeds")
        .expect("power exposes the C-22 Power Analyzer");
    assert_eq!(power.next_tick, FINAL_NEXT_TICK);
    assert_eq!(power.regions.len(), 1);
    assert_eq!(power.regions[0].total_nominal_demand, Energy(268));
    assert_eq!(power.loads.len(), 6);
    assert_eq!(simulation.state_hash(), before_analysis);

    ReplayArtifact::new(
        REPLAY_SCENARIO_PATH,
        Replay::new_v2(header, commands, Vec::new(), checkpoints)
            .expect("the retained C-22 Replay is valid"),
    )
    .expect("the retained C-22 Scenario locator is portable")
}

fn build_bytes() -> (Vec<u8>, Vec<u8>) {
    let scenario = scenario_bytes();
    let replay = encode_replay_artifact(&record(&scenario))
        .expect("the retained C-22 Replay encodes canonically");
    (scenario, replay)
}

fn write_or_print(path: &Path, bytes: &[u8], write: bool) {
    if write {
        std::fs::write(path, bytes).expect("the retained C-22 fixture writes");
        println!("wrote {}", path.display());
    } else {
        println!("{}", path.display());
        println!("{}", String::from_utf8_lossy(bytes));
    }
}

fn main() {
    let write = std::env::args().any(|argument| argument == "--write");
    let first = build_bytes();
    let second = build_bytes();
    assert_eq!(first, second, "two independent C-22 runs are byte-stable");

    let scenario_hash = decode_scenario_manifest(&first.0)
        .expect("the generated C-22 Scenario strictly decodes")
        .canonical_hash()
        .expect("the generated C-22 Scenario hashes");
    let artifact = aon_sim::decode_replay_artifact(&first.1)
        .expect("the generated C-22 Replay strictly decodes");
    println!(
        "scenarioHash={} initialHash={} finalHash={}",
        scenario_hash,
        artifact.replay().header().initial_state_hash,
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("the C-22 Replay has a final checkpoint")
            .state_hash
    );
    write_or_print(Path::new(SCENARIO_PATH), &first.0, write);
    write_or_print(Path::new(REPLAY_PATH), &first.1, write);
}
