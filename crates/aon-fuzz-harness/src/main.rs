use aon_fuzz_harness::{
    CommandExecutionObservation, MAX_COMMAND_INPUT_BYTES, MAX_DECODER_INPUT_BYTES,
    MAX_GEOMETRY_INPUT_BYTES, MAX_MOBILITY_RUNTIME_INPUT_BYTES, MAX_REPLAY_INPUT_BYTES,
    MAX_SIGNAL_RUNTIME_INPUT_BYTES, MAX_TOPOLOGY_RUNTIME_INPUT_BYTES,
    MobilityRuntimeExecutionObservation, SignalRuntimeExecutionObservation,
    StatefulCommandExecutionObservation, TopologyRuntimeExecutionObservation, exercise_commands,
    exercise_decoder, exercise_geometry, exercise_mobility_runtime, exercise_replay_decoder,
    exercise_signal_runtime, exercise_stateful_commands, exercise_topology_runtime,
};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("both"));
    let input_limit = MAX_DECODER_INPUT_BYTES
        .max(MAX_GEOMETRY_INPUT_BYTES)
        .max(MAX_REPLAY_INPUT_BYTES)
        .max(MAX_COMMAND_INPUT_BYTES)
        .max(MAX_SIGNAL_RUNTIME_INPUT_BYTES)
        .max(MAX_MOBILITY_RUNTIME_INPUT_BYTES)
        .max(MAX_TOPOLOGY_RUNTIME_INPUT_BYTES);
    let mut input = Vec::new();
    std::io::stdin()
        .take(u64::try_from(input_limit)?)
        .read_to_end(&mut input)?;

    match mode.as_str() {
        "decoder" => print_decoder(&input),
        "geometry" => print_geometry(&input),
        "replay" => print_replay(&input),
        "commands" => print_commands(&input)?,
        "signal" => print_signal_runtime(&input)?,
        "topology" => print_topology_runtime(&input)?,
        "mobility" => print_mobility_runtime(&input)?,
        "both" => {
            print_decoder(&input);
            print_geometry(&input);
            print_replay(&input);
        }
        "all" => {
            print_decoder(&input);
            print_geometry(&input);
            print_commands(&input)?;
            print_signal_runtime(&input)?;
            print_topology_runtime(&input)?;
            print_mobility_runtime(&input)?;
        }
        _ => {
            return Err(format!(
                "unknown mode `{mode}`; expected decoder, replay, geometry, commands, signal, topology, mobility, both, or all"
            )
            .into());
        }
    }
    Ok(())
}

fn print_mobility_runtime(input: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let observation = exercise_mobility_runtime(input);
    let coverage = observation.coverage;
    println!(
        "mobility input_bytes={} scenarios={} steps={} encodings={} hashes={} result={} permuted={} placed={} placement_rejections={} mobile_bindings={} track_bindings={} occupied={} mobile_removals={} track_removals={} straight={} left={} right={} reverse={} movements={} checked_max={} checked_min={}",
        observation.consumed_len,
        observation.generated_scenarios,
        observation.generated_steps,
        observation.encodings.len(),
        observation.state_hashes.len(),
        mobility_runtime_result(&observation.execution),
        coverage.permuted_command_batches,
        coverage.mobile_placements,
        coverage.placement_rejections,
        coverage.mobile_port_bindings,
        coverage.explicit_track_bindings,
        coverage.occupied_track_rejections,
        coverage.mobile_removals,
        coverage.track_removals,
        coverage.straight_turns,
        coverage.left_turns,
        coverage.right_turns,
        coverage.reverse_turns,
        coverage.movement_observations,
        coverage.checked_maximum_coordinate_paths,
        coverage.checked_minimum_coordinate_paths,
    );
    if let Some(failure) = observation.invariant_failure() {
        return Err(format!("mobility-runtime harness invariant failed: {failure}").into());
    }
    Ok(())
}

fn print_replay(input: &[u8]) {
    let observation = exercise_replay_decoder(input);
    match observation.result {
        Ok(()) => println!(
            "replay payload_bytes={} result=accepted",
            observation.payload_len
        ),
        Err(error) => println!(
            "replay payload_bytes={} result=rejected error={error}",
            observation.payload_len
        ),
    }
}

