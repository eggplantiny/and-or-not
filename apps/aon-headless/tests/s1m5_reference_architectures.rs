use aon_headless::{load_package, load_replay, run_replay_file};
use aon_sim::{
    DestructionKind, GatePort, GateType, InitialWorld, PowerRatio, ReferenceArchitectureArtifact,
    ReferenceArchitectureBindingEndpoint, ReferenceArchitectureEndpoint,
    ReferenceArchitectureFormatVersion, ReferenceArchitectureLocalId,
    ReferenceArchitectureMaterializationBatchKind, ReferenceArchitectureOperation,
    ReferenceArchitectureRole, ReferenceArchitectureRoutingDomain,
    ReferenceArchitectureScenarioResolution, ReferenceArchitectureSemanticTarget,
    ReferenceMetricArtifact, ReferenceMetricBoundaries, ReferenceMetricCollector,
    ReferenceMetricTickSample, ReferencePairFairnessInput, ReferenceResponseLatency,
    RenderSnapshot, Replay, RunStatus, Seed, Simulation, Tick, WireEnd,
    decode_reference_architecture_artifact, decode_reference_experiment_plan_v2,
    decode_reference_metric_artifact, decode_reference_metric_set_artifact,
    decode_reference_pair_manifest, decode_scenario_manifest, derive_reference_static_inventory,
    encode_reference_architecture_artifact, encode_reference_experiment_plan_v2,
    encode_reference_metric_artifact, encode_reference_metric_set_artifact,
    encode_reference_pair_manifest, materialize_reference_architecture_pair,
    reduce_reference_metrics, reference_architecture_command_log_hash,
    resolve_reference_response_observations, validate_reference_metric_bindings,
    validate_reference_pair_fairness,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(relative: &str) -> PathBuf {
    repository_root().join(relative)
}

fn design_path(role: ReferenceArchitectureRole) -> PathBuf {
    fixture(match role {
        ReferenceArchitectureRole::Brute => "fixtures/designs/s1-m5-brute-v2.json",
        ReferenceArchitectureRole::Computed => "fixtures/designs/s1-m5-computed-v2.json",
    })
}

fn replay_path(role: ReferenceArchitectureRole) -> PathBuf {
    fixture(match role {
        ReferenceArchitectureRole::Brute => "fixtures/replays/s1-m5/brute-v1.json",
        ReferenceArchitectureRole::Computed => "fixtures/replays/s1-m5/computed-v1.json",
    })
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|error| panic!("fixture `{}` must resolve: {error}", path.display()))
}

fn render(simulation: &Simulation) -> RenderSnapshot {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
}

fn step_replay_tick(simulation: &mut Simulation, replay: &Replay) {
    let tick = simulation.next_tick();
    let commands = replay.commands_for_tick(tick).cloned().collect::<Vec<_>>();
    let inputs = replay
        .world_inputs_for_tick(tick)
        .cloned()
        .collect::<Vec<_>>();
    simulation
        .step_with_world_inputs(&commands, &inputs)
        .expect("the retained Replay Tick succeeds");
}

fn scenario_resolution(simulation: &Simulation) -> ReferenceArchitectureScenarioResolution {
    ReferenceArchitectureScenarioResolution {
        main_core: simulation
            .main_core_state()
            .expect("the S1-M5 Scenario has a Main Core")
            .id(),
        power_sources: simulation
            .power_sources()
            .map(|source| source.id())
            .collect(),
        enemies: simulation
            .enemies()
            .iter()
            .map(|enemy| enemy.id())
            .collect(),
    }
}

fn checked_add(total: &mut u128, value: u64) {
    *total = total
        .checked_add(u128::from(value))
        .expect("the bounded retained oracle sum fits u128");
}

fn channel_names(prefix: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for sector in ["west", "south", "north", "east"] {
        for channel in 0..4 {
            names.insert(format!("{prefix}.{sector}.{channel}"));
        }
    }
    names
}

fn sector_names(prefix: &str) -> BTreeSet<String> {
    ["west", "south", "north", "east"]
        .into_iter()
        .map(|sector| format!("{prefix}.{sector}"))
        .collect()
}

fn local_id(value: u32) -> ReferenceArchitectureLocalId {
    ReferenceArchitectureLocalId::new(value).expect("the retained local ID is nonzero")
}

fn q_at(origin: aon_sim::FixedVec2, x: i64, y: i64) -> aon_sim::FixedVec2 {
    let quantum = aon_sim::FIXED_ONE / 64;
    aon_sim::FixedVec2::new(
        aon_sim::Fixed(origin.x.0 + x * quantum),
        aon_sim::Fixed(origin.y.0 + y * quantum),
    )
}

fn cp_at(origin: aon_sim::FixedVec2, x: i64, y: i64) -> aon_sim::FixedVec2 {
    q_at(origin, x * 16, y * 16)
}

fn gate_endpoint(gate: u32, port: GatePort) -> ReferenceArchitectureEndpoint {
    ReferenceArchitectureEndpoint::GatePort {
        gate: local_id(gate),
        port,
    }
}

fn sensor_endpoint(wire: u32) -> ReferenceArchitectureEndpoint {
    ReferenceArchitectureEndpoint::WireSensePort {
        wire: local_id(wire),
        end: WireEnd::A,
    }
}

fn cardinal_sector(position: aon_sim::FixedVec2) -> &'static str {
    let x = i128::from(position.x.0);
    let y = i128::from(position.y.0);
    assert_ne!(
        x.abs(),
        y.abs(),
        "a cardinal fixture point has one dominant axis"
    );
    if x.abs() > y.abs() {
        if x < 0 { "west" } else { "east" }
    } else if y < 0 {
        "south"
    } else {
        "north"
    }
}

fn has_positive_collinear_overlap(
    first: (aon_sim::FixedVec2, aon_sim::FixedVec2),
    second: (aon_sim::FixedVec2, aon_sim::FixedVec2),
) -> bool {
    let cross = |a: aon_sim::FixedVec2, b: aon_sim::FixedVec2, c: aon_sim::FixedVec2| {
        let ab_x = i128::from(b.x.0) - i128::from(a.x.0);
        let ab_y = i128::from(b.y.0) - i128::from(a.y.0);
        let ac_x = i128::from(c.x.0) - i128::from(a.x.0);
        let ac_y = i128::from(c.y.0) - i128::from(a.y.0);
        ab_x * ac_y - ab_y * ac_x
    };
    if cross(first.0, first.1, second.0) != 0 || cross(first.0, first.1, second.1) != 0 {
        return false;
    }
    let use_x = first.0.x.0 != first.1.x.0;
    let interval = |segment: (aon_sim::FixedVec2, aon_sim::FixedVec2)| {
        let values = if use_x {
            (segment.0.x.0, segment.1.x.0)
        } else {
            (segment.0.y.0, segment.1.y.0)
        };
        (values.0.min(values.1), values.0.max(values.1))
    };
    let first_interval = interval(first);
    let second_interval = interval(second);
    first_interval.0.max(second_interval.0) < first_interval.1.min(second_interval.1)
}

fn wire_segments(wire: &aon_sim::ReferenceWire) -> Vec<(aon_sim::FixedVec2, aon_sim::FixedVec2)> {
    wire.points
        .windows(2)
        .map(|points| (points[0], points[1]))
        .collect()
}

fn scheduled_endpoint(wire: u32, end: WireEnd) -> ReferenceArchitectureBindingEndpoint {
    ReferenceArchitectureBindingEndpoint {
        wire: local_id(wire),
        end,
    }
}

fn retained_binding_schedule(
    role: ReferenceArchitectureRole,
    design: &ReferenceArchitectureArtifact,
) -> Vec<Vec<ReferenceArchitectureBindingEndpoint>> {
    let mut batches = vec![Vec::new(); 4];
    for operation in &design.operations {
        let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
            continue;
        };
        for (end, target) in [(WireEnd::A, wire.endpoint_a), (WireEnd::B, wire.endpoint_b)] {
            if target != ReferenceArchitectureEndpoint::Free
                && !matches!(target, ReferenceArchitectureEndpoint::PowerSource { .. })
            {
                batches[0].push(ReferenceArchitectureBindingEndpoint { wire: wire.id, end });
            }
        }
    }

    for sector in 0..4_u32 {
        match role {
            ReferenceArchitectureRole::Brute => {
                for channel in 0..4_u32 {
                    let base = 100 + sector * 100 + channel * 10;
                    batches[3].extend([
                        scheduled_endpoint(base, WireEnd::A),
                        scheduled_endpoint(base + 3, WireEnd::B),
                    ]);
                }
            }
            ReferenceArchitectureRole::Computed => {
                let sensor_base = 2000 + sector * 100;
                let wire_base = 4000 + sector * 100;
                for sensor in 0..4 {
                    batches[0].push(scheduled_endpoint(sensor_base + sensor, WireEnd::A));
                }
                batches[0].push(scheduled_endpoint(wire_base + 30, WireEnd::A));
                batches[1].extend([
                    scheduled_endpoint(wire_base + 31, WireEnd::A),
                    scheduled_endpoint(wire_base + 32, WireEnd::A),
                ]);
                batches[2].push(scheduled_endpoint(wire_base + 35, WireEnd::A));
                batches[3].extend([
                    scheduled_endpoint(wire_base + 33, WireEnd::A),
                    scheduled_endpoint(wire_base + 34, WireEnd::A),
                    scheduled_endpoint(wire_base + 36, WireEnd::A),
                    scheduled_endpoint(wire_base + 21, WireEnd::B),
                ]);
            }
        }
    }
    for batch in &mut batches {
        batch.sort_unstable();
    }
    batches
}

