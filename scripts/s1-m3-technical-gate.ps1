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

    # Let PowerShell enumerate native output lines. Returning a nested array makes Windows
    # PowerShell 5.1 stringify the complete discovery stream into one line.
    return $output
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

    return $testListingCache[$cacheKey]
}

function Assert-ExactTargetTests {
    param(
        [Parameter(Mandatory)]
        [string] $Package,

        [Parameter(Mandatory)]
        [string[]] $TargetArguments,

        [Parameter(Mandatory)]
        [string[]] $ExpectedTests
    )

    $listing = @(Get-CargoTestListing -Package $Package -TargetArguments $TargetArguments)
    $discovered = @(
        $listing |
            Where-Object { $_ -match '^(.+): test$' } |
            ForEach-Object { [Regex]::Match($_, '^(.+): test$').Groups[1].Value } |
            Sort-Object
    )
    $expected = @($ExpectedTests | Sort-Object)
    $difference = @(Compare-Object -ReferenceObject $expected -DifferenceObject $discovered)
    if ($difference.Count -ne 0) {
        $details = $difference | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
        throw "Exact test inventory mismatch for $Package $($TargetArguments -join ' '): $($details -join '; ')"
    }
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

$profileTests = @(
    "v4_reference_fixture_is_strict_valid_and_matches_the_constructor",
    "v2_and_v3_hashes_are_retained_exactly",
    "v4_requires_all_three_probe_sections",
    "v2_and_v3_forbid_the_v4_support_section",
    "support_power_is_positive_and_independently_hash_sensitive",
    "v4_strengthens_quadratic_capacity_cost_without_changing_v3_acceptance",
    "v4_json_rejects_unknown_duplicate_zero_denominator_and_wrong_schema",
    "unsupported_balance_schema_precedes_strict_version_body_faults"
)

$kernelTests = @(
    "c22_exact_curve_and_sorted_proportional_distribution_match_independent_oracle",
    "one_final_ceil_preserves_fractional_cross_term_and_excess_is_monotonic",
    "zero_excess_short_circuits_and_active_invalid_coefficients_fail_closed",
    "distribution_is_permutation_stable_conservative_and_fail_closed",
    "curve_and_distribution_report_typed_overflow_without_saturation"
)

$runtimeTests = @(
    "c22_flows_through_phase4_power_and_phase8_with_exact_values",
    "v4_undercapacity_reports_explicit_zero_without_materializing_a_load",
    "source_less_overcapacity_loads_persist_but_receive_no_grant_or_heat",
    "abandoned_wire_raises_global_support_while_powered_region_uses_one_partial_ratio",
    "removal_recomputes_support_and_v3_remains_opted_out"
)

$requiredTests = @(
    @{
        Gates = @(2, 11)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "v4_reference_fixture_is_strict_valid_and_matches_the_constructor"
        Evidence = "Balance v4 reference bytes, constructor, schema, and semantic hash are exact"
    },
    @{
        Gates = @(1)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "v2_and_v3_hashes_are_retained_exactly"
        Evidence = "Balance v2 and v3 encodings and hash goldens remain unchanged"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "v4_requires_all_three_probe_sections"
        Evidence = "Balance v4 requires capacity, power, and capacity-support probes"
    },
    @{
        Gates = @(2)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "v2_and_v3_forbid_the_v4_support_section"
        Evidence = "legacy Balance schemas fail closed on the v4-only support section"
    },
    @{
        Gates = @(2, 4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "support_power_is_positive_and_independently_hash_sensitive"
        Evidence = "supportPowerPerNCU is positive, independently encoded, and hash sensitive"
    },
    @{
        Gates = @(1, 2, 4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "v4_strengthens_quadratic_capacity_cost_without_changing_v3_acceptance"
        Evidence = "v4 requires positive quadratic cost while retained v3 acceptance is stable"
    },
    @{
        Gates = @(2, 4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "v4_json_rejects_unknown_duplicate_zero_denominator_and_wrong_schema"
        Evidence = "strict Balance v4 JSON rejects every malformed boundary"
    },
    @{
        Gates = @(2, 11, 15)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_profile")
        TestName = "unsupported_balance_schema_precedes_strict_version_body_faults"
        Evidence = "Balance schema envelope selection precedes strict version-body faults"
    },
    @{
        Gates = @(2, 11)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "profile::tests::balance_v4_encoder_is_the_schema_tagged_v3_encoding_plus_one_exact_rational"
        Evidence = "Balance v4 canonical bytes append exactly one schema-tagged Rational"
    },
    @{
        Gates = @(3, 5, 10)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_capacity_support")
        TestName = "c22_exact_curve_and_sorted_proportional_distribution_match_independent_oracle"
        Evidence = "C-22 derives 28 Energy and ascending-Wire shares 17 and 11"
    },
    @{
        Gates = @(3, 4, 10, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_capacity_support")
        TestName = "one_final_ceil_preserves_fractional_cross_term_and_excess_is_monotonic"
        Evidence = "one final ceiling matches an independent exact oracle and is nondecreasing"
    },
    @{
        Gates = @(3, 4)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_capacity_support")
        TestName = "zero_excess_short_circuits_and_active_invalid_coefficients_fail_closed"
        Evidence = "zero excess is exact zero and active invalid coefficients are typed errors"
    },
    @{
        Gates = @(5, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_capacity_support")
        TestName = "distribution_is_permutation_stable_conservative_and_fail_closed"
        Evidence = "WireId remainder allocation is conservative, stable, and fail closed"
    },
    @{
        Gates = @(4, 5)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_capacity_support")
        TestName = "curve_and_distribution_report_typed_overflow_without_saturation"
        Evidence = "curve and distribution arithmetic overflow is typed and never saturates"
    },
    @{
        Gates = @(6, 7, 8, 9, 10, 14)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_runtime")
        TestName = "c22_flows_through_phase4_power_and_phase8_with_exact_values"
        Evidence = "production phases publish exact C-22 accounting, loads, grants, heat, and analyzers"
    },
    @{
        Gates = @(3, 6, 9, 14)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_runtime")
        TestName = "v4_undercapacity_reports_explicit_zero_without_materializing_a_load"
        Evidence = "under-capacity v4 reports explicit zero and creates no support load"
    },
    @{
        Gates = @(6, 7, 8, 9, 14)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_runtime")
        TestName = "source_less_overcapacity_loads_persist_but_receive_no_grant_or_heat"
        Evidence = "source-less support remains nominal but has zero grant and zero heat"
    },
    @{
        Gates = @(6, 7, 8, 9, 10, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_runtime")
        TestName = "abandoned_wire_raises_global_support_while_powered_region_uses_one_partial_ratio"
        Evidence = "global demand includes abandoned Wire share while powered loads share one partial ratio"
    },
    @{
        Gates = @(1, 6, 9, 13, 14)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m3_runtime")
        TestName = "removal_recomputes_support_and_v3_remains_opted_out"
        Evidence = "removal recomputes v4 while retained v3 remains bit-for-bit feature off"
    },
    @{
        Gates = @(4, 8, 14)
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "power_runtime::tests::capacity_support_heat_boundary_is_positive_unit_interval_and_legacy_none_is_identical"
        Evidence = "support runtime boundaries return exact typed errors, valid heat uses a unit fraction, and legacy solve remains identical"
    },
    @{
        Gates = @(1, 6, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "capacity_accounting")
        TestName = "phase4_counts_each_wire_body_once_in_raw_fixed_units_across_routing_domains"
        Evidence = "retained Capacity accounting still counts one physical Wire exactly once"
    },
    @{
        Gates = @(1, 7, 13)
        Package = "aon-sim"
        TargetArguments = @("--test", "s1m2_power_oracle")
        TestName = "solver_matches_an_independent_exhaustive_oracle_over_all_ratio_values"
        Evidence = "retained common-ratio solver still matches its exhaustive oracle"
    },
    @{
        Gates = @(1, 7, 11)
        Package = "aon-headless"
        TargetArguments = @("--test", "s1m2_retained_replays")
        TestName = "retained_c08_pair_is_exact_and_headless_with_real_full_and_half_runtime_reports"
        Evidence = "retained C-08 full and half brownout Replays remain exact"
    },
    @{
        Gates = @(8, 9, 10, 11)
        Package = "aon-headless"
        TargetArguments = @("--test", "s1m3_retained_replay")
        TestName = "retained_c22_is_canonical_headless_and_exact_across_support_power_and_heat"
        Evidence = "retained C-22 canonical artifact and headless trace prove exact runtime evidence"
    },
    @{
        Gates = @(12, 15)
        Package = "aon-app"
        TargetArguments = @("--test", "s1m3_replay_host")
        TestName = "retained_c22_v7_trace_and_reports_match_headless_and_bevy"
        Evidence = "C-22 reports and V7 checkpoints match Headless and native Bevy hosts"
    },
    @{
        Gates = @(2, 11, 13)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "s1m3_scenario_balance_and_replay_artifacts_reach_bounded_strict_decoders"
        Evidence = "bounded strict decoders retain Balance v4 and C-22 Scenario/Replay artifacts"
    },
    @{
        Gates = @(3, 4, 5, 13)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "regression_corpus")
        TestName = "s1m3_capacity_support_corpus_is_bounded_exact_and_property_complete"
        Evidence = "bounded public-kernel corpus proves oracle, monotonicity, conservation, and permutation properties"
    },
    @{
        Gates = @(13, 15)
        Package = "aon-fuzz-harness"
        TargetArguments = @("--test", "cli_modes")
        TestName = "all_mode_invokes_every_decoder_target_including_replay"
        Evidence = "all-mode retains every previous target and invokes capacity-support fuzzing"
    },
    @{
        Gates = @(14)
        Package = "aon-sim"
        TargetArguments = @("--test", "artifact_stage_features")
        TestName = "s1m2_features_require_the_power_world_and_later_features_remain_unsupported"
        Evidence = "Relay and later Stage features remain explicitly unsupported after S1-M3"
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
    throw "S1-M3 technical gate table must cover every executable Gate 1 through Gate 15 exactly"
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
    throw "S1-M3 technical gate table contains duplicate exact test entries"
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    Assert-ExactTargetTests `
        -Package "aon-sim" `
        -TargetArguments @("--test", "s1m3_profile") `
        -ExpectedTests $profileTests
    Assert-ExactTargetTests `
        -Package "aon-sim" `
        -TargetArguments @("--test", "s1m3_capacity_support") `
        -ExpectedTests $kernelTests
    Assert-ExactTargetTests `
        -Package "aon-sim" `
        -TargetArguments @("--test", "s1m3_runtime") `
        -ExpectedTests $runtimeTests

    foreach ($testCase in $requiredTests) {
        Invoke-ExactCargoTest `
            -Gates $testCase.Gates `
            -Package $testCase.Package `
            -TargetArguments $testCase.TargetArguments `
            -TestName $testCase.TestName `
            -Evidence $testCase.Evidence
    }
    Write-Host ""
    Write-Host "S1-M3 technical gate passed: $($requiredTests.Count) fail-closed exact tests cover executable Gates 1-15."
}
finally {
    Pop-Location
}