fn print_topology_runtime(input: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let observation = exercise_topology_runtime(input);
    let coverage = observation.coverage;
    println!(
        "topology input_bytes={} scenarios={} steps={} encodings={} hashes={} result={} permuted={} added={} removed={} retained={} replaced={} sync={} stale={} invalid_path={} slot_revisions={}",
        observation.consumed_len,
        observation.generated_scenarios,
        observation.generated_steps,
        observation.encodings.len(),
        observation.state_hashes.len(),
        topology_runtime_result(&observation.execution),
        coverage.permuted_command_batches,
        coverage.routes_added,
        coverage.routes_removed,
        coverage.routes_retained,
        coverage.routes_replaced,
        coverage.topology_sync_arrivals_staged,
        coverage.stale_revision_arrivals,
        coverage.invalid_path_arrivals,
        coverage.slot_revision_observations,
    );
    if let Some(failure) = observation.invariant_failure() {
        return Err(format!("topology-runtime harness invariant failed: {failure}").into());
    }
    Ok(())
}

fn print_signal_runtime(input: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let observation = exercise_signal_runtime(input);
    let coverage = observation.coverage;
    println!(
        "signal input_bytes={} prefix_steps={} generated_steps={} encodings={} hashes={} result={} valid_updates={} removed={} wrong_kind={} predicted={} simultaneous={} ordered_events={} coalesced={} max_strength={} driver_events={} arrivals={} gate_changes={} wire_changes={}",
        observation.consumed_len,
        observation.prefix_reports.len(),
        observation.generated_steps,
        observation.encodings.len(),
        observation.state_hashes.len(),
        signal_runtime_result(&observation.execution),
        coverage.valid_external_updates,
        coverage.removed_driver_attempts,
        coverage.wrong_kind_driver_attempts,
        coverage.predicted_driver_attempts,
        coverage.simultaneous_update_batches,
        coverage.simultaneous_driver_event_batches,
        coverage.coalesced_update_batches,
        coverage.max_strength_updates,
        coverage.driver_transitions_applied,
        coverage.signal_arrivals_applied,
        coverage.gate_output_changes,
        coverage.wire_excitation_changes,
    );
    if let Some(failure) = observation.invariant_failure() {
        return Err(format!("signal-runtime harness invariant failed: {failure}").into());
    }
    Ok(())
}

fn print_decoder(input: &[u8]) {
    let observation = exercise_decoder(input);
    match observation.result {
        Ok(()) => println!(
            "decoder target={:?} payload_bytes={} result=accepted",
            observation.target, observation.payload_len
        ),
        Err(error) => println!(
            "decoder target={:?} payload_bytes={} result=rejected error={error}",
            observation.target, observation.payload_len
        ),
    }
}

fn print_geometry(input: &[u8]) {
    let observation = exercise_geometry(input);
    let invalid_points = observation
        .validation_results
        .iter()
        .filter(|result| result.is_err())
        .count();
    println!(
        "geometry input_bytes={} points={} quantum={} invalid_points={} length={:?}",
        observation.consumed_len,
        observation.point_count,
        observation.quantum.0,
        invalid_points,
        observation.length
    );
}

fn print_commands(input: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let stateless = exercise_commands(input);
    let stateless_mismatches = stateless
        .encodings
        .iter()
        .filter(|encoding| !encoding.bytes_match)
        .count();
    println!(
        "commands target=stateless input_bytes={} envelopes={} variant_mask=0x{:02x} encoding_mismatches={} result={}",
        stateless.consumed_len,
        stateless.envelopes.len(),
        stateless.variant_mask,
        stateless_mismatches,
        command_result(&stateless.execution)
    );
    if let Some(failure) = stateless.invariant_failure() {
        return Err(format!("stateless command harness invariant failed: {failure}").into());
    }

    let stateful = exercise_stateful_commands(input);
    let stateful_mismatches = stateful
        .encodings
        .iter()
        .filter(|encoding| !encoding.bytes_match)
        .count();
    println!(
        "commands target=stateful input_bytes={} prefix_steps={} envelopes={} variant_mask=0x{:02x} encoding_mismatches={} result={}",
        stateful.consumed_len,
        stateful.prefix_reports.len(),
        stateful.envelopes.len(),
        stateful.variant_mask,
        stateful_mismatches,
        stateful_command_result(&stateful.execution)
    );
    if let Some(failure) = stateful.invariant_failure() {
        return Err(format!("stateful command harness invariant failed: {failure}").into());
    }
    Ok(())
}

