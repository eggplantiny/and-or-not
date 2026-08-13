use aon_headless::run_replay_file;
use aon_sim::{
    ArtifactBytes, DemandId, DemandKind, DriveStrength, Energy, EntityId, FIXED_ONE, Fixed, GateId,
    LogicLevel, MobileId, PowerRatio, ReplayArtifact, ReplayFormatVersion, Revision, Simulation,
    StateHash, StateHashVersion, Tick, WireId, WorldGeneratorVersion, decode_package,
    decode_replay_artifact, decode_scenario_manifest, encode_replay_artifact, scale_work,
};
use std::path::PathBuf;

const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/s1-m2-power-probe-alpha.json");

const C07_SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/s1-m2-c07-sensing-v1.json");
const C07_REPLAY: &[u8] = include_bytes!("../../../fixtures/replays/s1-m2-c07-sensing-v1.json");
const C08_FULL_SCENARIO: &[u8] =
    include_bytes!("../../../fixtures/scenarios/s1-m2-c08-brownout-full-v1.json");
const C08_FULL_REPLAY: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m2-c08-brownout-full-v1.json");
const C08_HALF_SCENARIO: &[u8] =
    include_bytes!("../../../fixtures/scenarios/s1-m2-c08-brownout-half-v1.json");
const C08_HALF_REPLAY: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m2-c08-brownout-half-v1.json");

const C07_SENSED_WIRE: WireId = WireId(EntityId(4));
const C07_PROBE_A: GateId = GateId(EntityId(5));
const C07_PROBE_B: GateId = GateId(EntityId(6));
const C08_MOBILE: MobileId = MobileId(EntityId(5));
const C08_GATE: GateId = GateId(EntityId(6));
const C08_SENSED_WIRE: WireId = WireId(EntityId(8));

const BALANCE_HASH: &str = "96d89224a7edc9b2bbd82b092891465d42b0c8e3954ebed6f9693af216cdcc63";
const C07_SCENARIO_HASH: &str = "5770222301e36fd352a859b4adce2907eac167ed233155ecfafa227f5cc59fef";
const C07_FINAL_HASH: &str = "f7e3c45129336c4f018e63ad942500701efd98c2963c903fdd0c4e6df6b70d47";
const C08_FULL_SCENARIO_HASH: &str =
    "98f73f4e267f1c1ddd706a1aafff2f075192592c5ce30dba1cbe17eb3f7af4d2";
const C08_FULL_FINAL_HASH: &str =
    "516070270ef1ef46bf312d2c2e906a0597974b6e3afa4546c7642a5e6224b3f3";
const C08_HALF_SCENARIO_HASH: &str =
    "d28c5a918675bd4e00d0b8c62c4cd12cff145f4e09bf1415a8002c508cc066a1";
const C08_HALF_FINAL_HASH: &str =
    "8565e47f3a2a9d652956a9ca692b7cc3c3baaaf5f2dbb07b334acfd25ee7cace";

fn package(scenario: &[u8]) -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the retained S1-M2 package decodes")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/replays/{name}"))
}

fn artifact(bytes: &[u8]) -> ReplayArtifact {
    let artifact = decode_replay_artifact(bytes).expect("the retained Replay strictly decodes");
    assert_eq!(
        encode_replay_artifact(&artifact).expect("the retained Replay canonically re-encodes"),
        bytes
    );
    let header = artifact.replay().header();
    assert_eq!(header.format_version, ReplayFormatVersion::V2);
    assert_eq!(header.state_hash_version, StateHashVersion::V6);
    assert_eq!(
        header.world_generator_version,
        WorldGeneratorVersion::MainCorePowerV1
    );
    assert_eq!(header.balance_profile_hash.to_string(), BALANCE_HASH);
    artifact
}

fn assert_scenario_hash(bytes: &[u8], expected: &str) {
    assert_eq!(
        decode_scenario_manifest(bytes)
            .expect("the retained Scenario strictly decodes")
            .canonical_hash()
            .expect("the retained Scenario hashes")
            .to_string(),
        expected
    );
}

