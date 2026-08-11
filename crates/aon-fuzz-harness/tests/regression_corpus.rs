use aon_fuzz_harness::{
    SignalRuntimeExecutionObservation, TopologyRuntimeExecutionObservation, exercise_commands,
    exercise_decoder, exercise_geometry, exercise_signal_runtime, exercise_stateful_commands,
    exercise_topology_runtime,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

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

#[test]
fn decoder_regression_corpus_never_panics() {
    for &(name, bytes) in DECODER_CORPUS {
        let result = catch_unwind(AssertUnwindSafe(|| exercise_decoder(bytes)));
        assert!(result.is_ok(), "decoder regression case `{name}` panicked");
    }
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
