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
        [Parameter(Mandatory)] [string[]] $Arguments,
        [Parameter(Mandatory)] [string] $Description
    )

    Write-Host ""
    Write-Host "==> $Description"
    Write-Host ("cargo " + ($Arguments -join " "))
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& cargo @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previous
    }
    foreach ($line in $output) {
        Write-Host $line.ToString()
    }
    if ($exitCode -ne 0) {
        throw "$Description failed with cargo exit code $exitCode"
    }
    return $output
}

$testListingCache = @{}

function Get-CargoTestListing {
    param(
        [Parameter(Mandatory)] [string] $Package,
        [Parameter(Mandatory)] [string[]] $TargetArguments
    )

    $key = $Package + "|" + ($TargetArguments -join "|")
    if (-not $testListingCache.ContainsKey($key)) {
        $arguments = @("test", "--color", "never", "-p", $Package) +
            $TargetArguments +
            @("--locked", "--offline", "--", "--list", "--format", "terse")
        $output = Invoke-CargoChecked `
            -Arguments $arguments `
            -Description "Fail-closed test discovery for $Package $($TargetArguments -join ' ')"
        $testListingCache[$key] = @($output | ForEach-Object { $_.ToString() })
    }
    return $testListingCache[$key]
}

function Assert-ExactPrefixedTests {
    param(
        [Parameter(Mandatory)] [string] $Package,
        [Parameter(Mandatory)] [string[]] $TargetArguments,
        [Parameter(Mandatory)] [AllowEmptyString()] [string] $Prefix,
        [Parameter(Mandatory)] [string[]] $ExpectedTests
    )

    $listing = @(Get-CargoTestListing -Package $Package -TargetArguments $TargetArguments)
    $discovered = @(
        $listing |
            Where-Object { $_ -match '^(.+): test$' } |
            ForEach-Object { [Regex]::Match($_, '^(.+): test$').Groups[1].Value } |
            Where-Object { $_.StartsWith($Prefix, [System.StringComparison]::Ordinal) } |
            Sort-Object
    )
    $expected = @($ExpectedTests | Sort-Object)
    $difference = @(Compare-Object -ReferenceObject $expected -DifferenceObject $discovered)
    if ($difference.Count -ne 0) {
        $details = $difference | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
        throw "Exact prefixed test inventory mismatch for $Package $($TargetArguments -join ' ') prefix '$Prefix': $($details -join '; ')"
    }
}

function Assert-ExactTargetTests {
    param(
        [Parameter(Mandatory)] [string] $Package,
        [Parameter(Mandatory)] [string[]] $TargetArguments,
        [Parameter(Mandatory)] [string[]] $ExpectedTests
    )
    Assert-ExactPrefixedTests `
        -Package $Package `
        -TargetArguments $TargetArguments `
        -Prefix "" `
        -ExpectedTests $ExpectedTests
}

function Invoke-ExactCargoTest {
    param(
        [Parameter(Mandatory)] [int[]] $Gates,
        [Parameter(Mandatory)] [string] $Package,
        [Parameter(Mandatory)] [string[]] $TargetArguments,
        [Parameter(Mandatory)] [string] $TestName,
        [Parameter(Mandatory)] [string] $Evidence
    )

    $label = "Gate " + ($Gates -join ",")
    $listing = @(Get-CargoTestListing -Package $Package -TargetArguments $TargetArguments)
    $escaped = [Regex]::Escape($TestName)
    $matches = @($listing | Where-Object { $_ -match ("^" + $escaped + ": test$") })
    if ($matches.Count -ne 1) {
        throw "$label expected exactly one listed test named '$TestName', but found $($matches.Count)"
    }

    $arguments = @("test", "--color", "never", "-p", $Package) +
        $TargetArguments +
        @("--locked", "--offline", $TestName, "--", "--exact", "--test-threads=1")
    $output = Invoke-CargoChecked -Arguments $arguments -Description "$label - $Evidence"
    $summaries = @(
        $output |
            ForEach-Object { $_.ToString() } |
            Where-Object { $_ -match '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;' }
    )
    if ($summaries.Count -ne 1) {
        throw "$label did not execute exactly one passing test named '$TestName'"
    }
}

