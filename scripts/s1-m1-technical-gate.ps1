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
        Gates = @(1, 10)
        Package = "aon-sim"
        TargetArguments = @("--test", "replay_golden")
        TestName = "retained_feedback_ring_is_the_exact_canonical_replay_encoding"
        Evidence = "regenerated feedback Replay preserves its command stream under the current V7 encoder"
    },
    @{
        Gates = @(1, 10)
        Package = "aon-sim"
        TargetArguments = @("--test", "replay_golden")
        TestName = "retained_100k_replay_round_trips_semantically_and_freezes_its_contract_golden"
        Evidence = "regenerated 100k Replay keeps its contract and command stream"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "scenario_decode_precedence_and_v1_v2_pairing_are_frozen"
        Evidence = "envelope-first strict Scenario v1/v2 dispatch and error precedence"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "scenario_v2_main_core_payload_is_strict_for_unknown_duplicate_float_and_overflow_fields"
        Evidence = "strict Scenario v2 unknown, duplicate, float, and integer-overflow rejection"
    },
    @{
        Gates = @(1, 2)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "scenario_v1_hash_is_preserved_and_v2_hash_is_main_core_sensitive"
        Evidence = "retained Scenario v1 hash and independent Scenario v2 hash encoder"
    },
    @{
        Gates = @(2, 5)
        Package = "aon-sim"
        TargetArguments = @("--test", "artifact_stage_features")
        TestName = "capacity_is_supported_only_by_a_main_core_initial_world"
        Evidence = "capacity feature and InitialWorld coherence"
    },
    @{
        Gates = @(2, 3, 5)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "simulation_construction_compound_errors_follow_frozen_precedence"
        Evidence = "unsupported feature, profile validity, contract, and coherence precedence"
    },
    @{
        Gates = @(3, 9)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "main_core_initial_state_has_implicit_anchor_capacity_and_read_only_projection"
        Evidence = "Main Core identity, frontier, fields, snapshot, Analyzer, and Phase 4 report"
    },
    @{
        Gates = @(3)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "main_core_position_requires_geometry_quantum_but_not_world_routing_pitch"
        Evidence = "Main Core position uses wire geometry quantum without world-pitch snapping"
    },
    @{
        Gates = @(3)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "main_core_removal_is_rejected_without_mutating_the_canonical_world"
        Evidence = "protected Main Core identity rejects removal atomically"
    },
    @{
        Gates = @(3, 4)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "signal_topology::tests::only_explicit_endpoint_targets_share_signal_nodes"
        Evidence = "Main Core anchor remains a physical terminal and creates no Signal routing"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "main_core_anchor_requires_exact_core_identity_position_and_open_world"
        Evidence = "exact live Core endpoint validation and canonical command tag"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "bind_port_accepts_the_exact_main_core_anchor_and_rejects_mismatched_identity"
        Evidence = "BindPort exact Main Core anchor behavior"
    },
    @{
        Gates = @(4)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "command::tests::wire_and_binding_encoding_uses_explicit_lengths_and_topology_tags"
        Evidence = "retained command endpoint tag ordering before the MainCoreAnchor append"
    },
    @{
        Gates = @(4, 10)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "replay_main_core_anchor_json_is_camel_case_and_round_trips"
        Evidence = "strict Replay JSON MainCoreAnchor encoding"
    },
    @{
        Gates = @(4, 10)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::main_core_anchor_endpoint_has_exact_v7_bytes"
        Evidence = "independent current-State MainCoreAnchor endpoint bytes"
    },
    @{
        Gates = @(5)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "capacity_feature_main_core_and_profile_dependency_triad_is_typed"
        Evidence = "capacity feature, Main Core, and Balance profile dependency triad"
    },
    @{
        Gates = @(5)
        Package = "aon-sim"
        TargetArguments = @("--test", "main_core_capacity")
        TestName = "oversized_capacity_probe_is_inert_for_empty_but_active_main_core_conversion_is_atomic"
        Evidence = "active conversion overflow and deferred profile fields remain fail-closed"
    },
    @{
        Gates = @(5)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "numeric::tests::whole_ncu_capacity_conversion_uses_fixed_raw_units_and_checks_overflow"
        Evidence = "whole NCU to raw Fixed-NCU exact conversion"
    },
    @{
        Gates = @(6)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "same_tick_removal_is_reflected_in_phase4_and_derived_accounting_is_not_hashed"
        Evidence = "Phase 0 removal is visible in same-Tick Phase 4 and Analyzer is derived-only"
    },
    @{
        Gates = @(6)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "equivalent_command_input_order_has_identical_accounting_and_state"
        Evidence = "equivalent command order preserves accounting and canonical state"
    },
    @{
        Gates = @(6)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::capacity_accounting_and_analyzer_are_invariant_to_wire_store_layout"
        Evidence = "clear/rebuild and dense store-layout permutations preserve derived accounting"
    },
    @{
        Gates = @(7, 9)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "phase4_counts_each_wire_body_once_in_raw_fixed_units_across_routing_domains"
        Evidence = "C-21 one-body multi-role usage, internal Wire usage, and sorted Analyzer rows"
    },
    @{
        Gates = @(7, 8)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "splitting_a_centerline_across_wire_entities_does_not_change_additive_usage"
        Evidence = "C-21 four-Wire split accounting and WireId-stable rows"
    },
    @{
        Gates = @(7)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "mobile_substrate_wire_body_contributes_once_to_network_usage"
        Evidence = "Mobile Substrate routing-domain Wire contributes one physical-body charge"
    },
    @{
        Gates = @(7, 10, 11)
        Package = "aon-headless"
        TargetArguments = @("--test", "capacity_retained_replay")
        TestName = "retained_capacity_replay_is_canonical_and_executes_headlessly_with_exact_c21_accounting"
        Evidence = "retained C-21 Replay exact 10/12 NCU reports, Analyzer rows, current trace, and restart"
    },
    @{
        Gates = @(8)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "redundant_vertices_preserve_length_while_bends_sum_maximal_runs"
        Evidence = "per-Wire collinear collapse and maximal-run bend law"
    },
    @{
        Gates = @(8)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "diagonal_rounding_is_per_wire_without_cross_wire_remainder_redistribution"
        Evidence = "46_341 direct versus 46_342 split-Wire rounding golden"
    },
    @{
        Gates = @(5, 8)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "capacity::tests::aggregate_usage_checks_u64_overflow_without_saturation"
        Evidence = "global capacity accumulation fails closed on u64 overflow"
    },
    @{
        Gates = @(8)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "numeric::tests::ceil_isqrt_satisfies_exhaustive_and_seeded_boundary_invariants"
        Evidence = "integer Euclidean ceil-sqrt property and numeric boundary coverage"
    },
    @{
        Gates = @(9, 12)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "over_capacity_build_is_accepted_and_reported_without_structural_side_effects"
        Evidence = "S1-M1 reports U and S without premature capacity build rejection"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--test", "bootstrap_simulation")
        TestName = "current_empty_state_v7_hash_has_a_golden_value"
        Evidence = "retained Empty world hash under the current global State encoder"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::state_encoding_v7_has_exact_contract_tick_revision_and_identity_order"
        Evidence = "independent Empty-compatible current-State prefix and identity field order"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "canonical::tests::main_core_v7_section_has_exact_anchor_order_and_is_hash_sensitive"
        Evidence = "independent Main Core current-State field order and sensitivity"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--test", "replay_golden")
        TestName = "retained_v3_empty_artifact_strictly_decodes_and_round_trips_exactly"
        Evidence = "strict retained V3 migration fixture rejects before Tick 0"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--test", "replay_golden")
        TestName = "retained_v4_empty_artifact_strictly_decodes_round_trips_and_is_execution_rejected"
        Evidence = "strict retained V4 migration fixture rejects before Tick 0"
    },
    @{
        Gates = @(10)
        Package = "aon-sim"
        TargetArguments = @("--test", "replay_golden")
        TestName = "main_core_v1_replay_rejects_a_nonzero_seed_before_execution"
        Evidence = "MainCoreV1 Replay rejects a nonzero world-generator seed before execution"
    },
    @{
        Gates = @(10)
        Package = "aon-headless"
        TargetArguments = @("--test", "mobility_retained_replay")
        TestName = "retained_mobility_stop_replay_is_canonical_and_executes_headlessly"
        Evidence = "regenerated retained-state mobility Replay command stream and current trace"
    },
    @{
        Gates = @(10)
        Package = "aon-headless"
        TargetArguments = @("--test", "mobility_retained_replay")
        TestName = "current_input_only_replay_is_canonical_and_resumes_after_the_matched_set_release"
        Evidence = "regenerated current-input mobility Replay command stream and current trace"
    },
    @{
        Gates = @(10)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "replay_regression_corpus_never_panics_and_preserves_acceptance_class"
        Evidence = "bounded decoder corpus includes the retained S1-M1 Replay"
    },
    @{
        Gates = @(11)
        Package = "aon-app"
        TargetArguments = @("--test", "replay_host")
        TestName = "retained_s1m1_capacity_replay_matches_headless_and_bevy_fixed_update"
        Evidence = "Headless and Bevy hosts preserve capacity reports and current hashes across presentation paths"
    },
    @{
        Gates = @(11)
        Package = "aon-app"
        TargetArguments = @("--test", "capacity_presenter")
        TestName = "network_view_and_inspector_project_the_main_core_without_mutating_the_snapshot"
        Evidence = "Main Core rendering, selection, and inspector projections are read-only"
    },
    @{
        Gates = @(12)
        Package = "aon-sim"
        TargetArguments = @("--test", "artifact_stage_features")
        TestName = "s1m2_features_require_the_power_world_and_later_features_remain_unsupported"
        Evidence = "Sensing and Power require their complete world while Relay, Payload, and Radiation remain unsupported"
    }
)

$coveredGates = @(
    $requiredTests |
        ForEach-Object { $_.Gates } |
        ForEach-Object { [int] $_ } |
        Sort-Object -Unique
)
$gateDifference = @(
    Compare-Object -ReferenceObject @(1..12) -DifferenceObject $coveredGates
)
if ($gateDifference.Count -ne 0) {
    throw "S1-M1 technical gate table must cover every executable Gate 1 through Gate 12 exactly"
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
    throw "S1-M1 technical gate table contains duplicate exact test entries"
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
    Write-Host "S1-M1 technical gate passed: $($requiredTests.Count) fail-closed exact tests cover executable Gates 1-12."
}
finally {
    Pop-Location
}