fn expected_hash(value: &str) -> StateHash {
    StateHash::from_hex(value).expect("the retained State hash is canonical lowercase hex")
}

#[test]
fn retained_c07_replay_is_exact_and_runs_headlessly_with_delayed_sense_a_b() {
    assert_scenario_hash(C07_SCENARIO, C07_SCENARIO_HASH);
    let artifact = artifact(C07_REPLAY);
    assert_eq!(
        artifact.scenario_path(),
        "../scenarios/s1-m2-c07-sensing-v1.json"
    );
    assert_eq!(artifact.replay().commands().len(), 6);
    assert_eq!(artifact.replay().final_next_tick(), Tick(10));
    assert_eq!(artifact.replay().checkpoints().len(), 11);
    assert_eq!(artifact.replay().world_inputs().len(), 3);
    assert_eq!(
        artifact
            .replay()
            .world_inputs()
            .iter()
            .map(|input| (input.target_tick(), input.hostiles().len()))
            .collect::<Vec<_>>(),
        [(Tick(3), 0), (Tick(4), 3), (Tick(5), 0)]
    );
    assert_eq!(
        artifact.replay().world_inputs()[1]
            .hostiles()
            .iter()
            .map(|hostile| hostile.id)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("C07 has a final checkpoint")
            .state_hash,
        expected_hash(C07_FINAL_HASH)
    );

    let headless = run_replay_file(fixture_path("s1-m2-c07-sensing-v1.json"))
        .expect("the retained C07 Replay runs headlessly");
    assert_eq!(headless.scenario_id(), "s1-m2-c07-sensing-v1");
    assert_eq!(headless.completed_ticks(), 10);
    assert_eq!(headless.final_hash(), expected_hash(C07_FINAL_HASH));

    let mut simulation = Simulation::new(package(C07_SCENARIO)).expect("C07 Simulation starts");
    artifact
        .replay()
        .validate_against(&simulation)
        .expect("C07 Replay matches its package");
    let mut trace = vec![simulation.state_hash()];
    let mut sense_ports = None;
    let mut probe_sinks = None;

    while simulation.next_tick() < artifact.replay().final_next_tick() {
        let target_tick = simulation.next_tick();
        let commands = artifact
            .replay()
            .commands_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let world_inputs = artifact
            .replay()
            .world_inputs_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step_with_world_inputs(&commands, &world_inputs)
            .expect("the retained C07 Tick succeeds");
        assert!(report.command_rejections.is_empty());
        trace.push(report.state_hash);

        if target_tick == Tick(1) {
            let power = report.power.expect("C07 reports its Power solve");
            for kind in [DemandKind::WireLeakage, DemandKind::WireSensing] {
                let load = power
                    .load(DemandId::new(C07_SENSED_WIRE.entity_id(), kind))
                    .expect("the sensed Wire load is in the powered region");
                assert_eq!(load.nominal, Energy(8));
                assert_eq!(load.granted, Energy(8));
                assert_eq!(load.ratio, PowerRatio::ONE);
            }
        }

        if target_tick == Tick(2) {
            let sense = simulation
                .wire_sense_state(C07_SENSED_WIRE)
                .expect("the retained straight Wire exposes Sense A/B");
            sense_ports = Some(sense.ports);
            probe_sinks = Some((
                simulation
                    .gate_signal_ports(C07_PROBE_A)
                    .expect("C07 probe A exists")
                    .input_a
                    .sink,
                simulation
                    .gate_signal_ports(C07_PROBE_B)
                    .expect("C07 probe B exists")
                    .input_a
                    .sink,
            ));
        }

        if matches!(target_tick, Tick(3) | Tick(4) | Tick(5)) {
            let expected_occupied = target_tick == Tick(4);
            let sense = simulation
                .wire_sense_state(C07_SENSED_WIRE)
                .expect("C07 sensed Wire remains live");
            assert_eq!(sense.sampled_presence, expected_occupied);
            assert_eq!(
                sense.intended_level,
                if expected_occupied {
                    LogicLevel::High
                } else {
                    LogicLevel::Low
                }
            );
        }

        let ports = sense_ports;
        if let Some(ports) = ports {
            let expected_driver = match target_tick {
                Tick(4) => Some((LogicLevel::Low, Revision(1), Tick(2))),
                Tick(5) => Some((LogicLevel::High, Revision(2), Tick(5))),
                tick if (Tick(6)..=Tick(9)).contains(&tick) => {
                    Some((LogicLevel::Low, Revision(3), Tick(6)))
                }
                _ => None,
            };
            if let Some((level, revision, emitted_at)) = expected_driver {
                for driver in [ports.a, ports.b] {
                    let sample = simulation
                        .driver_sample(driver)
                        .expect("C07 Sense Driver remains live");
                    assert_eq!(sample.level, level);
                    assert_eq!(sample.strength, DriveStrength(400));
                    assert_eq!(sample.revision, revision);
                    assert_eq!(sample.emitted_at, emitted_at);
                }
            }
        }

        if matches!(target_tick, Tick(8) | Tick(9)) {
            let ports = sense_ports.expect("C07 Sense ports were observed");
            let (sink_a, sink_b) = probe_sinks.expect("C07 probe sinks were observed");
            let expected_level = if target_tick == Tick(8) {
                LogicLevel::High
            } else {
                LogicLevel::Low
            };
            let expected_revision = if target_tick == Tick(8) {
                Revision(2)
            } else {
                Revision(3)
            };
            assert_eq!(report.signal_changes.len(), 2);
            for (sink, driver) in [(sink_a, ports.a), (sink_b, ports.b)] {
                let sample = simulation
                    .sink_driver_sample(sink, driver)
                    .expect("the delayed Sense sample reached its independent probe");
                assert_eq!(sample.level, expected_level);
                assert_eq!(sample.strength, DriveStrength(400));
                assert_eq!(sample.revision, expected_revision);
                assert_eq!(simulation.sink_level(sink), Some(expected_level));
            }
        }
    }

    artifact
        .replay()
        .verify_trace(&trace)
        .expect("manual C07 trace matches every retained checkpoint");
    assert_eq!(trace, headless.checkpoints());
}