function New-TestEvidence {
    param(
        [Parameter(Mandatory)] [int[]] $Gates,
        [Parameter(Mandatory)] [string] $Package,
        [Parameter(Mandatory)] [string[]] $TargetArguments,
        [Parameter(Mandatory)] [string] $TestName,
        [Parameter(Mandatory)] [string] $Evidence
    )
    return @{
        Gates = $Gates
        Package = $Package
        TargetArguments = $TargetArguments
        TestName = $TestName
        Evidence = $Evidence
    }
}

function Invoke-S1M5GeneratorEvidence {
    $output = Invoke-CargoChecked `
        -Arguments @(
            "run", "-p", "aon-sim", "--locked", "--offline",
            "--example", "generate_s1m5_reference_architectures"
        ) `
        -Description "Gates 3-9,11 - deterministic non-writing S1-M5 generator verification"
    $lines = @($output | ForEach-Object { $_.ToString() })
    foreach ($pattern in @(
        '^s1m5ReferenceStatus=verified$',
        '^bruteArtifactHash=[0-9a-f]{64}$',
        '^computedArtifactHash=[0-9a-f]{64}$',
        '^pairArtifactHash=[0-9a-f]{64}$',
        '^bruteRunId=[0-9a-f]{64}$',
        '^computedRunId=[0-9a-f]{64}$'
    )) {
        $matches = @($lines | Where-Object { $_ -match $pattern })
        if ($matches.Count -ne 1) {
            throw "S1-M5 generator must print exactly one line matching '$pattern'"
        }
    }
}

$architectureTests = @(
    "reference_architecture::tests::command_log_hash_is_complete_and_order_sensitive",
    "reference_architecture::tests::failed_late_batch_returns_no_partial_candidate_or_evidence",
    "reference_architecture::tests::materializer_executes_one_dependency_batch_per_tick_and_returns_complete_evidence",
    "reference_architecture::tests::plan_places_unbound_then_resolves_bindings_without_raw_artifact_ids",
    "reference_architecture::tests::plan_uses_all_seven_dependency_phases_and_sorts_each_batch",
    "reference_architecture::tests::scenario_ordinals_are_resolved_explicitly",
    "reference_architecture::tests::semantic_hash_normalizes_order_and_excludes_display_name",
    "reference_architecture::tests::strict_json_round_trips_to_canonical_order",
    "reference_architecture::tests::validation_binds_the_full_profile_contract",
    "reference_architecture::tests::validation_rejects_duplicate_dangling_wrong_kind_and_duplicate_bindings",
    "reference_architecture::tests::v2_materializer_records_each_earliest_post_stage_quiescence_barrier",
    "reference_architecture::tests::v2_pair_materializer_preserves_empty_stages_and_equal_build_end_atomically",
    "reference_architecture::tests::v2_schedule_validation_rejects_missing_duplicate_free_and_late_nonpower_bindings",
    "reference_architecture::tests::v2_strict_json_round_trip_and_hash_bind_the_schedule"
)

$experimentTests = @(
    "reference_experiment::tests::experiment_v2_envelope_errors_precede_strict_body_errors",
    "reference_experiment::tests::experiment_v2_rejects_unknown_body_fields",
    "reference_experiment::tests::experiment_v2_retains_pair_hash_and_resolve_revalidates_it",
    "reference_experiment::tests::experiment_v2_strict_json_round_trips_canonically",
    "reference_experiment::tests::pair_envelope_errors_precede_strict_body_errors",
    "reference_experiment::tests::canonical_scenario_sequence_hashes_normalize_order_and_bind_every_field",
    "reference_experiment::tests::fairness_checks_profile_ids_schemas_scenario_identity_and_sequence_bodies",
    "reference_experiment::tests::pair_hash_binds_capacity_and_power_source_sequence",
    "reference_experiment::tests::pair_requires_exact_cardinal_anchors_empty_shared_log_and_coherent_bindings",
    "reference_experiment::tests::pair_round_trips_and_hashes_independently_of_json",
    "reference_experiment::tests::pair_strict_json_rejects_unknown_fields_and_nonzero_seed",
    "reference_experiment::tests::v2_plan_resolves_exactly_two_distinct_runs",
    "reference_experiment::tests::v2_run_id_has_independent_domain_and_field_sensitivity"
)

