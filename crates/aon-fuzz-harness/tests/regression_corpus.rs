use aon_fuzz_harness::{
    CapacitySupportExecutionObservation, DecoderTarget, MobilityRuntimeExecutionObservation,
    S1m4KernelExecutionObservation, S1m4RuntimeExecutionObservation, S1m5ReferenceTarget,
    SignalRuntimeExecutionObservation, TopologyRuntimeExecutionObservation,
    exercise_capacity_support, exercise_commands, exercise_decoder, exercise_experiment_decoder,
    exercise_geometry, exercise_mobility_runtime, exercise_module_decoder, exercise_replay_decoder,
    exercise_s1m4_kernels, exercise_s1m4_runtime, exercise_s1m5_reference_artifacts,
    exercise_signal_runtime, exercise_stateful_commands, exercise_topology_runtime,
};
use aon_sim::{
    Fixed, FixedVec2, HashAlgorithmId, ProfileHash, ReferenceArchitectureArtifact,
    ReferenceArchitectureBindingEndpoint, ReferenceArchitectureEndpoint,
    ReferenceArchitectureFormatVersion, ReferenceArchitectureLocalId,
    ReferenceArchitectureMaterializationSchedule, ReferenceArchitectureOperation,
    ReferenceArchitectureRoutingDomain, ReferenceJunction, ReferenceMetricSetArtifact,
    ReferenceResponseObservationSpec, ReferenceWire, SemanticsVersion, SimulationContract, WireEnd,
    decode_balance_profile, encode_reference_architecture_artifact,
    encode_reference_metric_set_artifact,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn reference_architecture_id(value: u32) -> ReferenceArchitectureLocalId {
    ReferenceArchitectureLocalId::new(value).expect("the fuzz fixture local ID is nonzero")
}

fn reference_architecture_fixture(
    format_version: ReferenceArchitectureFormatVersion,
) -> ReferenceArchitectureArtifact {
    let profile_hash =
        ProfileHash::from_hex(&"00".repeat(32)).expect("the zero fuzz profile hash is canonical");
    let wire = reference_architecture_id(2);
    let materialization_schedule =
        (format_version == ReferenceArchitectureFormatVersion::V2).then(|| {
            ReferenceArchitectureMaterializationSchedule {
                binding_batches: vec![vec![
                    ReferenceArchitectureBindingEndpoint {
                        wire,
                        end: WireEnd::A,
                    },
                    ReferenceArchitectureBindingEndpoint {
                        wire,
                        end: WireEnd::B,
                    },
                ]],
            }
        });
    ReferenceArchitectureArtifact {
        format_version,
        hash_algorithm_id: HashAlgorithmId::Blake3V1,
        display_name: "S1-M5 fuzz schedule fixture".to_owned(),
        contract: SimulationContract {
            semantics_version: SemanticsVersion::AonV1,
            numeric_profile_hash: profile_hash,
            physical_scale_profile_hash: profile_hash,
            balance_profile_hash: profile_hash,
        },
        operations: vec![
            ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                id: reference_architecture_id(1),
                routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            }),
            ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                id: wire,
                routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                points: vec![
                    FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                    FixedVec2::new(Fixed(1), Fixed::ZERO),
                ],
                endpoint_a: ReferenceArchitectureEndpoint::MainCore,
                endpoint_b: ReferenceArchitectureEndpoint::Junction(reference_architecture_id(1)),
            }),
        ],
        role_bindings: Vec::new(),
        observation_bindings: Vec::new(),
        materialization_schedule,
    }
}

fn s1m5_architecture_input(source: &str) -> Vec<u8> {
    let mut input = Vec::with_capacity(source.len() + 1);
    input.push(0);
    input.extend_from_slice(source.as_bytes());
    input
}

const DECODER_CORPUS: &[(&str, &[u8])] = &[
    (
        "scenario-truncated",
        include_bytes!("../corpus/decoder/scenario-truncated.case"),
    ),
    (
        "scenario-unknown-field",
        include_bytes!("../corpus/decoder/scenario-unknown-field.case"),
    ),
    (
        "numeric-float",
        include_bytes!("../corpus/decoder/numeric-float.case"),
    ),
    (
        "physical-nesting",
        include_bytes!("../corpus/decoder/physical-nesting.case"),
    ),
    (
        "balance-unknown-field",
        include_bytes!("../corpus/decoder/balance-unknown-field.case"),
    ),
];

const GEOMETRY_CORPUS: &[(&str, &[u8])] = &[
    (
        "short-cycle",
        include_bytes!("../corpus/geometry/short-cycle.case"),
    ),
    (
        "collinear-like",
        include_bytes!("../corpus/geometry/collinear-like.case"),
    ),
    (
        "large-coordinates",
        include_bytes!("../corpus/geometry/large-coordinates.case"),
    ),
];

const REPLAY_CORPUS: &[(&str, &[u8], bool)] = &[
    (
        "valid-empty",
        include_bytes!("../corpus/replay/valid-empty.case"),
        true,
    ),
    (
        "valid-s1-m1-capacity",
        include_bytes!("../../../fixtures/replays/s1-m1-capacity-accounting-v1.json"),
        true,
    ),
    (
        "valid-s1-m2-c07-sensing",
        include_bytes!("../../../fixtures/replays/s1-m2-c07-sensing-v1.json"),
        true,
    ),
    (
        "valid-s1-m2-c08-brownout-half",
        include_bytes!("../../../fixtures/replays/s1-m2-c08-brownout-half-v1.json"),
        true,
    ),
    (
        "valid-s1-m3-c22-capacity-support",
        include_bytes!("../../../fixtures/replays/s1-m3-c22-capacity-support-v1.json"),
        true,
    ),
    (
        "truncated",
        include_bytes!("../corpus/replay/truncated.case"),
        false,
    ),
    (
        "unknown-field",
        include_bytes!("../corpus/replay/unknown-field.case"),
        false,
    ),
];

