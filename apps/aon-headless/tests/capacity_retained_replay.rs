use aon_headless::run_replay_file;
use aon_sim::{
    ArtifactBytes, Capacity, EntityId, FIXED_ONE, MainCoreId, Simulation, StateHashVersion, Tick,
    WireId, WorldGeneratorVersion, decode_package, decode_replay_artifact,
    decode_scenario_manifest, encode_replay_artifact,
};
use std::path::PathBuf;

const REPLAY_BYTES: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m1-capacity-accounting-v1.json");
const SCENARIO: &[u8] =
    include_bytes!("../../../fixtures/scenarios/s1-m1-capacity-accounting-v1.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/capacity-probe-alpha.json");

const FINAL_NEXT_TICK: Tick = Tick(3);
const EXPECTED_SUPPORTED: Capacity = Capacity(1_000 * FIXED_ONE as u64);
const EXPECTED_USED: Capacity = Capacity(12 * FIXED_ONE as u64);
const EXPECTED_INITIAL_HASH: &str =
    "39b3c5e4d9f0855c46c7b2e80d91d623f641675516fe0770416dc4e52402a230";
const EXPECTED_FINAL_HASH: &str =
    "ffa752a27489371a0b30d61b47dba49af275e7298ad9259548feec67fa238114";

fn package() -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the retained S1-M1 package decodes")
}