const REFERENCE_METRIC_NAMES_V1: [&str; 37] = [
    "totalWireLengthRaw",
    "totalWireNcu",
    "sharedWireLengthRaw",
    "sensorWireLengthRaw",
    "trunkWireLengthRaw",
    "defenseWireLengthRaw",
    "otherWireLengthRaw",
    "gateCount",
    "andCount",
    "orCount",
    "notCount",
    "plannedConstructionWork",
    "buildCommandCount",
    "commandLogHash",
    "survivedBoundary",
    "completedTicks",
    "terminalStatus",
    "measurementStartCoreIntegrity",
    "finalCoreIntegrity",
    "coreDamage",
    "powerGeneration",
    "powerNominalDemand",
    "powerGranted",
    "powerSourceCost",
    "powerTransmissionLoss",
    "brownoutTicks",
    "constructionRequested",
    "constructionNominalPower",
    "constructionGrantedWork",
    "constructionAppliedWork",
    "heatGenerated",
    "networkPeakUsedNcu",
    "networkFinalUsedNcu",
    "networkIntegralUsedNcu",
    "supportDemandIntegral",
    "enemyKills",
    "responseLatencyTicks",
];

#[test]
fn retained_s1m5_design_pair_plan_and_replays_are_strict_headless_artifacts() {
    let pair_path = fixture("fixtures/experiments/s1-m5-reference-pair-v1.json");
    let pair_bytes = fs::read(&pair_path).expect("the retained S1-M5 Pair exists");
    let pair = decode_reference_pair_manifest(&pair_bytes).expect("the Pair strictly decodes");
    assert_eq!(
        encode_reference_pair_manifest(&pair).expect("the Pair canonically encodes"),
        pair_bytes
    );
    assert_eq!(pair.build_end_tick(), Tick(18));
    assert_eq!(pair.measurement_start_tick(), Tick(18));
    assert_eq!(pair.max_ticks(), Tick(20));

    let plan_path = fixture("fixtures/experiments/s1-m5-reference-plan-v2.json");
    let plan_bytes = fs::read(&plan_path).expect("the retained Experiment Plan v2 exists");
    let plan = decode_reference_experiment_plan_v2(&plan_bytes)
        .expect("the retained Experiment Plan v2 strictly decodes");
    assert_eq!(
        encode_reference_experiment_plan_v2(&plan)
            .expect("the Experiment Plan v2 canonically encodes"),
        plan_bytes
    );
    plan.validate_against_pair(&pair)
        .expect("the retained Plan binds the exact Pair");
    assert_eq!(
        canonical(&plan_path.parent().unwrap().join(plan.pair().path())),
        canonical(&pair_path),
        "the Plan Pair locator must resolve to the exact retained artifact"
    );
    let runs = plan
        .resolve(&pair)
        .expect("the exact two-run plan resolves");
    assert_ne!(runs[0].run_id, runs[1].run_id);

    let scenario_path = fixture("fixtures/scenarios/s1-m5-reference-architectures-v1.json");
    let scenario_bytes = fs::read(&scenario_path).expect("the retained S1-M5 Scenario exists");
    let scenario =
        decode_scenario_manifest(&scenario_bytes).expect("the retained Scenario strictly decodes");
    assert_eq!(
        scenario
            .canonical_hash()
            .expect("the retained Scenario hashes"),
        pair.scenario().artifact_hash()
    );
    let fairness_package =
        load_package(&scenario_path).expect("the retained Scenario package loads for fairness");
    validate_reference_pair_fairness(
        &pair,
        ReferencePairFairnessInput {
            scenario: &scenario,
            contract: *fairness_package.contract(),
            profiles: fairness_package.profiles(),
            build_end_tick: pair.build_end_tick(),
            measurement_start_tick: pair.measurement_start_tick(),
            max_ticks: pair.max_ticks(),
            main_core_capacity: fairness_package
                .profiles()
                .balance
                .capacity_probe
                .expect("the retained Balance has Capacity")
                .main_core_capacity,
            territory: pair.territory(),
            shared_command_log_hash: reference_architecture_command_log_hash(&[])
                .expect("the canonical empty shared Command Log hashes"),
            seed: Seed::ZERO,
            metric_set_id: pair.metric_set_id(),
            metric_set_hash: pair.metric_set_hash(),
        },
    )
    .expect("the retained Pair independently satisfies every shared fairness input");

    let metric_set_path = fixture("fixtures/metrics/s1-m5/reference-metric-set-v1.json");
    let metric_set_bytes =
        fs::read(&metric_set_path).expect("the retained S1-M5 Metric Set exists");
    let metric_set = decode_reference_metric_set_artifact(&metric_set_bytes)
        .expect("the retained Metric Set strictly decodes");
    assert_eq!(
        encode_reference_metric_set_artifact(&metric_set)
            .expect("the retained Metric Set canonically encodes"),
        metric_set_bytes
    );
    assert_eq!(metric_set.metric_set_id(), pair.metric_set_id());
    assert_eq!(
        metric_set
            .metrics()
            .iter()
            .map(|metric| metric.as_str())
            .collect::<Vec<_>>(),
        REFERENCE_METRIC_NAMES_V1,
        "the retained Metric Set declares exactly the 37 frozen metric names in tag order"
    );
    let expected_response_rows = [
        ("east.0", "sensor.east.0", "defense.east.0", 3),
        ("north.0", "sensor.north.0", "defense.north.0", 2),
        ("south.0", "sensor.south.0", "defense.south.0", 1),
        ("west.0", "sensor.west.0", "defense.west.0", 0),
    ];
    assert_eq!(metric_set.response_observations().len(), 4);
    assert_eq!(pair.response_bindings().len(), 4);
    for ((metric_row, pair_row), expected) in metric_set
        .response_observations()
        .iter()
        .zip(pair.response_bindings())
        .zip(expected_response_rows)
    {
        assert_eq!(metric_row.name, expected.0);
        assert_eq!(metric_row.hostile_entry_binding, expected.1);
        assert_eq!(metric_row.defense_contact_binding, expected.2);
        assert_eq!(metric_row.enemy_ordinal, expected.3);
        assert_eq!(pair_row.name, expected.0);
        assert_eq!(pair_row.hostile_entry_binding, expected.1);
        assert_eq!(pair_row.defense_contact_binding, expected.2);
    }
    let InitialWorld::MainCorePowerEnemyV1 {
        power_sources,
        enemies,
        ..
    } = scenario.initial_world()
    else {
        panic!("the retained S1-M5 Scenario has the frozen Enemy world");
    };
    let mut ordered_enemies = enemies.iter().collect::<Vec<_>>();
    ordered_enemies.sort_unstable_by_key(|enemy| {
        (
            enemy.position().x.0,
            enemy.position().y.0,
            enemy.velocity_per_tick().x.0,
            enemy.velocity_per_tick().y.0,
            enemy.radius().0,
            enemy.integrity().0,
            enemy.heat_energy().0,
        )
    });
    let expected_semantic_sectors = ["west", "south", "north", "east"];
    assert_eq!(ordered_enemies.len(), expected_semantic_sectors.len());
    for (ordinal, (enemy, expected_sector)) in ordered_enemies
        .into_iter()
        .zip(expected_semantic_sectors)
        .enumerate()
    {
        assert_eq!(
            cardinal_sector(enemy.position()),
            expected_sector,
            "Scenario semantic Enemy ordinal {ordinal} retains the `{expected_sector}` sector"
        );
        let expected_row_name = format!("{expected_sector}.0");
        let row = metric_set
            .response_observations()
            .iter()
            .find(|row| row.name == expected_row_name)
            .expect("each Scenario sector has one response row");
        assert_eq!(usize::try_from(row.enemy_ordinal).unwrap(), ordinal);
        let source = power_sources
            .iter()
            .find(|source| cardinal_sector(source.position()) == expected_sector)
            .expect("each semantic sector has one Source anchor");
        assert_eq!(
            enemy.position(),
            q_at(source.position(), 34, -35),
            "Scenario Enemy ordinal {ordinal} retains local p0=q(34,-35)"
        );
        assert_eq!(
            enemy.velocity_per_tick(),
            q_at(
                aon_sim::FixedVec2::new(aon_sim::Fixed::ZERO, aon_sim::Fixed::ZERO),
                -1,
                1,
            ),
            "Scenario Enemy ordinal {ordinal} retains velocity=q(-1,+1)"
        );
    }
    assert_eq!(
        metric_set.semantic_hash().expect("the Metric Set hashes"),
        pair.metric_set_hash()
    );
    let brute_source = fs::read_to_string(design_path(ReferenceArchitectureRole::Brute))
        .expect("the retained Brute design exists and is UTF-8");
    let brute = decode_reference_architecture_artifact(&brute_source)
        .expect("the retained Brute design strictly decodes");
    let computed_source = fs::read_to_string(design_path(ReferenceArchitectureRole::Computed))
        .expect("the retained Computed design exists and is UTF-8");
    let computed = decode_reference_architecture_artifact(&computed_source)
        .expect("the retained Computed design strictly decodes");
    validate_reference_metric_bindings(&pair, &metric_set, &brute, &computed)
        .expect("the Pair and Metric Set resolve the same sensor/defense roles in both designs");
    assert_eq!(
        canonical(&pair_path.parent().unwrap().join(pair.scenario().path())),
        canonical(&scenario_path),
        "the Pair Scenario locator must resolve to the exact retained artifact"
    );

    for role in [
        ReferenceArchitectureRole::Brute,
        ReferenceArchitectureRole::Computed,
    ] {
        let retained_design_path = design_path(role);
        let source = fs::read_to_string(&retained_design_path)
            .expect("the retained Reference Architecture exists and is UTF-8");
        let design = decode_reference_architecture_artifact(&source)
            .expect("the retained Reference Architecture strictly decodes");
        assert_eq!(
            encode_reference_architecture_artifact(&design)
                .expect("the retained Reference Architecture canonically encodes"),
            source
        );
        let binding = pair
            .designs()
            .iter()
            .find(|binding| binding.role == role)
            .expect("the Pair contains each architecture role once");
        assert_eq!(
            design.semantic_hash().expect("the design hashes"),
            binding.design.artifact_hash()
        );
        assert_eq!(
            canonical(&pair_path.parent().unwrap().join(binding.design.path())),
            canonical(&retained_design_path),
            "the Pair design locator must resolve to the exact retained artifact"
        );

        let retained_replay_path = replay_path(role);
        let replay =
            load_replay(&retained_replay_path).expect("the retained Replay strictly decodes");
        assert_eq!(
            reference_architecture_command_log_hash(replay.replay().commands())
                .expect("the complete design Command Log hashes"),
            binding.command_log_hash
        );
        let replay_scenario = retained_replay_path
            .parent()
            .expect("the Replay has a parent")
            .join(replay.scenario_path());
        assert_eq!(canonical(&replay_scenario), canonical(&scenario_path));

        let metric_path = fixture(match role {
            ReferenceArchitectureRole::Brute => "fixtures/metrics/s1-m5/brute-v1.json",
            ReferenceArchitectureRole::Computed => "fixtures/metrics/s1-m5/computed-v1.json",
        });
        let metric_bytes = fs::read(&metric_path).expect("the retained Metric Artifact exists");
        let metric = decode_reference_metric_artifact(&metric_bytes, &metric_set)
            .expect("the retained Metric Artifact strictly decodes");
        assert_eq!(
            encode_reference_metric_artifact(&metric, &metric_set)
                .expect("the retained Metric Artifact canonically encodes"),
            metric_bytes
        );
        let run = runs
            .iter()
            .find(|run| run.design.role == role)
            .expect("the Plan resolves each architecture role once");
        assert_eq!(metric.run_id, run.run_id);
    }
}