const EXPERIMENT_CORPUS: &[(&str, &[u8], bool)] = &[
    (
        "valid-physical-scale-matrix",
        include_bytes!("../../../fixtures/experiments/s1-m0-physical-scale-v1.json"),
        true,
    ),
    (
        "truncated",
        include_bytes!("../corpus/experiment/truncated.case"),
        false,
    ),
    (
        "unknown-field",
        include_bytes!("../corpus/experiment/unknown-field.case"),
        false,
    ),
];

const MODULE_CORPUS: &[(&str, &[u8], bool)] = &[
    (
        "valid-empty",
        include_bytes!("../corpus/module/valid-empty.case"),
        true,
    ),
    (
        "truncated",
        include_bytes!("../corpus/module/truncated.case"),
        false,
    ),
    (
        "unknown-field",
        include_bytes!("../corpus/module/unknown-field.case"),
        false,
    ),
];

const STATEFUL_EFFECTIVE_PATHS: &[u8] =
    include_bytes!("../corpus/command/stateful-effective-paths.case");

const COMMAND_CORPUS: &[(&str, &[u8])] = &[
    (
        "empty-batch",
        include_bytes!("../corpus/command/empty-batch.case"),
    ),
    (
        "all-variants",
        include_bytes!("../corpus/command/all-variants.case"),
    ),
    (
        "stateful-references",
        include_bytes!("../corpus/command/stateful-references.case"),
    ),
    ("stateful-effective-paths", STATEFUL_EFFECTIVE_PATHS),
    (
        "s1m4-tag8-gate",
        include_bytes!("../corpus/command/s1m4-tag8-gate.case"),
    ),
    (
        "s1m4-tag8-wire",
        include_bytes!("../corpus/command/s1m4-tag8-wire.case"),
    ),
    (
        "s1m4-tag8-junction",
        include_bytes!("../corpus/command/s1m4-tag8-junction.case"),
    ),
    (
        "s1m4-tag8-fixed-substrate",
        include_bytes!("../corpus/command/s1m4-tag8-fixed-substrate.case"),
    ),
];

const SIGNAL_RUNTIME_COVERAGE: &[u8] = include_bytes!("../corpus/signal-runtime/coverage.case");

const SIGNAL_RUNTIME_CORPUS: &[(&str, &[u8])] = &[
    ("coverage", SIGNAL_RUNTIME_COVERAGE),
    (
        "event-order",
        include_bytes!("../corpus/signal-runtime/event-order.case"),
    ),
    (
        "checked-arithmetic",
        include_bytes!("../corpus/signal-runtime/checked-arithmetic.case"),
    ),
];

const TOPOLOGY_RUNTIME_COVERAGE: &[u8] =
    include_bytes!("../corpus/topology-runtime/s0-m4-coverage.case");

const TOPOLOGY_RUNTIME_CORPUS: &[(&str, &[u8])] = &[("s0-m4-coverage", TOPOLOGY_RUNTIME_COVERAGE)];

const MOBILITY_RUNTIME_COVERAGE: &[u8] =
    include_bytes!("../corpus/mobility-runtime/s0-m7-coverage.case");

const MOBILITY_RUNTIME_CORPUS: &[(&str, &[u8])] = &[("s0-m7-coverage", MOBILITY_RUNTIME_COVERAGE)];

const CAPACITY_SUPPORT_COVERAGE: &[u8] = include_bytes!("../corpus/capacity-support/coverage.case");

const S1M4_KERNEL_COVERAGE: &[u8] = include_bytes!("../corpus/s1m4/kernel-coverage.case");
const S1M4_RUNTIME_COVERAGE: &[u8] = include_bytes!("../corpus/s1m4/runtime-coverage.case");
const S1M4_THERMAL_TWO_TICK_COVERAGE: &[u8] =
    include_bytes!("../corpus/s1m4/thermal-two-tick.case");
const S1M5_REFERENCE_CORPUS: &[(&str, &[u8])] = &[
    (
        "architecture-unknown-field",
        include_bytes!("../corpus/s1m5-reference/architecture-unknown-field.case"),
    ),
    (
        "pair-truncated",
        include_bytes!("../corpus/s1m5-reference/pair-truncated.case"),
    ),
    (
        "experiment-unknown-field",
        include_bytes!("../corpus/s1m5-reference/experiment-unknown-field.case"),
    ),
    (
        "metric-set-duplicate",
        include_bytes!("../corpus/s1m5-reference/metric-set-duplicate.case"),
    ),
    (
        "metric-artifact-trailing",
        include_bytes!("../corpus/s1m5-reference/metric-artifact-trailing.case"),
    ),
];
const S1M4_REPLAY_ARTIFACTS: &[(&str, &[u8])] = &[
    (
        "construction-partial-multibuilder-v1",
        include_bytes!("../../../fixtures/replays/s1-m4/construction-partial-multibuilder-v1.json"),
    ),
    (
        "construction-four-targets-v1",
        include_bytes!("../../../fixtures/replays/s1-m4/construction-four-targets-v1.json"),
    ),
    (
        "c10-contact-v1",
        include_bytes!("../../../fixtures/replays/s1-m4/c10-contact-v1.json"),
    ),
    (
        "c09-wire-break-v1",
        include_bytes!("../../../fixtures/replays/s1-m4/c09-wire-break-v1.json"),
    ),
    (
        "terminal-v1",
        include_bytes!("../../../fixtures/replays/s1-m4/terminal-v1.json"),
    ),
];

#[test]
fn decoder_regression_corpus_never_panics() {
    for &(name, bytes) in DECODER_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_decoder(bytes)));
        assert!(result.is_ok(), "decoder regression case `{name}` panicked");
    }
}

