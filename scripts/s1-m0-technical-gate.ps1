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

function Invoke-ExactCargoTest {
    param(
        [Parameter(Mandatory)]
        [string] $Gate,

        [Parameter(Mandatory)]
        [string] $Package,

        [Parameter(Mandatory)]
        [string[]] $TargetArguments,

        [Parameter(Mandatory)]
        [string] $TestName,

        [Parameter(Mandatory)]
        [string] $Evidence
    )

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
    $output = Invoke-CargoChecked -Arguments $arguments -Description "$Gate - $Evidence"
    $lines = @($output | ForEach-Object { $_.ToString() })
    $passingSummaries = @(
        $lines | Where-Object {
            $_ -match '^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;'
        }
    )
    if ($passingSummaries.Count -ne 1) {
        throw "$Gate did not execute exactly one passing test named '$TestName'"
    }
}

$requiredTests = @(
    @{
        Gate = "S1-M0 Profile"
        Package = "aon-sim"
        TargetArguments = @("--test", "physical_scale_matrix")
        TestName = "two_by_two_by_two_matrix_has_eight_unique_hash_sorted_profiles"
        Evidence = "2 x 2 x 2 PhysicalScaleProfile matrix and hash-sorted publication"
    },
    @{
        Gate = "S1-M0 Profile"
        Package = "aon-sim"
        TargetArguments = @("--test", "physical_scale_matrix")
        TestName = "physical_hash_binds_geometry_anchor_and_both_pitch_axes_but_not_profile_id"
        Evidence = "isolated Physical hash sensitivity and profileId exclusion"
    },
    @{
        Gate = "S1-M0 Profile"
        Package = "aon-sim"
        TargetArguments = @("--test", "physical_scale_matrix")
        TestName = "valid_physical_product_above_frozen_limit_is_rejected_without_publication"
        Evidence = "valid oversized Physical product fails before publication"
    },
    @{
        Gate = "S1-M0 Profile"
        Package = "aon-sim"
        TargetArguments = @("--test", "contract_profiles")
        TestName = "standalone_physical_profile_artifact_round_trip_is_canonical_and_hash_stable"
        Evidence = "strict canonical PhysicalScaleProfile artifact round trip"
    },
    @{
        Gate = "S1-M0 Profile"
        Package = "aon-sim"
        TargetArguments = @("--test", "contract_profiles")
        TestName = "scenario_semantic_hash_includes_identity_but_excludes_paths_and_profile_ids"
        Evidence = "retained Scenario ArtifactHash golden and metadata exclusions"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "eight_physical_profiles_times_two_distances_make_sixteen_unique_runs"
        Evidence = "canonical 16-run experiment expansion"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "retained_plan_has_the_frozen_ordered_sixteen_run_ids"
        Evidence = "full retained ordered RunId golden set"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "relocated_artifact_paths_do_not_change_resolved_run_ids"
        Evidence = "artifact path relocation is excluded from resolved Run identity"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "long_wire_design_public_construction_preserves_positive_exact_geometry"
        Evidence = "LongWire checked construction and exact unsnapped geometry"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "compound_errors_follow_the_frozen_validation_precedence"
        Evidence = "frozen core validation precedence"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "every_selectable_run_identity_field_changes_the_run_id"
        Evidence = "RunId sensitivity across every selectable identity field"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "real_capacity_balance_variants_change_balance_and_run_identity_only"
        Evidence = "real Balance ownership changes Balance and Run identity only"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--test", "experiment_matrix")
        TestName = "run_id_uses_the_frozen_domain_and_contract_field_order"
        Evidence = "RunId literal golden, domain, and canonical field order"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment::tests::run_id_encoder_binds_every_field_independently"
        Evidence = "core RunId encoder checked by an independent field encoder"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment::tests::nonallocating_bound_helpers_cover_unreachable_public_plan_edges"
        Evidence = "typed cardinality overflow and text-length error classes"
    },
    @{
        Gate = "S1-M0 Experiment"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment::tests::duplicate_run_id_collision_guard_is_typed"
        Evidence = "typed duplicate semantic RunId collision guard"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::retained_plan_strictly_decodes_and_canonically_reencodes"
        Evidence = "retained Experiment v1 canonical byte re-encoding"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::format_stage_and_design_derivation_versions_are_required_and_strict"
        Evidence = "required strict Experiment format, stage, and design derivation versions"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::strict_json_rejects_unknown_duplicate_and_float_fields"
        Evidence = "strict Experiment v1 JSON grammar"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::structural_preflight_precedes_referenced_content_and_max_ticks"
        Evidence = "artifact structural preflight and deterministic precedence"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::references_must_be_portable_and_hashes_lowercase"
        Evidence = "portable artifact references and canonical lowercase hashes"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::direct_resolution_rechecks_structure_before_artifact_bytes"
        Evidence = "direct resolver repeats structure checks before referenced bytes"
    },
    @{
        Gate = "S1-M0 Artifact"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "experiment_artifact::tests::artifact_backed_resolution_rejects_schema_kind_invariant_id_and_hash_mismatches"
        Evidence = "typed artifact schema, kind, invariant, ID, and hash mismatch classes"
    },
    @{
        Gate = "S1-M0 Headless"
        Package = "aon-headless"
        TargetArguments = @("--test", "experiment_cli")
        TestName = "retained_plan_materializes_eight_profiles_and_sixteen_unique_runs"
        Evidence = "retained plan deterministic materialization"
    },
    @{
        Gate = "S1-M0 Headless"
        Package = "aon-headless"
        TargetArguments = @("--test", "experiment_cli")
        TestName = "cli_resolves_plan_relative_paths_outside_the_workspace"
        Evidence = "portable relative artifact resolution outside the workspace"
    },
    @{
        Gate = "S1-M0 Headless"
        Package = "aon-headless"
        TargetArguments = @("--test", "experiment_cli")
        TestName = "invalid_plan_has_typed_exit_and_publishes_nothing"
        Evidence = "typed failure with no partial publication"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "exact_contract_validation_preserves_every_absolute_coordinate"
        Evidence = "absolute module geometry with no scale, snap, or mutation"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "compatibility_mismatches_precede_invalid_geometry_and_do_not_mutate"
        Evidence = "exact compatibility precedence and no implicit conversion"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "valid_geometry_rejects_numeric_and_physical_contract_mismatches"
        Evidence = "valid geometry still requires exact Numeric and Physical contracts"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "retained_v1_fixture_exactly_reencodes_and_matches_the_hash_golden"
        Evidence = "retained Module v1 fixture and semantic hash golden"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "independent_module_encoder_matches_retained_literal_golden"
        Evidence = "Module hash stream checked by an independent canonical encoder"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "primitive_input_record_permutations_have_one_hash_and_canonical_json"
        Evidence = "canonical Module record ordering"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "strict_json_rejects_unknown_duplicate_trailing_and_floating_geometry"
        Evidence = "strict Module v1 JSON grammar"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "strict_json_rejects_unsupported_contract_and_malformed_hash_text"
        Evidence = "Module format, algorithm, semantics, hash, and local-ID rejection classes"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "module::tests::canonical_collection_count_over_u32_is_typed_without_allocation"
        Evidence = "typed Module canonical collection-count overflow"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "module_rejects_self_and_inter_wire_overlap_and_spacing"
        Evidence = "Module self/inter-wire overlap and exact routing-spacing laws"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "module_rejects_junction_in_wire_interior_and_invalid_gate_contacts"
        Evidence = "Module junction-interior and wire-gate contact laws"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "module_allows_crossings_shared_physical_endpoints_and_profile_anchors"
        Evidence = "lawful Module crossings, shared endpoints, and profile anchors"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "module_shape_and_reference_errors_precede_contract_mismatches"
        Evidence = "Module shape/reference precedence before compatibility"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "module_arithmetic_overflow_precedes_empty_aabb_bounds"
        Evidence = "Module checked arithmetic precedence before bounds"
    },
    @{
        Gate = "S1-M0 Module"
        Package = "aon-sim"
        TargetArguments = @("--test", "module_absolute_geometry")
        TestName = "module_geometry_error_precedence_is_overflow_then_quantum_then_pitch"
        Evidence = "Module overflow, quantum, and routing-pitch precedence"
    },
    @{
        Gate = "S1-M0 Replay"
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m0_replay")
        TestName = "generated_profile_replay_round_trips_with_the_identical_full_trace"
        Evidence = "generated-profile canonical replay full-trace equality"
    },
    @{
        Gate = "S1-M0 Replay"
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m0_replay")
        TestName = "different_generated_physical_profile_is_rejected_before_execution"
        Evidence = "generated physical profile mismatch rejects before execution"
    },
    @{
        Gate = "S1-M0 Replay"
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m0_replay")
        TestName = "matrix_generation_does_not_mutate_an_existing_simulation"
        Evidence = "Experiment matrix generation is simulation-state noninterference"
    },
    @{
        Gate = "S1-M0 Fuzz"
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "experiment_regression_corpus_never_panics_and_preserves_exact_outcome"
        Evidence = "bounded Experiment decoder regression corpus"
    },
    @{
        Gate = "S1-M0 Fuzz"
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "module_regression_corpus_never_panics_and_preserves_exact_outcome"
        Evidence = "bounded Module decoder regression corpus"
    },
    @{
        Gate = "S1-M0 Fuzz"
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "cli_modes")
        TestName = "all_mode_invokes_every_decoder_target_including_replay"
        Evidence = "all mode includes replay, Experiment, and Module targets"
    }
)

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    foreach ($testCase in $requiredTests) {
        Invoke-ExactCargoTest `
            -Gate $testCase.Gate `
            -Package $testCase.Package `
            -TargetArguments $testCase.TargetArguments `
            -TestName $testCase.TestName `
            -Evidence $testCase.Evidence
    }
    Write-Host ""
    Write-Host "S1-M0 technical gate passed: $($requiredTests.Count) exact profile, Experiment, artifact, headless, Module, replay, and fuzz tests."
}
finally {
    Pop-Location
}
