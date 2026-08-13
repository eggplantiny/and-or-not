use aon_headless::{load_package, load_replay, run_replay_file};
use aon_sim::{
    Capacity, CommandAcceptance, CommandRejection, CommandRejectionReason, ConnectionGeneration,
    ConstructionTarget, DemandId, DemandKind, DestructionKind, DestructionReport, DriverId,
    EndpointTarget, Energy, EntityId, FIXED_ONE, Fixed, FixedVec2, GateId, HeatEnergy,
    InteractionHeatKind, PowerRatio, PowerRegionId, ReplayArtifact, ReplayFormatVersion, Revision,
    RoutingDomain, RunEndCause, RunStatus, SignalArrivalKind, Simulation, SinkId, StateHash,
    StateHashVersion, Tick, WireId, WorldGeneratorVersion, decode_balance_profile,
    decode_replay_artifact, decode_scenario_manifest, encode_replay_artifact,
    required_construction_work,
};
use std::path::PathBuf;

const BALANCE: &[u8] =
    include_bytes!("../../../profiles/balance/s1-m4-construction-contact-damage-alpha.json");
const SCENARIO: &[u8] =
    include_bytes!("../../../fixtures/scenarios/s1-m4-construction-contact-damage-v1.json");
const PARTIAL: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m4/construction-partial-multibuilder-v1.json");
const FOUR: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m4/construction-four-targets-v1.json");
const C10: &[u8] = include_bytes!("../../../fixtures/replays/s1-m4/c10-contact-v1.json");
const C09: &[u8] = include_bytes!("../../../fixtures/replays/s1-m4/c09-wire-break-v1.json");
const TERMINAL: &[u8] = include_bytes!("../../../fixtures/replays/s1-m4/terminal-v1.json");

const BALANCE_HASH: &str = "88b8fdc40dae59563699a0f611adae21c40d770d3d1c9076f8262a756107311a";
const SCENARIO_HASH: &str = "a9770d7afc466087664f44846d65f56e93d479738705975c10ab6527b59817cd";
const INITIAL_HASH: &str = "51ace8554724d927c81c68716d15cc58a4115959076d031688ef85915e960111";

const CASES: [(&str, u64, &str, &[u8]); 5] = [
    (
        "construction-partial-multibuilder-v1.json",
        5,
        "31a1089b727c09776cd66796f116b1e0397286604bf2ef261ac38c0bec68efe1",
        PARTIAL,
    ),
    (
        "construction-four-targets-v1.json",
        21,
        "0670210744f55ef99e67d4170546bcd88f8a90a9dbe506d54163601c93fee3de",
        FOUR,
    ),
    (
        "c10-contact-v1.json",
        9,
        "5ba4d9a856a765cd59a98592b82b9de256a389ece2aecfe6cd34ef0e26c4b420",
        C10,
    ),
    (
        "c09-wire-break-v1.json",
        52,
        "7452a1b72aa6622f8d894cd64866707ce6c7fdb3c2faf8efdcc4c6ee0c7a0bd4",
        C09,
    ),
    (
        "terminal-v1.json",
        56,
        "fe1000209769a38c50440dd1bbcfe70d19d2cb09529343125590b06e4e129777",
        TERMINAL,
    ),
];

fn replay(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/s1-m4")
        .join(name)
}

fn scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/scenarios/s1-m4-construction-contact-damage-v1.json")
}

fn hash(value: &str) -> StateHash {
    StateHash::from_hex(value).expect("retained State hashes are lowercase canonical hex")
}

fn artifact(bytes: &[u8]) -> ReplayArtifact {
    let artifact = decode_replay_artifact(bytes).expect("the retained Replay strictly decodes");
    assert_eq!(
        encode_replay_artifact(&artifact).expect("the retained Replay canonically re-encodes"),
        bytes
    );
    let header = artifact.replay().header();
    assert_eq!(header.format_version, ReplayFormatVersion::V2);
    assert_eq!(header.state_hash_version, StateHashVersion::V7);
    assert_eq!(
        header.world_generator_version,
        WorldGeneratorVersion::MainCorePowerEnemyV1
    );
    assert_eq!(header.balance_profile_hash.to_string(), BALANCE_HASH);
    assert_eq!(header.initial_state_hash, hash(INITIAL_HASH));
    artifact
}