#[test]
fn s1m5_reference_artifact_corpus_is_bounded_deterministic_and_panic_free() {
    let expected_targets = [
        S1m5ReferenceTarget::Architecture,
        S1m5ReferenceTarget::Pair,
        S1m5ReferenceTarget::ExperimentPlan,
        S1m5ReferenceTarget::MetricSet,
        S1m5ReferenceTarget::MetricArtifact,
    ];
    for ((name, bytes), expected_target) in
        S1M5_REFERENCE_CORPUS.iter().copied().zip(expected_targets)
    {
        let observation = catch_unwind(AssertUnwindSafe(|| {
            exercise_s1m5_reference_artifacts(bytes)
        }))
        .unwrap_or_else(|_| panic!("S1-M5 reference corpus case `{name}` panicked"));
        assert_eq!(
            observation.target, expected_target,
            "S1-M5 reference corpus case `{name}` changed its selector boundary"
        );
        assert_eq!(
            exercise_s1m5_reference_artifacts(bytes),
            observation,
            "S1-M5 reference corpus case `{name}` changed strict outcome"
        );
        assert!(
            observation.result.is_err(),
            "malformed S1-M5 corpus case `{name}` was accepted"
        );
    }

    let oversized = vec![0xa5; 32 * 1024];
    let observation = exercise_s1m5_reference_artifacts(&oversized);
    assert_eq!(observation.payload_len, 16 * 1024 - 1);
    assert_eq!(
        observation,
        exercise_s1m5_reference_artifacts(&oversized[..16 * 1024]),
        "bytes beyond the documented S1-M5 bound must be ignored"
    );
}

#[test]
fn s1m5_architecture_v1_v2_schedules_reach_plan_and_malformed_cases_fail_closed() {
    let v1 = reference_architecture_fixture(ReferenceArchitectureFormatVersion::V1);
    let v2 = reference_architecture_fixture(ReferenceArchitectureFormatVersion::V2);
    let v1_bytes = encode_reference_architecture_artifact(&v1)
        .expect("the valid v1 architecture fuzz seed encodes");
    let v2_bytes = encode_reference_architecture_artifact(&v2)
        .expect("the valid v2 architecture fuzz seed encodes");

    for (name, artifact, source) in [("v1", &v1, &v1_bytes), ("v2", &v2, &v2_bytes)] {
        let input = s1m5_architecture_input(source);
        let observation = catch_unwind(AssertUnwindSafe(|| {
            exercise_s1m5_reference_artifacts(&input)
        }))
        .unwrap_or_else(|_| panic!("valid S1-M5 Architecture {name} seed panicked"));
        assert_eq!(observation.target, S1m5ReferenceTarget::Architecture);
        let canonical = observation
            .result
            .unwrap_or_else(|error| panic!("valid Architecture {name} was rejected: {error}"));
        assert_eq!(canonical.canonical_len, source.len());
        assert_eq!(
            canonical.semantic_hash,
            Some(artifact.semantic_hash().expect("the valid seed hashes"))
        );
    }

    let missing_schedule = v1_bytes.replacen("\"formatVersion\": 1", "\"formatVersion\": 2", 1);
    let unexpected_schedule = v2_bytes.replacen("\"formatVersion\": 2", "\"formatVersion\": 1", 1);
    let v2_prefix = missing_schedule
        .strip_suffix("}\n")
        .expect("canonical Architecture JSON has one final object delimiter");
    let empty_schedule = format!(
        "{v2_prefix},\n  \"materializationSchedule\": {{\n    \"bindingBatches\": []\n  }}\n}}\n"
    );
    let noncanonical_schedule = format!(
        "{v2_prefix},\n  \"materializationSchedule\": {{\n    \"bindingBatches\": [[{{\"wire\": 2, \"end\": \"a\"}}, {{\"wire\": 2, \"end\": \"a\"}}, {{\"wire\": 2, \"end\": \"b\"}}]]\n  }}\n}}\n"
    );

    for (name, source, expected) in [
        (
            "missing-v2-schedule",
            missing_schedule,
            "v2 requires materializationSchedule",
        ),
        (
            "unexpected-v1-schedule",
            unexpected_schedule,
            "v1 forbids materializationSchedule",
        ),
        (
            "empty-v2-schedule",
            empty_schedule,
            "binding batch count must be in 1..=16",
        ),
        (
            "noncanonical-v2-schedule",
            noncanonical_schedule,
            "not in canonical endpoint order",
        ),
    ] {
        let input = s1m5_architecture_input(&source);
        let observation = catch_unwind(AssertUnwindSafe(|| {
            exercise_s1m5_reference_artifacts(&input)
        }))
        .unwrap_or_else(|_| panic!("malformed S1-M5 schedule case `{name}` panicked"));
        let error = observation
            .result
            .expect_err("a malformed S1-M5 schedule must fail closed");
        assert!(
            error.contains(expected),
            "malformed schedule case `{name}` returned `{error}`"
        );
    }
}

#[test]
fn s1m5_metric_set_accepted_path_reaches_a_canonical_hash_stable_fixed_point() {
    let definition = ReferenceMetricSetArtifact::v1(vec![ReferenceResponseObservationSpec {
        name: "fuzz.response".to_owned(),
        hostile_entry_binding: "sensor.fuzz.0".to_owned(),
        defense_contact_binding: "defense.fuzz.0".to_owned(),
        enemy_ordinal: 0,
    }])
    .expect("the retained fuzz Metric Set is valid");
    let bytes = encode_reference_metric_set_artifact(&definition)
        .expect("the retained fuzz Metric Set encodes");
    let mut input = Vec::with_capacity(bytes.len() + 1);
    input.push(b'D');
    input.extend_from_slice(&bytes);

    let first = exercise_s1m5_reference_artifacts(&input);
    let second = exercise_s1m5_reference_artifacts(&input);
    assert_eq!(first, second);
    let canonical = first
        .result
        .expect("the retained Metric Set reaches the accepted strict path");
    assert_eq!(canonical.canonical_len, bytes.len());
    assert_eq!(
        canonical.semantic_hash,
        Some(definition.semantic_hash().unwrap())
    );
}

