use aon_headless::run_replay_file;
use aon_sim::{
    ArtifactBytes, Capacity, DemandId, DemandKind, Energy, EntityId, FIXED_ONE, HeatEnergy,
    MainCoreId, PowerHeatKind, PowerRatio, PowerSourceId, PowerStepReport, ReplayFormatVersion,
    Simulation, StateHash, StateHashVersion, Tick, WireId, WorldGeneratorVersion,
    decode_balance_profile, decode_package, decode_replay_artifact, decode_scenario_manifest,
    encode_replay_artifact,
};
use std::path::PathBuf;

const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] =
    include_bytes!("../../../profiles/balance/s1-m3-capacity-support-alpha.json");
const SCENARIO: &[u8] =
    include_bytes!("../../../fixtures/scenarios/s1-m3-c22-capacity-support-v1.json");
const REPLAY: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m3-c22-capacity-support-v1.json");

const CORE: MainCoreId = MainCoreId(EntityId(1));
const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));
const WIRE_LOW_ID: WireId = WireId(EntityId(4));
const WIRE_HIGH_ID: WireId = WireId(EntityId(5));
const FINAL_NEXT_TICK: Tick = Tick(3);

const BALANCE_HASH: &str = "a0a8974aebc87e30d602ffa019340e59c908912c0b36e0e0634e51214afc45ef";
const SCENARIO_HASH: &str = "bdebfe491a2f3a31dfdcd7c2470cf447415137459de5e4d65095d3d38f0e01a5";
const INITIAL_HASH: &str = "47cddc7a4a1a1371d6600953bb7c0acc7c7e5e465741869375026e7efcab9369";
const FINAL_HASH: &str = "7f687d752df7146141be826dbb74668866494c1a024ec6f157bb3eb264c3445c";

fn package() -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the retained C-22 package decodes")
}

fn replay_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/s1-m3-c22-capacity-support-v1.json")
}

fn state_hash(value: &str) -> StateHash {
    StateHash::from_hex(value).expect("the retained State hash is canonical lowercase hex")
}

fn assert_load(power: &PowerStepReport, wire: WireId, kind: DemandKind, expected: u64) {
    let load = power
        .load(DemandId::new(wire.entity_id(), kind))
        .expect("every C-22 intrinsic load is reported");
    assert_eq!(load.nominal, Energy(expected));
    assert_eq!(load.granted, Energy(expected));
    assert_eq!(load.ratio, PowerRatio::ONE);
    assert_eq!(load.transmission_loss, Energy(0));
}

