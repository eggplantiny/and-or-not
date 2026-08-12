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

function Assert-CanonicalCoreDependencyBoundary {
    $arguments = @(
        "tree",
        "--color", "never",
        "-p", "aon-sim",
        "--edges", "all",
        "--prefix", "none",
        "--all-features",
        "--target", "all",
        "--locked",
        "--offline"
    )
    $output = Invoke-CargoChecked `
        -Arguments $arguments `
        -Description "Architecture - canonical core dependency boundary"
    $tree = ($output | ForEach-Object { $_.ToString() }) -join "`n"
    if ($tree -match '(?m)^(bevy|winit|wgpu)(?:$|[-_ ])') {
        throw "aon-sim must not depend on Bevy, winit, or wgpu"
    }
}

$requiredTests = @(
    @{
        Gate = "C-01"
        Package = "aon-sim"
        TargetArguments = @("--test", "signal_conformance")
        TestName = "c01_not_transitions_at_t1_and_eight_wu_wire_arrives_at_t4"
        Evidence = "Gate + Wire Delay"
    },
    @{
        Gate = "C-02"
        Package = "aon-sim"
        TargetArguments = @("--test", "signal_conformance")
        TestName = "c02_two_tick_pulse_is_filtered_and_reserved_energy_becomes_heat"
        Evidence = "Inertial Filtering"
    },
    @{
        Gate = "C-03"
        Package = "aon-sim"
        TargetArguments = @("--test", "signal_conformance")
        TestName = "c03_one_tick_pulse_survives_twelve_wu_transport_after_exactly_five_ticks"
        Evidence = "Wire Transport"
    },
    @{
        Gate = "C-05"
        Package = "aon-sim"
        TargetArguments = @("--test", "feedback_conformance")
        TestName = "c05_one_not_ring_has_exact_two_d_period_without_deleted_edges"
        Evidence = "Feedback Ring"
    },
    @{
        Gate = "C-06"
        Package = "aon-sim"
        TargetArguments = @("--test", "feedback_conformance")
        TestName = "c06_symmetric_startup_is_independent_of_input_slice_order"
        Evidence = "Symmetric Latch Startup"
    },
    @{
        Gate = "C-14"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "mobility::tests::c14_control_table_selects_straight_left_right_reverse_and_stops_on_x"
        Evidence = "Mobile Junction exact control table"
    },
    @{
        Gate = "C-14"
        Package = "aon-sim"
        TargetArguments = @("--test", "mobility_stage0")
        TestName = "routed_unknown_stop_left_or_right_blocks_the_entire_junction_tick"
        Evidence = "routed STOP/LEFT/RIGHT X integration"
    },
    @{
        Gate = "C-16"
        Package = "aon-app"
        TargetArguments = @("--test", "replay_host")
        TestName = "mobility_replay_matches_headless_bevy_presenter_and_frame_partitions"
        Evidence = "Mobility Replay Headless/Bevy full-trace determinism"
    },
    @{
        Gate = "C-17"
        Package = "aon-sim"
        TargetArguments = @("--test", "conformance_stage0")
        TestName = "c17_numeric_geometry_uses_integer_euclidean_length_and_floor_cells"
        Evidence = "Numeric Geometry"
    },
    @{
        Gate = "C-18"
        Package = "aon-sim"
        TargetArguments = @("--test", "topology_sync")
        TestName = "local_zero_sync_is_same_tick_while_c18_physical_sync_waits_exact_delay"
        Evidence = "Topology Synchronization"
    },
    @{
        Gate = "C-19"
        Package = "aon-sim"
        TargetArguments = @("--test", "topology_sync")
        TestName = "replaced_shorter_route_sync_and_same_tick_revision_win_preserve_c19"
        Evidence = "Stale Route Arrival"
    },
    @{
        Gate = "C-20"
        Package = "aon-sim"
        TargetArguments = @("--test", "command_ordering")
        TestName = "c20_same_tick_order_accepts_the_lower_ordinal_gate_only"
        Evidence = "Same-tick Command Ordering"
    },
    @{
        Gate = "C-25"
        Package = "aon-app"
        TargetArguments = @("--test", "host_laboratory")
        TestName = "paused_edit_single_step_matches_direct_core_and_reset_starts_a_new_session"
        Evidence = "Laboratory Edit Equivalence"
    }
)