#[test]
fn s1m5_generated_byte_streams_are_bounded_deterministic_and_panic_free() {
    let mut state = 0x51a5_1f5e_d12a_7005_u64;
    let mut reached_targets = [false; 5];
    for case_index in 0..2_048_usize {
        let length = case_index % 257;
        let mut bytes = vec![0_u8; length];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state.to_le_bytes()[0];
        }
        let first = catch_unwind(AssertUnwindSafe(|| {
            exercise_s1m5_reference_artifacts(&bytes)
        }))
        .unwrap_or_else(|_| panic!("S1-M5 decoder lane panicked for case {case_index}"));
        reached_targets[match first.target {
            S1m5ReferenceTarget::Architecture => 0,
            S1m5ReferenceTarget::Pair => 1,
            S1m5ReferenceTarget::ExperimentPlan => 2,
            S1m5ReferenceTarget::MetricSet => 3,
            S1m5ReferenceTarget::MetricArtifact => 4,
        }] = true;
        assert_eq!(
            first,
            exercise_s1m5_reference_artifacts(&bytes),
            "S1-M5 decoder lane changed outcome for case {case_index}"
        );
    }
    assert!(
        reached_targets.into_iter().all(|reached| reached),
        "the generated S1-M5 stream set must reach all five strict decoder boundaries"
    );
}

#[test]
fn s1m2_scenario_and_balance_artifacts_reach_the_bounded_decoder_without_panics() {
    let mut scenario = vec![0];
    scenario.extend_from_slice(include_bytes!(
        "../../../fixtures/scenarios/s1-m2-c07-sensing-v1.json"
    ));
    let scenario = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&scenario)))
        .expect("S1-M2 Scenario decoder input must not panic");
    assert!(
        scenario.result.is_ok(),
        "S1-M2 Scenario fixture must remain accepted: {:?}",
        scenario.result
    );

    let balance = include_bytes!("../../../profiles/balance/s1-m2-power-probe-alpha.json");
    let balance = catch_unwind(AssertUnwindSafe(|| decode_balance_profile(balance)))
        .expect("S1-M2 Balance decoder input must not panic");
    assert!(
        balance.is_ok(),
        "S1-M2 Balance fixture must remain accepted: {:?}",
        balance
    );
}

#[test]
fn s1m3_scenario_balance_and_replay_artifacts_reach_bounded_strict_decoders() {
    let mut scenario = vec![0];
    scenario.extend_from_slice(include_bytes!(
        "../../../fixtures/scenarios/s1-m3-c22-capacity-support-v1.json"
    ));
    let scenario = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&scenario)))
        .expect("S1-M3 Scenario decoder input must not panic");
    assert_eq!(scenario.target, DecoderTarget::Scenario);
    assert!(
        scenario.result.is_ok(),
        "retained C-22 Scenario must remain accepted: {:?}",
        scenario.result
    );

    let balance_bytes =
        include_bytes!("../../../profiles/balance/s1-m3-capacity-support-alpha.json");
    let mut bounded_balance = vec![3];
    bounded_balance.extend_from_slice(balance_bytes);
    let bounded_balance = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&bounded_balance)))
        .expect("S1-M3 Balance decoder input must not panic");
    assert_eq!(bounded_balance.target, DecoderTarget::BalanceProfile);
    assert_eq!(bounded_balance.payload_len, balance_bytes.len());
    assert!(
        bounded_balance.result.is_ok(),
        "retained Balance v4 must reach an accepted bounded result: {:?}",
        bounded_balance.result
    );
    assert!(
        decode_balance_profile(balance_bytes).is_ok(),
        "retained Balance v4 must remain strictly accepted"
    );

    let replay = include_bytes!("../../../fixtures/replays/s1-m3-c22-capacity-support-v1.json");
    let replay = catch_unwind(AssertUnwindSafe(|| exercise_replay_decoder(replay)))
        .expect("S1-M3 Replay decoder input must not panic");
    assert!(
        replay.result.is_ok(),
        "retained C-22 Replay must remain strictly accepted: {:?}",
        replay.result
    );
}

#[test]
fn s1m4_scenario_balance_and_all_replays_reach_bounded_strict_decoders() {
    let scenario_bytes =
        include_bytes!("../../../fixtures/scenarios/s1-m4-construction-contact-damage-v1.json");
    let mut scenario_input = Vec::with_capacity(scenario_bytes.len() + 1);
    scenario_input.push(0);
    scenario_input.extend_from_slice(scenario_bytes);
    let scenario = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&scenario_input)))
        .expect("S1-M4 Scenario v4 decoder input must not panic");
    assert_eq!(scenario.target, DecoderTarget::Scenario);
    assert_eq!(scenario.payload_len, scenario_bytes.len());
    assert!(
        scenario.result.is_ok(),
        "S1-M4 Scenario v4 must remain strictly accepted: {:?}",
        scenario.result
    );
    assert_eq!(exercise_decoder(&scenario_input), scenario);

    let balance_bytes =
        include_bytes!("../../../profiles/balance/s1-m4-construction-contact-damage-alpha.json");
    let mut balance_input = Vec::with_capacity(balance_bytes.len() + 1);
    balance_input.push(3);
    balance_input.extend_from_slice(balance_bytes);
    let balance = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&balance_input)))
        .expect("S1-M4 Balance v5 decoder input must not panic");
    assert_eq!(balance.target, DecoderTarget::BalanceProfile);
    assert_eq!(balance.payload_len, balance_bytes.len());
    assert!(
        balance.result.is_ok(),
        "S1-M4 Balance v5 must remain strictly accepted: {:?}",
        balance.result
    );
    assert!(
        decode_balance_profile(balance_bytes).is_ok(),
        "S1-M4 Balance v5 must remain accepted by the strict standalone decoder"
    );
    assert_eq!(exercise_decoder(&balance_input), balance);

    for &(name, replay_bytes) in S1M4_REPLAY_ARTIFACTS {
        let replay = catch_unwind(AssertUnwindSafe(|| exercise_replay_decoder(replay_bytes)))
            .unwrap_or_else(|_| panic!("S1-M4 Replay v2 `{name}` decoder input must not panic"));
        assert_eq!(replay.payload_len, replay_bytes.len());
        assert!(
            replay.result.is_ok(),
            "S1-M4 Replay v2 `{name}` must remain strictly accepted: {:?}",
            replay.result
        );
        assert_eq!(
            exercise_replay_decoder(replay_bytes),
            replay,
            "S1-M4 Replay v2 `{name}` must have a deterministic strict decode outcome"
        );
    }
}