#[test]
fn retained_capacity_replay_is_canonical_and_executes_headlessly_with_exact_c21_accounting() {
    assert_eq!(
        decode_scenario_manifest(SCENARIO)
            .expect("the retained Scenario strictly decodes")
            .canonical_hash()
            .expect("the retained Scenario hashes")
            .to_string(),
        "f81b15ab86e4c172275b2e2c1c9a13289c04997e3fc1e80f14deedcd76d964ae"
    );
    let artifact = decode_replay_artifact(REPLAY_BYTES)
        .expect("the retained capacity Replay strictly decodes");
    assert_eq!(
        encode_replay_artifact(&artifact).expect("the retained capacity Replay encodes"),
        REPLAY_BYTES
    );
    assert_eq!(
        artifact.scenario_path(),
        "../scenarios/s1-m1-capacity-accounting-v1.json"
    );
    assert_eq!(artifact.replay().commands().len(), 11);
    assert_eq!(artifact.replay().final_next_tick(), FINAL_NEXT_TICK);
    assert_eq!(
        artifact.replay().header().state_hash_version,
        StateHashVersion::V6
    );
    assert_eq!(
        artifact.replay().header().world_generator_version,
        WorldGeneratorVersion::MainCoreV1
    );
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .iter()
            .map(|checkpoint| checkpoint.next_tick)
            .collect::<Vec<_>>(),
        [Tick(0), Tick(1), Tick(2), FINAL_NEXT_TICK]
    );

    let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/s1-m1-capacity-accounting-v1.json");
    let headless = run_replay_file(replay_path).expect("the retained capacity Replay runs");
    assert_eq!(headless.scenario_id(), "s1-m1-capacity-accounting-v1");
    assert_eq!(headless.completed_ticks(), FINAL_NEXT_TICK.0);
    assert_eq!(headless.checkpoints().len(), 4);
    assert_eq!(headless.final_hash().to_string(), EXPECTED_FINAL_HASH);
    assert_eq!(
        headless.final_hash(),
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("the retained Replay has a final checkpoint")
            .state_hash
    );

    let mut simulation = Simulation::new(package()).expect("the retained capacity world starts");
    let core = simulation
        .main_core_state()
        .expect("the retained capacity world has one Main Core");
    assert_eq!(core.id(), MainCoreId(EntityId(1)));
    assert_eq!(core.capacity(), EXPECTED_SUPPORTED);

    let initial_hash = simulation.state_hash();
    assert_eq!(initial_hash.to_string(), EXPECTED_INITIAL_HASH);
    let initial_analyzer = simulation
        .network_analyzer_snapshot()
        .expect("the initial Network Analyzer succeeds")
        .expect("capacity enables the Network Analyzer");
    assert_eq!(initial_analyzer.next_tick(), Tick(0));
    assert_eq!(initial_analyzer.accounting().used(), Capacity(0));
    assert_eq!(
        initial_analyzer.accounting().supported(),
        EXPECTED_SUPPORTED
    );
    assert!(initial_analyzer.wires().is_empty());
    assert_eq!(
        simulation.state_hash(),
        initial_hash,
        "the derived Network Analyzer must not mutate canonical state"
    );

    let mut trace = vec![simulation.state_hash()];
    while simulation.next_tick() < artifact.replay().final_next_tick() {
        let target_tick = simulation.next_tick();
        let commands = artifact
            .replay()
            .commands_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step(&commands)
            .expect("the retained capacity Tick succeeds");
        assert!(report.command_rejections.is_empty());
        let accounting = report
            .network_accounting
            .expect("capacity-enabled Ticks report accounting");
        assert_eq!(accounting.supported(), EXPECTED_SUPPORTED);
        assert_eq!(
            accounting.used(),
            match report.next_tick {
                Tick(1) | Tick(2) => Capacity(10 * FIXED_ONE as u64),
                Tick(3) => EXPECTED_USED,
                _ => unreachable!("the retained Replay has exactly three Ticks"),
            }
        );
        let analyzer = simulation
            .network_analyzer_snapshot()
            .expect("the checkpoint Network Analyzer succeeds")
            .expect("capacity keeps the checkpoint Analyzer enabled");
        assert_eq!(analyzer.next_tick(), report.next_tick);
        assert_eq!(analyzer.accounting(), accounting);
        assert!(
            analyzer
                .wires()
                .windows(2)
                .all(|pair| pair[0].wire() < pair[1].wire())
        );
        assert_eq!(
            analyzer
                .wires()
                .iter()
                .map(|row| row.length().0)
                .sum::<u64>(),
            accounting.used().0
        );
        let expected_rows = match report.next_tick {
            Tick(1) => vec![(WireId(EntityId(2)), Capacity(10 * FIXED_ONE as u64))],
            Tick(2) => vec![
                (WireId(EntityId(7)), Capacity(2 * FIXED_ONE as u64)),
                (WireId(EntityId(8)), Capacity(3 * FIXED_ONE as u64)),
                (WireId(EntityId(9)), Capacity(3 * FIXED_ONE as u64)),
                (WireId(EntityId(10)), Capacity(2 * FIXED_ONE as u64)),
            ],
            Tick(3) => vec![
                (WireId(EntityId(7)), Capacity(2 * FIXED_ONE as u64)),
                (WireId(EntityId(8)), Capacity(3 * FIXED_ONE as u64)),
                (WireId(EntityId(9)), Capacity(3 * FIXED_ONE as u64)),
                (WireId(EntityId(10)), Capacity(2 * FIXED_ONE as u64)),
                (WireId(EntityId(11)), Capacity(2 * FIXED_ONE as u64)),
            ],
            _ => unreachable!("the retained Replay has exactly three Ticks"),
        };
        assert_eq!(
            analyzer
                .wires()
                .iter()
                .map(|row| (row.wire(), row.length()))
                .collect::<Vec<_>>(),
            expected_rows
        );
        trace.push(report.state_hash);
    }
    artifact
        .replay()
        .verify_trace(&trace)
        .expect("the manual trace matches every retained checkpoint");
    assert_eq!(trace, headless.checkpoints());

    let before_analysis = simulation.state_hash();
    let analyzer = simulation
        .network_analyzer_snapshot()
        .expect("the final Network Analyzer succeeds")
        .expect("capacity keeps the Network Analyzer enabled");
    assert_eq!(analyzer.next_tick(), FINAL_NEXT_TICK);
    assert_eq!(analyzer.accounting().used(), EXPECTED_USED);
    assert_eq!(analyzer.accounting().supported(), EXPECTED_SUPPORTED);
    assert_eq!(
        analyzer
            .wires()
            .iter()
            .map(|row| (row.wire(), row.length()))
            .collect::<Vec<_>>(),
        [
            (WireId(EntityId(7)), Capacity(2 * FIXED_ONE as u64)),
            (WireId(EntityId(8)), Capacity(3 * FIXED_ONE as u64)),
            (WireId(EntityId(9)), Capacity(3 * FIXED_ONE as u64)),
            (WireId(EntityId(10)), Capacity(2 * FIXED_ONE as u64)),
            (WireId(EntityId(11)), Capacity(2 * FIXED_ONE as u64)),
        ],
        "the four-Wire split retains 10 NCU and the internal Wire adds 2 NCU"
    );
    assert_eq!(
        simulation.state_hash(),
        before_analysis,
        "analysis remains a read-only derived projection"
    );
}
