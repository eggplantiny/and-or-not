use aon_fuzz_harness::{
    CommandExecutionObservation, MAX_COMMAND_INPUT_BYTES, MAX_DECODER_INPUT_BYTES,
    MAX_GEOMETRY_INPUT_BYTES, StatefulCommandExecutionObservation, exercise_commands,
    exercise_decoder, exercise_geometry, exercise_stateful_commands,
};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("both"));
    let input_limit = MAX_DECODER_INPUT_BYTES
        .max(MAX_GEOMETRY_INPUT_BYTES)
        .max(MAX_COMMAND_INPUT_BYTES);
    let mut input = Vec::new();
    std::io::stdin()
        .take(u64::try_from(input_limit)?)
        .read_to_end(&mut input)?;

    match mode.as_str() {
        "decoder" => print_decoder(&input),
        "geometry" => print_geometry(&input),
        "commands" => print_commands(&input)?,
        "both" => {
            print_decoder(&input);
            print_geometry(&input);
        }
        "all" => {
            print_decoder(&input);
            print_geometry(&input);
            print_commands(&input)?;
        }
        _ => {
            return Err(format!(
                "unknown mode `{mode}`; expected decoder, geometry, commands, both, or all"
            )
            .into());
        }
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