#[test]
fn s1m3_capacity_support_corpus_is_bounded_exact_and_property_complete() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        exercise_capacity_support(CAPACITY_SUPPORT_COVERAGE)
    }));
    let Ok(observation) = result else {
        panic!("capacity-support regression corpus panicked");
    };
    assert_eq!(
        observation.execution,
        CapacitySupportExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.consumed_len, CAPACITY_SUPPORT_COVERAGE.len());
    assert_eq!(observation.generated_cases, CAPACITY_SUPPORT_COVERAGE.len());
    assert_eq!(observation.cases.len(), CAPACITY_SUPPORT_COVERAGE.len());
    assert_eq!(
        exercise_capacity_support(CAPACITY_SUPPORT_COVERAGE),
        observation,
        "identical bounded input must reproduce every exact outcome"
    );

    let coverage = observation.coverage;
    assert!(coverage.zero_excess_cases > 0);
    assert!(coverage.active_curve_cases > 0);
    assert!(coverage.fractional_final_ceil_cases > 0);
    assert!(coverage.multi_wire_cases > 0);
    assert!(coverage.nonzero_remainder_cases > 0);
    assert_eq!(
        coverage.permutation_checks,
        u64::try_from(observation.generated_cases).expect("bounded count fits u64")
    );
    assert_eq!(coverage.monotonicity_checks, coverage.permutation_checks);

    let oversized = vec![0xff; 256];
    let truncated = exercise_capacity_support(&oversized);
    assert_eq!(truncated.consumed_len, 128);
    assert_eq!(truncated.generated_cases, 128);
    assert_eq!(truncated.invariant_failure(), None);
}

#[test]
fn s1m4_kernel_corpus_is_bounded_exact_and_property_complete() {
    let observation = catch_unwind(AssertUnwindSafe(|| {
        exercise_s1m4_kernels(S1M4_KERNEL_COVERAGE)
    }))
    .expect("S1-M4 kernel corpus must not panic");
    assert_eq!(
        observation.execution,
        S1m4KernelExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.consumed_len, S1M4_KERNEL_COVERAGE.len());
    assert_eq!(observation.generated_cases, S1M4_KERNEL_COVERAGE.len());
    assert_eq!(
        exercise_s1m4_kernels(S1M4_KERNEL_COVERAGE),
        observation,
        "identical S1-M4 bytes must reproduce all exact observations"
    );
    let coverage = observation.coverage;
    assert!(coverage.gate_work_cases > 0);
    assert!(coverage.junction_work_cases > 0);
    assert!(coverage.wire_work_cases > 0);
    assert!(coverage.substrate_work_cases > 0);
    assert!(coverage.redundant_vertex_checks > 0);
    assert!(coverage.strict_wire_growth_checks > 0);
    assert!(coverage.construction_progress_checks > 0);
    assert!(coverage.live_demand_cases > 0);
    assert!(coverage.live_final_ceil_cases > 0);
    assert!(coverage.contact_allocation_cases > 0);
    assert!(coverage.contact_remainder_cases > 0);
    assert!(coverage.contact_permutation_checks > 0);
    assert!(coverage.contact_conservation_checks > 0);
    assert!(coverage.heat_integration_cases > 0);
    assert!(coverage.damage_cases > 0);
    assert!(coverage.thermal_tie_checks > 0);
    assert_eq!(coverage.order_rejection_checks, 4);
    assert_eq!(coverage.numeric_boundary_checks, 4);
    assert_eq!(coverage.command_tag8_checks, 4);

    let oversized = vec![0xff; 256];
    let truncated = exercise_s1m4_kernels(&oversized);
    assert_eq!(truncated.consumed_len, 128);
    assert_eq!(truncated.generated_cases, 128);
    assert_eq!(truncated.invariant_failure(), None);
}

#[test]
fn s1m4_stateful_runtime_corpus_reaches_construction_c09_and_run_end() {
    let observation = catch_unwind(AssertUnwindSafe(|| {
        exercise_s1m4_runtime(S1M4_RUNTIME_COVERAGE)
    }))
    .expect("S1-M4 runtime corpus must not panic");
    assert_eq!(
        observation.execution,
        S1m4RuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.consumed_len, S1M4_RUNTIME_COVERAGE.len());
    assert_eq!(observation.generated_scenarios, S1M4_RUNTIME_COVERAGE.len());
    assert_eq!(
        exercise_s1m4_runtime(S1M4_RUNTIME_COVERAGE),
        observation,
        "identical S1-M4 runtime bytes must reproduce every State-hash trace"
    );
    let coverage = observation.coverage;
    assert!(coverage.construction_progress > 0);
    assert!(coverage.construction_next_phase0_activation > 0);
    assert!(coverage.construction_fresh_identity > 0);
    assert!(coverage.c09_pending_wire_current_tick > 0);
    assert!(coverage.c09_next_phase0_removal > 0);
    assert!(coverage.c09_stale_arrival > 0);
    assert!(coverage.terminal_tick_commits > 0);
    assert!(coverage.terminal_later_step_rejections > 0);
    assert!(coverage.terminal_read_only_checks > 0);
    assert!(coverage.mutual_lethal_current_tick_completions > 0);
    assert!(coverage.mutual_lethal_next_phase0_removals > 0);
    assert!(coverage.hostile_frame_sensing_only_checks > 0);
    assert_eq!(
        coverage.reproducibility_checks,
        u64::try_from(observation.generated_scenarios).unwrap()
    );

    let oversized = vec![0xff; 32];
    let truncated = exercise_s1m4_runtime(&oversized);
    assert_eq!(truncated.consumed_len, 12);
    assert_eq!(truncated.generated_scenarios, 12);
    assert_eq!(truncated.invariant_failure(), None);
}