fn assert_power(power: &PowerStepReport) {
    assert_eq!(power.regions.len(), 1);
    assert_eq!(power.regions[0].generation, Energy(268));
    assert_eq!(power.regions[0].total_nominal_demand, Energy(268));
    assert_eq!(power.regions[0].ratio, PowerRatio::ONE);
    assert_eq!(power.loads.len(), 6);
    assert_load(power, WIRE_LOW_ID, DemandKind::WireLeakage, 70);
    assert_load(power, WIRE_LOW_ID, DemandKind::WireSensing, 70);
    assert_load(power, WIRE_LOW_ID, DemandKind::OvercapacitySupport, 17);
    assert_load(power, WIRE_HIGH_ID, DemandKind::WireLeakage, 50);
    assert_load(power, WIRE_HIGH_ID, DemandKind::WireSensing, 50);
    assert_load(power, WIRE_HIGH_ID, DemandKind::OvercapacitySupport, 11);

    assert_eq!(
        power
            .heat_contributions
            .iter()
            .filter(|heat| heat.kind == PowerHeatKind::OvercapacitySupport)
            .map(|heat| (heat.owner, heat.demand, heat.energy))
            .collect::<Vec<_>>(),
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
}

fn assert_live_c22_state(simulation: &Simulation) {
    let before = simulation.state_hash();
    let core = simulation
        .main_core_state()
        .expect("the C-22 Main Core remains alive");
    assert_eq!(core.id(), CORE);
    assert_eq!(core.capacity(), Capacity(100 * FIXED_ONE as u64));
    assert_eq!(core.integrity().0, 1000);
    assert_eq!(core.heat_energy(), HeatEnergy(0));
    assert_eq!(
        simulation
            .power_source_state(SOURCE)
            .expect("the C-22 Source remains alive")
            .generation_per_tick(),
        Energy(268)
    );

    let network = simulation
        .network_analyzer_snapshot()
        .expect("the C-22 Network Analyzer succeeds")
        .expect("capacity exposes the C-22 Network Analyzer");
    assert_eq!(network.next_tick(), simulation.next_tick());
    assert_eq!(
        network.accounting().used(),
        Capacity(120 * FIXED_ONE as u64)
    );
    assert_eq!(
        network.accounting().supported(),
        Capacity(100 * FIXED_ONE as u64)
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
    assert_eq!(power.next_tick, simulation.next_tick());
    assert_eq!(power.regions.len(), 1);
    assert_eq!(power.regions[0].generation, Energy(268));
    assert_eq!(power.regions[0].total_nominal_demand, Energy(268));
    assert_eq!(power.regions[0].ratio, PowerRatio::ONE);
    assert_eq!(power.loads.len(), 6);
    for (wire, leakage, sensing, support) in [(WIRE_LOW_ID, 70, 70, 17), (WIRE_HIGH_ID, 50, 50, 11)]
    {
        for (kind, expected) in [
            (DemandKind::WireLeakage, leakage),
            (DemandKind::WireSensing, sensing),
            (DemandKind::OvercapacitySupport, support),
        ] {
            let load = power
                .loads
                .iter()
                .find(|load| load.demand == DemandId::new(wire.entity_id(), kind))
                .expect("the C-22 Analyzer exposes every intrinsic load");
            assert_eq!(load.nominal, Energy(expected));
            assert_eq!(load.granted, Energy(expected));
            assert_eq!(load.ratio, PowerRatio::ONE);
        }
    }
    assert_eq!(
        simulation.state_hash(),
        before,
        "Main Core, Source, Network Analyzer, and Power Analyzer reads are non-mutating"
    );
}

#[test]
fn retained_c22_is_canonical_headless_and_exact_across_support_power_and_heat() {
    let balance =
        decode_balance_profile(BALANCE).expect("the retained Balance v4 strictly decodes");
    assert_eq!(
        balance
            .canonical_hash()
            .expect("the retained Balance v4 hashes")
            .to_string(),
        BALANCE_HASH
    );
    assert_eq!(
        decode_scenario_manifest(SCENARIO)
            .expect("the retained C-22 Scenario strictly decodes")
            .canonical_hash()
            .expect("the retained C-22 Scenario hashes")
            .to_string(),
        SCENARIO_HASH
    );
    let mut reencoded_scenario = serde_json::to_vec_pretty(
        &serde_json::from_slice::<serde_json::Value>(SCENARIO)
            .expect("the retained C-22 Scenario is strict JSON"),
    )
    .expect("the retained C-22 Scenario JSON re-encodes");
    reencoded_scenario.push(b'\n');
    assert_eq!(reencoded_scenario, SCENARIO);

    let artifact =
        decode_replay_artifact(REPLAY).expect("the retained C-22 Replay strictly decodes");
    assert_eq!(
        encode_replay_artifact(&artifact).expect("the retained C-22 Replay canonically re-encodes"),
        REPLAY
    );
    assert_eq!(
        artifact.scenario_path(),
        "../scenarios/s1-m3-c22-capacity-support-v1.json"
    );
    let header = artifact.replay().header();
    assert_eq!(header.format_version, ReplayFormatVersion::V2);
    assert_eq!(header.state_hash_version, StateHashVersion::V6);
    assert_eq!(
        header.world_generator_version,
        WorldGeneratorVersion::MainCorePowerV1
    );
    assert_eq!(header.balance_profile_hash.to_string(), BALANCE_HASH);
    assert_eq!(header.initial_state_hash, state_hash(INITIAL_HASH));
    assert_eq!(artifact.replay().commands().len(), 3);
    assert!(artifact.replay().world_inputs().is_empty());
    assert_eq!(artifact.replay().final_next_tick(), FINAL_NEXT_TICK);
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .iter()
            .map(|checkpoint| checkpoint.next_tick)
            .collect::<Vec<_>>(),
        [Tick(0), Tick(1), Tick(2), Tick(3)]
    );
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("the retained C-22 Replay has a final checkpoint")
            .state_hash,
        state_hash(FINAL_HASH)
    );

    let headless =
        run_replay_file(replay_path()).expect("the retained C-22 Replay runs headlessly");
    assert_eq!(headless.scenario_id(), "s1-m3-c22-capacity-support-v1");
    assert_eq!(headless.completed_ticks(), FINAL_NEXT_TICK.0);
    assert_eq!(headless.checkpoints().len(), 4);
    assert_eq!(headless.final_hash(), state_hash(FINAL_HASH));
    assert_eq!(headless.reports().len(), 3);
    assert_power(
        headless.reports()[1]
            .power
            .as_ref()
            .expect("the headless evidence Tick reports Power"),
    );
    assert_power(
        headless.reports()[2]
            .power
            .as_ref()
            .expect("the headless stable Tick reports Power"),
    );

    let mut simulation = Simulation::new(package()).expect("the retained C-22 Simulation starts");
    artifact
        .replay()
        .validate_against(&simulation)
        .expect("the retained C-22 Replay matches its package");
    assert_eq!(simulation.state_hash(), state_hash(INITIAL_HASH));
    let initial_before_reads = simulation.state_hash();
    let initial_network = simulation
        .network_analyzer_snapshot()
        .expect("the initial C-22 Network Analyzer succeeds")
        .expect("capacity exposes the initial C-22 Network Analyzer");
    assert_eq!(initial_network.accounting().used(), Capacity(0));
    assert_eq!(initial_network.accounting().excess(), Some(Capacity(0)));
    assert_eq!(
        initial_network.accounting().total_support_demand(),
        Some(Energy(0))
    );
    assert!(initial_network.wires().is_empty());
    let initial_power = simulation
        .power_sense_analyzer_snapshot()
        .expect("the initial C-22 Power Analyzer succeeds")
        .expect("power exposes the initial C-22 Power Analyzer");
    assert!(initial_power.loads.is_empty());
    assert_eq!(simulation.state_hash(), initial_before_reads);

    let mut trace = vec![simulation.state_hash()];
    while simulation.next_tick() < FINAL_NEXT_TICK {
        let target_tick = simulation.next_tick();
        let commands = artifact
            .replay()
            .commands_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step(&commands)
            .expect("the retained C-22 Tick succeeds");
        assert!(report.command_rejections.is_empty());
        trace.push(report.state_hash);

        if target_tick == Tick(1) {
            assert_eq!(
                report
                    .command_acceptances
                    .iter()
                    .map(|accepted| accepted.created_entity)
                    .collect::<Vec<_>>(),
                [
                    Some(WIRE_LOW_ID.entity_id()),
                    Some(WIRE_HIGH_ID.entity_id())
                ]
            );
        }
        if target_tick >= Tick(1) {
            assert_power(report.power.as_ref().expect("C-22 reports Power"));
            assert_live_c22_state(&simulation);
        }
    }
    artifact
        .replay()
        .verify_trace(&trace)
        .expect("the manual C-22 trace matches every retained checkpoint");
    assert_eq!(trace, headless.checkpoints());
    assert_eq!(simulation.state_hash(), state_hash(FINAL_HASH));
}
