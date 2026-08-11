#![forbid(unsafe_code)]

use aon_sim::{
    ArtifactBytes, Fixed, FixedVec2, GeometryError, NumericError, PackageError, decode_package,
    decode_scenario_manifest, polyline_length, validate_quantized,
};

const REFERENCE_SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const REFERENCE_NUMERIC_PROFILE: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const REFERENCE_PHYSICAL_SCALE_PROFILE: &[u8] =
    include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const REFERENCE_BALANCE_PROFILE: &[u8] =
    include_bytes!("../../../profiles/balance/stage0-alpha.json");

/// Maximum number of bytes interpreted by one decoder invocation.
pub const MAX_DECODER_INPUT_BYTES: usize = 16 * 1024;

/// Maximum number of bytes interpreted by one geometry invocation.
pub const MAX_GEOMETRY_INPUT_BYTES: usize = 4 * 1024;

/// Maximum number of points constructed by one geometry invocation.
pub const MAX_POLYLINE_POINTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoderTarget {
    Scenario,
    NumericProfile,
    PhysicalScaleProfile,
    BalanceProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoderObservation {
    pub target: DecoderTarget,
    pub payload_len: usize,
    pub result: Result<(), PackageError>,
}

/// Interprets an arbitrary byte stream as one of the four public artifact decode paths.
///
/// The low two bits of the first byte select the target. The remaining, bounded bytes are
/// supplied to that decoder. Profile targets use the checked-in reference artifacts for the
/// other package members so the selected profile decoder is always reached.
pub fn exercise_decoder(input: &[u8]) -> DecoderObservation {
    let bounded = &input[..input.len().min(MAX_DECODER_INPUT_BYTES)];
    let selector = bounded.first().copied().unwrap_or(0);
    let payload = bounded.get(1..).unwrap_or_default();
    let target = match selector & 0b11 {
        0 => DecoderTarget::Scenario,
        1 => DecoderTarget::NumericProfile,
        2 => DecoderTarget::PhysicalScaleProfile,
        _ => DecoderTarget::BalanceProfile,
    };

    let result = match target {
        DecoderTarget::Scenario => decode_scenario_manifest(payload).map(|_| ()),
        DecoderTarget::NumericProfile => decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: payload,
            physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
            balance_profile: REFERENCE_BALANCE_PROFILE,
        })
        .map(|_| ()),
        DecoderTarget::PhysicalScaleProfile => decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: REFERENCE_NUMERIC_PROFILE,
            physical_scale_profile: payload,
            balance_profile: REFERENCE_BALANCE_PROFILE,
        })
        .map(|_| ()),
        DecoderTarget::BalanceProfile => decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: REFERENCE_NUMERIC_PROFILE,
            physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
            balance_profile: payload,
        })
        .map(|_| ()),
    };

    DecoderObservation {
        target,
        payload_len: payload.len(),
        result,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryObservation {
    pub consumed_len: usize,
    pub quantum: Fixed,
    pub point_count: usize,
    pub validation_results: Vec<Result<(), GeometryError>>,
    pub length: Result<Fixed, NumericError>,
}

/// Maps an arbitrary byte stream to a bounded, quantized polyline and executes its validation
/// and canonical length paths.
///
/// Bytes are consumed cyclically, which makes even short inputs useful while keeping the mapping
/// deterministic. The quantum is a positive power of two up to `2^31`; coordinates are signed
/// 32-bit quantum multiples, so construction itself cannot overflow `i64`.
pub fn exercise_geometry(input: &[u8]) -> GeometryObservation {
    let bounded = &input[..input.len().min(MAX_GEOMETRY_INPUT_BYTES)];
    let mut cursor = CyclicBytes::new(bounded);
    let point_count = usize::from(cursor.next_u8()) % (MAX_POLYLINE_POINTS + 1);
    let quantum = Fixed(1_i64 << (cursor.next_u8() % 32));
    let mut points = Vec::with_capacity(point_count);

    for _ in 0..point_count {
        let x = i64::from(cursor.next_i32()) * quantum.0;
        let y = i64::from(cursor.next_i32()) * quantum.0;
        points.push(FixedVec2::new(Fixed(x), Fixed(y)));
    }

    let validation_results = points
        .iter()
        .copied()
        .map(|point| validate_quantized(point, quantum))
        .collect();
    let length = polyline_length(&points);

    GeometryObservation {
        consumed_len: bounded.len(),
        quantum,
        point_count,
        validation_results,
        length,
    }
}

struct CyclicBytes<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> CyclicBytes<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn next_u8(&mut self) -> u8 {
        if self.bytes.is_empty() {
            return 0;
        }
        let value = self.bytes[self.position % self.bytes.len()];
        self.position += 1;
        value
    }

    fn next_i32(&mut self) -> i32 {
        i32::from_le_bytes([
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecoderTarget, MAX_DECODER_INPUT_BYTES, MAX_GEOMETRY_INPUT_BYTES,
        REFERENCE_BALANCE_PROFILE, REFERENCE_NUMERIC_PROFILE, REFERENCE_PHYSICAL_SCALE_PROFILE,
        REFERENCE_SCENARIO, exercise_decoder, exercise_geometry,
    };
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn decoder_selector_reaches_every_typed_path() {
        assert_eq!(exercise_decoder(&[0]).target, DecoderTarget::Scenario);
        assert_eq!(exercise_decoder(&[1]).target, DecoderTarget::NumericProfile);
        assert_eq!(
            exercise_decoder(&[2]).target,
            DecoderTarget::PhysicalScaleProfile
        );
        assert_eq!(exercise_decoder(&[3]).target, DecoderTarget::BalanceProfile);
    }

    #[test]
    fn each_selected_reference_artifact_is_accepted() {
        for (selector, artifact) in [
            (0_u8, REFERENCE_SCENARIO),
            (1, REFERENCE_NUMERIC_PROFILE),
            (2, REFERENCE_PHYSICAL_SCALE_PROFILE),
            (3, REFERENCE_BALANCE_PROFILE),
        ] {
            let mut input = Vec::with_capacity(artifact.len() + 1);
            input.push(selector);
            input.extend_from_slice(artifact);
            assert!(
                exercise_decoder(&input).result.is_ok(),
                "reference artifact selected by {selector} was rejected"
            );
        }
    }

    #[test]
    fn arbitrary_geometry_is_quantized_by_construction() {
        let observation = exercise_geometry(b"deterministic quantized polyline");
        assert!(observation.validation_results.iter().all(Result::is_ok));
    }

    #[test]
    fn oversized_inputs_are_ignored_after_the_documented_limits() {
        let mut decoder = vec![0x5a; MAX_DECODER_INPUT_BYTES];
        let expected_decoder = exercise_decoder(&decoder);
        decoder.extend_from_slice(b"ignored decoder suffix");
        assert_eq!(exercise_decoder(&decoder), expected_decoder);

        let mut geometry = vec![0xa5; MAX_GEOMETRY_INPUT_BYTES];
        let expected_geometry = exercise_geometry(&geometry);
        geometry.extend_from_slice(b"ignored geometry suffix");
        assert_eq!(exercise_geometry(&geometry), expected_geometry);
    }

    #[test]
    fn deterministic_random_streams_do_not_panic() {
        let mut state = 0xd1b5_4a32_d192_ed03_u64;
        for case_index in 0..2_048_usize {
            let length = case_index % 257;
            let mut bytes = vec![0_u8; length];
            for byte in &mut bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state.to_le_bytes()[0];
            }

            let decoder = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&bytes)));
            assert!(
                decoder.is_ok(),
                "decoder panicked for generated case {case_index}"
            );

            let geometry = catch_unwind(AssertUnwindSafe(|| exercise_geometry(&bytes)));
            assert!(
                geometry.is_ok(),
                "geometry panicked for generated case {case_index}"
            );
        }
    }
}
