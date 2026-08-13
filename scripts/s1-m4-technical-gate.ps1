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

function Assert-ExactTargetTests {
    param(
        [Parameter(Mandatory)] [string] $Package,
        [Parameter(Mandatory)] [string[]] $TargetArguments,
        [Parameter(Mandatory)] [string[]] $ExpectedTests
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

function Invoke-S1M4GeneratorEvidence {
    $expectedScenarioHash = "a9770d7afc466087664f44846d65f56e93d479738705975c10ab6527b59817cd"
    $arguments = @(
        "run",
        "-p", "aon-sim",
        "--locked",
        "--offline",
        "--example", "generate_s1m4_replay"
    )
    $output = Invoke-CargoChecked `
        -Arguments $arguments `
        -Description "Gate 3,5,12,13,14 - deterministic non-writing S1-M4 generator assertions"
    $lines = @($output | ForEach-Object { $_.ToString() })
    $hashLines = @(
        $lines | Where-Object {
            $_ -eq "scenarioHash=$expectedScenarioHash"
        }
    )
    if ($hashLines.Count -ne 1) {
        throw "S1-M4 generator must run all exact trace assertions twice and print exactly the frozen Scenario hash $expectedScenarioHash"
    }
}

$profileTests = @(
    "every_new_v5_field_is_independently_semantic_hash_sensitive",
    "every_positive_v5_scalar_and_nested_kind_rejects_zero",
    "rational_sign_unit_interval_and_temperature_boundaries_are_exact",
    "retained_v2_v3_v4_semantic_hashes_remain_exact",
    "schema_matrix_forbids_new_sections_before_v5_and_requires_all_v5_sections",
    "unsupported_schema_and_supported_body_validation_precedence_are_exact",
    "v4_is_still_currently_valid_and_forbids_v5_sections",
    "v5_fixture_is_strict_valid_and_matches_the_exact_reference_constructor",
    "v5_json_rejects_unknown_duplicate_float_and_zero_denominator"
)

$scenarioTests = @(
    "package_decode_enforces_v5_integrity_and_physical_quantum_coherence",
    "public_reference_v5_matches_the_artifact_used_by_scenario_v4",
    "selected_schema_has_an_exact_feature_and_world_shape",
    "v4_normalizes_the_complete_enemy_key_and_hashes_the_exact_v4_stream",
    "v4_rejects_empty_invalid_overflowing_and_duplicate_enemies"
)

$constructionTests = @(
    "all_target_kinds_use_exact_one_final_ceiling_work_laws",
    "canonical_store_rejects_incoherent_progress_and_duplicate_site_ids",
    "construction_demand_is_mobile_owned_track_attached_and_grant_reuses_scale_work",
    "duplicate_contributions_are_rejected_independent_of_input_order",
    "malformed_or_zero_work_inputs_fail_with_typed_errors",
    "site_store_sorts_tombstones_and_applies_multi_builder_work_atomically",
    "tag8_payload_reuses_each_direct_target_encoding_after_kind_tag"
)

$runtimeTests = @(
    "all_four_sites_complete_in_phase11_and_activate_with_fresh_ids_next_phase0",
    "c09_pending_wire_is_usable_then_all_surfaces_leave_together_and_arrival_stales",
    "c10_runtime_conserves_exact_grant_and_orders_equal_enemy_contacts",
    "fatal_core_tick_commits_terminal_hash_and_later_steps_are_strictly_read_only",
    "live_wire_report_exposes_partial_and_source_less_actual_grants",
    "source_less_construction_reports_positive_nominal_zero_grant_and_no_progress",
    "wire_site_uses_a_round_capsule_at_the_mobile_aabb_corner"
)

$presenterTests = @(
    "inspector_exposes_run_enemy_site_damage_and_build_state",
    "network_view_draws_and_picks_enemy_and_site_from_canonical_positions",
    "v5_mobile_build_uses_the_unused_corner_as_a_host_only_visual_anchor"
)

$appTerminalTests = @(
    "bevy_and_laboratory_accept_an_exact_terminal_boundary",
    "bevy_and_laboratory_report_the_same_typed_later_boundary_error",
    "bevy_and_laboratory_verify_the_terminal_checkpoint_before_the_later_boundary",
    "interactive_laboratory_keeps_simulation_run_ended_semantics"
)

$headlessTerminalTests = @(
    "exact_terminal_boundary_returns_the_fatal_report_and_hash",
    "later_checkpoint_command_or_world_input_uses_the_same_typed_boundary_error",
    "terminal_checkpoint_divergence_precedes_a_later_boundary_error"
)

$headlessRetainedTests = @(
    "retained_s1m4_reports_pin_construction_c10_c09_and_terminal_facts",
    "retained_s1m4_set_is_exact_headlessly"
)

$appRetainedTests = @(
    "retained_s1m4_complete_reports_and_v7_hashes_match_headless_and_bevy"
)

$fuzzRegressionTests = @(
    "command_regression_corpus_replays_stateless_and_stateful_targets_without_panics",
    "decoder_regression_corpus_never_panics",
    "experiment_regression_corpus_never_panics_and_preserves_exact_outcome",
    "geometry_regression_corpus_never_panics_and_is_quantized",
    "mobility_runtime_regression_corpus_replays_without_panics_or_silent_run_errors",
    "module_regression_corpus_never_panics_and_preserves_exact_outcome",
    "replay_regression_corpus_never_panics_and_preserves_acceptance_class",
    "retained_mobility_case_reaches_verified_s0_m7_paths",
    "retained_signal_case_reaches_s0_m3_fuzz_completion_paths",
    "retained_stateful_case_reaches_effective_bind_remove_tombstone_and_wrong_kind",
    "retained_topology_case_reaches_verified_s0_m4_paths",
    "s1m2_scenario_and_balance_artifacts_reach_the_bounded_decoder_without_panics",
    "s1m3_capacity_support_corpus_is_bounded_exact_and_property_complete",
    "s1m3_scenario_balance_and_replay_artifacts_reach_bounded_strict_decoders",
    "s1m4_hostile_frame_overlap_changes_only_sense_on_an_armed_live_wire",
    "s1m4_heat_integrates_before_exact_next_tick_thermal_damage_and_pending",
    "s1m4_kernel_corpus_is_bounded_exact_and_property_complete",
    "s1m4_mutual_lethal_runtime_completes_both_then_sorts_next_phase0_destruction",
    "s1m4_scenario_balance_and_all_replays_reach_bounded_strict_decoders",
    "s1m4_command_tag8_corpus_reaches_all_target_kinds_and_both_encoders",
    "s1m4_stateful_runtime_corpus_reaches_construction_c09_and_run_end",
    "signal_runtime_regression_corpus_replays_without_panics_or_silent_run_errors",
    "topology_runtime_regression_corpus_replays_without_panics_or_silent_run_errors"
)

$requiredTests = @(
    New-TestEvidence @(2) "aon-sim" @("--test", "s1m4_profile") "v5_fixture_is_strict_valid_and_matches_the_exact_reference_constructor" "Balance v5 fixture, reference constructor, and hash are exact"
    New-TestEvidence @(2) "aon-sim" @("--lib") "profile::tests::balance_v5_encoder_is_the_schema_tagged_retained_v4_stream_plus_exact_frozen_suffix" "Balance v5 encoding is the schema-tagged retained v4 stream plus the exact frozen suffix"
    New-TestEvidence @(1, 2) "aon-sim" @("--test", "s1m4_profile") "retained_v2_v3_v4_semantic_hashes_remain_exact" "retained Balance hashes and encodings do not migrate"
    New-TestEvidence @(1, 2) "aon-sim" @("--test", "s1m4_profile") "schema_matrix_forbids_new_sections_before_v5_and_requires_all_v5_sections" "Balance schema matrix is fail closed"
    New-TestEvidence @(2) "aon-sim" @("--test", "s1m4_profile") "every_new_v5_field_is_independently_semantic_hash_sensitive" "every v5 field reaches semantic hashing"
    New-TestEvidence @(2, 15) "aon-sim" @("--test", "s1m4_profile") "every_positive_v5_scalar_and_nested_kind_rejects_zero" "all positive scalar and kind fields reject zero"
    New-TestEvidence @(2, 15) "aon-sim" @("--test", "s1m4_profile") "rational_sign_unit_interval_and_temperature_boundaries_are_exact" "rational and temperature boundaries are typed and exact"
    New-TestEvidence @(2, 15) "aon-sim" @("--test", "s1m4_profile") "v5_json_rejects_unknown_duplicate_float_and_zero_denominator" "strict v5 JSON rejects all forbidden categories"
    New-TestEvidence @(2, 15) "aon-sim" @("--test", "s1m4_profile") "unsupported_schema_and_supported_body_validation_precedence_are_exact" "Balance error precedence is exact"
    New-TestEvidence @(1, 2) "aon-sim" @("--test", "s1m4_profile") "v4_is_still_currently_valid_and_forbids_v5_sections" "retained v4 acceptance and forbidden sections remain exact"

    New-TestEvidence @(3) "aon-sim" @("--test", "s1m4_scenario") "v4_normalizes_the_complete_enemy_key_and_hashes_the_exact_v4_stream" "Scenario v4 Enemy normalization and canonical stream are exact"
    New-TestEvidence @(1, 3, 15) "aon-sim" @("--test", "s1m4_scenario") "selected_schema_has_an_exact_feature_and_world_shape" "selected Scenario versions keep strict independent shapes"
    New-TestEvidence @(3, 15) "aon-sim" @("--test", "s1m4_scenario") "v4_rejects_empty_invalid_overflowing_and_duplicate_enemies" "Enemy invalid, duplicate, and overflow cases fail closed"
    New-TestEvidence @(3) "aon-sim" @("--test", "s1m4_scenario") "package_decode_enforces_v5_integrity_and_physical_quantum_coherence" "Scenario/Profile coherence and precedence are exact"
    New-TestEvidence @(2, 3) "aon-sim" @("--test", "s1m4_scenario") "public_reference_v5_matches_the_artifact_used_by_scenario_v4" "Scenario v4 consumes the authoritative Balance v5 artifact"
    New-TestEvidence @(3, 15) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_package_rejects_empty_nonpositive_overflowing_and_duplicate_enemies" "direct packages fail closed on empty, invalid, overflowing, and complete-duplicate Enemy inputs"
    New-TestEvidence @(3, 15) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_package_rejects_nonpositive_and_duplicate_power_sources" "direct packages reject zero-generation and duplicate-position Sources"
    New-TestEvidence @(3, 15) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_package_requires_the_complete_eight_feature_set" "direct packages require the complete S1-M4 feature set"
    New-TestEvidence @(3, 15) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_package_rejects_every_off_quantum_enemy_coordinate" "direct packages reject every off-quantum Enemy geometry field"
    New-TestEvidence @(3) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_enemy_permutations_allocate_the_same_sorted_ids" "Core, normalized Sources, and complete-key-normalized Enemies allocate exact sorted identities"
    New-TestEvidence @(3, 15) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_quantum_error_precedes_profile_integrity_mismatch" "direct-package validation precedence reports geometry quantum before profile-integrity mismatch"
    New-TestEvidence @(3, 15) "aon-sim" @("--lib") "simulation::tests::direct_s1m4_world_faults_precede_numeric_hash_and_profile_body_faults" "direct-package world faults precede Numeric hash mismatch and malformed Balance body faults"

    New-TestEvidence @(5) "aon-sim" @("--test", "s1m4_construction") "all_target_kinds_use_exact_one_final_ceiling_work_laws" "all four Work laws are exact, including 1 WU = 3 less than WU + 1 = 4 equal to redundant-long = 4"
    New-TestEvidence @(5) "aon-sim" @("--test", "s1m4_construction") "malformed_or_zero_work_inputs_fail_with_typed_errors" "malformed Work inputs, full-domain length overflow, and u64 Work conversion boundaries fail with exact typed errors"
    New-TestEvidence @(6) "aon-sim" @("--test", "s1m4_construction") "construction_demand_is_mobile_owned_track_attached_and_grant_reuses_scale_work" "tag-12 demand ownership and retained scale_work are exact"
    New-TestEvidence @(5, 6, 11, 14) "aon-sim" @("--test", "s1m4_construction") "site_store_sorts_tombstones_and_applies_multi_builder_work_atomically" "Site progress is stable, conservative, and atomic"
    New-TestEvidence @(5, 14, 15) "aon-sim" @("--test", "s1m4_construction") "duplicate_contributions_are_rejected_independent_of_input_order" "duplicate Work is rejected independently of input order"
    New-TestEvidence @(5, 15) "aon-sim" @("--test", "s1m4_construction") "canonical_store_rejects_incoherent_progress_and_duplicate_site_ids" "Site store invariants fail closed"
    New-TestEvidence @(15) "aon-sim" @("--test", "s1m4_construction") "tag8_payload_reuses_each_direct_target_encoding_after_kind_tag" "Command v1 tag 8 appends exact target tags 0 through 3"

    New-TestEvidence @(5, 6) "aon-sim" @("--test", "s1m4_runtime_evidence") "all_four_sites_complete_in_phase11_and_activate_with_fresh_ids_next_phase0" "all four Sites complete then activate next Phase 0 with fresh identity"
    New-TestEvidence @(6) "aon-sim" @("--test", "s1m4_runtime_evidence") "source_less_construction_reports_positive_nominal_zero_grant_and_no_progress" "source-less Construction retains positive requested and nominal Work while ratio, grant, application, and progress remain zero"
    New-TestEvidence @(7, 8, 9, 10, 11, 14) "aon-sim" @("--test", "s1m4_runtime_evidence") "c10_runtime_conserves_exact_grant_and_orders_equal_enemy_contacts" "C-10 exact 20,2,1,1 conservation flows through production phases"
    New-TestEvidence @(7, 9) "aon-sim" @("--test", "s1m4_runtime_evidence") "live_wire_report_exposes_partial_and_source_less_actual_grants" "partial and source-less Live Wire grants create no Energy"
    New-TestEvidence @(7, 8, 11, 12, 14) "aon-sim" @("--test", "s1m4_runtime_evidence") "c09_pending_wire_is_usable_then_all_surfaces_leave_together_and_arrival_stales" "C-09 retains the Wire through the lethal Tick and removes every surface before stale discard"
    New-TestEvidence @(13, 14) "aon-sim" @("--test", "s1m4_runtime_evidence") "fatal_core_tick_commits_terminal_hash_and_later_steps_are_strictly_read_only" "terminal Tick commits and all later steps are mutation-free RunEnded"
    New-TestEvidence @(5) "aon-sim" @("--test", "s1m4_runtime_evidence") "wire_site_uses_a_round_capsule_at_the_mobile_aabb_corner" "Wire Site interaction uses the exact rounded capsule"

    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::state_encoding_v7_has_exact_contract_tick_revision_and_identity_order" "State V7 root contract and order are exact"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::v7_optional_damage_state_has_exact_presence_integrity_and_heat_bytes" "V7 optional damage bytes and sensitivity are exact"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::v7_enemy_store_has_exact_sorted_rows_and_every_field_is_sensitive" "V7 Enemy store order and field sensitivity are exact"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::v7_construction_site_and_run_status_have_exact_bytes_and_sensitivity" "V7 Site and RunStatus bytes are exact"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::v7_pending_destruction_store_is_counted_and_entity_id_ordered" "V7 pending-destruction store count and EntityId order are exact"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::v7_mobile_signal_row_appends_exact_optional_build_sink" "V7 Mobile signal row appends the exact optional BUILD sink"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::mobile_port_endpoint_has_exact_v7_bytes_for_all_control_sinks" "every Mobile control-sink endpoint has exact V7 bytes"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::main_core_power_enemy_v7_full_world_has_a_fixed_initial_golden" "full v4 world State V7 initial hash has an independent golden"
    New-TestEvidence @(4) "aon-sim" @("--lib") "canonical::tests::every_new_v7_root_store_reaches_the_full_state_hash" "every V7 store reaches the State hash"
    New-TestEvidence @(4) "aon-sim" @("--test", "bootstrap_simulation") "current_empty_state_v7_hash_has_a_golden_value" "Empty initial State V7 has a fixed external golden"
    New-TestEvidence @(4) "aon-sim" @("--test", "s1m2_state_replay") "retained_main_core_power_initial_state_v7_hash_has_a_fixed_golden" "retained MainCorePower initial State V7 has a fixed external golden"

    New-TestEvidence @(7, 15) "aon-sim" @("--lib") "contact::tests::live_wire_demand_uses_one_final_ceil_and_reference_strength_length" "Live demand uses one ceiling and checked aggregate strength"
    New-TestEvidence @(8, 15) "aon-sim" @("--lib") "contact::tests::swept_path_crossing_tangency_static_contact_and_reversal_are_exact" "contact geometry is closed, swept, exact, and reversal stable"
    New-TestEvidence @(8, 15) "aon-sim" @("--lib") "contact::tests::full_i64_domain_is_widened_and_symmetric" "contact geometry covers the full i64 boundary without float"
    New-TestEvidence @(8, 15) "aon-sim" @("--lib") "contact::tests::polyline_segment_order_and_bends_do_not_change_contact" "contact is invariant to polyline segment order and equivalent bends"
    New-TestEvidence @(9) "aon-sim" @("--lib") "contact::tests::c10_equal_weights_conserve_granted_energy_and_leak_to_heat" "C-10 kernel returns exact 5,5,10"
    New-TestEvidence @(9) "aon-sim" @("--lib") "contact::tests::odd_remainder_goes_to_the_lowest_enemy_id" "odd target remainder goes to the lowest Enemy ID"
    New-TestEvidence @(9, 15) "aon-sim" @("--lib") "contact::tests::invalid_candidates_denominator_and_u128_overflow_are_typed" "contact duplicates, zero weights, and overflow fail closed"
    New-TestEvidence @(9) "aon-sim" @("--lib") "contact::tests::zero_grant_and_no_contacts_have_explicit_conservative_results" "zero grant and empty contact sets return explicit conservative zero absorption and exact Wire Heat"
    New-TestEvidence @(10, 15) "aon-sim" @("--lib") "thermal_damage::tests::heat_integration_requires_positive_canonical_unique_rows" "Heat integration requires positive unique sorted rows"
    New-TestEvidence @(10, 11) "aon-sim" @("--lib") "thermal_damage::tests::electrical_and_phase1_thermal_damage_reduce_simultaneously" "Electrical floor and phase-1 Thermal damage resolve together"
    New-TestEvidence @(10, 11) "aon-sim" @("--lib") "thermal_damage::tests::all_seven_thermal_object_kind_selectors_use_their_exact_profile_fields" "all seven ThermalObjectKind tags select their exact thermal-capacity and electrical-tolerance fields"
    New-TestEvidence @(11, 15) "aon-sim" @("--lib") "thermal_damage::tests::exposure_order_duplicate_target_and_temperature_faults_are_typed" "damage input order and boundary faults are typed"
    New-TestEvidence @(10) "aon-sim" @("--lib") "signal::tests::build_identity_is_v5_only_and_cancelled_heat_is_consumed_once" "BUILD is v5-only and cancelled switching Heat is consumed once"
    New-TestEvidence @(10) "aon-sim" @("--lib") "simulation::tests::phase9_keeps_retained_power_heat_separate_from_live_wire_remainder" "retained Power Heat and Live remainder integrate exactly once"
    New-TestEvidence @(11, 15) "aon-sim" @("--lib") "simulation::tests::phase10_rejects_orphan_exposure_before_mutating_canonical_state" "orphan exposure fails atomically"
    New-TestEvidence @(5, 6, 11) "aon-sim" @("--lib") "structural::tests::construction_site_reserves_geometry_and_activates_with_fresh_damageable_id" "Site geometry blocks duplicate Site and overlapping active placement without consuming identity, then activation uses a fresh damageable ID"
    New-TestEvidence @(5, 11) "aon-sim" @("--lib") "structural::tests::phase0_remove_entity_cancels_a_live_site_and_preserves_its_tombstone" "Phase-0 RemoveEntity cancels a live Site, preserves its identity tombstone, removes its reservation, and never reuses the ID"
    New-TestEvidence @(5) "aon-sim" @("--lib") "structural::tests::construction_work_overflow_rejects_phase0_atomically_before_identity_or_site_mutation" "Construction arithmetic overflow rejects the Phase-0 candidate atomically before identity or Site mutation"
    New-TestEvidence @(11) "aon-sim" @("--lib") "structural::tests::all_five_structural_kinds_receive_exact_v5_integrity_and_zero_heat" "Gate, Wire, Junction, Fixed Substrate, and Mobile receive their exact v5 initial Integrity and zero Heat"
    New-TestEvidence @(5, 11) "aon-sim" @("--lib") "structural::tests::site_dependency_blocks_player_removal_and_damage_cascade_cancels_site" "player reservation and damage cascade paths remain distinct and exact"
    New-TestEvidence @(11) "aon-sim" @("--lib") "structural::tests::destruction_scc_order_is_dependent_first_and_cycle_stable" "destruction SCC order is deterministic"

    New-TestEvidence @(8, 15) "aon-sim" @("--lib") "simulation::tests::mobile_wire_sensing_uses_world_geometry_for_enemy_and_hostile" "nonzero-origin Mobile Wire sensing transforms local geometry to world"
    New-TestEvidence @(5, 15) "aon-sim" @("--lib") "simulation::tests::mobile_construction_targets_use_world_geometry_for_all_routed_kinds" "all Mobile-routed Site kinds use world geometry"
    New-TestEvidence @(8, 15) "aon-sim" @("--lib") "simulation::tests::mobile_wire_contact_and_enemy_attack_use_world_geometry_not_local_ghost" "Mobile Wire contact and attack reject local-space ghosts"
    New-TestEvidence @(14) "aon-sim" @("--lib") "simulation::tests::construction_contact_damage_analyzer_is_sorted_and_read_only" "S1-M4 analyzer is sorted and immutable"
    New-TestEvidence @(1, 3, 15) "aon-sim" @("--lib") "simulation::tests::s1m4_feature_triad_is_rejected_by_retained_worlds_before_tick_zero" "retained worlds cannot fake-enable the S1-M4 feature triad"

    New-TestEvidence @(14) "aon-app" @("--test", "s1m4_presenter") "network_view_draws_and_picks_enemy_and_site_from_canonical_positions" "Bevy view consumes canonical Enemy and Site positions"
    New-TestEvidence @(14) "aon-app" @("--test", "s1m4_presenter") "v5_mobile_build_uses_the_unused_corner_as_a_host_only_visual_anchor" "BUILD rendering is host-only presentation"
    New-TestEvidence @(14) "aon-app" @("--test", "s1m4_presenter") "inspector_exposes_run_enemy_site_damage_and_build_state" "inspector exposes complete S1-M4 truth"
    New-TestEvidence @(13, 14) "aon-headless" @("--test", "s1m4_terminal_replay") "terminal_checkpoint_divergence_precedes_a_later_boundary_error" "Headless verifies terminal checkpoint before boundary mismatch"
    New-TestEvidence @(13, 14) "aon-headless" @("--test", "s1m4_terminal_replay") "exact_terminal_boundary_returns_the_fatal_report_and_hash" "Headless accepts the exact fatal boundary"
    New-TestEvidence @(13, 14) "aon-headless" @("--test", "s1m4_terminal_replay") "later_checkpoint_command_or_world_input_uses_the_same_typed_boundary_error" "Headless terminal boundary error is uniform"
    New-TestEvidence @(13, 14) "aon-app" @("--test", "s1m4_terminal_replay") "bevy_and_laboratory_verify_the_terminal_checkpoint_before_the_later_boundary" "Bevy and Laboratory share terminal precedence"
    New-TestEvidence @(13, 14) "aon-app" @("--test", "s1m4_terminal_replay") "bevy_and_laboratory_accept_an_exact_terminal_boundary" "Bevy and Laboratory accept the exact terminal boundary"
    New-TestEvidence @(13, 14) "aon-app" @("--test", "s1m4_terminal_replay") "bevy_and_laboratory_report_the_same_typed_later_boundary_error" "Bevy and Laboratory share the typed later-boundary error"
    New-TestEvidence @(13, 14) "aon-app" @("--test", "s1m4_terminal_replay") "interactive_laboratory_keeps_simulation_run_ended_semantics" "interactive Laboratory preserves Simulation RunEnded semantics"

    New-TestEvidence @(1, 4, 14, 15) "aon-headless" @("--test", "s1m4_retained_replay") "retained_s1m4_set_is_exact_headlessly" "all five authoritative Replay v2 artifacts retain exact Scenario identity, Tick count, initial V7 hash, final V7 hash, and complete reports"
    New-TestEvidence @(5, 6, 9, 12, 13, 14) "aon-headless" @("--test", "s1m4_retained_replay") "retained_s1m4_reports_pin_construction_c10_c09_and_terminal_facts" "retained reports pin multi-builder Work, four-target activation, C-10 allocation, C-09 pending and stale arrival, and terminal RunStatus"
    New-TestEvidence @(4, 14, 15) "aon-app" @("--test", "s1m4_replay_host") "retained_s1m4_complete_reports_and_v7_hashes_match_headless_and_bevy" "all five strict retained Replays produce complete identical reports and V7 checkpoints in headless and both Bevy host schedules"

    New-TestEvidence @(5, 7, 9, 10, 11, 15) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_kernel_corpus_is_bounded_exact_and_property_complete" "bounded independent Work, demand, allocation, Heat, damage, order, and numeric oracles pass"
    New-TestEvidence @(5, 6, 8, 12, 13, 15) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_stateful_runtime_corpus_reaches_construction_c09_and_run_end" "bounded public runtime covers activation, C-09 removal and stale arrival, and RunEnd"
    New-TestEvidence @(8, 11, 14) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_mutual_lethal_runtime_completes_both_then_sorts_next_phase0_destruction" "actual Live Wire contact and Enemy attack are mutually lethal in one Tick, then both destructions are sorted next Phase 0"
    New-TestEvidence @(8, 10, 11, 14) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_hostile_frame_overlap_changes_only_sense_on_an_armed_live_wire" "HostileFrame changes only Sense on an armed HIGH Wire while positive Live grant and remainder, zero contact or damage, and canonical Enemy state match the no-Hostile control"
    New-TestEvidence @(10, 11, 14) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_heat_integrates_before_exact_next_tick_thermal_damage_and_pending" "Tick t integrates Live Heat without damage, then Tick t+1 uses that exact Phase-1 temperature for lethal Thermal damage and pending destruction"
    New-TestEvidence @(2, 3, 15) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_scenario_balance_and_all_replays_reach_bounded_strict_decoders" "authoritative Balance v5, Scenario v4, and all five Replay v2 artifacts reach bounded strict deterministic decoders"
    New-TestEvidence @(15) "aon-fuzz-harness" @("--test", "regression_corpus") "s1m4_command_tag8_corpus_reaches_all_target_kinds_and_both_encoders" "bounded retained Command corpus reaches tag 8 with target tags 0 through 3 through both canonical encoders and public Simulation"
    New-TestEvidence @(15) "aon-fuzz-harness" @("--test", "cli_modes") "all_mode_invokes_every_decoder_target_including_replay" "all-mode invokes strict artifact targets plus the S1-M4 kernel and runtime lanes"
    New-TestEvidence @(15) "aon-sim" @("--test", "artifact_stage_features") "s1m2_features_require_the_power_world_and_later_features_remain_unsupported" "Relay, Payload, and Radiation remain unsupported later scope"
)

$staticEvidence = @(
    @{
        Gates = @(3, 5, 12, 13, 14)
        Evidence = "non-writing generator has no randomness dependency or input, double-builds equal bytes, matches every checked-in artifact, and pins Construction/C-09/C-10/terminal facts"
    },
    @{
        Gates = @(1)
        Evidence = "CI retains prior technical gates and invokes the S1-M4 gate once on Linux pwsh and once on Windows native; Gate 16 remains a post-commit clean-clone closure gate"
    }
)

$allEvidence = @($requiredTests) + @($staticEvidence)
$coveredGates = @(
    $allEvidence |
        ForEach-Object { $_.Gates } |
        ForEach-Object { [int] $_ } |
        Sort-Object -Unique
)
$gateDifference = @(Compare-Object -ReferenceObject @(1..15) -DifferenceObject $coveredGates)
if ($gateDifference.Count -ne 0) {
    throw "S1-M4 technical gate table must cover every in-tree executable Gate 1 through Gate 15; Gate 16 requires the post-commit clean-clone procedure"
}

$duplicateTests = @(
    $requiredTests |
        ForEach-Object { $_.Package + "|" + ($_.TargetArguments -join "|") + "|" + $_.TestName } |
        Group-Object |
        Where-Object { $_.Count -ne 1 }
)
if ($duplicateTests.Count -ne 0) {
    throw "S1-M4 technical gate table contains duplicate exact test entries"
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    Assert-ExactTargetTests "aon-sim" @("--test", "s1m4_profile") $profileTests
    Assert-ExactTargetTests "aon-sim" @("--test", "s1m4_scenario") $scenarioTests
    Assert-ExactTargetTests "aon-sim" @("--test", "s1m4_construction") $constructionTests
    Assert-ExactTargetTests "aon-sim" @("--test", "s1m4_runtime_evidence") $runtimeTests
    Assert-ExactTargetTests "aon-app" @("--test", "s1m4_presenter") $presenterTests
    Assert-ExactTargetTests "aon-app" @("--test", "s1m4_terminal_replay") $appTerminalTests
    Assert-ExactTargetTests "aon-headless" @("--test", "s1m4_terminal_replay") $headlessTerminalTests
    Assert-ExactTargetTests "aon-headless" @("--test", "s1m4_retained_replay") $headlessRetainedTests
    Assert-ExactTargetTests "aon-app" @("--test", "s1m4_replay_host") $appRetainedTests
    Assert-ExactTargetTests "aon-fuzz-harness" @("--test", "regression_corpus") $fuzzRegressionTests

    $ciText = Get-Content -Raw ".github/workflows/ci.yml"
    foreach ($prior in @(
        "stage0-technical-gate.ps1",
        "s1-m0-technical-gate.ps1",
        "s1-m1-technical-gate.ps1",
        "s1-m2-technical-gate.ps1",
        "s1-m3-technical-gate.ps1"
    )) {
        if ([Regex]::Matches($ciText, [Regex]::Escape("./scripts/$prior")).Count -ne 2) {
            throw "CI must retain exactly two native/pwsh references to $prior"
        }
    }
    if ([Regex]::Matches($ciText, [Regex]::Escape("./scripts/s1-m4-technical-gate.ps1")).Count -ne 2) {
        throw "CI must reference the S1-M4 technical gate exactly twice"
    }
    if ($ciText -notmatch '(?ms)runs-on: ubuntu-24\.04.*?name: S1-M4 technical gate\s+shell: pwsh\s+run: \./scripts/s1-m4-technical-gate\.ps1') {
        throw "CI must run the S1-M4 gate through pwsh on Linux"
    }
    if ($ciText -notmatch '(?ms)runs-on: windows-latest.*?name: Native S1-M4 technical gate\s+shell: pwsh\s+run: \./scripts/s1-m4-technical-gate\.ps1') {
        throw "CI must run the S1-M4 gate on Windows native"
    }

    foreach ($testCase in $requiredTests) {
        Invoke-ExactCargoTest `
            -Gates $testCase.Gates `
            -Package $testCase.Package `
            -TargetArguments $testCase.TargetArguments `
            -TestName $testCase.TestName `
            -Evidence $testCase.Evidence
    }
    Invoke-S1M4GeneratorEvidence

    Write-Host ""
    Write-Host "S1-M4 technical gate passed: $($requiredTests.Count) unique exact tests and $($staticEvidence.Count) executable/static invariants cover Gates 1-15; Gate 16 CI wiring is exact and clean-clone closure remains external."
}
finally {
    Pop-Location
}