#[test]
fn s1m4_mutual_lethal_runtime_completes_both_then_sorts_next_phase0_destruction() {
    let observation = catch_unwind(AssertUnwindSafe(|| exercise_s1m4_runtime(&[3])))
        .expect("S1-M4 mutual-lethal public-runtime case must not panic");
    assert_eq!(
        observation.execution,
        S1m4RuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.consumed_len, 1);
    assert_eq!(observation.generated_scenarios, 1);
    assert_eq!(
        observation.coverage.mutual_lethal_current_tick_completions,
        1
    );
    assert_eq!(observation.coverage.mutual_lethal_next_phase0_removals, 1);
    assert_eq!(observation.coverage.hostile_frame_sensing_only_checks, 0);
    assert_eq!(
        exercise_s1m4_runtime(&[3]),
        observation,
        "the isolated mutual-lethal case must reproduce its State-hash trace"
    );
}

#[test]
fn s1m4_hostile_frame_overlap_changes_only_sense_on_an_armed_live_wire() {
    let observation = catch_unwind(AssertUnwindSafe(|| exercise_s1m4_runtime(&[4])))
        .expect("S1-M4 HostileFrame sensing-only public-runtime case must not panic");
    assert_eq!(
        observation.execution,
        S1m4RuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.consumed_len, 1);
    assert_eq!(observation.generated_scenarios, 1);
    assert_eq!(observation.coverage.hostile_frame_sensing_only_checks, 1);
    assert_eq!(
        observation.coverage.mutual_lethal_current_tick_completions,
        0
    );
    assert_eq!(observation.coverage.mutual_lethal_next_phase0_removals, 0);
    assert_eq!(
        exercise_s1m4_runtime(&[4]),
        observation,
        "the isolated HostileFrame case must reproduce its State-hash trace"
    );
}

#[test]
fn s1m4_heat_integrates_before_exact_next_tick_thermal_damage_and_pending() {
    let input = &S1M4_THERMAL_TWO_TICK_COVERAGE[..1];
    let observation = catch_unwind(AssertUnwindSafe(|| exercise_s1m4_runtime(input)))
        .expect("S1-M4 two-Tick thermal public-runtime case must not panic");
    assert_eq!(
        observation.execution,
        S1m4RuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.consumed_len, 1);
    assert_eq!(observation.generated_scenarios, 1);
    assert_eq!(observation.coverage.thermal_heat_tick_checks, 1);
    assert_eq!(observation.coverage.thermal_next_tick_damage_checks, 1);
    assert_eq!(
        exercise_s1m4_runtime(input),
        observation,
        "the isolated two-Tick thermal case must reproduce its State-hash trace"
    );
}

#[test]
fn geometry_regression_corpus_never_panics_and_is_quantized() {
    for &(name, bytes) in GEOMETRY_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_geometry(bytes)));
        let Ok(observation) = result else {
            panic!("geometry regression case `{name}` panicked");
        };
        assert!(
            observation.validation_results.iter().all(Result::is_ok),
            "geometry regression case `{name}` produced an unquantized point"
        );
    }
}

#[test]
fn replay_regression_corpus_never_panics_and_preserves_acceptance_class() {
    for &(name, bytes, expected_acceptance) in REPLAY_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_replay_decoder(bytes)));
        let Ok(observation) = result else {
            panic!("Replay regression case `{name}` panicked");
        };
        assert_eq!(
            observation.result.is_ok(),
            expected_acceptance,
            "Replay regression case `{name}` changed acceptance class: {:?}",
            observation.result
        );
    }
}

#[test]
fn command_regression_corpus_replays_stateless_and_stateful_targets_without_panics() {
    for &(name, bytes) in COMMAND_CORPUS {
        let stateless = catch_unwind(AssertUnwindSafe(|| exercise_commands(bytes)));
        let Ok(stateless) = stateless else {
            panic!("stateless command regression case `{name}` panicked");
        };
        assert!(
            stateless
                .encodings
                .iter()
                .all(|encoding| encoding.bytes_match),
            "stateless command regression case `{name}` disagreed between encoders"
        );
        assert_eq!(
            stateless.invariant_failure(),
            None,
            "stateless command regression case `{name}` violated a harness invariant"
        );

        let stateful = catch_unwind(AssertUnwindSafe(|| exercise_stateful_commands(bytes)));
        let Ok(stateful) = stateful else {
            panic!("stateful command regression case `{name}` panicked");
        };
        assert_eq!(
            stateful.prefix_reports.len(),
            3,
            "stateful command regression case `{name}` did not finish its prefix"
        );
        assert!(
            stateful
                .encodings
                .iter()
                .all(|encoding| encoding.bytes_match),
            "stateful command regression case `{name}` disagreed between encoders"
        );
        assert_eq!(
            stateful.invariant_failure(),
            None,
            "stateful command regression case `{name}` violated a harness invariant"
        );
    }
}