const fn command_result(execution: &CommandExecutionObservation) -> &'static str {
    match execution {
        CommandExecutionObservation::PackageRejected(_) => "package-rejected",
        CommandExecutionObservation::SimulationRejected(_) => "simulation-rejected",
        CommandExecutionObservation::Stepped(Ok(_)) => "stepped",
        CommandExecutionObservation::Stepped(Err(_)) => "run-error",
    }
}

const fn stateful_command_result(execution: &StatefulCommandExecutionObservation) -> &'static str {
    match execution {
        StatefulCommandExecutionObservation::PackageRejected(_) => "package-rejected",
        StatefulCommandExecutionObservation::SimulationRejected(_) => "simulation-rejected",
        StatefulCommandExecutionObservation::PrefixRunError { .. } => "prefix-run-error",
        StatefulCommandExecutionObservation::PrefixCommandRejected { .. } => {
            "prefix-command-rejected"
        }
        StatefulCommandExecutionObservation::Stepped(Ok(_)) => "stepped",
        StatefulCommandExecutionObservation::Stepped(Err(_)) => "run-error",
    }
}

const fn signal_runtime_result(execution: &SignalRuntimeExecutionObservation) -> &'static str {
    match execution {
        SignalRuntimeExecutionObservation::PackageRejected(_) => "package-rejected",
        SignalRuntimeExecutionObservation::SimulationRejected { .. } => "simulation-rejected",
        SignalRuntimeExecutionObservation::PrefixRunError { .. } => "prefix-run-error",
        SignalRuntimeExecutionObservation::PrefixCommandRejected { .. } => {
            "prefix-command-rejected"
        }
        SignalRuntimeExecutionObservation::PrefixInvariantViolation { .. } => {
            "prefix-invariant-violation"
        }
        SignalRuntimeExecutionObservation::DeterminismMismatch { .. } => "determinism-mismatch",
        SignalRuntimeExecutionObservation::RunError { .. } => "run-error",
        SignalRuntimeExecutionObservation::Completed => "completed",
    }
}

const fn topology_runtime_result(execution: &TopologyRuntimeExecutionObservation) -> &'static str {
    match execution {
        TopologyRuntimeExecutionObservation::PackageRejected(_) => "package-rejected",
        TopologyRuntimeExecutionObservation::SimulationRejected { .. } => "simulation-rejected",
        TopologyRuntimeExecutionObservation::RunError { .. } => "run-error",
        TopologyRuntimeExecutionObservation::EncoderMismatch { .. } => "encoder-mismatch",
        TopologyRuntimeExecutionObservation::DeterminismMismatch { .. } => "determinism-mismatch",
        TopologyRuntimeExecutionObservation::ExpectationMismatch { .. } => "expectation-mismatch",
        TopologyRuntimeExecutionObservation::Completed => "completed",
    }
}

const fn mobility_runtime_result(execution: &MobilityRuntimeExecutionObservation) -> &'static str {
    match execution {
        MobilityRuntimeExecutionObservation::PackageRejected(_) => "package-rejected",
        MobilityRuntimeExecutionObservation::SimulationRejected { .. } => "simulation-rejected",
        MobilityRuntimeExecutionObservation::RunError { .. } => "run-error",
        MobilityRuntimeExecutionObservation::EncoderMismatch { .. } => "encoder-mismatch",
        MobilityRuntimeExecutionObservation::DeterminismMismatch { .. } => "determinism-mismatch",
        MobilityRuntimeExecutionObservation::ExpectationMismatch { .. } => "expectation-mismatch",
        MobilityRuntimeExecutionObservation::Completed => "completed",
    }
}
