use aon_fuzz_harness::{exercise_decoder, exercise_geometry};
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