$metricTests = @(
    "reference_metrics::tests::format_and_hash_envelopes_precede_unknown_body_fields",
    "reference_metrics::tests::inventory_and_latency_invariants_reject_inconsistent_results",
    "reference_metrics::tests::metric_artifact_hash_binds_new_ncu_core_baseline_and_runtime_fields",
    "reference_metrics::tests::metric_set_and_result_round_trip_with_stable_semantic_hashes",
    "reference_metrics::tests::metric_set_hash_binds_portable_observation_names_and_enemy_ordinal",
    "reference_metrics::tests::metric_set_v1_declares_every_hashed_static_and_runtime_field_in_exact_tag_order",
    "reference_metrics::tests::portable_response_names_resolve_across_independent_local_namespaces",
    "reference_metrics::tests::reducer_exactly_applies_window_power_heat_support_kills_and_latency_once",
    "reference_metrics::tests::reducer_failure_is_atomic_and_order_faults_precede_numeric_overflow",
    "reference_metrics::tests::static_inventory_and_response_resolution_recompute_materialized_facts",
    "reference_metrics::tests::strict_json_and_canonical_decimal_fail_closed",
    "reference_metrics::tests::v2_metric_evidence_closes_empty_pair_batches_and_rejects_timeline_tampering"
)

$headlessTests = @(
    "retained_s1m5_boundaries_quiescence_and_shared_inputs_are_exact",
    "retained_s1m5_brute_and_computed_structural_oracles_are_exact",
    "retained_s1m5_design_pair_plan_and_replays_are_strict_headless_artifacts",
    "retained_s1m5_direct_and_headless_complete_traces_are_identical",
    "retained_s1m5_metric_reduction_is_read_only_and_matches_both_goldens"
)

$appTests = @(
    "retained_s1m5_complete_reports_and_v7_hashes_match_headless_and_bevy"
)

$fuzzTests = @(
    "s1m5_architecture_v1_v2_schedules_reach_plan_and_malformed_cases_fail_closed",
    "s1m5_generated_byte_streams_are_bounded_deterministic_and_panic_free",
    "s1m5_metric_set_accepted_path_reaches_a_canonical_hash_stable_fixed_point",
    "s1m5_reference_artifact_corpus_is_bounded_deterministic_and_panic_free"
)

