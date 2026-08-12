use aon_sim::{
    ArtifactHash, BalanceProfile, BinaryGatePortAnchors, ExperimentArtifactBytes, ExperimentPlan,
    ExperimentPlanError, ExperimentRunId, FIXED_ONE, Fixed, GateFootprint, GateFootprintTable,
    GateGeometryVariant, GatePortTable, LongWireDesign, MAX_EXPERIMENT_RUNS, NumericProfile,
    PhysicalScaleMatrix, PhysicalScaleProfile, PortAnchor, ProfileHash, Rational, Seed,
    UnaryGatePortAnchors, decode_experiment_plan_artifact, resolve_experiment_plan_artifact,
};
use std::collections::{BTreeMap, BTreeSet};

const RETAINED_PLAN: &[u8] =
    include_bytes!("../../../fixtures/experiments/s1-m0-physical-scale-v1.json");
const RETAINED_SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const RETAINED_NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const RETAINED_PHYSICAL: &[u8] =
    include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const RETAINED_BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

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

fn plan() -> ExperimentPlan {
    ExperimentPlan {
        experiment_id: "s1m0-physical-scale-baseline".to_owned(),
        scenario_artifact_hash: ArtifactHash::from_bytes([0x42; 32]),
        physical_scale_matrix: PhysicalScaleMatrix {
            base_profile: PhysicalScaleProfile::stage0_alpha("experiment-base"),
            gate_geometries: vec![
                square_geometry(FIXED_ONE / 4),
                square_geometry(FIXED_ONE / 2),
            ],
            circuit_routing_pitches: vec![Fixed(FIXED_ONE / 4), Fixed(FIXED_ONE / 2)],
            world_routing_pitches: vec![Fixed(FIXED_ONE), Fixed(FIXED_ONE * 2)],
        },
        long_wire_distances: vec![Fixed(FIXED_ONE * 2), Fixed(FIXED_ONE * 4)],
        numeric_profile_hashes: vec![
            NumericProfile::reference_v1("numeric")
                .canonical_hash()
                .expect("numeric profile is valid"),
        ],
        balance_profile_hashes: vec![
            BalanceProfile::stage0_alpha("balance")
                .canonical_hash()
                .expect("balance profile is valid"),
        ],
        seeds: vec![Seed::from_hex(&"12".repeat(32)).expect("seed is canonical")],
        max_ticks: 10_000,
        metric_set_id: "s1m0-crossover-v1".to_owned(),
    }
}

#[test]
fn eight_physical_profiles_times_two_distances_make_sixteen_unique_runs() {
    let resolved = plan().resolve().expect("experiment plan is valid");

    assert_eq!(resolved.physical_scale_profiles().len(), 8);
    assert_eq!(resolved.runs().len(), 16);
    let run_ids = resolved
        .runs()
        .iter()
        .map(|run| run.run_id())
        .collect::<BTreeSet<_>>();
    assert_eq!(run_ids.len(), 16);

    let mut profile_frequency = BTreeMap::new();
    let mut design_frequency = BTreeMap::new();
    for run in resolved.runs() {
        *profile_frequency
            .entry(run.contract().physical_scale_profile_hash)
            .or_insert(0) += 1;
        *design_frequency
            .entry(run.design_artifact_hash())
            .or_insert(0) += 1;
        assert_eq!(
            run.physical_scale_profile().canonical_hash(),
            Ok(run.contract().physical_scale_profile_hash)
        );
    }
    assert_eq!(profile_frequency.len(), 8);
    assert!(profile_frequency.values().all(|frequency| *frequency == 2));
    assert_eq!(design_frequency.len(), 2);
    assert!(design_frequency.values().all(|frequency| *frequency == 8));
}