#[test]
fn s1m4_command_tag8_corpus_reaches_all_target_kinds_and_both_encoders() {
    use aon_fuzz_harness::CommandExecutionObservation;
    use aon_sim::{Command, CommandRejectionReason, ConstructionTarget};

    let cases = &COMMAND_CORPUS[COMMAND_CORPUS.len() - 4..];
    for (expected_kind, &(name, bytes)) in cases.iter().enumerate() {
        let observation = catch_unwind(AssertUnwindSafe(|| exercise_commands(bytes)))
            .unwrap_or_else(|_| panic!("S1-M4 Command tag-8 corpus `{name}` panicked"));
        assert_eq!(observation.envelopes.len(), 1);
        assert_eq!(observation.variant_mask, 1 << 8);
        assert_eq!(observation.invariant_failure(), None);
        assert_eq!(observation.encodings.len(), 1);
        assert!(observation.encodings[0].bytes_match);
        assert!(observation.encodings[0].allocated_result.is_ok());
        assert!(observation.encodings[0].streamed_result.is_ok());
        let CommandExecutionObservation::Stepped(Ok(report)) = &observation.execution else {
            panic!("S1-M4 Command corpus `{name}` did not reach public Simulation")
        };
        assert!(report.command_acceptances.is_empty());
        assert_eq!(report.command_rejections.len(), 1);
        assert_eq!(
            report.command_rejections[0].reason,
            CommandRejectionReason::UnsupportedPlacement
        );
        let Command::PlaceConstructionSite(command) = &observation.envelopes[0].command else {
            panic!("S1-M4 Command corpus `{name}` did not select tag 8")
        };
        let actual_kind = match command.target {
            ConstructionTarget::Gate { .. } => 0,
            ConstructionTarget::Wire { .. } => 1,
            ConstructionTarget::Junction { .. } => 2,
            ConstructionTarget::FixedSubstrate { .. } => 3,
        };
        assert_eq!(actual_kind, expected_kind, "wrong target kind in `{name}`");
        assert_eq!(
            exercise_commands(bytes),
            observation,
            "S1-M4 Command tag-8 corpus `{name}` must be deterministic"
        );
    }
}

#[test]
fn retained_stateful_case_reaches_effective_bind_remove_tombstone_and_wrong_kind() {
    use aon_fuzz_harness::StatefulCommandExecutionObservation;
    use aon_sim::{CommandAcceptance, CommandRejection, CommandRejectionReason, Tick};

    let observation = exercise_stateful_commands(STATEFUL_EFFECTIVE_PATHS);
    assert_eq!(observation.variant_mask, 0x60);
    let StatefulCommandExecutionObservation::Stepped(Ok(report)) = observation.execution else {
        panic!("retained stateful path case must complete with ordinary command results");
    };
    assert_eq!(
        report.command_acceptances,
        vec![
            CommandAcceptance {
                target_tick: Tick(3),
                ordinal: 0,
                created_entity: None,
            },
            CommandAcceptance {
                target_tick: Tick(3),
                ordinal: 3,
                created_entity: None,
            },
        ]
    );
    assert_eq!(
        report.command_rejections,
        vec![
            CommandRejection {
                target_tick: Tick(3),
                ordinal: 1,
                reason: CommandRejectionReason::InvalidPortBinding,
            },
            CommandRejection {
                target_tick: Tick(3),
                ordinal: 2,
                reason: CommandRejectionReason::RemovedEntity,
            },
        ]
    );
    assert!(report.topology_changed);
    assert_eq!(report.next_tick, Tick(4));
}

#[test]
fn signal_runtime_regression_corpus_replays_without_panics_or_silent_run_errors() {
    for &(name, bytes) in SIGNAL_RUNTIME_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_signal_runtime(bytes)));
        let Ok(observation) = result else {
            panic!("signal-runtime regression case `{name}` panicked");
        };
        assert_eq!(
            observation.invariant_failure(),
            None,
            "signal-runtime regression case `{name}` violated a harness invariant"
        );
        assert_eq!(
            observation.execution,
            SignalRuntimeExecutionObservation::Completed,
            "signal-runtime regression case `{name}` did not complete"
        );
        assert_eq!(
            observation.state_hashes.len(),
            observation.generated_steps,
            "signal-runtime regression case `{name}` skipped a state-hash observation"
        );
        assert!(
            observation
                .encodings
                .iter()
                .all(|encoding| encoding.bytes_match),
            "signal-runtime regression case `{name}` disagreed between command encoders"
        );
    }
}

#[test]
fn retained_signal_case_reaches_s0_m3_fuzz_completion_paths() {
    let observation = exercise_signal_runtime(SIGNAL_RUNTIME_COVERAGE);
    assert_eq!(
        observation.execution,
        SignalRuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.prefix_reports.len(), 12);
    assert_eq!(observation.generated_steps, SIGNAL_RUNTIME_COVERAGE.len());

    let coverage = observation.coverage;
    assert!(coverage.valid_external_updates > 0);
    assert!(coverage.removed_driver_attempts > 0);
    assert!(coverage.wrong_kind_driver_attempts > 0);
    assert!(coverage.predicted_driver_attempts > 0);
    assert!(coverage.simultaneous_update_batches > 0);
    assert!(coverage.simultaneous_driver_event_batches > 0);
    assert!(coverage.coalesced_update_batches > 0);
    assert!(coverage.permuted_insertion_batches > 0);
    assert!(coverage.max_strength_updates > 0);
    assert!(coverage.driver_transitions_applied > 0);
    assert!(coverage.signal_arrivals_applied > 0);
    assert!(coverage.sinks_resolved > 0);
    assert!(coverage.driver_changes > 0);
    assert!(coverage.signal_changes > 0);
    assert!(coverage.gate_output_changes > 0);
    assert!(coverage.wire_excitation_changes > 0);
    assert!(coverage.pending_gate_observations > 0);
    assert!(coverage.nonzero_wire_observations > 0);
}

#[test]
fn topology_runtime_regression_corpus_replays_without_panics_or_silent_run_errors() {
    for &(name, bytes) in TOPOLOGY_RUNTIME_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_topology_runtime(bytes)));
        let Ok(observation) = result else {
            panic!("topology-runtime regression case `{name}` panicked");
        };
        assert_eq!(
            observation.invariant_failure(),
            None,
            "topology-runtime regression case `{name}` violated a harness invariant"
        );
        assert_eq!(
            observation.execution,
            TopologyRuntimeExecutionObservation::Completed,
            "topology-runtime regression case `{name}` did not complete"
        );
        assert_eq!(
            observation.state_hashes.len(),
            observation.generated_steps,
            "topology-runtime regression case `{name}` skipped a per-Tick hash"
        );
        assert!(
            observation.encodings.iter().all(|encoding| {
                encoding.allocated_result.is_ok()
                    && encoding.streamed_result.is_ok()
                    && encoding.bytes_match
            }),
            "topology-runtime regression case `{name}` failed command encoding"
        );
    }
}

