use aon_sim::{
    ArtifactHash, ReferenceArchitectureError, ReferenceExperimentError, ReferenceMetricError,
    ReferenceMetricSetArtifact, ReferenceResponseObservationSpec,
    decode_reference_architecture_artifact, decode_reference_experiment_plan_v2,
    decode_reference_metric_artifact, decode_reference_metric_set_artifact,
    decode_reference_pair_manifest, encode_reference_architecture_artifact,
    encode_reference_experiment_plan_v2, encode_reference_metric_artifact,
    encode_reference_metric_set_artifact, encode_reference_pair_manifest,
};

/// Maximum number of bytes interpreted by one S1-M5 reference-artifact invocation.
pub const MAX_S1M5_REFERENCE_INPUT_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum S1m5ReferenceTarget {
    Architecture,
    Pair,
    ExperimentPlan,
    MetricSet,
    MetricArtifact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct S1m5ReferenceCanonicalObservation {
    pub canonical_len: usize,
    pub semantic_hash: Option<ArtifactHash>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct S1m5ReferenceObservation {
    pub target: S1m5ReferenceTarget,
    pub payload_len: usize,
    /// A strict decoder rejection is an ordinary arbitrary-input outcome. An accepted value is
    /// reported only after a second decode/encode pass produces identical canonical bytes.
    pub result: Result<S1m5ReferenceCanonicalObservation, String>,
}

/// Supplies a bounded arbitrary byte stream to all five new S1-M5 strict artifact boundaries.
///
/// Byte zero selects the target. The remaining bytes are never repaired or lossy-decoded. When a
/// document is accepted, the harness requires canonical encoding to reach a byte-stable fixed
/// point and, for hash-bearing artifacts, requires the semantic hash to survive that round trip.
pub fn exercise_s1m5_reference_artifacts(input: &[u8]) -> S1m5ReferenceObservation {
    let bounded = &input[..input.len().min(MAX_S1M5_REFERENCE_INPUT_BYTES)];
    let selector = bounded.first().copied().unwrap_or(0);
    let payload = bounded.get(1..).unwrap_or_default();
    let target = match selector % 5 {
        0 => S1m5ReferenceTarget::Architecture,
        1 => S1m5ReferenceTarget::Pair,
        2 => S1m5ReferenceTarget::ExperimentPlan,
        3 => S1m5ReferenceTarget::MetricSet,
        _ => S1m5ReferenceTarget::MetricArtifact,
    };
    let result = match target {
        S1m5ReferenceTarget::Architecture => exercise_architecture(payload),
        S1m5ReferenceTarget::Pair => exercise_pair(payload),
        S1m5ReferenceTarget::ExperimentPlan => exercise_plan(payload),
        S1m5ReferenceTarget::MetricSet => exercise_metric_set(payload),
        S1m5ReferenceTarget::MetricArtifact => exercise_metric_artifact(payload),
    };
    S1m5ReferenceObservation {
        target,
        payload_len: payload.len(),
        result,
    }
}

fn exercise_architecture(payload: &[u8]) -> Result<S1m5ReferenceCanonicalObservation, String> {
    let source = std::str::from_utf8(payload)
        .map_err(|error| format!("invalid Reference Architecture UTF-8: {error}"))?;
    let decoded = decode_reference_architecture_artifact(source).map_err(display_architecture)?;
    let first_hash = decoded.semantic_hash().map_err(display_architecture)?;
    let first_plan = decoded
        .materialization_plan()
        .map_err(display_architecture)?;
    let canonical =
        encode_reference_architecture_artifact(&decoded).map_err(display_architecture)?;
    let round_trip =
        decode_reference_architecture_artifact(&canonical).map_err(display_architecture)?;
    let second_plan = round_trip
        .materialization_plan()
        .map_err(display_architecture)?;
    let second =
        encode_reference_architecture_artifact(&round_trip).map_err(display_architecture)?;
    if canonical != second {
        return Err("Reference Architecture canonical encoding was not idempotent".to_owned());
    }
    let second_hash = round_trip.semantic_hash().map_err(display_architecture)?;
    if first_hash != second_hash {
        return Err(
            "Reference Architecture semantic hash changed after canonicalization".to_owned(),
        );
    }
    if first_plan != second_plan {
        return Err(
            "Reference Architecture materialization plan changed after canonicalization".to_owned(),
        );
    }
    Ok(S1m5ReferenceCanonicalObservation {
        canonical_len: canonical.len(),
        semantic_hash: Some(first_hash),
    })
}

fn exercise_pair(payload: &[u8]) -> Result<S1m5ReferenceCanonicalObservation, String> {
    let decoded = decode_reference_pair_manifest(payload).map_err(display_experiment)?;
    let first_hash = decoded.semantic_hash().map_err(display_experiment)?;
    let canonical = encode_reference_pair_manifest(&decoded).map_err(display_experiment)?;
    let round_trip = decode_reference_pair_manifest(&canonical).map_err(display_experiment)?;
    let second = encode_reference_pair_manifest(&round_trip).map_err(display_experiment)?;
    if canonical != second {
        return Err("Reference Pair canonical encoding was not idempotent".to_owned());
    }
    let second_hash = round_trip.semantic_hash().map_err(display_experiment)?;
    if first_hash != second_hash {
        return Err("Reference Pair semantic hash changed after canonicalization".to_owned());
    }
    Ok(S1m5ReferenceCanonicalObservation {
        canonical_len: canonical.len(),
        semantic_hash: Some(first_hash),
    })
}

fn exercise_plan(payload: &[u8]) -> Result<S1m5ReferenceCanonicalObservation, String> {
    let decoded = decode_reference_experiment_plan_v2(payload).map_err(display_experiment)?;
    let canonical = encode_reference_experiment_plan_v2(&decoded).map_err(display_experiment)?;
    let round_trip = decode_reference_experiment_plan_v2(&canonical).map_err(display_experiment)?;
    let second = encode_reference_experiment_plan_v2(&round_trip).map_err(display_experiment)?;
    if canonical != second {
        return Err("Experiment Plan v2 canonical encoding was not idempotent".to_owned());
    }
    Ok(S1m5ReferenceCanonicalObservation {
        canonical_len: canonical.len(),
        semantic_hash: None,
    })
}

fn exercise_metric_set(payload: &[u8]) -> Result<S1m5ReferenceCanonicalObservation, String> {
    let decoded = decode_reference_metric_set_artifact(payload).map_err(display_metric)?;
    let first_hash = decoded.semantic_hash().map_err(display_metric)?;
    let canonical = encode_reference_metric_set_artifact(&decoded).map_err(display_metric)?;
    let round_trip = decode_reference_metric_set_artifact(&canonical).map_err(display_metric)?;
    let second = encode_reference_metric_set_artifact(&round_trip).map_err(display_metric)?;
    if canonical != second {
        return Err("Reference Metric Set canonical encoding was not idempotent".to_owned());
    }
    let second_hash = round_trip.semantic_hash().map_err(display_metric)?;
    if first_hash != second_hash {
        return Err("Reference Metric Set semantic hash changed after canonicalization".to_owned());
    }
    Ok(S1m5ReferenceCanonicalObservation {
        canonical_len: canonical.len(),
        semantic_hash: Some(first_hash),
    })
}

fn exercise_metric_artifact(payload: &[u8]) -> Result<S1m5ReferenceCanonicalObservation, String> {
    let definition = fuzz_metric_definition()?;
    let decoded = decode_reference_metric_artifact(payload, &definition).map_err(display_metric)?;
    let first_hash = decoded.semantic_hash(&definition).map_err(display_metric)?;
    let canonical =
        encode_reference_metric_artifact(&decoded, &definition).map_err(display_metric)?;
    let round_trip =
        decode_reference_metric_artifact(&canonical, &definition).map_err(display_metric)?;
    let second =
        encode_reference_metric_artifact(&round_trip, &definition).map_err(display_metric)?;
    if canonical != second {
        return Err("Reference Metric Artifact canonical encoding was not idempotent".to_owned());
    }
    let second_hash = round_trip
        .semantic_hash(&definition)
        .map_err(display_metric)?;
    if first_hash != second_hash {
        return Err(
            "Reference Metric Artifact semantic hash changed after canonicalization".to_owned(),
        );
    }
    Ok(S1m5ReferenceCanonicalObservation {
        canonical_len: canonical.len(),
        semantic_hash: Some(first_hash),
    })
}

fn fuzz_metric_definition() -> Result<ReferenceMetricSetArtifact, String> {
    ReferenceMetricSetArtifact::v1(vec![ReferenceResponseObservationSpec {
        name: "fuzz.response".to_owned(),
        hostile_entry_binding: "sensor.fuzz.0".to_owned(),
        defense_contact_binding: "defense.fuzz.0".to_owned(),
        enemy_ordinal: 0,
    }])
    .map_err(display_metric)
}

fn display_architecture(error: ReferenceArchitectureError) -> String {
    error.to_string()
}

fn display_experiment(error: ReferenceExperimentError) -> String {
    error.to_string()
}

fn display_metric(error: ReferenceMetricError) -> String {
    error.to_string()
}
