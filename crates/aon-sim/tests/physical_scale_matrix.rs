use aon_sim::{
    BinaryGatePortAnchors, ExperimentPlanError, FIXED_ONE, Fixed, GateFootprint,
    GateFootprintTable, GateGeometryVariant, GatePortTable, PhysicalScaleMatrix,
    PhysicalScaleProfile, PortAnchor, UnaryGatePortAnchors,
};
use std::collections::BTreeSet;

fn point(x: i64, y: i64) -> PortAnchor {
    PortAnchor {
        x: Fixed(x),
        y: Fixed(y),
    }
}

fn square_geometry(half_extent: i64) -> GateGeometryVariant {
    let footprint = GateFootprint {
        width: Fixed(half_extent * 2),
        height: Fixed(half_extent * 2),
    };
    let binary = BinaryGatePortAnchors {
        input_a: point(-half_extent, -half_extent / 2),
        input_b: point(-half_extent, half_extent / 2),
        output: point(half_extent, 0),
        power: point(0, -half_extent),
    };
    GateGeometryVariant {
        gate_footprints: GateFootprintTable {
            and_gate: footprint,
            or_gate: footprint,
            not_gate: footprint,
        },
        gate_port_anchors: GatePortTable {
            and_gate: binary,
            or_gate: binary,
            not_gate: UnaryGatePortAnchors {
                input: point(-half_extent, 0),
                output: point(half_extent, 0),
                power: point(0, -half_extent),
            },
        },
    }
}

fn matrix() -> PhysicalScaleMatrix {
    PhysicalScaleMatrix {
        base_profile: PhysicalScaleProfile::stage0_alpha("matrix-base"),
        gate_geometries: vec![
            square_geometry(FIXED_ONE / 4),
            square_geometry(FIXED_ONE / 2),
        ],
        circuit_routing_pitches: vec![Fixed(FIXED_ONE / 4), Fixed(FIXED_ONE / 2)],
        world_routing_pitches: vec![Fixed(FIXED_ONE), Fixed(FIXED_ONE * 2)],
    }
}