#[derive(Default)]
struct C09Facts {
    revision: Option<Revision>,
    generation: Option<ConnectionGeneration>,
    capacity: Option<Capacity>,
    region: Option<PowerRegionId>,
    region_count: Option<usize>,
    source: Option<DriverId>,
    sink: Option<SinkId>,
}

fn assert_c09_tick(report: &aon_sim::StepReport, simulation: &Simulation, facts: &mut C09Facts) {
    let victim = WireId(EntityId(11));
    match report.completed_tick {
        Tick(45) => {
            assert_eq!(
                report
                    .damage
                    .iter()
                    .find(|row| row.target == victim.entity_id())
                    .map(|row| {
                        (
                            row.electrical_exposure,
                            row.integrity_before.0,
                            row.integrity_after.0,
                            row.pending_destruction,
                        )
                    }),
                Some((Energy(10), 10, 0, true))
            );
            assert!(report.signal_arrivals.is_empty());
            let mut snapshot = aon_sim::RenderSnapshot::default();
            simulation.write_render_snapshot(&mut snapshot);
            let wire = snapshot
                .wires()
                .iter()
                .find(|row| row.id == victim)
                .expect("the pending C-09 Wire remains on its Track surface");
            assert_eq!(wire.connection_generation, ConnectionGeneration::INITIAL);
            assert!(simulation.wire_signal_state(victim).is_some());
            assert!(simulation.wire_sense_state(victim).is_some());

            let accounting = report
                .network_accounting
                .expect("the pending C-09 Tick reports Capacity");
            let network = simulation
                .network_analyzer_snapshot()
                .expect("the C-09 Capacity analyzer succeeds")
                .expect("Capacity is enabled");
            assert!(network.wires().iter().any(|row| {
                row.wire() == victim
                    && row.length()
                        == Capacity(u64::try_from(20 * FIXED_ONE).expect("20 WU is positive"))
            }));
            let power = report
                .power
                .as_ref()
                .expect("the pending Tick reports Power");
            let victim_load = power
                .load(DemandId::new(victim.entity_id(), DemandKind::WireLeakage))
                .expect("the pending Wire remains in Power");
            assert_eq!(victim_load.granted, Energy(0));
            let source = simulation
                .gate_signal_ports(GateId(EntityId(9)))
                .expect("the source Gate exists")
                .output;
            let sink = simulation
                .gate_signal_ports(GateId(EntityId(10)))
                .expect("the downstream Gate exists")
                .input_a
                .sink;
            assert!(simulation.sink_driver_sample(sink, source).is_some());
            facts.revision = Some(simulation.topology_revision());
            facts.generation = Some(wire.connection_generation);
            facts.capacity = Some(accounting.used());
            facts.region = Some(victim_load.region);
            facts.region_count = Some(power.regions.len());
            facts.source = Some(source);
            facts.sink = Some(sink);
        }
        Tick(46) => {
            assert_eq!(
                report.destructions,
                [DestructionReport {
                    target: victim.entity_id(),
                    kind: DestructionKind::Damage,
                }]
            );
            assert!(report.topology_changed);
            assert_eq!(
                simulation.topology_revision(),
                Revision(facts.revision.expect("pending revision").0 + 1)
            );
            assert_eq!(facts.generation, Some(ConnectionGeneration::INITIAL));
            assert_eq!(simulation.wire_signal_state(victim), None);
            assert_eq!(simulation.wire_sense_state(victim), None);
            let mut snapshot = aon_sim::RenderSnapshot::default();
            simulation.write_render_snapshot(&mut snapshot);
            assert!(snapshot.wires().iter().all(|row| row.id != victim));
            assert!(snapshot.mobiles().is_empty());
            assert_eq!(
                report.command_acceptances,
                [CommandAcceptance {
                    target_tick: Tick(46),
                    ordinal: 0,
                    created_entity: Some(EntityId(13)),
                }]
            );
            assert_eq!(
                report.command_rejections,
                [CommandRejection {
                    target_tick: Tick(46),
                    ordinal: 1,
                    reason: CommandRejectionReason::UnsupportedPlacement,
                }]
            );
            assert!(
                snapshot
                    .junctions()
                    .iter()
                    .any(|row| row.id.entity_id() == EntityId(13))
            );
            assert_ne!(EntityId(13), victim.entity_id());

            let before = facts.capacity.expect("pending Capacity");
            assert_eq!(
                report
                    .network_accounting
                    .expect("the removal Tick reports Capacity")
                    .used(),
                Capacity(before.0 - u64::try_from(20 * FIXED_ONE).expect("20 WU is positive"))
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
            assert_eq!(power.regions.len().checked_add(1), facts.region_count);
            assert!(
                power
                    .regions
                    .iter()
                    .all(|region| Some(region.region) != facts.region)
            );
            assert!(simulation.construction_sites().is_empty());
            assert_eq!(
                simulation.sink_driver_sample(
                    facts.sink.expect("retained downstream Sink"),
                    facts.source.expect("retained source Driver"),
                ),
                None
            );
        }
        Tick(51) => {
            assert_eq!(report.signal_counters.invalid_path_arrivals, 1);
            assert_eq!(report.signal_counters.signal_arrivals_applied, 0);
            assert!(report.signal_arrivals.iter().any(|arrival| {
                arrival.due_tick == Tick(51)
                    && arrival.kind == SignalArrivalKind::Propagation
                    && Some(arrival.source_driver) == facts.source
                    && Some(arrival.sink) == facts.sink
            }));
        }
        _ => {}
    }
}

#[test]
fn retained_s1m4_set_is_exact_headlessly() {
    let balance =
        decode_balance_profile(BALANCE).expect("the retained Balance v5 strictly decodes");
    assert_eq!(
        balance
            .canonical_hash()
            .expect("the retained Balance v5 hashes")
            .to_string(),
        BALANCE_HASH
    );
    assert_eq!(
        decode_scenario_manifest(SCENARIO)
            .expect("the retained Scenario v4 strictly decodes")
            .canonical_hash()
            .expect("the retained Scenario v4 hashes")
            .to_string(),
        SCENARIO_HASH
    );
    let mut scenario_reencoded = serde_json::to_vec_pretty(
        &serde_json::from_slice::<serde_json::Value>(SCENARIO)
            .expect("the retained Scenario is strict JSON"),
    )
    .expect("the retained Scenario JSON re-encodes");
    scenario_reencoded.push(b'\n');
    assert_eq!(scenario_reencoded, SCENARIO);

    let expected_scenario = scenario()
        .canonicalize()
        .expect("the retained Scenario exists");
    for (name, completed_ticks, final_hash, bytes) in CASES {
        let path = replay(name);
        let retained = artifact(bytes);
        assert_eq!(
            retained.scenario_path(),
            "../../scenarios/s1-m4-construction-contact-damage-v1.json"
        );
        let resolved = path
            .parent()
            .expect("Replay has a parent")
            .join(retained.scenario_path());
        assert!(resolved.is_file(), "the Replay Scenario locator resolves");
        assert_eq!(
            resolved
                .canonicalize()
                .expect("the referenced Scenario canonicalizes"),
            expected_scenario
        );
        assert_eq!(
            load_replay(&path).expect("the file Replay strictly decodes"),
            retained
        );
        assert_eq!(retained.replay().final_next_tick(), Tick(completed_ticks));
        assert_eq!(
            retained
                .replay()
                .checkpoints()
                .last()
                .expect("the Replay has a final checkpoint")
                .state_hash,
            hash(final_hash)
        );

        let headless = run_replay_file(&path).expect("the retained S1-M4 Replay runs");
        assert_eq!(
            headless.scenario_id(),
            "s1-m4-construction-contact-damage-v1"
        );
        assert_eq!(headless.completed_ticks(), completed_ticks);
        assert_eq!(headless.checkpoints()[0], hash(INITIAL_HASH));
        assert_eq!(headless.final_hash(), hash(final_hash));
        assert_eq!(
            headless.reports().len(),
            usize::try_from(completed_ticks).expect("retained Tick count fits usize")
        );

        let package = load_package(&resolved).expect("the retained Scenario package loads");
        let mut simulation = Simulation::new(package).expect("the direct Simulation starts");
        retained
            .replay()
            .validate_against(&simulation)
            .expect("the Replay matches the direct Simulation");
        let mut direct_trace = vec![simulation.state_hash()];
        let mut c09 = C09Facts::default();
        let mut terminal_attack_ticks = Vec::new();
        while simulation.next_tick() < retained.replay().final_next_tick() {
            let tick = simulation.next_tick();
            let commands = retained
                .replay()
                .commands_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            let world_inputs = retained
                .replay()
                .world_inputs_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            let report = simulation
                .step_with_world_inputs(&commands, &world_inputs)
                .expect("the direct retained Tick succeeds");
            let index = usize::try_from(tick.0).expect("retained Tick fits usize");
            assert_eq!(&report, &headless.reports()[index]);
            assert_eq!(report.state_hash, headless.checkpoints()[index + 1]);
            direct_trace.push(report.state_hash);

            if name == "c09-wire-break-v1.json" {
                assert_c09_tick(&report, &simulation, &mut c09);
            }
            if name == "terminal-v1.json"
                && report.damage.iter().any(|row| row.target == EntityId(1))
            {
                let core = report
                    .damage
                    .iter()
                    .find(|row| row.target == EntityId(1))
                    .expect("the terminal Core row exists");
                assert_eq!(core.electrical_exposure, Energy(10));
                terminal_attack_ticks.push(report.completed_tick);
            }
        }
        retained
            .replay()
            .verify_trace(&direct_trace)
            .expect("the direct trace matches every retained V7 checkpoint");
        assert_eq!(direct_trace, headless.checkpoints());
        if name == "terminal-v1.json" {
            assert_eq!(
                terminal_attack_ticks,
                (46..=55).map(Tick).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn retained_s1m4_reports_pin_construction_c10_c09_and_terminal_facts() {
    let partial = run_replay_file(replay("construction-partial-multibuilder-v1.json"))
        .expect("partial Construction Replay runs");
    let report = &partial.reports()[3];
    let rows = &report.construction_work;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .map(|row| {
                (
                    row.site.entity_id(),
                    row.builder.entity_id(),
                    row.requested,
                    row.nominal_power,
                    row.granted_work,
                    row.applied_work,
                    row.completed_work,
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                EntityId(12),
                EntityId(10),
                Energy(8),
                Energy(8),
                Energy(4),
                Energy(4),
                Energy(4)
            ),
            (
                EntityId(12),
                EntityId(11),
                Energy(8),
                Energy(8),
                Energy(4),
                Energy(4),
                Energy(8)
            ),
        ]
    );
    let partial_ratio = PowerRatio::new(Fixed(36_864)).expect("the retained ratio is valid");
    let power = report
        .power
        .as_ref()
        .expect("partial Construction reports Power");
    for builder in [EntityId(10), EntityId(11)] {
        let load = power
            .load(DemandId::new(builder, DemandKind::Construction))
            .expect("each builder owns a Construction load");
        assert_eq!(
            (load.nominal, load.granted, load.ratio),
            (Energy(8), Energy(4), partial_ratio)
        );
    }

    let four = run_replay_file(replay("construction-four-targets-v1.json"))
        .expect("four-target Construction Replay runs");
    assert_eq!(
        four.reports()[13]
            .network_accounting
            .expect("completion reports Capacity")
            .used(),
        Capacity(u64::try_from(FIXED_ONE).expect("WU is positive"))
    );
    assert_eq!(
        four.reports()[14]
            .network_accounting
            .expect("activation reports Capacity")
            .used(),
        Capacity(2 * u64::try_from(FIXED_ONE).expect("WU is positive"))
    );
    assert_eq!(
        [3_usize, 8, 13, 18]
            .into_iter()
            .map(|tick| four.reports()[tick].construction_work[0].requested)
            .collect::<Vec<_>>(),
        [Energy(8), Energy(4), Energy(3), Energy(1)]
    );

    let probe = decode_balance_profile(BALANCE)
        .expect("the retained Balance decodes")
        .construction_probe
        .expect("the retained Construction probe exists");
    let wire = |points| ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points,
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    let short = wire(vec![
        FixedVec2::new(Fixed(0), Fixed(0)),
        FixedVec2::new(Fixed(FIXED_ONE), Fixed(0)),
    ]);
    let long = wire(vec![
        FixedVec2::new(Fixed(0), Fixed(0)),
        FixedVec2::new(Fixed(FIXED_ONE + 1), Fixed(0)),
    ]);
    let redundant = wire(vec![
        FixedVec2::new(Fixed(0), Fixed(0)),
        FixedVec2::new(Fixed(FIXED_ONE / 2), Fixed(0)),
        FixedVec2::new(Fixed(FIXED_ONE + 1), Fixed(0)),
    ]);
    assert_eq!(
        (
            required_construction_work(&short, &probe),
            required_construction_work(&long, &probe),
            required_construction_work(&redundant, &probe),
        ),
        (Ok(Energy(3)), Ok(Energy(4)), Ok(Energy(4)))
    );

    let c10 = run_replay_file(replay("c10-contact-v1.json")).expect("C-10 Replay runs");
    let report = &c10.reports()[8];
    let live_demand = DemandId::new(EntityId(11), DemandKind::LiveWire);
    let live = report
        .power
        .as_ref()
        .expect("C-10 reports Power")
        .load(live_demand)
        .expect("C-10 reports Live Wire demand");
    assert_eq!(
        (live.nominal, live.granted, live.ratio),
        (Energy(20), Energy(20), PowerRatio::ONE)
    );
    assert_eq!(
        report
            .contacts
            .iter()
            .map(|row| (row.wire, row.target.entity_id(), row.weight, row.absorbed))
            .collect::<Vec<_>>(),
        [
            (WireId(EntityId(11)), EntityId(5), 1, Energy(5)),
            (WireId(EntityId(11)), EntityId(6), 1, Energy(5)),
        ]
    );
    let remainder = report
        .interaction_heat
        .iter()
        .find(|row| row.kind == InteractionHeatKind::LiveWireRemainder)
        .expect("C-10 reports its contact remainder");
    assert_eq!(
        (remainder.owner, remainder.demand, remainder.energy),
        (EntityId(11), Some(live_demand), HeatEnergy(10))
    );
    assert_eq!(
        report
            .contacts
            .iter()
            .map(|row| row.absorbed.0)
            .sum::<u64>()
            + remainder.energy.0,
        live.granted.0
    );

    let c09 = run_replay_file(replay("c09-wire-break-v1.json")).expect("C-09 Replay runs");
    assert_eq!(
        c09.reports()[45]
            .damage
            .iter()
            .find(|row| row.target == EntityId(11))
            .map(|row| (
                row.electrical_exposure,
                row.integrity_after.0,
                row.pending_destruction
            )),
        Some((Energy(10), 0, true))
    );
    assert_eq!(
        c09.reports()[46].destructions,
        [DestructionReport {
            target: EntityId(11),
            kind: DestructionKind::Damage,
        }]
    );
    assert_eq!(c09.reports()[51].signal_counters.invalid_path_arrivals, 1);
    assert_eq!(c09.reports()[51].signal_counters.signal_arrivals_applied, 0);

    let terminal = run_replay_file(replay("terminal-v1.json")).expect("terminal Replay runs");
    assert_eq!(
        terminal.reports()[55].run_status,
        RunStatus::Ended {
            completed_tick: Tick(55),
            cause: RunEndCause::MainCoreDestroyed
        }
    );
}