#[test]
fn retained_plan_has_the_frozen_ordered_sixteen_run_ids() {
    let artifact = decode_experiment_plan_artifact(RETAINED_PLAN).expect("retained plan decodes");
    let resolved = resolve_experiment_plan_artifact(
        &artifact,
        ExperimentArtifactBytes {
            scenario: RETAINED_SCENARIO,
            base_physical_scale_profile: RETAINED_PHYSICAL,
            numeric_profiles: &[RETAINED_NUMERIC],
            balance_profiles: &[RETAINED_BALANCE],
        },
    )
    .expect("retained plan resolves");
    let expected = include_str!("../../../fixtures/experiments/s1-m0-retained-run-ids-v1.txt")
        .lines()
        .collect::<Vec<_>>();
    let actual = resolved
        .runs()
        .iter()
        .map(|run| run.run_id().to_string())
        .collect::<Vec<_>>();

    assert_eq!(actual.len(), 16);
    assert_eq!(
        actual.iter().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn relocated_artifact_paths_do_not_change_resolved_run_ids() {
    let original = decode_experiment_plan_artifact(RETAINED_PLAN).expect("retained plan decodes");
    let relocated = String::from_utf8(RETAINED_PLAN.to_vec())
        .expect("retained plan is UTF-8")
        .replace("../scenarios/empty.json", "relocated/scenario.json")
        .replace(
            "../../profiles/physical-scale/stage0-alpha.json",
            "relocated/physical.json",
        )
        .replace("../../profiles/numeric/v1.json", "relocated/numeric.json")
        .replace(
            "../../profiles/balance/stage0-alpha.json",
            "relocated/balance.json",
        );
    let relocated =
        decode_experiment_plan_artifact(relocated.as_bytes()).expect("relocated plan decodes");
    let resolve = |artifact| {
        resolve_experiment_plan_artifact(
            artifact,
            ExperimentArtifactBytes {
                scenario: RETAINED_SCENARIO,
                base_physical_scale_profile: RETAINED_PHYSICAL,
                numeric_profiles: &[RETAINED_NUMERIC],
                balance_profiles: &[RETAINED_BALANCE],
            },
        )
        .expect("semantic artifacts resolve independently of locators")
    };

    assert_eq!(
        resolve(&original)
            .runs()
            .iter()
            .map(|run| run.run_id())
            .collect::<Vec<_>>(),
        resolve(&relocated)
            .runs()
            .iter()
            .map(|run| run.run_id())
            .collect::<Vec<_>>()
    );
}

#[test]
fn long_wire_distance_is_outside_the_physical_profile_hash() {
    let mut near_only = plan();
    near_only.long_wire_distances = vec![Fixed(FIXED_ONE * 2)];
    let mut far_only = plan();
    far_only.long_wire_distances = vec![Fixed(FIXED_ONE * 4)];

    let near = near_only.resolve().expect("near plan is valid");
    let far = far_only.resolve().expect("far plan is valid");

    assert_eq!(
        near.physical_scale_profiles(),
        far.physical_scale_profiles()
    );
    assert_ne!(
        near.runs()[0].design_artifact_hash(),
        far.runs()[0].design_artifact_hash()
    );
    assert_ne!(near.runs()[0].run_id(), far.runs()[0].run_id());
}

#[test]
fn all_axis_permutations_produce_the_same_run_specs() {
    let original = plan();
    let mut permuted = original.clone();
    permuted.physical_scale_matrix.gate_geometries.reverse();
    permuted
        .physical_scale_matrix
        .circuit_routing_pitches
        .reverse();
    permuted
        .physical_scale_matrix
        .world_routing_pitches
        .reverse();
    permuted.long_wire_distances.reverse();

    assert_eq!(original.resolve(), permuted.resolve());
}

#[test]
fn profile_and_seed_axes_expand_in_canonical_tuple_order() {
    let mut expanded = plan();
    expanded.numeric_profile_hashes = vec![
        ProfileHash::from_hex(&"22".repeat(32)).expect("hash is canonical"),
        ProfileHash::from_hex(&"11".repeat(32)).expect("hash is canonical"),
    ];
    expanded.balance_profile_hashes = vec![
        ProfileHash::from_hex(&"44".repeat(32)).expect("hash is canonical"),
        ProfileHash::from_hex(&"33".repeat(32)).expect("hash is canonical"),
    ];
    expanded.seeds = vec![
        Seed::from_hex(&"66".repeat(32)).expect("seed is canonical"),
        Seed::from_hex(&"55".repeat(32)).expect("seed is canonical"),
    ];

    let resolved = expanded.resolve().expect("expanded plan is valid");
    assert_eq!(resolved.runs().len(), 128);
    assert!(resolved.runs().windows(2).all(|runs| {
        let key = |run: &aon_sim::ExperimentRunSpec| {
            (
                run.contract().numeric_profile_hash,
                run.contract().physical_scale_profile_hash,
                run.contract().balance_profile_hash,
                run.long_wire_distance(),
                run.seed(),
            )
        };
        key(&runs[0]) < key(&runs[1])
    }));
}

#[test]
fn duplicate_profile_and_seed_axis_values_are_typed_errors() {
    let mut duplicate_numeric = plan();
    duplicate_numeric
        .numeric_profile_hashes
        .push(duplicate_numeric.numeric_profile_hashes[0]);
    assert!(matches!(
        duplicate_numeric.resolve(),
        Err(ExperimentPlanError::DuplicateProfileHash { .. })
    ));

    let mut duplicate_seed = plan();
    duplicate_seed.seeds.push(duplicate_seed.seeds[0]);
    assert!(matches!(
        duplicate_seed.resolve(),
        Err(ExperimentPlanError::DuplicateSeed { .. })
    ));
}

#[test]
fn distances_must_be_positive_unique_and_aligned_for_every_profile() {
    let mut nonpositive = plan();
    nonpositive.long_wire_distances = vec![Fixed::ZERO];
    assert_eq!(
        nonpositive.resolve(),
        Err(ExperimentPlanError::NonPositiveLongWireDistance {
            distance: Fixed::ZERO
        })
    );

    let mut duplicate = plan();
    duplicate.long_wire_distances = vec![Fixed(FIXED_ONE * 2); 2];
    assert_eq!(
        duplicate.resolve(),
        Err(ExperimentPlanError::DuplicateLongWireDistance {
            distance: Fixed(FIXED_ONE * 2)
        })
    );

    let mut unaligned = plan();
    unaligned.long_wire_distances = vec![Fixed(FIXED_ONE * 3)];
    assert!(matches!(
        unaligned.resolve(),
        Err(ExperimentPlanError::LongWireDistanceNotWorldPitchAligned {
            distance: Fixed(value),
            world_routing_pitch: Fixed(pitch),
            ..
        }) if value == FIXED_ONE * 3 && pitch == FIXED_ONE * 2
    ));
}

#[test]
fn long_wire_design_public_construction_preserves_positive_exact_geometry() {
    for distance in [Fixed::ZERO, Fixed(-1), Fixed(i64::MIN)] {
        assert_eq!(
            LongWireDesign::try_from_distance(distance),
            Err(ExperimentPlanError::NonPositiveLongWireDistance { distance })
        );
    }

    let unsnapped =
        LongWireDesign::try_from_distance(Fixed(1)).expect("positive distance is exact");
    assert_eq!(unsnapped.start(), aon_sim::FixedVec2::default());
    assert_eq!(
        unsnapped.end(),
        aon_sim::FixedVec2::new(Fixed(1), Fixed::ZERO)
    );
    assert_eq!(unsnapped.distance(), Fixed(1));

    let maximum = LongWireDesign::try_from_distance(Fixed(i64::MAX))
        .expect("maximum positive Fixed distance is representable");
    assert_eq!(maximum.end().x, Fixed(i64::MAX));
    assert_eq!(
        aon_sim::segment_length(maximum.start(), maximum.end()),
        Ok(Fixed(i64::MAX))
    );
    assert_ne!(maximum.canonical_hash(), unsnapped.canonical_hash());

    let mut independent = blake3::Hasher::new();
    independent.update(b"AON\0LONG-WIRE-DESIGN\0V1\0");
    independent.update(&1_u16.to_le_bytes());
    for coordinate in [
        maximum.start().x,
        maximum.start().y,
        maximum.end().x,
        maximum.end().y,
    ] {
        independent.update(&coordinate.0.to_le_bytes());
    }
    assert_eq!(
        maximum.canonical_hash(),
        ArtifactHash::from_bytes(*independent.finalize().as_bytes())
    );
}

#[test]
fn checked_run_limit_rejects_the_cartesian_product_before_publication() {
    assert_eq!(
        plan().resolve_with_run_limit(15),
        Err(ExperimentPlanError::TooManyExperimentRuns {
            maximum: 15,
            actual: 16
        })
    );
}

#[test]
fn caller_cannot_raise_the_frozen_run_limit() {
    let mut oversized = plan();
    let quantum = oversized
        .physical_scale_matrix
        .base_profile
        .wire_geometry_quantum
        .0;
    oversized.physical_scale_matrix.gate_geometries = (1_i64..=64)
        .map(|multiple| square_geometry(multiple * quantum * 2))
        .collect();
    oversized.physical_scale_matrix.circuit_routing_pitches = (1_i64..=64)
        .map(|multiple| Fixed(multiple * quantum))
        .collect();
    oversized.physical_scale_matrix.world_routing_pitches = vec![Fixed(FIXED_ONE)];
    oversized.long_wire_distances = (1_i64..=17)
        .map(|multiple| Fixed(multiple * FIXED_ONE))
        .collect();

    assert_eq!(
        oversized.resolve_with_run_limit(usize::MAX),
        Err(ExperimentPlanError::TooManyExperimentRuns {
            maximum: MAX_EXPERIMENT_RUNS,
            actual: 69_632,
        })
    );
}

#[test]
fn compound_errors_follow_the_frozen_validation_precedence() {
    let mut empty_experiment_id = plan();
    empty_experiment_id.experiment_id = " \t".to_owned();
    empty_experiment_id.max_ticks = 0;
    assert_eq!(
        empty_experiment_id.resolve(),
        Err(ExperimentPlanError::EmptyTextField {
            field: aon_sim::ExperimentTextField::ExperimentId,
        })
    );

    let mut empty_metric_set = plan();
    empty_metric_set.metric_set_id.clear();
    empty_metric_set
        .physical_scale_matrix
        .gate_geometries
        .clear();
    assert_eq!(
        empty_metric_set.resolve(),
        Err(ExperimentPlanError::EmptyTextField {
            field: aon_sim::ExperimentTextField::MetricSetId,
        })
    );

    let mut empty_before_max_ticks = plan();
    empty_before_max_ticks
        .physical_scale_matrix
        .world_routing_pitches
        .clear();
    empty_before_max_ticks.max_ticks = 0;
    empty_before_max_ticks
        .physical_scale_matrix
        .base_profile
        .wire_geometry_quantum = Fixed::ZERO;
    assert_eq!(
        empty_before_max_ticks.resolve(),
        Err(ExperimentPlanError::EmptyAxis {
            axis: aon_sim::ExperimentAxis::WorldRoutingPitch,
        })
    );

    let mut max_ticks_before_profile = plan();
    max_ticks_before_profile.max_ticks = 0;
    max_ticks_before_profile
        .physical_scale_matrix
        .base_profile
        .wire_geometry_quantum = Fixed::ZERO;
    assert_eq!(
        max_ticks_before_profile.resolve(),
        Err(ExperimentPlanError::NonPositiveMaxTicks)
    );

    let mut profile_before_distance = plan();
    profile_before_distance
        .physical_scale_matrix
        .base_profile
        .wire_geometry_quantum = Fixed::ZERO;
    profile_before_distance.long_wire_distances = vec![Fixed::ZERO, Fixed::ZERO];
    assert!(matches!(
        profile_before_distance.resolve(),
        Err(ExperimentPlanError::Profile(_))
    ));

    let mut physical_duplicate_before_distance = plan();
    physical_duplicate_before_distance
        .physical_scale_matrix
        .gate_geometries
        .push(
            physical_duplicate_before_distance
                .physical_scale_matrix
                .gate_geometries[0],
        );
    physical_duplicate_before_distance.long_wire_distances = vec![Fixed::ZERO];
    assert!(matches!(
        physical_duplicate_before_distance.resolve(),
        Err(ExperimentPlanError::DuplicatePhysicalScaleProfile { .. })
    ));

    let mut distance_before_seed = plan();
    distance_before_seed.long_wire_distances = vec![Fixed::ZERO];
    distance_before_seed
        .seeds
        .push(distance_before_seed.seeds[0]);
    assert!(matches!(
        distance_before_seed.resolve(),
        Err(ExperimentPlanError::NonPositiveLongWireDistance { .. })
    ));

    let mut seed_before_alignment = plan();
    seed_before_alignment.long_wire_distances = vec![Fixed(FIXED_ONE * 3)];
    seed_before_alignment
        .seeds
        .push(seed_before_alignment.seeds[0]);
    assert!(matches!(
        seed_before_alignment.resolve(),
        Err(ExperimentPlanError::DuplicateSeed { .. })
    ));

    let mut alignment_before_limit = plan();
    alignment_before_limit.long_wire_distances = vec![Fixed(FIXED_ONE * 3)];
    assert!(matches!(
        alignment_before_limit.resolve_with_run_limit(1),
        Err(ExperimentPlanError::LongWireDistanceNotWorldPitchAligned { .. })
    ));
}

#[test]
fn seed_is_bound_into_the_run_id_but_not_the_design_or_profile_hash() {
    let original = plan().resolve().expect("plan is valid");
    let mut changed_seed = plan();
    changed_seed.seeds = vec![Seed::from_hex(&"34".repeat(32)).expect("seed is canonical")];
    let changed = changed_seed.resolve().expect("changed plan is valid");

    assert_eq!(
        original.physical_scale_profiles(),
        changed.physical_scale_profiles()
    );
    let original_designs = original
        .runs()
        .iter()
        .map(|run| run.design_artifact_hash())
        .collect::<BTreeSet<_>>();
    let changed_designs = changed
        .runs()
        .iter()
        .map(|run| run.design_artifact_hash())
        .collect::<BTreeSet<_>>();
    assert_eq!(original_designs, changed_designs);
    assert_ne!(
        original
            .runs()
            .iter()
            .map(|run| run.run_id())
            .collect::<BTreeSet<_>>(),
        changed
            .runs()
            .iter()
            .map(|run| run.run_id())
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn every_selectable_run_identity_field_changes_the_run_id() {
    let mut baseline_plan = plan();
    baseline_plan
        .physical_scale_matrix
        .gate_geometries
        .truncate(1);
    baseline_plan
        .physical_scale_matrix
        .circuit_routing_pitches
        .truncate(1);
    baseline_plan
        .physical_scale_matrix
        .world_routing_pitches
        .truncate(1);
    baseline_plan.long_wire_distances.truncate(1);
    let baseline = baseline_plan.resolve().expect("baseline plan resolves");
    assert_eq!(baseline.runs().len(), 1);
    let baseline_id = baseline.runs()[0].run_id();

    let mut changed_experiment = baseline_plan.clone();
    changed_experiment.experiment_id.push_str("-changed");
    let mut changed_scenario = baseline_plan.clone();
    changed_scenario.scenario_artifact_hash = ArtifactHash::from_bytes([0x43; 32]);
    let mut changed_numeric = baseline_plan.clone();
    changed_numeric.numeric_profile_hashes[0] =
        ProfileHash::from_hex(&"23".repeat(32)).expect("test hash is canonical");
    let mut changed_physical = baseline_plan.clone();
    changed_physical
        .physical_scale_matrix
        .circuit_routing_pitches[0] = Fixed(FIXED_ONE / 2);
    let mut changed_balance = baseline_plan.clone();
    changed_balance.balance_profile_hashes[0] =
        ProfileHash::from_hex(&"45".repeat(32)).expect("test hash is canonical");
    let mut changed_distance = baseline_plan.clone();
    changed_distance.long_wire_distances[0] = Fixed(FIXED_ONE * 4);
    let mut changed_seed = baseline_plan.clone();
    changed_seed.seeds[0] = Seed::from_hex(&"67".repeat(32)).expect("test Seed is canonical");
    let mut changed_max_ticks = baseline_plan.clone();
    changed_max_ticks.max_ticks += 1;
    let mut changed_metric = baseline_plan.clone();
    changed_metric.metric_set_id.push_str("-changed");

    for (field, changed) in [
        ("experimentId", changed_experiment),
        ("scenarioArtifactHash", changed_scenario),
        ("numericProfileHash", changed_numeric),
        ("physicalScaleProfileHash", changed_physical),
        ("balanceProfileHash", changed_balance),
        ("longWireDistance and designArtifactHash", changed_distance),
        ("seed", changed_seed),
        ("maxTicks", changed_max_ticks),
        ("metricSetId", changed_metric),
    ] {
        let resolved = changed.resolve().expect("changed plan resolves");
        assert_eq!(resolved.runs().len(), 1, "{field}");
        assert_ne!(resolved.runs()[0].run_id(), baseline_id, "{field}");
    }

    let mut metadata_only = baseline_plan;
    metadata_only.physical_scale_matrix.base_profile.profile_id = "metadata-only-change".to_owned();
    assert_eq!(
        metadata_only
            .resolve()
            .expect("metadata plan resolves")
            .runs()[0]
            .run_id(),
        baseline_id
    );
}

#[test]
fn real_capacity_balance_variants_change_balance_and_run_identity_only() {
    let base_balance = BalanceProfile::capacity_probe_alpha("capacity-base");
    let mut changed_capacity = base_balance.clone();
    changed_capacity
        .capacity_probe
        .as_mut()
        .expect("capacity profile is present")
        .main_core_capacity += 1;
    let mut changed_support = base_balance.clone();
    changed_support
        .capacity_probe
        .as_mut()
        .expect("capacity profile is present")
        .support_heat_fraction = Rational::new(1, 5).expect("support coefficient is valid");

    let balance_hashes = [base_balance, changed_capacity, changed_support].map(|profile| {
        profile
            .canonical_hash()
            .expect("real Balance variant is valid")
    });
    assert_eq!(balance_hashes.into_iter().collect::<BTreeSet<_>>().len(), 3);

    let mut singleton = plan();
    singleton.physical_scale_matrix.gate_geometries.truncate(1);
    singleton
        .physical_scale_matrix
        .circuit_routing_pitches
        .truncate(1);
    singleton
        .physical_scale_matrix
        .world_routing_pitches
        .truncate(1);
    singleton.long_wire_distances.truncate(1);

    let runs = balance_hashes.map(|balance_profile_hash| {
        let mut candidate = singleton.clone();
        candidate.balance_profile_hashes = vec![balance_profile_hash];
        candidate
            .resolve()
            .expect("Balance variant plan resolves")
            .runs()[0]
            .clone()
    });
    assert!(runs.windows(2).all(|pair| {
        pair[0].contract().physical_scale_profile_hash
            == pair[1].contract().physical_scale_profile_hash
    }));
    assert_eq!(
        runs.iter()
            .map(|run| run.contract().balance_profile_hash)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    assert_eq!(
        runs.iter()
            .map(|run| run.run_id())
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
}

#[test]
fn run_id_uses_the_frozen_domain_and_contract_field_order() {
    let resolved = plan().resolve().expect("plan is valid");
    let run = &resolved.runs()[0];
    assert_eq!(
        run.run_id().to_string(),
        include_str!("../../../fixtures/experiments/s1-m0-first-run-id-v1.txt").trim()
    );
    let contract = run.contract();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"AON\0EXPERIMENT-RUN\0V1\0");
    hasher.update(&1_u16.to_le_bytes());
    hasher.update(&(run.experiment_id().len() as u32).to_le_bytes());
    hasher.update(run.experiment_id().as_bytes());
    hasher.update(run.scenario_artifact_hash().as_bytes());
    hasher.update(run.design_artifact_hash().as_bytes());
    let semantics = contract.semantics_version.as_str().as_bytes();
    hasher.update(&(semantics.len() as u32).to_le_bytes());
    hasher.update(semantics);
    hasher.update(contract.numeric_profile_hash.as_bytes());
    hasher.update(contract.physical_scale_profile_hash.as_bytes());
    hasher.update(contract.balance_profile_hash.as_bytes());
    hasher.update(&run.long_wire_distance().0.to_le_bytes());
    hasher.update(run.seed().as_bytes());
    hasher.update(&run.max_ticks().to_le_bytes());
    hasher.update(&(run.metric_set_id().len() as u32).to_le_bytes());
    hasher.update(run.metric_set_id().as_bytes());

    assert_eq!(
        run.run_id(),
        ExperimentRunId::from_bytes(*hasher.finalize().as_bytes())
    );
}

#[test]
fn strong_hash_types_use_canonical_lowercase_hex_round_trips() {
    let resolved = plan().resolve().expect("plan is valid");
    let run = &resolved.runs()[0];
    let artifact_encoded = run.design_artifact_hash().to_string();
    let run_encoded = run.run_id().to_string();

    assert_eq!(artifact_encoded.len(), 64);
    assert_eq!(run_encoded.len(), 64);
    assert_eq!(
        ArtifactHash::from_hex(&artifact_encoded),
        Ok(run.design_artifact_hash())
    );
    assert_eq!(ExperimentRunId::from_hex(&run_encoded), Ok(run.run_id()));
}
