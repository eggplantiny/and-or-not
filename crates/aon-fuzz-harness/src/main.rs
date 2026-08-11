use aon_fuzz_harness::{
    MAX_DECODER_INPUT_BYTES, MAX_GEOMETRY_INPUT_BYTES, exercise_decoder, exercise_geometry,
};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("both"));
    let input_limit = MAX_DECODER_INPUT_BYTES.max(MAX_GEOMETRY_INPUT_BYTES);
    let mut input = Vec::new();
    std::io::stdin()
        .take(u64::try_from(input_limit)?)
        .read_to_end(&mut input)?;

    match mode.as_str() {
        "decoder" => print_decoder(&input),
        "geometry" => print_geometry(&input),
        "both" => {
            print_decoder(&input);
            print_geometry(&input);
        }
        _ => {
            return Err(
                format!("unknown mode `{mode}`; expected decoder, geometry, or both").into(),
            );
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