#[test]
fn retained_c08_pair_is_exact_and_headless_with_real_full_and_half_runtime_reports() {
    assert_scenario_hash(C08_FULL_SCENARIO, C08_FULL_SCENARIO_HASH);
    assert_scenario_hash(C08_HALF_SCENARIO, C08_HALF_SCENARIO_HASH);
    let full = artifact(C08_FULL_REPLAY);
    let half = artifact(C08_HALF_REPLAY);
    assert_eq!(full.replay().commands(), half.replay().commands());
    assert!(full.replay().world_inputs().is_empty());
    assert!(half.replay().world_inputs().is_empty());
    assert_eq!(full.replay().final_next_tick(), Tick(5));
    assert_eq!(half.replay().final_next_tick(), Tick(5));
    assert_eq!(full.replay().checkpoints().len(), 6);
    assert_eq!(half.replay().checkpoints().len(), 6);
    assert_eq!(
        full.replay()
            .checkpoints()
            .last()
            .expect("full C08 final checkpoint exists")
            .state_hash,
        expected_hash(C08_FULL_FINAL_HASH)
    );
    assert_eq!(
        half.replay()
            .checkpoints()
            .last()
            .expect("half C08 final checkpoint exists")
            .state_hash,
        expected_hash(C08_HALF_FINAL_HASH)
    );

    let full_headless = run_replay_file(fixture_path("s1-m2-c08-brownout-full-v1.json"))
        .expect("the full-Power C08 Replay runs headlessly");
    let half_headless = run_replay_file(fixture_path("s1-m2-c08-brownout-half-v1.json"))
        .expect("the half-Power C08 Replay runs headlessly");
    assert_eq!(
        full_headless.final_hash(),
        expected_hash(C08_FULL_FINAL_HASH)
    );
    assert_eq!(
        half_headless.final_hash(),
        expected_hash(C08_HALF_FINAL_HASH)
    );

    let mut full_simulation =
        Simulation::new(package(C08_FULL_SCENARIO)).expect("full C08 Simulation starts");
    let mut half_simulation =
        Simulation::new(package(C08_HALF_SCENARIO)).expect("half C08 Simulation starts");
    full.replay()
        .validate_against(&full_simulation)
        .expect("full C08 Replay matches its package");
    half.replay()
        .validate_against(&half_simulation)
        .expect("half C08 Replay matches its package");
    let mut full_trace = vec![full_simulation.state_hash()];
    let mut half_trace = vec![half_simulation.state_hash()];

    while full_simulation.next_tick() < Tick(5) {
        let target_tick = full_simulation.next_tick();
        assert_eq!(half_simulation.next_tick(), target_tick);
        let full_commands = full
            .replay()
            .commands_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        let half_commands = half
            .replay()
            .commands_for_tick(target_tick)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(full_commands, half_commands);
        let full_report = full_simulation
            .step(&full_commands)
            .expect("full C08 Tick succeeds");
        let half_report = half_simulation
            .step(&half_commands)
            .expect("half C08 Tick succeeds");
        full_trace.push(full_report.state_hash);
        half_trace.push(half_report.state_hash);

        let expected_full_movement = Fixed(FIXED_ONE);
        let expected_half_movement = Fixed(FIXED_ONE / 2);
        if target_tick >= Tick(1) {
            assert_eq!(full_report.mobile_movements.len(), 1);
            assert_eq!(half_report.mobile_movements.len(), 1);
        }
        if target_tick >= Tick(2) {
            assert_eq!(
                full_report.mobile_movements[0].granted_budget,
                expected_full_movement
            );
            assert_eq!(
                half_report.mobile_movements[0].granted_budget,
                expected_half_movement
            );
            assert_eq!(full_report.mobile_movements[0].mobile, C08_MOBILE);
            assert_eq!(half_report.mobile_movements[0].mobile, C08_MOBILE);
        }

        if target_tick == Tick(2) {
            let full_power = full_report.power.expect("full C08 Power report exists");
            let half_power = half_report.power.expect("half C08 Power report exists");
            assert_eq!(full_power.regions.len(), 1);
            assert_eq!(half_power.regions.len(), 1);
            assert_eq!(full_power.regions[0].generation, Energy(51));
            assert_eq!(half_power.regions[0].generation, Energy(24));
            assert_eq!(full_power.regions[0].total_nominal_demand, Energy(51));
            assert_eq!(half_power.regions[0].total_nominal_demand, Energy(51));
            assert_eq!(full_power.regions[0].ratio, PowerRatio::ONE);
            assert_eq!(
                half_power.regions[0].ratio,
                PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("one half is a valid ratio")
            );
            assert_eq!(full_power.loads.len(), 9);
            assert_eq!(half_power.loads.len(), 9);
            assert!(
                full_power
                    .loads
                    .iter()
                    .all(|load| load.ratio == PowerRatio::ONE)
            );
            assert!(half_power.loads.iter().all(|load| {
                load.ratio
                    == PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("one half is a valid ratio")
            }));
            for demand in [
                DemandId::new(C08_GATE.entity_id(), DemandKind::GateIdle),
                DemandId::new(C08_GATE.entity_id(), DemandKind::GateSwitch),
                DemandId::new(C08_SENSED_WIRE.entity_id(), DemandKind::WireSensing),
                DemandId::new(C08_MOBILE.entity_id(), DemandKind::Movement),
            ] {
                assert_eq!(
                    full_power
                        .load(demand)
                        .expect("full C08 runtime load exists")
                        .ratio,
                    PowerRatio::ONE
                );
                assert_eq!(
                    half_power
                        .load(demand)
                        .expect("half C08 runtime load exists")
                        .ratio,
                    PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("one half is a valid ratio")
                );
            }
            assert_eq!(scale_work(Energy(8), PowerRatio::ONE), Ok(Energy(8)));
            assert_eq!(
                scale_work(
                    Energy(8),
                    PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("one half is a valid ratio")
                ),
                Ok(Energy(4))
            );

            let full_gate = full_simulation
                .gate_signal_state(C08_GATE)
                .expect("full C08 Gate exists");
            let half_gate = half_simulation
                .gate_signal_state(C08_GATE)
                .expect("half C08 Gate exists");
            assert_eq!(full_gate.pending_due_tick, Some(Tick(3)));
            assert_eq!(half_gate.pending_due_tick, Some(Tick(4)));
            assert!(half_gate.pending_due_tick > full_gate.pending_due_tick);
            let full_sense = full_simulation
                .wire_sense_state(C08_SENSED_WIRE)
                .expect("full C08 Sense exists");
            let half_sense = half_simulation
                .wire_sense_state(C08_SENSED_WIRE)
                .expect("half C08 Sense exists");
            assert_eq!(full_sense.intended_strength, DriveStrength(400));
            assert_eq!(half_sense.intended_strength, DriveStrength(200));
            assert!(half_sense.intended_strength < full_sense.intended_strength);
            assert!(expected_half_movement < expected_full_movement);
        }

        if target_tick == Tick(3) {
            let full_gate = full_simulation
                .gate_signal_state(C08_GATE)
                .expect("full C08 Gate exists");
            let half_gate = half_simulation
                .gate_signal_state(C08_GATE)
                .expect("half C08 Gate exists");
            assert_eq!(full_gate.current_output, LogicLevel::High);
            assert_eq!(half_gate.current_output, LogicLevel::Low);
            assert_eq!(half_gate.pending_due_tick, Some(Tick(4)));
            for (simulation, strength) in [
                (&full_simulation, DriveStrength(400)),
                (&half_simulation, DriveStrength(200)),
            ] {
                let sense = simulation
                    .wire_sense_state(C08_SENSED_WIRE)
                    .expect("C08 Sense remains live");
                for driver in [sense.ports.a, sense.ports.b] {
                    let sample = simulation
                        .driver_sample(driver)
                        .expect("C08 Sense Driver remains live");
                    assert_eq!(sample.level, LogicLevel::Low);
                    assert_eq!(sample.strength, strength);
                    assert_eq!(sample.revision, Revision(1));
                }
            }
        }

        if target_tick == Tick(4) {
            let full_output = full_simulation
                .gate_signal_state(C08_GATE)
                .expect("full C08 Gate exists")
                .ports
                .output;
            let half_output = half_simulation
                .gate_signal_state(C08_GATE)
                .expect("half C08 Gate exists")
                .ports
                .output;
            let full_sample = full_simulation
                .driver_sample(full_output)
                .expect("full Gate output exists");
            let half_sample = half_simulation
                .driver_sample(half_output)
                .expect("half Gate output exists");
            assert_eq!(full_sample.level, LogicLevel::High);
            assert_eq!(half_sample.level, LogicLevel::High);
            assert_eq!(full_sample.strength, DriveStrength(400));
            assert_eq!(half_sample.strength, DriveStrength(200));
        }
    }

    full.replay()
        .verify_trace(&full_trace)
        .expect("manual full C08 trace matches its checkpoints");
    half.replay()
        .verify_trace(&half_trace)
        .expect("manual half C08 trace matches its checkpoints");
    assert_eq!(full_trace, full_headless.checkpoints());
    assert_eq!(half_trace, half_headless.checkpoints());
}