#[test]
fn retained_s1m5_brute_and_computed_structural_oracles_are_exact() {
    let pair_bytes = fs::read(fixture("fixtures/experiments/s1-m5-reference-pair-v1.json"))
        .expect("the retained S1-M5 Pair exists");
    let pair = decode_reference_pair_manifest(&pair_bytes).expect("the retained Pair decodes");
    let sensor_names = channel_names("sensor");

    for role in [
        ReferenceArchitectureRole::Brute,
        ReferenceArchitectureRole::Computed,
    ] {
        let source = fs::read_to_string(design_path(role))
            .expect("the retained architecture exists and is UTF-8");
        let design = decode_reference_architecture_artifact(&source)
            .expect("the retained architecture strictly decodes");
        let wires = design
            .operations
            .iter()
            .filter_map(|operation| match operation {
                ReferenceArchitectureOperation::PlaceWire(wire) => Some((wire.id, wire)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let wire_ids = wires.keys().copied().collect::<BTreeSet<_>>();
        let gates = design
            .operations
            .iter()
            .filter_map(|operation| match operation {
                ReferenceArchitectureOperation::PlaceGate(gate) => Some((gate.id, gate)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let junctions = design
            .operations
            .iter()
            .filter_map(|operation| match operation {
                ReferenceArchitectureOperation::PlaceJunction(junction) => {
                    Some((junction.id, junction))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let fixed_substrates = design
            .operations
            .iter()
            .filter_map(|operation| match operation {
                ReferenceArchitectureOperation::PlaceFixedSubstrate(substrate) => {
                    Some((substrate.id, substrate))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let mobile_substrate_count = design
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation,
                    ReferenceArchitectureOperation::PlaceMobileSubstrate(_)
                )
            })
            .count();
        assert_eq!(
            design.format_version,
            ReferenceArchitectureFormatVersion::V2,
            "both retained S1-M5 designs use the staged Architecture format"
        );
        let schedule = design
            .materialization_schedule
            .as_ref()
            .expect("a retained v2 design has a binding schedule");
        let expected_schedule = retained_binding_schedule(role, &design);
        assert_eq!(
            schedule.binding_batches, expected_schedule,
            "the retained {role:?} four-stage endpoint partition is exact"
        );
        let expected_batch_lengths: &[usize] = match role {
            ReferenceArchitectureRole::Brute => &[48, 0, 0, 32],
            ReferenceArchitectureRole::Computed => &[156, 8, 4, 16],
        };
        assert_eq!(
            schedule
                .binding_batches
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            expected_batch_lengths
        );
        let scheduled = schedule
            .binding_batches
            .iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            scheduled.len(),
            schedule.binding_batches.iter().map(Vec::len).sum::<usize>(),
            "a retained schedule covers each Wire end only once"
        );
        let expected_non_free = wires
            .values()
            .flat_map(|wire| {
                [(WireEnd::A, wire.endpoint_a), (WireEnd::B, wire.endpoint_b)]
                    .into_iter()
                    .filter_map(|(end, target)| {
                        (target != ReferenceArchitectureEndpoint::Free)
                            .then_some(ReferenceArchitectureBindingEndpoint { wire: wire.id, end })
                    })
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(scheduled, expected_non_free);
        let stage_zero = schedule.binding_batches[0]
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        for binding in &expected_non_free {
            let wire = wires[&binding.wire];
            let target = match binding.end {
                WireEnd::A => wire.endpoint_a,
                WireEnd::B => wire.endpoint_b,
            };
            if !matches!(target, ReferenceArchitectureEndpoint::PowerSource { .. }) {
                assert!(
                    stage_zero.contains(binding),
                    "every non-Source endpoint is bound in A0"
                );
            }
        }
        for binding in schedule.binding_batches.iter().skip(1).flatten() {
            let wire = wires[&binding.wire];
            let target = match binding.end {
                WireEnd::A => wire.endpoint_a,
                WireEnd::B => wire.endpoint_b,
            };
            assert!(matches!(
                target,
                ReferenceArchitectureEndpoint::PowerSource { .. }
            ));
        }
        let plan = design
            .materialization_plan()
            .expect("the retained staged design has a canonical plan");
        for (stage, expected_len) in expected_batch_lengths.iter().copied().enumerate() {
            assert_eq!(
                plan.batch(ReferenceArchitectureMaterializationBatchKind::Binding {
                    stage: u8::try_from(stage).expect("four stages fit u8"),
                })
                .expect("every v2 stage, including an empty one, is retained")
                .len(),
                expected_len
            );
        }
        let gate_count = |gate_type| {
            design
                .operations
                .iter()
                .filter(|operation| {
                    matches!(
                        operation,
                        ReferenceArchitectureOperation::PlaceGate(gate)
                            if gate.gate_type == gate_type
                    )
                })
                .count()
        };
        let names_with_prefix = |prefix: &str| {
            design
                .role_bindings
                .iter()
                .filter(|binding| binding.name.starts_with(prefix))
                .map(|binding| binding.name.clone())
                .collect::<BTreeSet<_>>()
        };
        let distinct_local_targets = |prefix: &str| {
            design
                .role_bindings
                .iter()
                .filter(|binding| binding.name.starts_with(prefix))
                .map(|binding| match binding.target {
                    ReferenceArchitectureSemanticTarget::LocalEntity(local_id) => local_id,
                    _ => panic!(
                        "the retained {role:?} mandatory role `{}` binds a local entity",
                        binding.name
                    ),
                })
                .collect::<BTreeSet<_>>()
        };
        let role_target = |name: &str| {
            let binding = design
                .role_bindings
                .iter()
                .find(|binding| binding.name == name)
                .unwrap_or_else(|| panic!("the retained {role:?} design binds `{name}`"));
            let ReferenceArchitectureSemanticTarget::LocalEntity(local_id) = binding.target else {
                panic!("the retained {role:?} role `{name}` binds a local entity");
            };
            local_id
        };
        let assert_wire_roles = |expected: &BTreeSet<String>| {
            for name in expected {
                let binding = design
                    .role_bindings
                    .iter()
                    .find(|binding| &binding.name == name)
                    .unwrap_or_else(|| panic!("the retained {role:?} design binds `{name}`"));
                let ReferenceArchitectureSemanticTarget::LocalEntity(local_id) = binding.target
                else {
                    panic!("the retained {role:?} role `{name}` binds a local entity");
                };
                assert!(
                    wire_ids.contains(&local_id),
                    "the retained {role:?} role `{name}` denotes a Wire"
                );
            }
        };

        let observation_names = design
            .observation_bindings
            .iter()
            .map(|binding| binding.name.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(observation_names, sensor_names);
        for observation in &design.observation_bindings {
            let ReferenceArchitectureSemanticTarget::WireSensePort { wire, .. } =
                observation.target
            else {
                panic!(
                    "the retained {role:?} sensor observation `{}` binds a Wire sense port",
                    observation.name
                );
            };
            assert!(wire_ids.contains(&wire));
        }
        assert_eq!(names_with_prefix("sensor."), sensor_names);
        assert_wire_roles(&sensor_names);
        assert_eq!(
            fixed_substrates.keys().copied().collect::<BTreeSet<_>>(),
            (1..=4).map(local_id).collect::<BTreeSet<_>>()
        );

        match role {
            ReferenceArchitectureRole::Brute => {
                let trunk_names = channel_names("trunk");
                let defense_names = channel_names("defense");
                assert_eq!(wire_ids.len(), 48);
                assert_eq!(junctions.len(), 16);
                assert_eq!(gates.len(), 0);
                assert_eq!(fixed_substrates.len(), 4);
                assert_eq!(mobile_substrate_count, 0);
                assert_eq!(design.operations.len(), 68);
                assert_eq!(names_with_prefix("trunk."), trunk_names);
                assert_eq!(names_with_prefix("defense."), defense_names);
                assert!(names_with_prefix("shared.").is_empty());
                assert_eq!(distinct_local_targets("sensor.").len(), 16);
                assert_eq!(distinct_local_targets("trunk.").len(), 16);
                assert_eq!(distinct_local_targets("defense.").len(), 16);
                assert_wire_roles(&trunk_names);
                assert_wire_roles(&defense_names);
                let all_mandatory_targets = sensor_names
                    .iter()
                    .chain(&trunk_names)
                    .chain(&defense_names)
                    .map(|name| role_target(name))
                    .collect::<BTreeSet<_>>();
                assert_eq!(all_mandatory_targets.len(), 48);
                let role_wire = |name: &str| {
                    wires
                        .get(&role_target(name))
                        .copied()
                        .expect("the Brute role target is a retained Wire")
                };
                let mut expected_wire_ids = BTreeSet::new();
                for (sector_ordinal, sector_name) in
                    ["west", "south", "north", "east"].into_iter().enumerate()
                {
                    let sector = u32::try_from(sector_ordinal).expect("four sectors");
                    let substrate = local_id(1 + sector);
                    let routing_domain =
                        ReferenceArchitectureRoutingDomain::FixedSubstrate(substrate);
                    let mut branch_quadrants = BTreeSet::new();
                    let mut sector_source = None;
                    for channel in 0..4_u32 {
                        let base = 100 + sector * 100 + channel * 10;
                        let sensor_id = local_id(base);
                        let trunk_id = local_id(base + 1);
                        let junction_id = local_id(base + 2);
                        let defense_id = local_id(base + 3);
                        expected_wire_ids.extend([sensor_id, trunk_id, defense_id]);

                        assert_eq!(
                            role_target(&format!("sensor.{sector_name}.{channel}")),
                            sensor_id
                        );
                        assert_eq!(
                            role_target(&format!("trunk.{sector_name}.{channel}")),
                            trunk_id
                        );
                        assert_eq!(
                            role_target(&format!("defense.{sector_name}.{channel}")),
                            defense_id
                        );

                        let observation = design
                            .observation_bindings
                            .iter()
                            .find(|binding| {
                                binding.name == format!("sensor.{sector_name}.{channel}")
                            })
                            .expect("each Brute sensor observation exists");
                        assert_eq!(
                            observation.target,
                            ReferenceArchitectureSemanticTarget::WireSensePort {
                                wire: sensor_id,
                                end: WireEnd::A,
                            }
                        );

                        let sensor = wires[&sensor_id];
                        assert_eq!(sensor.routing_domain, routing_domain);
                        assert_eq!(
                            sensor.endpoint_a,
                            ReferenceArchitectureEndpoint::PowerSource { ordinal: sector }
                        );
                        assert_eq!(sensor.endpoint_b, ReferenceArchitectureEndpoint::Free);
                        assert_eq!(sensor.points.len(), 2);
                        let source = sensor.points[0];
                        assert_eq!(*sector_source.get_or_insert(source), source);

                        let junction = junctions[&junction_id];
                        assert_eq!(junction.routing_domain, routing_domain);
                        let trunk = wires[&trunk_id];
                        assert_eq!(trunk.routing_domain, routing_domain);
                        assert_eq!(trunk.endpoint_a, sensor_endpoint(base));
                        assert_eq!(
                            trunk.endpoint_b,
                            ReferenceArchitectureEndpoint::Junction(junction_id)
                        );
                        assert_eq!(trunk.points.first().copied(), Some(source));
                        assert_eq!(trunk.points.last().copied(), Some(junction.position));

                        let defense = wires[&defense_id];
                        assert_eq!(defense.routing_domain, routing_domain);
                        assert_eq!(
                            defense.endpoint_a,
                            ReferenceArchitectureEndpoint::Junction(junction_id)
                        );
                        assert_eq!(
                            defense.endpoint_b,
                            ReferenceArchitectureEndpoint::PowerSource { ordinal: sector }
                        );
                        assert_eq!(defense.points.first().copied(), Some(junction.position));
                        assert_eq!(defense.points.last().copied(), Some(source));
                        if channel == 0 {
                            assert_eq!(
                                defense.points.as_slice(),
                                &[q_at(source, 128, 128), q_at(source, 80, -48), source,],
                                "Brute channel 0 retains the exact shared response corridor"
                            );
                        }
                        let delta_x = junction.position.x.0 - source.x.0;
                        let delta_y = junction.position.y.0 - source.y.0;
                        assert_ne!(delta_x, 0);
                        assert_ne!(delta_y, 0);
                        branch_quadrants.insert((delta_x.signum(), delta_y.signum()));
                    }
                    assert_eq!(
                        branch_quadrants,
                        BTreeSet::from([(-1, -1), (-1, 1), (1, -1), (1, 1)]),
                        "the four Brute defense ribs blanket all quadrants around {sector_name}"
                    );
                }
                assert_eq!(wire_ids, expected_wire_ids);
                let trunks = trunk_names
                    .iter()
                    .map(|name| role_wire(name))
                    .collect::<Vec<_>>();
                for (index, trunk) in trunks.iter().enumerate() {
                    assert!(matches!(
                        trunk.endpoint_a,
                        aon_sim::ReferenceArchitectureEndpoint::WireSensePort { .. }
                    ));
                    assert!(matches!(
                        trunk.endpoint_b,
                        aon_sim::ReferenceArchitectureEndpoint::Junction(_)
                    ));
                    for other in trunks.iter().skip(index + 1) {
                        assert!(
                            wire_segments(trunk).iter().all(|first| {
                                wire_segments(other)
                                    .iter()
                                    .all(|second| !has_positive_collinear_overlap(*first, *second))
                            }),
                            "distinct Brute channel trunks share no positive-length segment"
                        );
                    }
                }
                for name in &defense_names {
                    let defense = role_wire(name);
                    assert!(
                        defense.points.len() >= 3,
                        "each Brute defense is a bent rib"
                    );
                    assert!(matches!(
                        defense.endpoint_a,
                        aon_sim::ReferenceArchitectureEndpoint::Junction(_)
                    ));
                    assert!(matches!(
                        defense.endpoint_b,
                        aon_sim::ReferenceArchitectureEndpoint::PowerSource { .. }
                    ));
                    assert!(
                        wire_segments(defense)
                            .iter()
                            .all(|(start, end)| start != end),
                        "every Brute defense segment has positive length"
                    );
                }
                assert_eq!(gate_count(GateType::And), 0);
                assert_eq!(gate_count(GateType::Or), 0);
                assert_eq!(gate_count(GateType::Not), 0);
            }
            ReferenceArchitectureRole::Computed => {
                let shared_names = sector_names("shared");
                let defense_names = ["west", "south", "north", "east"]
                    .into_iter()
                    .map(|sector| format!("defense.{sector}.0"))
                    .collect::<BTreeSet<_>>();
                let state_names = ["west", "south", "north", "east"]
                    .into_iter()
                    .flat_map(|sector| {
                        ["q", "qbar"]
                            .into_iter()
                            .map(move |state| format!("state.{sector}.{state}"))
                    })
                    .collect::<BTreeSet<_>>();
                let reduction_names = ["west", "south", "north", "east"]
                    .into_iter()
                    .flat_map(|sector| {
                        ["pair0", "pair1", "presence"]
                            .into_iter()
                            .map(move |reduction| format!("reduction.{sector}.{reduction}"))
                    })
                    .collect::<BTreeSet<_>>();
                assert_eq!(wire_ids.len(), 100);
                assert_eq!(junctions.len(), 8);
                assert_eq!(gates.len(), 28);
                assert_eq!(fixed_substrates.len(), 4);
                assert_eq!(mobile_substrate_count, 0);
                assert_eq!(design.operations.len(), 140);
                assert_eq!(names_with_prefix("shared."), shared_names);
                assert_eq!(names_with_prefix("defense."), defense_names);
                assert_eq!(names_with_prefix("state."), state_names);
                assert_eq!(names_with_prefix("reduction."), reduction_names);
                assert!(names_with_prefix("trunk.").is_empty());
                assert_eq!(distinct_local_targets("sensor.").len(), 16);
                assert_eq!(distinct_local_targets("shared.").len(), 4);
                assert_eq!(distinct_local_targets("defense.").len(), 4);
                assert_eq!(distinct_local_targets("state.").len(), 8);
                assert_eq!(distinct_local_targets("reduction.").len(), 12);
                assert_wire_roles(&shared_names);
                assert_wire_roles(&defense_names);
                let all_mandatory_targets = sensor_names
                    .iter()
                    .chain(&shared_names)
                    .chain(&defense_names)
                    .chain(&state_names)
                    .chain(&reduction_names)
                    .map(|name| role_target(name))
                    .collect::<BTreeSet<_>>();
                assert_eq!(all_mandatory_targets.len(), 44);

                let mut expected_wire_ids = BTreeSet::new();
                let mut expected_gate_ids = BTreeSet::new();
                let mut expected_junction_ids = BTreeSet::new();
                for (sector_ordinal, sector_name) in
                    ["west", "south", "north", "east"].into_iter().enumerate()
                {
                    let sector = u32::try_from(sector_ordinal).expect("four sectors");
                    let substrate = local_id(1 + sector);
                    let routing_domain =
                        ReferenceArchitectureRoutingDomain::FixedSubstrate(substrate);
                    let origin = fixed_substrates[&substrate].origin;
                    let gate_base = 1000 + sector * 100;
                    let sensor_base = 2000 + sector * 100;
                    let junction_base = 3000 + sector * 100;
                    let wire_base = 4000 + sector * 100;

                    for (offset, gate_type, x, y) in [
                        (0, GateType::Or, 2, 3),
                        (1, GateType::Or, 2, -3),
                        (2, GateType::Or, 5, 3),
                        (3, GateType::Or, 8, -3),
                        (4, GateType::Not, 11, -3),
                        (5, GateType::Or, 8, 3),
                        (6, GateType::Not, 11, 3),
                    ] {
                        let gate_id = local_id(gate_base + offset);
                        expected_gate_ids.insert(gate_id);
                        let gate = gates[&gate_id];
                        assert_eq!(gate.routing_domain, routing_domain);
                        assert_eq!(gate.gate_type, gate_type);
                        assert_eq!(
                            gate.origin,
                            cp_at(origin, x, y),
                            "Computed G{offset} retains its compact coordinate"
                        );
                    }
                    for (offset, x, y) in [(0, 6, 0), (1, 4, -1)] {
                        let junction_id = local_id(junction_base + offset);
                        expected_junction_ids.insert(junction_id);
                        assert_eq!(junctions[&junction_id].routing_domain, routing_domain);
                        assert_eq!(
                            junctions[&junction_id].position,
                            cp_at(origin, x, y),
                            "Computed J{offset} retains its compact coordinate"
                        );
                    }

                    assert_eq!(
                        role_target(&format!("reduction.{sector_name}.pair0")),
                        local_id(gate_base)
                    );
                    assert_eq!(
                        role_target(&format!("reduction.{sector_name}.pair1")),
                        local_id(gate_base + 1)
                    );
                    assert_eq!(
                        role_target(&format!("reduction.{sector_name}.presence")),
                        local_id(gate_base + 2)
                    );
                    assert_eq!(
                        role_target(&format!("state.{sector_name}.q")),
                        local_id(gate_base + 4)
                    );
                    assert_eq!(
                        role_target(&format!("state.{sector_name}.qbar")),
                        local_id(gate_base + 6)
                    );
                    assert_eq!(
                        role_target(&format!("shared.{sector_name}")),
                        local_id(wire_base + 20)
                    );
                    assert_eq!(
                        role_target(&format!("defense.{sector_name}.0")),
                        local_id(wire_base + 21)
                    );

                    let mut source = None;
                    for channel in 0..4_u32 {
                        let sensor_id = local_id(sensor_base + channel);
                        expected_wire_ids.insert(sensor_id);
                        assert_eq!(
                            role_target(&format!("sensor.{sector_name}.{channel}")),
                            sensor_id
                        );
                        let observation = design
                            .observation_bindings
                            .iter()
                            .find(|binding| {
                                binding.name == format!("sensor.{sector_name}.{channel}")
                            })
                            .expect("each Computed sensor observation exists");
                        assert_eq!(
                            observation.target,
                            ReferenceArchitectureSemanticTarget::WireSensePort {
                                wire: sensor_id,
                                end: WireEnd::A,
                            }
                        );
                        let sensor = wires[&sensor_id];
                        assert_eq!(sensor.routing_domain, routing_domain);
                        assert_eq!(
                            sensor.endpoint_a,
                            ReferenceArchitectureEndpoint::PowerSource { ordinal: sector }
                        );
                        assert_eq!(sensor.endpoint_b, ReferenceArchitectureEndpoint::Free);
                        assert_eq!(sensor.points.len(), 2);
                        assert_eq!(*source.get_or_insert(sensor.points[0]), sensor.points[0]);
                    }
                    let source = source.expect("the four Computed sensors share their Source");
                    assert_eq!(source, origin);

                    let assert_wire =
                        |offset: u32,
                         endpoint_a: ReferenceArchitectureEndpoint,
                         endpoint_b: ReferenceArchitectureEndpoint| {
                            let wire_id = local_id(wire_base + offset);
                            let wire = wires.get(&wire_id).copied().unwrap_or_else(|| {
                                panic!("Computed W{offset} exists in {sector_name}")
                            });
                            assert_eq!(wire.routing_domain, routing_domain);
                            assert_eq!(wire.endpoint_a, endpoint_a);
                            assert_eq!(wire.endpoint_b, endpoint_b);
                            assert!(wire.points.len() >= 2);
                            assert!(
                                wire_segments(wire).iter().all(|(start, end)| start != end),
                                "Computed W{offset} has only positive-length segments"
                            );
                            wire_id
                        };

                    for (offset, sensor, gate, port) in [
                        (0, sensor_base, gate_base, GatePort::InputA),
                        (1, sensor_base + 1, gate_base, GatePort::InputB),
                        (2, sensor_base + 2, gate_base + 1, GatePort::InputA),
                        (3, sensor_base + 3, gate_base + 1, GatePort::InputB),
                    ] {
                        expected_wire_ids.insert(assert_wire(
                            offset,
                            sensor_endpoint(sensor),
                            gate_endpoint(gate, port),
                        ));
                    }
                    for (offset, endpoint_a, endpoint_b) in [
                        (
                            4,
                            gate_endpoint(gate_base, GatePort::Output),
                            gate_endpoint(gate_base + 2, GatePort::InputA),
                        ),
                        (
                            5,
                            gate_endpoint(gate_base + 1, GatePort::Output),
                            gate_endpoint(gate_base + 2, GatePort::InputB),
                        ),
                        (
                            6,
                            gate_endpoint(gate_base + 2, GatePort::Output),
                            gate_endpoint(gate_base + 5, GatePort::InputB),
                        ),
                        (
                            10,
                            gate_endpoint(gate_base + 3, GatePort::Output),
                            gate_endpoint(gate_base + 4, GatePort::InputA),
                        ),
                        (
                            11,
                            gate_endpoint(gate_base + 5, GatePort::Output),
                            gate_endpoint(gate_base + 6, GatePort::InputA),
                        ),
                        (
                            12,
                            gate_endpoint(gate_base + 4, GatePort::Output),
                            ReferenceArchitectureEndpoint::Junction(local_id(junction_base)),
                        ),
                        (
                            13,
                            ReferenceArchitectureEndpoint::Junction(local_id(junction_base)),
                            gate_endpoint(gate_base + 5, GatePort::InputA),
                        ),
                        (
                            14,
                            gate_endpoint(gate_base + 6, GatePort::Output),
                            gate_endpoint(gate_base + 3, GatePort::InputB),
                        ),
                        (
                            20,
                            ReferenceArchitectureEndpoint::Junction(local_id(junction_base)),
                            ReferenceArchitectureEndpoint::Junction(local_id(junction_base + 1)),
                        ),
                        (
                            21,
                            ReferenceArchitectureEndpoint::Junction(local_id(junction_base + 1)),
                            ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
                        ),
                    ] {
                        expected_wire_ids.insert(assert_wire(offset, endpoint_a, endpoint_b));
                    }
                    assert_eq!(
                        wires[&local_id(wire_base + 20)].points.first().copied(),
                        Some(junctions[&local_id(junction_base)].position)
                    );
                    assert_eq!(
                        wires[&local_id(wire_base + 20)].points.last().copied(),
                        Some(junctions[&local_id(junction_base + 1)].position)
                    );
                    assert_eq!(
                        wires[&local_id(wire_base + 21)].points.first().copied(),
                        Some(junctions[&local_id(junction_base + 1)].position)
                    );
                    assert_eq!(
                        wires[&local_id(wire_base + 21)].points.last().copied(),
                        Some(source)
                    );
                    assert_eq!(
                        wires[&local_id(wire_base + 21)].points.as_slice(),
                        &[cp_at(origin, 4, -1), cp_at(origin, 5, -3), origin],
                        "Computed W21 retains J1->(5,-3)CP->Source"
                    );

                    for offset in 0..7_u32 {
                        expected_wire_ids.insert(assert_wire(
                            30 + offset,
                            ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
                            gate_endpoint(gate_base + offset, GatePort::Power),
                        ));
                    }

                    let reset = gate_endpoint(gate_base + 3, GatePort::InputA);
                    assert!(
                        wires
                            .values()
                            .all(|wire| { wire.endpoint_a != reset && wire.endpoint_b != reset }),
                        "the retained Computed Reset input stays unbound LOW"
                    );
                }
                assert_eq!(wire_ids, expected_wire_ids);
                assert_eq!(
                    gates.keys().copied().collect::<BTreeSet<_>>(),
                    expected_gate_ids
                );
                assert_eq!(
                    junctions.keys().copied().collect::<BTreeSet<_>>(),
                    expected_junction_ids
                );
                assert_eq!(gate_count(GateType::And), 0);
                assert_eq!(gate_count(GateType::Or), 20);
                assert_eq!(gate_count(GateType::Not), 8);
            }
        }

        let replay = load_replay(replay_path(role)).expect("the retained Replay strictly decodes");
        assert!(
            replay
                .replay()
                .commands()
                .iter()
                .all(|command| command.target_tick < pair.build_end_tick()),
            "the retained {role:?} Replay has no command at or after buildEndTick"
        );
    }
}

#[test]
fn retained_s1m5_boundaries_quiescence_and_shared_inputs_are_exact() {
    let pair_bytes = fs::read(fixture("fixtures/experiments/s1-m5-reference-pair-v1.json"))
        .expect("the retained S1-M5 Pair exists");
    let pair = decode_reference_pair_manifest(&pair_bytes).expect("the retained Pair decodes");
    let scenario_path = fixture("fixtures/scenarios/s1-m5-reference-architectures-v1.json");
    let package = load_package(&scenario_path).expect("the retained Scenario package loads");
    let brute_design_source = fs::read_to_string(design_path(ReferenceArchitectureRole::Brute))
        .expect("the retained Brute architecture exists and is UTF-8");
    let brute_design = decode_reference_architecture_artifact(&brute_design_source)
        .expect("the retained Brute architecture strictly decodes");
    let computed_design_source =
        fs::read_to_string(design_path(ReferenceArchitectureRole::Computed))
            .expect("the retained Computed architecture exists and is UTF-8");
    let computed_design = decode_reference_architecture_artifact(&computed_design_source)
        .expect("the retained Computed architecture strictly decodes");
    let brute_candidate =
        Simulation::new(package.clone()).expect("the private Brute candidate starts");
    let computed_candidate =
        Simulation::new(package.clone()).expect("the private Computed candidate starts");
    let brute_resolution = scenario_resolution(&brute_candidate);
    let computed_resolution = scenario_resolution(&computed_candidate);
    let ((brute_materialized, brute_evidence), (computed_materialized, computed_evidence)) =
        materialize_reference_architecture_pair(
            (brute_candidate, &brute_design, &brute_resolution),
            (computed_candidate, &computed_design, &computed_resolution),
        )
        .expect("the retained architectures materialize atomically in lockstep");

    assert_eq!(
        brute_evidence.build_end_tick,
        computed_evidence.build_end_tick
    );
    assert_eq!(brute_evidence.binding_stage_evidence.len(), 4);
    assert_eq!(
        brute_evidence.binding_stage_evidence, computed_evidence.binding_stage_evidence,
        "both designs retain identical stage Ticks and common barrier evidence"
    );
    let expected_stage_boundaries = [
        (Tick(3), Tick(8)),
        (Tick(8), Tick(11)),
        (Tick(11), Tick(14)),
        (Tick(14), Tick(18)),
    ];
    for (stage_index, (stage, expected)) in brute_evidence
        .binding_stage_evidence
        .iter()
        .zip(expected_stage_boundaries)
        .enumerate()
    {
        assert_eq!(stage.stage, stage_index as u8);
        assert_eq!(
            (stage.command_tick, stage.quiescent_tick),
            expected,
            "the retained paired command-to-quiescent boundary is exact"
        );
        if stage_index > 0 {
            assert_eq!(
                stage.command_tick,
                brute_evidence.binding_stage_evidence[stage_index - 1].quiescent_tick,
                "the next paired stage begins exactly at the prior common barrier"
            );
        }
        let expected_barrier_ticks = ((stage.command_tick.0 + 1)..stage.quiescent_tick.0)
            .map(aon_sim::Tick)
            .collect::<Vec<_>>();
        assert_eq!(stage.barrier_ticks, expected_barrier_ticks);
    }
    let actual_common_build_end = brute_evidence.build_end_tick;
    assert_eq!(actual_common_build_end, Tick(18));
    assert_eq!(pair.build_end_tick(), actual_common_build_end);
    assert_eq!(
        pair.measurement_start_tick(),
        actual_common_build_end,
        "the final paired barrier is already the earliest common post-build quiet boundary"
    );
    assert_eq!(pair.max_ticks(), Tick(20));
    assert_eq!(brute_materialized.next_tick(), actual_common_build_end);
    assert_eq!(computed_materialized.next_tick(), actual_common_build_end);
    assert!(
        brute_materialized
            .signal_quiescence_snapshot()
            .expect("Brute post-final quiescence is readable")
            .is_quiescent()
    );
    assert!(
        computed_materialized
            .signal_quiescence_snapshot()
            .expect("Computed post-final quiescence is readable")
            .is_quiescent()
    );

    let brute = load_replay(replay_path(ReferenceArchitectureRole::Brute))
        .expect("the retained Brute Replay strictly decodes");
    let computed = load_replay(replay_path(ReferenceArchitectureRole::Computed))
        .expect("the retained Computed Replay strictly decodes");
    let metric_set_bytes = fs::read(fixture(
        "fixtures/metrics/s1-m5/reference-metric-set-v1.json",
    ))
    .expect("the retained Metric Set exists");
    let metric_set = decode_reference_metric_set_artifact(&metric_set_bytes)
        .expect("the retained Metric Set strictly decodes");
    let brute_response = resolve_reference_response_observations(
        &metric_set,
        &brute_design,
        &brute_evidence,
        &brute_resolution,
    )
    .expect("the Brute response observations resolve");
    let computed_response = resolve_reference_response_observations(
        &metric_set,
        &computed_design,
        &computed_evidence,
        &computed_resolution,
    )
    .expect("the Computed response observations resolve");
    assert_eq!(brute_evidence.commands, brute.replay().commands());
    assert_eq!(computed_evidence.commands, computed.replay().commands());
    assert_eq!(
        brute.replay().world_inputs(),
        computed.replay().world_inputs(),
        "Brute and Computed receive one byte-identical complete WorldInput stream"
    );
    assert!(
        brute.replay().world_inputs().is_empty(),
        "the retained v1 shared WorldInput stream is exactly empty"
    );
    for (role, replay, response) in [
        (
            ReferenceArchitectureRole::Brute,
            &brute,
            brute_response.as_slice(),
        ),
        (
            ReferenceArchitectureRole::Computed,
            &computed,
            computed_response.as_slice(),
        ),
    ] {
        assert!(
            replay
                .replay()
                .commands()
                .iter()
                .all(|command| command.target_tick < actual_common_build_end),
            "neither retained Replay has a command at or after common buildEndTick"
        );
        assert_eq!(replay.replay().final_next_tick(), pair.max_ticks());
        let headless = run_replay_file(replay_path(role))
            .expect("the retained Replay executes for build-window inspection");
        for report in headless
            .reports()
            .iter()
            .filter(|report| report.completed_tick < actual_common_build_end)
        {
            assert!(
                report.contacts.is_empty(),
                "contact is forbidden during paired materialization"
            );
            assert!(
                report.destructions.is_empty(),
                "destruction is forbidden during paired materialization"
            );
            assert_eq!(
                report.run_status,
                RunStatus::Running,
                "run-end is forbidden during paired materialization"
            );
        }
        let stimulus = headless
            .reports()
            .iter()
            .find(|report| report.completed_tick == Tick(18))
            .expect("the retained trace contains T18");
        assert!(
            stimulus.contacts.is_empty(),
            "T18 observes the stimulus before either design contacts an Enemy"
        );
        let stimulus_power = stimulus
            .power
            .as_ref()
            .expect("the T18 report contains Power sensing");
        for binding in response {
            assert!(
                stimulus_power.sense.iter().any(|sense| {
                    sense.wire == binding.sensor_wire
                        && sense.end == binding.sensor_end
                        && sense.sampled_presence
                }),
                "T18 samples the `{}` hostile-entry stimulus",
                binding.name
            );
        }

        let response_tick = headless
            .reports()
            .iter()
            .find(|report| report.completed_tick == Tick(19))
            .expect("the retained trace contains T19");
        let positive_contacts = response_tick
            .contacts
            .iter()
            .filter(|contact| contact.absorbed.0 > 0)
            .collect::<Vec<_>>();
        assert_eq!(
            positive_contacts.len(),
            4,
            "T19 has exactly four positive response contacts for {role:?}"
        );
        assert!(
            positive_contacts
                .iter()
                .all(|contact| contact.absorbed.0 == 1),
            "every T19 positive response contact absorbs exactly one Energy"
        );
        for binding in response {
            assert_eq!(
                positive_contacts
                    .iter()
                    .filter(|contact| {
                        contact.wire == binding.defense_wire && contact.target == binding.enemy
                    })
                    .count(),
                1,
                "T19 contains exactly one positive `{}` response contact",
                binding.name
            );
        }
        assert_eq!(
            response_tick.completed_tick.0 - stimulus.completed_tick.0,
            1,
            "the exact response latency is one Tick"
        );
        assert!(
            headless
                .reports()
                .iter()
                .all(|report| report.destructions.is_empty()),
            "neither retained design has a destruction through maxTicks"
        );
    }

    let mut brute_simulation =
        Simulation::new(package.clone()).expect("the Brute Replay Simulation starts");
    let mut computed_simulation =
        Simulation::new(package).expect("the Computed Replay Simulation starts");
    brute
        .replay()
        .validate_against(&brute_simulation)
        .expect("the Brute Replay matches its fresh package");
    computed
        .replay()
        .validate_against(&computed_simulation)
        .expect("the Computed Replay matches its fresh package");

    let mut common_quiescence = Vec::new();
    loop {
        assert_eq!(
            brute_simulation.next_tick(),
            computed_simulation.next_tick()
        );
        let tick = brute_simulation.next_tick();
        if tick >= actual_common_build_end {
            let brute_snapshot = brute_simulation
                .signal_quiescence_snapshot()
                .expect("Brute quiescence is readable");
            let computed_snapshot = computed_simulation
                .signal_quiescence_snapshot()
                .expect("Computed quiescence is readable");
            assert_eq!(brute_snapshot.next_tick, tick);
            assert_eq!(computed_snapshot.next_tick, tick);
            common_quiescence.push((
                tick,
                brute_snapshot.is_quiescent() && computed_snapshot.is_quiescent(),
            ));
        }
        if tick == pair.measurement_start_tick() {
            break;
        }
        assert!(
            tick < pair.measurement_start_tick(),
            "measurementStartTick is reachable from common buildEndTick"
        );
        step_replay_tick(&mut brute_simulation, brute.replay());
        step_replay_tick(&mut computed_simulation, computed.replay());
    }
    let first_common_quiescent = common_quiescence
        .iter()
        .find_map(|(tick, quiescent)| quiescent.then_some(*tick))
        .expect("both retained designs reach a common quiescent state");
    assert_eq!(first_common_quiescent, pair.measurement_start_tick());
    assert!(
        common_quiescence
            .iter()
            .filter(|(tick, _)| *tick < pair.measurement_start_tick())
            .all(|(_, quiescent)| !quiescent),
        "every earlier common candidate has at least one non-quiescent design"
    );
}

#[test]
fn retained_s1m5_direct_and_headless_complete_traces_are_identical() {
    for role in [
        ReferenceArchitectureRole::Brute,
        ReferenceArchitectureRole::Computed,
    ] {
        let path = replay_path(role);
        let retained = load_replay(&path).expect("the retained S1-M5 Replay strictly decodes");
        let scenario_path = path
            .parent()
            .expect("the Replay has a parent")
            .join(retained.scenario_path());
        let package = load_package(&scenario_path).expect("the retained Scenario package loads");
        let headless = run_replay_file(&path).expect("the retained Replay runs headlessly");

        let mut direct = Simulation::new(package).expect("the direct Simulation starts");
        retained
            .replay()
            .validate_against(&direct)
            .expect("the Replay matches the direct Simulation");
        let mut checkpoints = vec![direct.state_hash()];
        while direct.next_tick() < retained.replay().final_next_tick() {
            let tick = direct.next_tick();
            let commands = retained
                .replay()
                .commands_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            let inputs = retained
                .replay()
                .world_inputs_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            let report = direct
                .step_with_world_inputs(&commands, &inputs)
                .expect("the direct retained Tick succeeds");
            let index = usize::try_from(tick.0).expect("the retained Tick fits usize");
            assert_eq!(&report, &headless.reports()[index]);
            assert_eq!(report.state_hash, headless.checkpoints()[index + 1]);
            checkpoints.push(report.state_hash);
        }
        retained
            .replay()
            .verify_trace(&checkpoints)
            .expect("the direct trace matches every retained checkpoint");
        assert_eq!(checkpoints, headless.checkpoints());
        assert_eq!(headless.final_hash(), direct.state_hash());
    }
}

#[test]
fn retained_s1m5_metric_reduction_is_read_only_and_matches_both_goldens() {
    let pair_bytes = fs::read(fixture("fixtures/experiments/s1-m5-reference-pair-v1.json"))
        .expect("the retained Pair exists");
    let pair = decode_reference_pair_manifest(&pair_bytes).expect("the retained Pair decodes");
    let plan_bytes = fs::read(fixture("fixtures/experiments/s1-m5-reference-plan-v2.json"))
        .expect("the retained Plan exists");
    let plan = decode_reference_experiment_plan_v2(&plan_bytes).expect("the retained Plan decodes");
    let runs = plan.resolve(&pair).expect("the retained Plan resolves");
    let metric_set_bytes = fs::read(fixture(
        "fixtures/metrics/s1-m5/reference-metric-set-v1.json",
    ))
    .expect("the retained Metric Set exists");
    let metric_set = decode_reference_metric_set_artifact(&metric_set_bytes)
        .expect("the retained Metric Set decodes");
    let brute_design_source = fs::read_to_string(design_path(ReferenceArchitectureRole::Brute))
        .expect("the retained Brute design exists and is UTF-8");
    let brute_design = decode_reference_architecture_artifact(&brute_design_source)
        .expect("the retained Brute design strictly decodes");
    let computed_design_source =
        fs::read_to_string(design_path(ReferenceArchitectureRole::Computed))
            .expect("the retained Computed design exists and is UTF-8");
    let computed_design = decode_reference_architecture_artifact(&computed_design_source)
        .expect("the retained Computed design strictly decodes");

    for role in [
        ReferenceArchitectureRole::Brute,
        ReferenceArchitectureRole::Computed,
    ] {
        let design = match role {
            ReferenceArchitectureRole::Brute => &brute_design,
            ReferenceArchitectureRole::Computed => &computed_design,
        };
        let replay_path = replay_path(role);
        let replay = load_replay(&replay_path).expect("the retained Replay strictly decodes");
        let scenario_path = replay_path
            .parent()
            .expect("the Replay has a parent")
            .join(replay.scenario_path());
        let package = load_package(&scenario_path).expect("the retained Scenario package loads");

        let pristine = Simulation::new(package.clone()).expect("the pristine Simulation starts");
        let initial_sample = ReferenceMetricTickSample::from_snapshot(&render(&pristine))
            .expect("the initial metric sample has a Main Core");
        let brute_candidate =
            Simulation::new(package.clone()).expect("the private Brute candidate starts");
        let computed_candidate =
            Simulation::new(package.clone()).expect("the private Computed candidate starts");
        let brute_resolution = scenario_resolution(&brute_candidate);
        let computed_resolution = scenario_resolution(&computed_candidate);
        let (brute_materialization, computed_materialization) =
            materialize_reference_architecture_pair(
                (brute_candidate, &brute_design, &brute_resolution),
                (computed_candidate, &computed_design, &computed_resolution),
            )
            .expect("the retained pair materializes atomically in lockstep");
        let (materialized, evidence, resolution) = match role {
            ReferenceArchitectureRole::Brute => (
                brute_materialization.0,
                brute_materialization.1,
                brute_resolution,
            ),
            ReferenceArchitectureRole::Computed => (
                computed_materialization.0,
                computed_materialization.1,
                computed_resolution,
            ),
        };
        assert_eq!(evidence.build_end_tick, pair.build_end_tick());
        assert_eq!(evidence.commands, replay.replay().commands());
        let construction_probe = package
            .profiles()
            .balance
            .construction_probe
            .expect("the retained Balance v5 has Construction Work kernels");
        let inventory =
            derive_reference_static_inventory(design, &construction_probe, &evidence, &resolution)
                .expect("static inventory derives from materialization evidence");
        inventory
            .validate_materialized_snapshot(&render(&materialized))
            .expect("materialized Wire and Gate facts match the static inventory");
        let response =
            resolve_reference_response_observations(&metric_set, design, &evidence, &resolution)
                .expect("portable response observations resolve to runtime identities");
        let response_oracle = response.clone();
        let collector = ReferenceMetricCollector::new(
            ReferenceMetricBoundaries {
                build_end_tick: pair.build_end_tick(),
                measurement_start_tick: pair.measurement_start_tick(),
                max_ticks: pair.max_ticks(),
            },
            inventory,
            initial_sample,
            resolution.enemies.clone(),
            response,
        )
        .expect("the exact metric collector starts");

        let mut simulation = Simulation::new(package).expect("the direct Replay Simulation starts");
        let mut reports = Vec::new();
        let mut samples = Vec::new();
        while simulation.next_tick() < replay.replay().final_next_tick() {
            let tick = simulation.next_tick();
            let commands = replay
                .replay()
                .commands_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            let inputs = replay
                .replay()
                .world_inputs_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            let report = simulation
                .step_with_world_inputs(&commands, &inputs)
                .expect("the retained metric Tick succeeds");
            let sample = ReferenceMetricTickSample::from_snapshot(&render(&simulation))
                .expect("the post-Tick sample has a Main Core");
            reports.push(report);
            samples.push(sample);
        }
        let before_reduce = (simulation.next_tick(), simulation.state_hash());
        let result =
            reduce_reference_metrics(collector, reports.iter().zip(samples.iter().copied()))
                .expect("the complete retained trace reduces without omission");
        assert_eq!(
            (simulation.next_tick(), simulation.state_hash()),
            before_reduce,
            "metric reduction must not mutate Tick or canonical State"
        );

        let window = reports
            .iter()
            .filter(|report| report.completed_tick >= pair.measurement_start_tick())
            .collect::<Vec<_>>();
        let mut power_generation = 0_u128;
        let mut power_nominal_demand = 0_u128;
        let mut power_granted = 0_u128;
        let mut power_source_cost = 0_u128;
        let mut power_transmission_loss = 0_u128;
        let mut brownout_ticks = 0_u64;
        let mut construction_requested = 0_u128;
        let mut construction_nominal_power = 0_u128;
        let mut construction_granted_work = 0_u128;
        let mut construction_applied_work = 0_u128;
        let mut heat_generated = 0_u128;
        let mut network_peak: Option<aon_sim::Capacity> = None;
        let mut network_final: Option<aon_sim::Capacity> = None;
        let mut network_integral = 0_u128;
        let mut support_integral = 0_u128;
        let mut killed_enemies = BTreeSet::new();
        for report in &window {
            let power = report
                .power
                .as_ref()
                .expect("each retained comparison Tick has Power output");
            for region in &power.regions {
                checked_add(&mut power_generation, region.generation.0);
            }
            if power
                .loads
                .iter()
                .any(|load| load.nominal.0 > 0 && load.ratio < PowerRatio::ONE)
            {
                brownout_ticks = brownout_ticks.checked_add(1).expect("bounded Tick count");
            }
            for load in &power.loads {
                checked_add(&mut power_nominal_demand, load.nominal.0);
                checked_add(&mut power_granted, load.granted.0);
                checked_add(&mut power_source_cost, load.source_cost.0);
                checked_add(&mut power_transmission_loss, load.transmission_loss.0);
            }
            for heat in &power.heat_contributions {
                checked_add(&mut heat_generated, heat.energy.0);
            }
            for work in &report.construction_work {
                checked_add(&mut construction_requested, work.requested.0);
                checked_add(&mut construction_nominal_power, work.nominal_power.0);
                checked_add(&mut construction_granted_work, work.granted_work.0);
                checked_add(&mut construction_applied_work, work.applied_work.0);
            }
            for heat in &report.interaction_heat {
                checked_add(&mut heat_generated, heat.energy.0);
            }
            let accounting = report
                .network_accounting
                .expect("each retained comparison Tick has Network accounting");
            let used = accounting.used();
            network_peak = Some(network_peak.map_or(used, |prior| prior.max(used)));
            network_final = Some(used);
            checked_add(&mut network_integral, used.0);
            checked_add(
                &mut support_integral,
                accounting
                    .total_support_demand()
                    .expect("Balance v5 reports support demand")
                    .0,
            );
            for destruction in &report.destructions {
                let enemy = aon_sim::EnemyId(destruction.target);
                if destruction.kind == DestructionKind::Damage
                    && resolution.enemies.contains(&enemy)
                {
                    killed_enemies.insert(enemy);
                }
            }
        }
        let runtime = &result.runtime_metrics;
        assert_eq!(runtime.power_generation, power_generation);
        assert_eq!(runtime.power_nominal_demand, power_nominal_demand);
        assert_eq!(runtime.power_granted, power_granted);
        assert_eq!(runtime.power_source_cost, power_source_cost);
        assert_eq!(runtime.power_transmission_loss, power_transmission_loss);
        assert_eq!(runtime.brownout_ticks, brownout_ticks);
        assert_eq!(runtime.construction_requested, construction_requested);
        assert_eq!(
            runtime.construction_nominal_power,
            construction_nominal_power
        );
        assert_eq!(runtime.construction_granted_work, construction_granted_work);
        assert_eq!(runtime.construction_applied_work, construction_applied_work);
        assert_eq!(runtime.heat_generated, heat_generated);
        assert_eq!(runtime.network_peak_used_ncu, network_peak.unwrap());
        assert_eq!(runtime.network_final_used_ncu, network_final.unwrap());
        assert_eq!(runtime.network_integral_used_ncu, network_integral);
        assert_eq!(runtime.support_demand_integral, support_integral);
        assert_eq!(runtime.enemy_kills, killed_enemies.len() as u64);

        let latency_oracle = response_oracle
            .iter()
            .map(|binding| {
                let mut stimulus = None;
                let mut response = None;
                for report in &window {
                    let power = report.power.as_ref().expect("window Power report");
                    let sampled = power.sense.iter().any(|sense| {
                        sense.wire == binding.sensor_wire
                            && sense.end == binding.sensor_end
                            && sense.sampled_presence
                    });
                    if stimulus.is_none() && sampled {
                        stimulus = Some(report.completed_tick);
                    }
                    let contacted = report.contacts.iter().any(|contact| {
                        contact.wire == binding.defense_wire
                            && contact.target == binding.enemy
                            && contact.absorbed.0 > 0
                    });
                    if stimulus.is_some() && response.is_none() && contacted {
                        response = Some(report.completed_tick);
                    }
                }
                let stimulus_tick = stimulus.expect("the retained stimulus is observed");
                let response_tick = response.expect("the retained response is observed");
                ReferenceResponseLatency {
                    name: binding.name.clone(),
                    stimulus_tick,
                    response_tick,
                    latency_ticks: response_tick
                        .0
                        .checked_sub(stimulus_tick.0)
                        .expect("response is not before stimulus"),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(result.response_latency_ticks, latency_oracle);
        assert!(result.response_latency_ticks.iter().all(|latency| {
            latency.stimulus_tick == Tick(18)
                && latency.response_tick == Tick(19)
                && latency.latency_ticks == 1
        }));

        let measurement_start_integrity = if pair.measurement_start_tick().0 == 0 {
            initial_sample.core_integrity
        } else {
            samples
                .iter()
                .find(|sample| sample.next_tick == pair.measurement_start_tick())
                .expect("the measurement-start sample exists")
                .core_integrity
        };
        let final_integrity = samples
            .last()
            .expect("the retained trace has a final sample")
            .core_integrity;
        assert_eq!(
            runtime.measurement_start_core_integrity,
            measurement_start_integrity
        );
        assert_eq!(runtime.final_core_integrity, final_integrity);
        assert_eq!(
            runtime.core_damage.0,
            measurement_start_integrity
                .0
                .checked_sub(final_integrity.0)
                .expect("Core integrity cannot increase")
        );
        let run_id = runs
            .iter()
            .find(|run| run.design.role == role)
            .expect("the Plan resolves each role once")
            .run_id;
        let artifact = ReferenceMetricArtifact::v1(&metric_set, run_id, result)
            .expect("the reduced result forms a strict Metric Artifact");
        let encoded = encode_reference_metric_artifact(&artifact, &metric_set)
            .expect("the reduced Metric Artifact canonically encodes");
        let expected_path = fixture(match role {
            ReferenceArchitectureRole::Brute => "fixtures/metrics/s1-m5/brute-v1.json",
            ReferenceArchitectureRole::Computed => "fixtures/metrics/s1-m5/computed-v1.json",
        });
        assert_eq!(
            encoded,
            fs::read(expected_path).expect("the retained Metric Artifact exists")
        );
    }
}