$architectureTests = @(
    @{
        Gate = "Architecture"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "identity::tests::removal_leaves_a_tombstone_and_ids_are_never_reused"
        Evidence = "Stable EntityId tombstones and non-reuse"
    },
    @{
        Gate = "Architecture"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "simulation::tests::explicit_twelve_phase_sequence_is_total_ordered_and_enforced_by_step"
        Evidence = "explicit 12-Phase Tick order"
    },
    @{
        Gate = "Architecture"
        Package = "aon-sim"
        TargetArguments = @("--test", "event_calendar")
        TestName = "event_key_order_uses_every_field_in_the_declared_order"
        Evidence = "event-based Signal canonical ordering"
    },
    @{
        Gate = "Architecture"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "signal::tests::driver_revision_advances_only_for_a_real_sample_change"
        Evidence = "Driver Revision progression"
    },
    @{
        Gate = "Architecture"
        Package = "aon-sim"
        TargetArguments = @("--lib")
        TestName = "path_certificate::tests::consumption_leaves_a_tombstone_and_never_reuses_the_id"
        Evidence = "Path Certificate allocation and consumption"
    },
    @{
        Gate = "Architecture"
        Package = "aon-sim"
        TargetArguments = @("--test", "contract_profiles")
        TestName = "declared_contract_hash_mismatch_is_rejected_by_simulation_new"
        Evidence = "immutable Simulation Contract validation"
    },
    @{
        Gate = "Architecture"
        Package = "aon-headless"
        TargetArguments = @("--test", "mobility_retained_replay")
        TestName = "current_input_only_replay_is_canonical_and_resumes_after_the_matched_set_release"
        Evidence = "paired current-input/retained-state headless A/B control"
    },
    @{
        Gate = "Architecture"
        Package = "aon-app"
        TargetArguments = @("--lib")
        TestName = "tests::both_stage0_product_designs_open_at_the_matched_ready_checkpoint"
        Evidence = "both product designs load at the matched ready checkpoint"
    },
    @{
        Gate = "Architecture"
        Package = "aon-app"
        TargetArguments = @("--lib")
        TestName = "tests::stage0_ab_designs_share_the_exact_set_timeline_but_diverge_after_release"
        Evidence = "matched SET input timeline and retained-state divergence"
    },
    @{
        Gate = "Architecture"
        Package = "aon-app"
        TargetArguments = @("--lib")
        TestName = "tests::both_stage0_ab_documents_fit_and_label_the_default_window_at_critical_ticks"
        Evidence = "both A/B waveform documents fit and label every critical Tick"
    },
    @{
        Gate = "Architecture"
        Package = "aon-app"
        TargetArguments = @("--lib")
        TestName = "tests::f5_and_f6_replace_the_session_and_reset_to_the_matched_ready_tick"
        Evidence = "F5/F6 direct-play session switching resets the full probe state"
    },
    @{
        Gate = "Architecture"
        Package = "aon-app"
        TargetArguments = @("--lib")
        TestName = "native_editor::tests::selected_wire_gate_and_probe_controls_resolve_snapshot_identities"
        Evidence = "profile-strength native HIGH/X drive reaches Core"
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
    foreach ($testCase in $architectureTests) {
        Invoke-ExactCargoTest `
            -Gate $testCase.Gate `
            -Package $testCase.Package `
            -TargetArguments $testCase.TargetArguments `
            -TestName $testCase.TestName `
            -Evidence $testCase.Evidence
    }
    Assert-CanonicalCoreDependencyBoundary
    Write-Host ""
    Write-Host "Stage 0 technical gate passed: 12 required conformance IDs, 13 conformance tests, 12 architecture tests, and the canonical core dependency boundary."
}
finally {
    Pop-Location
}
