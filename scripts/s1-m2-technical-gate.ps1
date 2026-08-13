[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Invoke-CargoChecked {
    param(
        [Parameter(Mandatory)]
        [string[]] $Arguments,

        [Parameter(Mandatory)]
        [string] $Description
    )

    Write-Host ""
    Write-Host "==> $Description"
    Write-Host ("cargo " + ($Arguments -join " "))

    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& cargo @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    foreach ($line in $output) {
        Write-Host $line.ToString()
    }
    if ($exitCode -ne 0) {
        throw "$Description failed with cargo exit code $exitCode"
    }

    return ,$output
}

$testListingCache = @{}

function Get-CargoTestListing {
    param(
        [Parameter(Mandatory)]
        [string] $Package,

        [Parameter(Mandatory)]
        [string[]] $TargetArguments
    )

    $cacheKey = $Package + "|" + ($TargetArguments -join "|")
    if (-not $testListingCache.ContainsKey($cacheKey)) {
        $arguments = @(
            "test",
            "--color", "never",
            "-p", $Package
        ) + $TargetArguments + @(
            "--locked",
            "--offline",
            "--",
            "--list",
            "--format", "terse"
        )
        $output = Invoke-CargoChecked `
            -Arguments $arguments `
            -Description "Fail-closed test discovery for $Package $($TargetArguments -join ' ')"
        $testListingCache[$cacheKey] = @($output | ForEach-Object { $_.ToString() })
    }

    return ,$testListingCache[$cacheKey]
}

function Invoke-ExactCargoTest {
    param(
        [Parameter(Mandatory)]
        [int[]] $Gates,

        [Parameter(Mandatory)]
        [string] $Package,

        [Parameter(Mandatory)]
        [string[]] $TargetArguments,

        [Parameter(Mandatory)]
        [string] $TestName,

        [Parameter(Mandatory)]
        [string] $Evidence
    )

    $gateLabel = "Gate " + ($Gates -join ",")
    $listing = @(Get-CargoTestListing -Package $Package -TargetArguments $TargetArguments)
    $escapedTestName = [Regex]::Escape($TestName)
    $listedTests = @(
        $listing | Where-Object {
            $_ -match ("^" + $escapedTestName + ": test$")
        }
    )
    if ($listedTests.Count -ne 1) {
        throw "$gateLabel expected exactly one listed test named '$TestName', but found $($listedTests.Count)"
    }

    $arguments = @(
        "test",
        "--color", "never",
        "-p", $Package
    ) + $TargetArguments + @(
        "--locked",
        "--offline",
        $TestName,
        "--",
        "--exact",
        "--test-threads=1"
    )
    $output = Invoke-CargoChecked -Arguments $arguments -Description "$gateLabel - $Evidence"
    $lines = @($output | ForEach-Object { $_.ToString() })
    $passingSummaries = @(
        $lines | Where-Object {
            $_ -match '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;'
        }
    )
    if ($passingSummaries.Count -ne 1) {
        throw "$gateLabel did not execute exactly one passing test named '$TestName'"
    }
}

$requiredTests = @(
    @{
        Gates = @(1)
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "retained_plan_has_the_frozen_ordered_sixteen_run_ids"
        Evidence = "retained S1-M0 ordered RunId golden set"
    },
    @{
        Gates = @(1)
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "retained_v1_fixture_exactly_reencodes_and_matches_the_hash_golden"
        Evidence = "retained Module v1 canonical bytes and semantic hash"
    },
    @{
        Gates = @(1, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "replay_golden")
        TestName = "retained_feedback_ring_is_the_exact_canonical_replay_encoding"
        Evidence = "retained feedback Replay command stream under Replay v2 and State V6"
    },
    @{
        Gates = @(1, 15)
        Package = "aon-app"
        TargetArguments = @("--test", "replay_host")
        TestName = "retained_s1m1_capacity_replay_matches_headless_and_bevy_fixed_update"
        Evidence = "key prior Headless and Bevy capacity host regression"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "profile::tests::balance_v3_requires_and_hashes_the_complete_power_probe_without_changing_v2"
        Evidence = "strict Balance v2/v3 powerProbe split, validation, and independent hashing"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "profile::tests::balance_v3_every_power_probe_field_is_hash_sensitive_and_boundary_validated"
        Evidence = "all nine PowerProbe fields are independently hash-sensitive and boundary validated"
    },
    @{
        Gates = @(1, 2)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::balance_v3_power_probe_rejects_retained_empty_world_before_tick_zero"
        Evidence = "Balance v3 PowerProbe rejects a retained Empty world before Tick 0"
    },
    @{
        Gates = @(1, 2)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::balance_v3_power_probe_rejects_retained_main_core_world_before_tick_zero"
        Evidence = "Balance v3 PowerProbe rejects a retained MainCoreV1 world before Tick 0"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "artifact::tests::scenario_v3_sorts_sources_and_hashes_an_independent_canonical_stream"
        Evidence = "Scenario v3 source normalization and independent canonical encoder"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "artifact::tests::scenario_v3_allows_source_less_and_rejects_invalid_duplicate_cross_version_payloads"
        Evidence = "strict Scenario v1/v2/v3 payload and duplicate Source boundaries"
    },
    @{
        Gates = @(2, 3)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_world_generation")
        TestName = "scenario_v3_package_generation_sorts_sources_and_assigns_stable_ids"
        Evidence = "Core identity and sorted Source identities are stable from package construction"
    },
    @{
        Gates = @(3, 7, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_world_generation")
        TestName = "source_anchor_is_the_explicit_bridge_into_a_fixed_substrate_power_region"
        Evidence = "exact SourceAnchor bridge reaches a real FixedSubstrate Gate power port"
    },
    @{
        Gates = @(3)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_world_generation")
        TestName = "substrate_source_bridge_still_requires_the_source_point_inside_the_routing_area"
        Evidence = "Source attachment requires exact identity, position, and substrate routing area"
    },
    @{
        Gates = @(2, 3)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_source::tests::construction_rejects_invalid_or_ambiguous_sources"
        Evidence = "Power Source store rejects invalid identity, generation, position, and duplicate records"
    },
    @{
        Gates = @(3, 7, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_completion_edges")
        TestName = "mobile_source_anchor_bridge_requires_exact_in_area_position_and_powers_gate"
        Evidence = "SourceAnchor is the sole exact-position bridge into a MobileSubstrate Gate power surface"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_state_replay")
        TestName = "replay_v2_hostile_frames_are_strict_normalized_and_round_trip_exactly"
        Evidence = "Replay v2 typed complete hostile frames normalize and round-trip canonically"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_state_replay")
        TestName = "replay_v2_rejects_duplicate_frames_and_invalid_hostile_identity_or_radius"
        Evidence = "duplicate frames and invalid hostile identity or radius fail closed"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_state_replay")
        TestName = "explicit_empty_hostile_frame_and_omission_are_simulation_equivalent"
        Evidence = "missing and explicit empty hostile frames are exactly equivalent"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_world_generation")
        TestName = "world_input_tick_frame_and_hostile_validation_is_atomic"
        Evidence = "current-Tick frame, hostile bounds, and final-run validation are atomic"
    },
    @{
        Gates = @(4, 13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "replay::tests::replay_format_envelope_precedes_version_specific_body_shape_errors"
        Evidence = "unsupported Replay format wins before strict version-specific body faults"
    },
    @{
        Gates = @(5)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_sensing")
        TestName = "closed_capsules_include_exact_tangency_at_interiors_endcaps_and_bends"
        Evidence = "closed capsule interiors, endcaps, tangency, bends, and zero radius are exact"
    },
    @{
        Gates = @(5, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_sensing")
        TestName = "sparse_grid_matches_the_direct_exact_oracle_and_never_omits_a_hit"
        Evidence = "spatial cache matches an independent brute-force oracle at numeric boundaries"
    },
    @{
        Gates = @(5)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_sensing")
        TestName = "wire_and_hostile_permutations_are_invariant_and_multiplicity_is_one_bit"
        Evidence = "sensing is order invariant and observes occupancy only"
    },
    @{
        Gates = @(6)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "signal::tests::wire_sense_drivers_are_feature_activated_in_a_then_b_order_and_removed_as_tombstones"
        Evidence = "every powered Wire owns isolated stable A and B Sense drivers"
    },
    @{
        Gates = @(6)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "structural::tests::wire_sense_binding_requires_an_exact_live_other_wire_endpoint"
        Evidence = "Sense binding rejects self, tie, wrong-kind, and dangling endpoint cases"
    },
    @{
        Gates = @(6)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "structural::tests::removing_sense_owner_detaches_incident_wire_endpoint"
        Evidence = "Sense owner removal detaches incident bindings deterministically"
    },
    @{
        Gates = @(6, 13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::canonical_validator_rejects_x_as_a_wire_sense_intended_level"
        Evidence = "canonical Wire Sense intent accepts only occupancy LOW or HIGH and rejects X"
    },
    @{
        Gates = @(6, 9, 11)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_completion_edges")
        TestName = "c07_count_one_to_three_does_not_retrigger_and_source_less_sense_is_passive_low"
        Evidence = "C-07 occupancy ignores same-level count changes and source-less Sense is passive LOW"
    },
    @{
        Gates = @(7, 13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_topology::tests::regions_are_stable_under_every_input_vector_reversal"
        Evidence = "Power graph regions and routes are invariant to store and adjacency permutations"
    },
    @{
        Gates = @(1, 7)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "phase4_counts_each_wire_body_once_in_raw_fixed_units_across_routing_domains"
        Evidence = "retained Capacity accounting still counts one physical Wire body once in every domain"
    },
    @{
        Gates = @(7)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_topology::tests::intrinsic_midpoint_is_orientation_neutral_for_odd_raw_length"
        Evidence = "odd raw midpoint remainder follows endpoint key order and reverses invariantly"
    },
    @{
        Gates = @(7, 8)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_topology::tests::source_less_component_compiles_a_none_route"
        Evidence = "source-less components compile explicitly and solve with zero generation"
    },
    @{
        Gates = @(7, 8, 12)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_topology::tests::exact_source_endpoint_load_compiles_a_zero_wire_zero_loss_route"
        Evidence = "a load exactly at its Source endpoint compiles a zero-wire zero-loss route"
    },
    @{
        Gates = @(7, 9)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "mobility::tests::powered_movement_stops_at_an_unpowered_junction_edge_boundary"
        Evidence = "derived TrackGraph seam spends the starting ratio budget once and stops before an unpowered next edge"
    },
    @{
        Gates = @(8)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_runtime::tests::nominal_collection_is_exact_ceiling_sorted_and_complete_before_solve"
        Evidence = "all nominal demand formulas, tags, ceil rules, and collection order are exact"
    },
    @{
        Gates = @(8)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_topology::tests::route_priority_is_length_then_segments_then_semantic_path_tokens"
        Evidence = "route ties use length, segment count, then semantic path tokens"
    },
    @{
        Gates = @(8, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_kernels")
        TestName = "common_ratio_has_exact_full_half_and_source_less_zero_boundaries"
        Evidence = "production solver derives exact one, one-half, and source-less zero ratios"
    },
    @{
        Gates = @(8, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_kernels")
        TestName = "region_solution_is_invariant_to_source_and_demand_permutation"
        Evidence = "every demand receives one common ratio without input-order monopoly"
    },
    @{
        Gates = @(8, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_oracle")
        TestName = "solver_matches_an_independent_exhaustive_oracle_over_all_ratio_values"
        Evidence = "the fixed 17-step maximal-ratio solver matches a public independent exhaustive oracle"
    },
    @{
        Gates = @(8, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_oracle")
        TestName = "power_solver_overflow_is_typed_and_never_saturates"
        Evidence = "Power kernel numeric overflow is typed and never saturates"
    },
    @{
        Gates = @(8, 13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power::tests::route_and_region_invariants_fail_closed"
        Evidence = "route mismatch, wrong-region, duplicate, and arithmetic overflow boundaries fail closed"
    },
    @{
        Gates = @(8, 9, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_kernels")
        TestName = "lower_ratio_raises_delay_and_lowers_drive_movement_and_work_with_exact_rounding"
        Evidence = "brownout Delay, Drive, Movement, and pure Work seams use frozen exact rounding"
    },
    @{
        Gates = @(8, 9)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power::tests::grant_kernels_use_nearest_even_and_gate_delay_uses_ceil"
        Evidence = "grant and delay helpers cover zero, half ties, floor, threshold, and one exactly"
    },
    @{
        Gates = @(9, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_runtime_boundaries")
        TestName = "pending_logic_due_is_frozen_while_rho_strength_responds_at_t_plus_one_and_same_due_merges"
        Evidence = "event-order boundary property: due logic is never retimed while strength changes propagate at t+1"
    },
    @{
        Gates = @(9, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_runtime_boundaries")
        TestName = "under_threshold_desired_reversion_cancels_conflicting_pending_without_rollback"
        Evidence = "event-order boundary property: threshold blocking cancels a conflicting pending intent"
    },
    @{
        Gates = @(9, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_completion_edges")
        TestName = "gate_retention_expires_on_exact_third_under_threshold_tick_and_recovery_cancels_reset"
        Evidence = "Gate retention preserves before Tick 3, expires on the third low-power Tick, and resets on recovery"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_runtime_reports")
        TestName = "actual_simulation_hands_nonzero_loss_and_leakage_to_phase8_without_thermal_state"
        Evidence = "actual Phase 5 grants publish stable leakage and transmission heat only in Phase 8"
    },
    @{
        Gates = @(8, 10)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_runtime::tests::region_solve_reports_common_ratio_routes_and_only_real_heat"
        Evidence = "Power reports expose canonical routes and only granted leakage and transmission heat"
    },
    @{
        Gates = @(10, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_kernels")
        TestName = "nonzero_loss_is_ceiled_and_all_heat_is_conserved_by_wire_id"
        Evidence = "nonzero loss and stable WireId remainder distribution conserve heat exactly"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::phase8_is_the_only_seam_that_publishes_phase5_heat_scratch"
        Evidence = "Phase 8 exclusively moves ephemeral Phase 5 heat into the public report"
    },
    @{
        Gates = @(10, 14)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_runtime_reports")
        TestName = "feature_off_power_sense_analyzer_is_none_and_noninterfering"
        Evidence = "derived Analyzer is read-only and no thermal or later-stage state is created"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::enabled_power_analyzer_preserves_signal_frontiers_and_state_hash"
        Evidence = "enabled Power/Sense Analyzer reads preserve frontiers and canonical state"
    },
    @{
        Gates = @(11)
        Package = "aon-headless"
        TargetArguments = @("--test", "s1m2_retained_replays")
        TestName = "retained_c07_replay_is_exact_and_runs_headlessly_with_delayed_sense_a_b"
        Evidence = "authoritative C-07 0-to-3-to-0 Replay yields exact delayed LOW-HIGH-LOW at A and B"
    },
    @{
        Gates = @(12)
        Package = "aon-headless"
        TargetArguments = @("--test", "s1m2_retained_replays")
        TestName = "retained_c08_pair_is_exact_and_headless_with_real_full_and_half_runtime_reports"
        Evidence = "authoritative C-08 full and half Replay pair derives exact real runtime reports"
    },
    @{
        Gates = @(10, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_runtime_reports")
        TestName = "c08_reports_are_deterministic_sorted_and_cover_sense_gate_and_mobile_rows"
        Evidence = "C-08 reports are deterministic, sorted, complete, and derived from production seams"
    },
    @{
        Gates = @(13)
        Package = "aon-sim"
        TargetArguments = @("--test", "bootstrap_simulation")
        TestName = "s1m2_empty_state_v6_hash_has_a_golden_value"
        Evidence = "independent Empty-world State V6 hash golden"
    },
    @{
        Gates = @(13)
        Package = "aon-sim"
        TargetArguments = @("--test", "structural_lifecycle")
        TestName = "every_structural_kind_has_an_independently_encoded_state_hash_golden"
        Evidence = "independent populated structural State V6 byte stream and hash golden"
    },
    @{
        Gates = @(13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::main_core_v6_section_has_exact_anchor_order_and_is_hash_sensitive"
        Evidence = "independent MainCoreV1 State V6 section has exact bytes and field sensitivity"
    },
    @{
        Gates = @(13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::power_source_v6_section_has_exact_sorted_records_and_field_sensitive_hash"
        Evidence = "MainCorePower Source State V6 section has exact sorted bytes and field sensitivity"
    },
    @{
        Gates = @(9, 13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::gate_v6_row_appends_exact_unpowered_tick_counter"
        Evidence = "State V6 Gate retention counter bytes and sensitivity are exact"
    },
    @{
        Gates = @(6, 13)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::wire_v6_row_has_exact_optional_sense_state_and_field_sensitive_hash"
        Evidence = "State V6 optional Wire Sense records have exact bytes and sensitivity"
    },
    @{
        Gates = @(13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_state_replay")
        TestName = "every_current_initial_world_advertises_state_v6_in_a_v2_header"
        Evidence = "Empty, MainCoreV1, and MainCorePowerV1 advertise Replay v2 and State V6"
    },
    @{
        Gates = @(13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_state_replay")
        TestName = "retained_state_v3_v4_and_v5_headers_are_typed_but_execution_rejected"
        Evidence = "retained State headers decode but current mismatch rejects before Tick 0"
    },
    @{
        Gates = @(4, 13, 15)
        Package = "aon-app"
        TargetArguments = @("--test", "s1m2_replay_hosts")
        TestName = "replay_v2_hostile_frames_match_direct_headless_and_bevy_and_empty_is_omittable"
        Evidence = "Replay v2 World inputs match direct, Headless, and native Bevy host execution"
    },
    @{
        Gates = @(11, 12, 13, 15)
        Package = "aon-app"
        TargetArguments = @("--test", "s1m2_replay_hosts")
        TestName = "retained_c07_and_c08_reports_and_v6_hashes_match_headless_and_bevy"
        Evidence = "Headless and native Bevy match every retained C-07/C-08 report and V6 hash"
    },
    @{
        Gates = @(2, 13)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "s1m2_scenario_and_balance_artifacts_reach_the_bounded_decoder_without_panics"
        Evidence = "bounded decoder corpus reaches valid and hostile Scenario v3 and Balance v3 artifacts"
    },
    @{
        Gates = @(4, 13)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "replay_regression_corpus_never_panics_and_preserves_acceptance_class"
        Evidence = "Replay corpus covers retained v2/V6 streams and bounded migration inputs"
    },
    @{
        Gates = @(5, 13)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "geometry_regression_corpus_never_panics_and_is_quantized"
        Evidence = "spatial and numeric-boundary geometry corpus remains panic free"
    },
    @{
        Gates = @(7, 13)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "topology_runtime_regression_corpus_replays_without_panics_or_silent_run_errors"
        Evidence = "topology mutation and replay restart corpus remains panic and order free"
    },
    @{
        Gates = @(14)
        Package = "aon-sim"
        TargetArguments = @("--test", "artifact_stage_features")
        TestName = "s1m2_features_require_the_power_world_and_later_features_remain_unsupported"
        Evidence = "S1-M3 and S1-M4 features and behaviors remain explicitly unsupported"
    }
)

$coveredGates = @(
    $requiredTests |
        ForEach-Object { $_.Gates } |
        ForEach-Object { [int] $_ } |
        Sort-Object -Unique
)
$gateDifference = @(
    Compare-Object -ReferenceObject @(1..15) -DifferenceObject $coveredGates
)
if ($gateDifference.Count -ne 0) {
    throw "S1-M2 technical gate table must cover every executable Gate 1 through Gate 15 exactly"
}

$duplicateTests = @(
    $requiredTests |
        ForEach-Object {
            $_.Package + "|" + ($_.TargetArguments -join "|") + "|" + $_.TestName
        } |
        Group-Object |
        Where-Object { $_.Count -ne 1 }
)
if ($duplicateTests.Count -ne 0) {
    throw "S1-M2 technical gate table contains duplicate exact test entries"
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    foreach ($testCase in $requiredTests) {
        Invoke-ExactCargoTest `
            -Gates $testCase.Gates `
            -Package $testCase.Package `
            -TargetArguments $testCase.TargetArguments `
            -TestName $testCase.TestName `
            -Evidence $testCase.Evidence
    }
    Write-Host ""
    Write-Host "S1-M2 technical gate passed: $($requiredTests.Count) fail-closed exact tests cover executable Gates 1-15."
}
finally {
    Pop-Location
}