#[test]
fn retained_topology_case_reaches_verified_s0_m4_paths() {
    let observation = exercise_topology_runtime(TOPOLOGY_RUNTIME_COVERAGE);
    assert_eq!(
        observation.execution,
        TopologyRuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(
        observation.generated_scenarios,
        TOPOLOGY_RUNTIME_COVERAGE.len()
    );
    assert_eq!(observation.state_hashes.len(), observation.generated_steps);

    let coverage = observation.coverage;
    assert_eq!(
        coverage.completed_scenarios,
        u64::try_from(observation.generated_scenarios).expect("bounded scenario count fits u64")
    );
    assert!(coverage.permuted_command_batches > 0);
    assert!(coverage.routes_added > 0);
    assert!(coverage.routes_removed > 0);
    assert!(coverage.routes_retained > 0);
    assert!(coverage.routes_replaced > 0);
    assert!(coverage.topology_sync_arrivals_staged > 0);
    assert!(coverage.stale_revision_arrivals > 0);
    assert!(coverage.invalid_path_arrivals > 0);
    assert!(coverage.add_revision_race_outcomes > 0);
    assert!(coverage.remove_in_flight_outcomes > 0);
    assert!(coverage.rebind_in_flight_outcomes > 0);
    assert!(coverage.bind_away_back_outcomes > 0);
    assert!(coverage.rebuild_outcomes > 0);
    assert!(coverage.unrelated_edit_outcomes > 0);
    assert!(coverage.removed_slot_outcomes > 0);
    assert!(coverage.checked_max_sample_outcomes > 0);
    assert!(coverage.slot_revision_observations > 0);
}

#[test]
fn experiment_regression_corpus_never_panics_and_preserves_exact_outcome() {
    for &(name, bytes, expected_acceptance) in EXPERIMENT_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_experiment_decoder(bytes)));
        let Ok(observation) = result else {
            panic!("Experiment regression case `{name}` panicked");
        };
        assert_eq!(
            observation.result.is_ok(),
            expected_acceptance,
            "Experiment regression case `{name}` changed acceptance class: {:?}",
            observation.result
        );
        assert_eq!(
            exercise_experiment_decoder(bytes),
            observation,
            "Experiment regression case `{name}` changed its typed outcome on replay"
        );
    }
}

#[test]
fn module_regression_corpus_never_panics_and_preserves_exact_outcome() {
    for &(name, bytes, expected_acceptance) in MODULE_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_module_decoder(bytes)));
        let Ok(observation) = result else {
            panic!("Module regression case `{name}` panicked");
        };
        assert_eq!(
            observation.result.is_ok(),
            expected_acceptance,
            "Module regression case `{name}` changed acceptance class: {:?}",
            observation.result
        );
        assert_eq!(
            exercise_module_decoder(bytes),
            observation,
            "Module regression case `{name}` changed its typed outcome on replay"
        );
    }
}

#[test]
fn mobility_runtime_regression_corpus_replays_without_panics_or_silent_run_errors() {
    for &(name, bytes) in MOBILITY_RUNTIME_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_mobility_runtime(bytes)));
        let Ok(observation) = result else {
            panic!("mobility-runtime regression case `{name}` panicked");
        };
        assert_eq!(
            observation.invariant_failure(),
            None,
            "mobility-runtime regression case `{name}` violated a harness invariant"
        );
        assert_eq!(
            observation.execution,
            MobilityRuntimeExecutionObservation::Completed,
            "mobility-runtime regression case `{name}` did not complete"
        );
        assert_eq!(
            observation.state_hashes.len(),
            observation.generated_steps,
            "mobility-runtime regression case `{name}` skipped a per-Tick hash"
        );
        assert!(
            observation.encodings.iter().all(|encoding| {
                encoding.allocated_result.is_ok()
                    && encoding.streamed_result.is_ok()
                    && encoding.bytes_match
            }),
            "mobility-runtime regression case `{name}` failed command encoding"
        );
    }
}

#[test]
fn retained_mobility_case_reaches_verified_s0_m7_paths() {
    let observation = exercise_mobility_runtime(MOBILITY_RUNTIME_COVERAGE);
    assert_eq!(
        observation.execution,
        MobilityRuntimeExecutionObservation::Completed
    );
    assert_eq!(observation.invariant_failure(), None);
    assert_eq!(observation.generated_scenarios, 8);
    assert_eq!(observation.state_hashes.len(), observation.generated_steps);

    let coverage = observation.coverage;
    assert_eq!(coverage.completed_scenarios, 8);
    assert!(coverage.permuted_command_batches > 0);
    assert_eq!(coverage.mobile_placements, 8);
    assert_eq!(coverage.placement_rejections, 1);
    assert_eq!(coverage.mobile_port_bindings, 4);
    assert_eq!(coverage.explicit_track_bindings, 2);
    assert_eq!(coverage.occupied_track_rejections, 2);
    assert_eq!(coverage.mobile_removals, 1);
    assert_eq!(coverage.track_removals, 2);
    assert_eq!(coverage.straight_turns, 1);
    assert_eq!(coverage.left_turns, 1);
    assert_eq!(coverage.right_turns, 1);
    assert_eq!(coverage.reverse_turns, 1);
    assert!(coverage.movement_observations > 0);
    assert_eq!(coverage.checked_maximum_coordinate_paths, 1);
    assert_eq!(coverage.checked_minimum_coordinate_paths, 1);
}