$requiredTests = @(
    New-TestEvidence @(1, 2, 3) "aon-sim" @("--lib") "reference_architecture::tests::strict_json_round_trips_to_canonical_order" "Architecture v1 strict bytes, hash domain, canonical order, and immediate-binding behavior remain exact"
    New-TestEvidence @(3) "aon-sim" @("--lib") "reference_architecture::tests::semantic_hash_normalizes_order_and_excludes_display_name" "Architecture v1 semantic identity includes content and excludes presentation"
    New-TestEvidence @(1, 3, 7) "aon-sim" @("--lib") "reference_architecture::tests::plan_uses_all_seven_dependency_phases_and_sorts_each_batch" "the retained v1 dependency plan and local ordering remain exact"
    New-TestEvidence @(3, 7) "aon-sim" @("--lib") "reference_architecture::tests::plan_places_unbound_then_resolves_bindings_without_raw_artifact_ids" "portable local IDs resolve only after dependency placement"
    New-TestEvidence @(3, 7) "aon-sim" @("--lib") "reference_architecture::tests::command_log_hash_is_complete_and_order_sensitive" "the complete Command-v1 stream reaches the design Command Log hash"
    New-TestEvidence @(3, 7) "aon-sim" @("--lib") "reference_architecture::tests::failed_late_batch_returns_no_partial_candidate_or_evidence" "late materialization rejection publishes no candidate or partial evidence"
    New-TestEvidence @(3, 7) "aon-sim" @("--lib") "reference_architecture::tests::materializer_executes_one_dependency_batch_per_tick_and_returns_complete_evidence" "accepted batches publish exact commands, IDs, and build boundary"
    New-TestEvidence @(3, 10) "aon-sim" @("--lib") "reference_architecture::tests::validation_rejects_duplicate_dangling_wrong_kind_and_duplicate_bindings" "the closed primitive schema fails on malformed references and bindings"
    New-TestEvidence @(3, 4) "aon-sim" @("--lib") "reference_architecture::tests::scenario_ordinals_are_resolved_explicitly" "Scenario anchors resolve in explicit canonical ordinal order"
    New-TestEvidence @(3, 4) "aon-sim" @("--lib") "reference_architecture::tests::validation_binds_the_full_profile_contract" "architecture validation binds the exact contract and physical profile"
    New-TestEvidence @(2, 3) "aon-sim" @("--lib") "reference_architecture::tests::v2_strict_json_round_trip_and_hash_bind_the_schedule" "Architecture v2 strictly round-trips and its independent hash domain binds every ordered schedule row"
    New-TestEvidence @(2, 3, 7, 10) "aon-sim" @("--lib") "reference_architecture::tests::v2_schedule_validation_rejects_missing_duplicate_free_and_late_nonpower_bindings" "v2 rejects malformed schedule coverage, Free ends, duplicates, and later non-Source bindings before execution"
    New-TestEvidence @(7, 10) "aon-sim" @("--lib") "reference_architecture::tests::v2_materializer_records_each_earliest_post_stage_quiescence_barrier" "each staged materialization records its bounded earliest queue-quiescence barrier and forbids build-time contact, destruction, or run-end"
    New-TestEvidence @(4, 7, 10) "aon-sim" @("--lib") "reference_architecture::tests::v2_pair_materializer_preserves_empty_stages_and_equal_build_end_atomically" "paired v2 execution preserves empty stages, shared command Ticks, common barriers, equal buildEnd, and atomic failure"

    New-TestEvidence @(2, 4) "aon-sim" @("--lib") "reference_experiment::tests::pair_envelope_errors_precede_strict_body_errors" "Pair v1 selects its envelope before strict body typing"
    New-TestEvidence @(2, 4) "aon-sim" @("--lib") "reference_experiment::tests::pair_strict_json_rejects_unknown_fields_and_nonzero_seed" "Pair v1 rejects unknown fields and nonzero Seed"
    New-TestEvidence @(4) "aon-sim" @("--lib") "reference_experiment::tests::pair_hash_binds_capacity_and_power_source_sequence" "Pair identity binds Core capacity and canonical Power Source sequence"
    New-TestEvidence @(4) "aon-sim" @("--lib") "reference_experiment::tests::canonical_scenario_sequence_hashes_normalize_order_and_bind_every_field" "Scenario Power and Enemy sequence hashes normalize order and bind every field"
    New-TestEvidence @(4, 10) "aon-sim" @("--lib") "reference_experiment::tests::pair_requires_exact_cardinal_anchors_empty_shared_log_and_coherent_bindings" "Pair cardinal anchors, shared log, and response bindings are exact"
    New-TestEvidence @(4) "aon-sim" @("--lib") "reference_experiment::tests::fairness_checks_profile_ids_schemas_scenario_identity_and_sequence_bodies" "fairness validates IDs, versions, schema bodies, and source/enemy sequences"
    New-TestEvidence @(4) "aon-sim" @("--lib") "reference_experiment::tests::pair_round_trips_and_hashes_independently_of_json" "Pair v1 canonical bytes and semantic hash are stable"
    New-TestEvidence @(2) "aon-sim" @("--lib") "reference_experiment::tests::experiment_v2_envelope_errors_precede_strict_body_errors" "Experiment v2 selects its envelope first"
    New-TestEvidence @(2) "aon-sim" @("--lib") "reference_experiment::tests::experiment_v2_rejects_unknown_body_fields" "Experiment v2 body is strict"
    New-TestEvidence @(2, 3) "aon-sim" @("--lib") "reference_experiment::tests::experiment_v2_strict_json_round_trips_canonically" "Experiment v2 canonical bytes are stable"
    New-TestEvidence @(3, 4) "aon-sim" @("--lib") "reference_experiment::tests::experiment_v2_retains_pair_hash_and_resolve_revalidates_it" "Run resolution revalidates the exact Pair"
    New-TestEvidence @(3) "aon-sim" @("--lib") "reference_experiment::tests::v2_plan_resolves_exactly_two_distinct_runs" "the plan resolves exactly Brute and Computed with distinct IDs"
    New-TestEvidence @(3) "aon-sim" @("--lib") "reference_experiment::tests::v2_run_id_has_independent_domain_and_field_sensitivity" "Run ID v2 is domain-separated and field-sensitive"

    New-TestEvidence @(7) "aon-sim" @("--lib") "simulation::tests::signal_quiescence_snapshot_counts_every_deferred_mechanism_without_mutation" "the public read-only quiescence snapshot counts driver, signal-arrival, and Gate-transition work without mutation"
    New-TestEvidence @(7) "aon-sim" @("--lib") "simulation::tests::signal_quiescence_ignores_unscheduled_unpowered_desired_level" "an unpowered Gate output mismatch with no scheduled transition is quiescent rather than phantom pending work"

    New-TestEvidence @(2, 8) "aon-sim" @("--lib") "reference_metrics::tests::strict_json_and_canonical_decimal_fail_closed" "Metric JSON, decimal, ordering, and structural faults fail closed"
    New-TestEvidence @(2, 8) "aon-sim" @("--lib") "reference_metrics::tests::format_and_hash_envelopes_precede_unknown_body_fields" "Metric Set and Artifact envelope faults precede strict body faults"
    New-TestEvidence @(2, 8) "aon-sim" @("--lib") "reference_metrics::tests::metric_set_and_result_round_trip_with_stable_semantic_hashes" "Metric Set and Result canonical bytes and hashes are stable"
    New-TestEvidence @(4, 8) "aon-sim" @("--lib") "reference_metrics::tests::metric_set_hash_binds_portable_observation_names_and_enemy_ordinal" "Metric Set identity binds portable response names and the Scenario Enemy ordinal"
    New-TestEvidence @(2, 8) "aon-sim" @("--lib") "reference_metrics::tests::metric_set_v1_declares_every_hashed_static_and_runtime_field_in_exact_tag_order" "Metric Set v1 declares every hashed static and runtime field in its exact 37-tag order"
    New-TestEvidence @(4, 8) "aon-sim" @("--lib") "reference_metrics::tests::portable_response_names_resolve_across_independent_local_namespaces" "portable response names independently resolve across Brute and Computed local-ID namespaces"
    New-TestEvidence @(8) "aon-sim" @("--lib") "reference_metrics::tests::metric_artifact_hash_binds_new_ncu_core_baseline_and_runtime_fields" "Metric identity binds planned NCU, measurement-start Core integrity, and every runtime field"
    New-TestEvidence @(5, 6, 8) "aon-sim" @("--lib") "reference_metrics::tests::inventory_and_latency_invariants_reject_inconsistent_results" "static inventory and response latency invariants reject inconsistent facts"
    New-TestEvidence @(4, 5, 6, 8) "aon-sim" @("--lib") "reference_metrics::tests::static_inventory_and_response_resolution_recompute_materialized_facts" "materialized commands, local IDs, exact routing, NCU, planned Work, and response bindings are recomputed"
    New-TestEvidence @(4, 7, 8, 10) "aon-sim" @("--lib") "reference_metrics::tests::v2_metric_evidence_closes_empty_pair_batches_and_rejects_timeline_tampering" "metric derivation requires the exact paired placement, binding, empty-side, barrier, and build-end execution timeline"
    New-TestEvidence @(8) "aon-sim" @("--lib") "reference_metrics::tests::reducer_exactly_applies_window_power_heat_support_kills_and_latency_once" "the reducer applies the exact window, Heat, Power, support, unique-kill, and response-latency rules once"
    New-TestEvidence @(2, 8, 10) "aon-sim" @("--lib") "reference_metrics::tests::reducer_failure_is_atomic_and_order_faults_precede_numeric_overflow" "metric reduction is atomic and deterministic structural/order faults precede checked overflow"

    New-TestEvidence @(2, 11) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m5_reference_artifact_corpus_is_bounded_deterministic_and_panic_free" "all S1-M5 strict decoder boundaries are bounded, deterministic, and panic-free"
    New-TestEvidence @(2, 3, 11) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m5_architecture_v1_v2_schedules_reach_plan_and_malformed_cases_fail_closed" "valid Architecture v1/v2 seeds reach canonical materialization plans while malformed schedules fail closed"
    New-TestEvidence @(2, 11) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m5_generated_byte_streams_are_bounded_deterministic_and_panic_free" "2,048 generated byte streams reach all five bounded S1-M5 decoder targets without panic"
    New-TestEvidence @(2, 8, 11) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m5_metric_set_accepted_path_reaches_a_canonical_hash_stable_fixed_point" "an accepted Metric Set reaches a canonical hash-stable fixed point"
    New-TestEvidence @(11) "aon-fuzz-harness" @("--test", "cli_modes") "all_mode_invokes_every_decoder_target_including_replay" "all-mode retains every prior target and invokes the S1-M5 reference lane"

    New-TestEvidence @(2, 3, 4, 8, 9) "aon-headless" @("--test", "s1m5_reference_architectures") "retained_s1m5_design_pair_plan_and_replays_are_strict_headless_artifacts" "retained locators, hashes, Run IDs, command logs, canonical bytes, fairness inputs, and cross-design Metric bindings are exact"
    New-TestEvidence @(5, 6, 7, 10) "aon-headless" @("--test", "s1m5_reference_architectures") "retained_s1m5_brute_and_computed_structural_oracles_are_exact" "exact Brute and Computed IDs, endpoints, topology, roles, primitive counts, complete non-Free coverage, and the generated retained stage partitions are independently frozen"
    New-TestEvidence @(4, 7, 10) "aon-headless" @("--test", "s1m5_reference_architectures") "retained_s1m5_boundaries_quiescence_and_shared_inputs_are_exact" "paired stage Ticks, bounded common barrier evidence, equal post-final-quiet buildEnd, forbidden build-time contact/destruction/run-end, empty shared inputs, and post-build autonomy exactly match the Pair"
    New-TestEvidence @(7, 9) "aon-headless" @("--test", "s1m5_reference_architectures") "retained_s1m5_direct_and_headless_complete_traces_are_identical" "both direct and headless runs reproduce every report and checkpoint"
    New-TestEvidence @(4, 8, 9) "aon-headless" @("--test", "s1m5_reference_architectures") "retained_s1m5_metric_reduction_is_read_only_and_matches_both_goldens" "an independent checked report oracle matches both read-only reductions and exact retained Metric Artifact bytes"
    New-TestEvidence @(7, 9) "aon-app" @("--test", "s1m5_reference_architectures") "retained_s1m5_complete_reports_and_v7_hashes_match_headless_and_bevy" "both Bevy schedules reproduce complete headless reports and V7 checkpoints"

    New-TestEvidence @(1, 10) "aon-sim" @("--test", "artifact_stage_features") "s1m2_features_require_the_power_world_and_later_features_remain_unsupported" "later Relay, Payload, and Radiation scope remains unsupported"
    New-TestEvidence @(1, 2) "aon-sim" @("--lib") "experiment_artifact::tests::retained_plan_strictly_decodes_and_canonically_reencodes" "retained Experiment v1 bytes remain strict and canonical"
    New-TestEvidence @(1, 3) "aon-sim" @("--lib") "experiment::tests::run_id_encoder_binds_every_field_independently" "retained Run ID v1 remains field-sensitive"
)