#[test]
fn two_by_two_by_two_matrix_has_eight_unique_hash_sorted_profiles() {
    let resolved = matrix().resolve().expect("matrix is valid");

    assert_eq!(resolved.len(), 8);
    assert!(
        resolved
            .windows(2)
            .all(|profiles| profiles[0].profile_hash() < profiles[1].profile_hash())
    );

    let hashes = resolved
        .iter()
        .map(|profile| profile.profile_hash())
        .collect::<BTreeSet<_>>();
    assert_eq!(hashes.len(), 8);

    let combinations = resolved
        .iter()
        .map(|resolved| {
            let profile = resolved.profile();
            (
                profile.gate_footprints.and_gate.width.0,
                profile.circuit_routing_pitch.0,
                profile.world_routing_pitch.0,
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(combinations.len(), 8);

    for resolved in resolved {
        assert_eq!(
            resolved.profile().canonical_hash(),
            Ok(resolved.profile_hash())
        );
        assert_eq!(
            resolved.profile().profile_id,
            format!("s1m0-physical-{}", resolved.profile_hash())
        );
    }
}

#[test]
fn physical_hash_binds_geometry_anchor_and_both_pitch_axes_but_not_profile_id() {
    let base = PhysicalScaleProfile::stage0_alpha("physical-hash-base");
    let base_hash = base.canonical_hash().expect("base profile hashes");
    let quantum = base.wire_geometry_quantum;

    // Give every gate's power port an x-boundary anchor so changing only that
    // footprint's height remains valid and proves footprint-only ownership.
    for gate in ["and", "or", "not"] {
        let mut footprint_baseline = base.clone();
        let half_width = Fixed(footprint_baseline.gate_footprints.and_gate.width.0 / 2);
        match gate {
            "and" => footprint_baseline.gate_port_anchors.and_gate.power = point(-half_width.0, 0),
            "or" => footprint_baseline.gate_port_anchors.or_gate.power = point(-half_width.0, 0),
            "not" => footprint_baseline.gate_port_anchors.not_gate.power = point(-half_width.0, 0),
            _ => unreachable!(),
        }
        let baseline_hash = footprint_baseline
            .canonical_hash()
            .unwrap_or_else(|error| panic!("{gate} footprint baseline must be valid: {error}"));
        let mut changed = footprint_baseline;
        match gate {
            "and" => changed.gate_footprints.and_gate.height.0 += quantum.0 * 2,
            "or" => changed.gate_footprints.or_gate.height.0 += quantum.0 * 2,
            "not" => changed.gate_footprints.not_gate.height.0 += quantum.0 * 2,
            _ => unreachable!(),
        }
        assert_ne!(
            changed
                .canonical_hash()
                .unwrap_or_else(|error| panic!("{gate} footprint mutation must be valid: {error}")),
            baseline_hash,
            "{gate} footprint only"
        );
    }

    for gate in ["and", "or", "not"] {
        let mut anchor = base.clone();
        match gate {
            "and" => anchor.gate_port_anchors.and_gate.input_a.y.0 += quantum.0,
            "or" => anchor.gate_port_anchors.or_gate.input_a.y.0 += quantum.0,
            "not" => anchor.gate_port_anchors.not_gate.input.y.0 += quantum.0,
            _ => unreachable!(),
        }
        assert_ne!(
            anchor
                .canonical_hash()
                .unwrap_or_else(|error| panic!("{gate} anchor mutation must be valid: {error}")),
            base_hash,
            "{gate} port anchor only"
        );
    }

    let mut circuit_pitch = base.clone();
    circuit_pitch.circuit_routing_pitch = Fixed(FIXED_ONE / 2);
    let mut world_pitch = base.clone();
    world_pitch.world_routing_pitch = Fixed(FIXED_ONE * 2);

    for (field, changed) in [
        ("circuitRoutingPitch", circuit_pitch),
        ("worldRoutingPitch", world_pitch),
    ] {
        let changed_hash = changed
            .canonical_hash()
            .unwrap_or_else(|error| panic!("{field} mutation must remain valid: {error}"));
        assert_ne!(changed_hash, base_hash, "{field}");
    }

    let mut metadata_only = base;
    metadata_only.profile_id = "different-display-and-path-label".to_owned();
    assert_eq!(metadata_only.canonical_hash(), Ok(base_hash));
}

#[test]
fn input_axis_permutations_produce_identical_resolved_profiles() {
    let original = matrix();
    let mut permuted = original.clone();
    permuted.gate_geometries.reverse();
    permuted.circuit_routing_pitches.reverse();
    permuted.world_routing_pitches.reverse();

    assert_eq!(original.resolve(), permuted.resolve());
}

#[test]
fn explicit_anchor_variants_are_retained_without_implicit_scaling() {
    let resolved = matrix().resolve().expect("matrix is valid");

    for generated in resolved {
        let profile = generated.profile();
        let half_extent = profile.gate_footprints.and_gate.width.0 / 2;
        assert_eq!(profile.gate_port_anchors.and_gate.output.x.0, half_extent);
        assert_eq!(profile.gate_port_anchors.not_gate.input.x.0, -half_extent);
        assert_eq!(profile.gate_port_anchors.not_gate.power.y.0, -half_extent);
    }
}

#[test]
fn duplicate_semantic_profile_is_a_typed_error() {
    let mut duplicate = matrix();
    duplicate.gate_geometries.push(duplicate.gate_geometries[0]);

    assert!(matches!(
        duplicate.resolve(),
        Err(ExperimentPlanError::DuplicatePhysicalScaleProfile { .. })
    ));

    let mut duplicate_circuit_pitch = matrix();
    duplicate_circuit_pitch
        .circuit_routing_pitches
        .push(duplicate_circuit_pitch.circuit_routing_pitches[0]);
    assert!(matches!(
        duplicate_circuit_pitch.resolve(),
        Err(ExperimentPlanError::DuplicatePhysicalScaleProfile { .. })
    ));

    let mut duplicate_world_pitch = matrix();
    duplicate_world_pitch
        .world_routing_pitches
        .push(duplicate_world_pitch.world_routing_pitches[0]);
    assert!(matches!(
        duplicate_world_pitch.resolve(),
        Err(ExperimentPlanError::DuplicatePhysicalScaleProfile { .. })
    ));
}

#[test]
fn every_matrix_axis_is_required() {
    for (axis, mut candidate) in [
        ("gate", matrix()),
        ("circuit", matrix()),
        ("world", matrix()),
    ] {
        match axis {
            "gate" => candidate.gate_geometries.clear(),
            "circuit" => candidate.circuit_routing_pitches.clear(),
            "world" => candidate.world_routing_pitches.clear(),
            _ => unreachable!(),
        }
        assert!(matches!(
            candidate.resolve(),
            Err(ExperimentPlanError::EmptyAxis { .. })
        ));
    }
}

#[test]
fn validation_and_exact_duplicates_precede_the_physical_product_limit() {
    let quantum = matrix().base_profile.wire_geometry_quantum.0;

    let mut invalid_candidate = matrix();
    invalid_candidate.gate_geometries = (1_i64..=64)
        .map(|multiple| square_geometry(multiple * quantum * 2))
        .collect();
    invalid_candidate.circuit_routing_pitches = (1_i64..=64)
        .map(|multiple| Fixed(multiple * quantum))
        .chain([Fixed(1)])
        .collect();
    invalid_candidate.world_routing_pitches = vec![Fixed(FIXED_ONE)];
    assert!(matches!(
        invalid_candidate.resolve(),
        Err(ExperimentPlanError::Profile(_))
    ));

    let mut exact_duplicate = matrix();
    exact_duplicate.gate_geometries = (1_i64..=64)
        .map(|multiple| square_geometry(multiple * quantum * 2))
        .collect();
    exact_duplicate
        .gate_geometries
        .push(exact_duplicate.gate_geometries[0]);
    exact_duplicate.circuit_routing_pitches = (1_i64..=64)
        .map(|multiple| Fixed(multiple * quantum))
        .collect();
    exact_duplicate.world_routing_pitches = vec![Fixed(FIXED_ONE)];
    assert!(matches!(
        exact_duplicate.resolve(),
        Err(ExperimentPlanError::DuplicatePhysicalScaleProfile { .. })
    ));
}

#[test]
fn valid_physical_product_above_frozen_limit_is_rejected_without_publication() {
    let mut oversized = matrix();
    let quantum = oversized.base_profile.wire_geometry_quantum.0;
    oversized.gate_geometries = (1_i64..=65)
        .map(|multiple| square_geometry(multiple * quantum * 2))
        .collect();
    oversized.circuit_routing_pitches = (1_i64..=64)
        .map(|multiple| Fixed(multiple * quantum))
        .collect();
    oversized.world_routing_pitches = vec![Fixed(FIXED_ONE)];

    assert_eq!(
        oversized.resolve(),
        Err(ExperimentPlanError::TooManyPhysicalScaleProfiles {
            maximum: aon_sim::MAX_PHYSICAL_SCALE_PROFILES,
            actual: 65 * 64,
        })
    );
}