$staticEvidence = @(
    @{ Gates = @(5, 6, 7, 8, 9, 11); Evidence = "the non-writing generator double-builds and verifies both exact retained designs, traces, metrics, and all checked-in bytes" },
    @{ Gates = @(1); Evidence = "CI retains Stage 0 and S1-M0 through S1-M4 gates before invoking S1-M5 on Linux pwsh and Windows native" },
    @{ Gates = @(10); Evidence = "the admitted Reference Architecture operation enum contains only existing placement primitives; the generator asserts no post-build architecture commands or Stage 1 claim" }
)

$allEvidence = @($requiredTests) + @($staticEvidence)
$coveredGates = @(
    $allEvidence |
        ForEach-Object { $_.Gates } |
        ForEach-Object { [int] $_ } |
        Sort-Object -Unique
)
$gateDifference = @(Compare-Object -ReferenceObject @(1..11) -DifferenceObject $coveredGates)
if ($gateDifference.Count -ne 0) {
    throw "S1-M5 technical gate table must cover every in-tree executable Gate 1 through Gate 11; Gate 12 remains the external post-commit clean-clone gate"
}

$duplicateTests = @(
    $requiredTests |
        ForEach-Object { $_.Package + "|" + ($_.TargetArguments -join "|") + "|" + $_.TestName } |
        Group-Object |
        Where-Object { $_.Count -ne 1 }
)
if ($duplicateTests.Count -ne 0) {
    throw "S1-M5 technical gate table contains duplicate exact test entries"
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    Assert-ExactPrefixedTests "aon-sim" @("--lib") "reference_architecture::tests::" $architectureTests
    Assert-ExactPrefixedTests "aon-sim" @("--lib") "reference_experiment::tests::" $experimentTests
    Assert-ExactPrefixedTests "aon-sim" @("--lib") "reference_metrics::tests::" $metricTests
    Assert-ExactTargetTests "aon-headless" @("--test", "s1m5_reference_architectures") $headlessTests
    Assert-ExactTargetTests "aon-app" @("--test", "s1m5_reference_architectures") $appTests
    Assert-ExactPrefixedTests "aon-fuzz-harness" @("--test", "regression_corpus") "s1m5_" $fuzzTests
    Assert-ExactTargetTests "aon-fuzz-harness" @("--test", "cli_modes") @("all_mode_invokes_every_decoder_target_including_replay")

    $ciText = Get-Content -Raw ".github/workflows/ci.yml"
    foreach ($prior in @(
        "stage0-technical-gate.ps1",
        "s1-m0-technical-gate.ps1",
        "s1-m1-technical-gate.ps1",
        "s1-m2-technical-gate.ps1",
        "s1-m3-technical-gate.ps1",
        "s1-m4-technical-gate.ps1"
    )) {
        if ([Regex]::Matches($ciText, [Regex]::Escape("./scripts/$prior")).Count -ne 2) {
            throw "CI must retain exactly two native/pwsh references to $prior"
        }
    }
    if ([Regex]::Matches($ciText, [Regex]::Escape("./scripts/s1-m5-technical-gate.ps1")).Count -ne 2) {
        throw "CI must reference the S1-M5 technical gate exactly twice"
    }
    if ($ciText -notmatch '(?ms)runs-on: ubuntu-24\.04.*?name: S1-M5 technical gate\s+shell: pwsh\s+run: \./scripts/s1-m5-technical-gate\.ps1') {
        throw "CI must run the S1-M5 gate through pwsh on Linux"
    }
    if ($ciText -notmatch '(?ms)runs-on: windows-latest.*?name: Native S1-M5 technical gate\s+shell: pwsh\s+run: \./scripts/s1-m5-technical-gate\.ps1') {
        throw "CI must run the S1-M5 gate on Windows native"
    }

    foreach ($testCase in $requiredTests) {
        Invoke-ExactCargoTest `
            -Gates $testCase.Gates `
            -Package $testCase.Package `
            -TargetArguments $testCase.TargetArguments `
            -TestName $testCase.TestName `
            -Evidence $testCase.Evidence
    }
    Invoke-S1M5GeneratorEvidence

    Write-Host ""
    Write-Host "S1-M5 technical gate passed: $($requiredTests.Count) unique exact tests and $($staticEvidence.Count) executable/static invariants cover Gates 1-11; Gate 12 remains external."
}
finally {
    Pop-Location
}
