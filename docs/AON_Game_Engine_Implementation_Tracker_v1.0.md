# A/O/N — Game Engine Implementation Tracker

**Version:** v1.0
**Source baseline:** PRD v1.0 GO / SSS v1.0 Draft / TRD v1.0 Draft
**Completion boundary:** TRD §40.4 MVP Definition of Done

This tracker distinguishes an implemented system from an experimentally proven product gate.
The game engine is complete only after every milestone, conformance test, regression gate,
cross-host/replay gate, and MVP scenario below has authoritative evidence.

## Stage 0 — Emergence Probe

- [x] S0-M0 — Bootstrap
- [x] S0-M1 — Contract / Numeric / Identity
- [x] S0-M2 — Command / Geometry / Structural Phase
- [x] S0-M3 — Signal Topology / Event Runtime
- [x] S0-M4 — Topology Sync / Path Certificate
- [x] S0-M5 — Feedback / Replay
- [x] S0-M6 — Bevy ASCII Probe *(committed fresh clean-checkout and Windows-native smoke passed)*
- [x] S0-M7 — Mobility *(committed fresh clean-checkout, retained Replay, and Windows-native smoke passed)*
- [x] Stage 0 technical gate *(Windows-native `scripts/stage0-technical-gate.ps1` passed)*
- [x] Stage 0 product gate *(user direct-play A/B PASS recorded 2026-08-12)*

Required conformance: C-01, C-02, C-03, C-05, C-06, C-14, C-16, C-17,
C-18, C-19, C-20, C-25.

## Stage 1 — Capacity Economy Probe

- [x] S1-M0 — Physical Scale Experiment Baseline *(committed fresh clean-checkout and Windows-native gate passed)*
- [x] S1-M1 — Main Core / Capacity Accounting *(committed fresh clean-checkout and Windows-native gate passed)*
- [x] S1-M2 — Sensing / Power / Brownout *(committed fresh clean-checkout and Windows-native gate passed)*
- [x] S1-M3 — Capacity Support Load *(committed fresh clean-checkout and Windows-native gate passed)*
- [x] S1-M4 — Construction / Contact / Damage *(committed fresh clean-checkout and Windows-native gate passed)*
- [ ] S1-M5 — Reference Architecture Fixture
- [ ] S1-M6 — Parameter Sweep
- [ ] Stage 1 technical gate
- [ ] Stage 1 product gate

Required conformance: C-07, C-08, C-09, C-10, C-21, C-22 plus every
Stage 0 regression.

## Stage 2 — Relay Expansion Probe

- [ ] S2-M0 — Relay World Fixture
- [ ] S2-M1 — Relay Store / Anchor Connectivity
- [ ] S2-M2 — Activation / Upkeep / Restart
- [ ] S2-M3 — Destruction / Reconstruction Site
- [ ] Stage 2 technical gate
- [ ] Stage 2 product gate

Required conformance: C-23, C-24 plus every Stage 0 and Stage 1 regression.

## MVP — Emergent Defense

- [ ] MVP-M0 — Payload / Transfer
- [ ] MVP-M1 — Full Reconstruction Loop
- [ ] MVP-M2 — Quartz
- [ ] MVP-M3 — Radiation
- [ ] MVP-M4 — Enemy Pressure Set
- [ ] MVP-M5 — Module Library
- [ ] MVP-M6 — Laboratory Expansion
- [ ] MVP core reconstruction scenario
- [ ] MVP technical gate
- [ ] MVP product gate / PRD V1–V11 evidence

Required conformance: C-11, C-12, C-13, C-15 plus every previous Stage test.
C-04 fan-out crossover remains a required semantic regression even though TRD §40 does not
repeat it in a Stage DoD list.

## Global verification

- [ ] C-01 through C-25 have executable fixtures and assertions
- [ ] Headless and Bevy produce the same per-Tick hashes
- [x] Golden replay and 100,000-Tick Stage 0 fixture pass
- [ ] Same replay passes on the supported cross-platform matrix
- [ ] Profile hash, state hash, module, replay, and migration compatibility are versioned
- [ ] Numeric and ordering properties pass deterministic property tests
- [ ] Artifact and command decoders have fuzz targets with a retained regression corpus
- [ ] Native probe UX and the MVP scenario pass manual runtime verification
- [ ] Clean checkout CI passes without warnings

## Completed slice — S0-M1

S0-M1 completed on 2026-08-11 at commit `4a0d02c`. Its acceptance evidence is:

- [x] `SimulationContract` validates all three canonical profile hashes
- [x] Numeric v1 and Stage 0 physical/balance profile artifacts load and validate
- [x] Same semantic profile content produces the same hash regardless of JSON formatting or ID
- [x] `floor_div`, `ceil_div_nonnegative`, and ties-to-even division pass edge/property cases
- [x] `ceil_isqrt`, segment/polyline length, cell coordinate, and quantization pass C-17
- [x] Stable `EntityId` allocation is monotonic and never reuses a destroyed ID
- [x] `ConnectionGeneration` increments with checked overflow
- [x] Canonical state encoding includes contract, Tick, topology revision, and identity state
- [x] Decoder/geometry fuzz harness replays its retained regression corpus without panic
- [x] All workspace and clean-checkout quality gates pass offline without warnings

## Completed slice — S0-M2

S0-M2 completed on 2026-08-12 at commit `f978b7e`. Its acceptance evidence is:

- [x] Command payload, port/node identity, created-ID return, duplicate ordinal, placement, and
  geometry boundary decisions are versioned
- [x] `CommandEnvelope` and Stage 0 structural command types replace the placeholder
- [x] Gate, Wire, Junction, and Fixed Substrate canonical no-compaction stores are implemented
- [x] Phase 0 validates commands in ordinal order on a clone and swaps only after fatal checks pass
- [x] Geometry quantum, routing pitch, overlap, crossing, support, endpoint, and Gate-port rules are
  enforced with exact full-range integer predicates
- [x] Accepted structural changes update connection generations once per Phase and topology
  revision once per topology-changing Phase
- [x] Stable rejection precedence produces deterministic command results without partial mutation
- [x] Single/multi-segment length, coordinate, Tick, EntityId, generation, and revision overflow
  paths roll back Tick, hash, topology, and allocation state
- [x] Canonical hash includes EntityId-ordered raw records and excludes SoA slot, capacity, arena
  range, and derived-cache layout
- [x] Stateful command fuzz mapping reaches effective bind/remove, tombstone, and wrong-kind paths;
  retained corpus replay fails on encoder disagreement or invariant errors
- [x] C-20, invalid-geometry no-panic, permutation, hash, strict Clippy, and clean-checkout offline
  gates pass

## Completed slice — S0-M3

S0-M3 completed on 2026-08-12. This completes only the static-topology signal/event slice; Stage 0
and the game engine remain incomplete.

- [x] S0-M3 signal/event representation and ordering decisions are versioned
- [x] Driver/Sink stores and Gate/Wire signal state are canonical
- [x] Signal adjacency and deterministic Driver-to-Sink routes compile from explicit bindings
- [x] Event Calendar orders DriverTransition and SignalArrival deterministically
- [x] Inertial generation tokens discard canceled arrivals and Sink resolution is deterministic
- [x] Scheduled events and signal state participate in canonical state hashing
- [x] C-01, C-02, C-03, deterministic command-stream, fuzz, and clean-checkout gates pass

The 19 completion gates in `docs/AON_S0_M3_Canonical_Decisions_v1.0.md` have the following
executable evidence:

1. Endpoint namespace independence, monotonic allocation, tombstones, and non-reuse:
   `crates/aon-sim/tests/signal_determinism.rs` and `crates/aon-sim/src/signal.rs`.
2. Same-batch predicted-ID rejection and next-Tick observed-ID acceptance:
   `crates/aon-sim/tests/signal_determinism.rs`.
3. Unknown/removed/wrong-kind rejection, same-value no-op, and ordinal-last coalescing:
   `crates/aon-sim/tests/signal_determinism.rs`.
4. Explicit-only connectivity, including distinct Free ends and Gate/Junction nodes:
   `crates/aon-sim/src/signal_topology.rs` and crossing regressions in
   `crates/aon-sim/tests/structural_geometry_commands.rs`.
5. Length/segment/path-key route ties independent of adjacency insertion order:
   `crates/aon-sim/src/signal_topology.rs`.
6. Zero-Tick local and positive superlinear physical delay:
   `crates/aon-sim/src/signal_topology.rs` plus C-01/C-03.
7. Checked load, fanout, delay, energy, due-Tick, and generation whole-Tick rollback, plus the
   valid-Driver accumulator upper bound and defensive no-mutation check:
   `crates/aon-sim/tests/signal_overflow.rs`, `crates/aon-sim/src/simulation.rs`,
   `crates/aon-sim/src/signal.rs`, and `crates/aon-sim/src/signal_topology.rs`.
8. Permutation-invariant Low/High/X multi-driver resolution and once-per-dirty-Sink accounting:
   `crates/aon-sim/src/signal.rs` and `crates/aon-sim/tests/signal_conformance.rs`.
9. Inertial replacement/cancel, harmless stale events, and exact canceled heat: C-02 in
   `crates/aon-sim/tests/signal_conformance.rs`.
10. One-Tick transport pulse preservation: C-03 in
    `crates/aon-sim/tests/signal_conformance.rs`.
11. Unchanged samples emit no transition or arrival:
    `crates/aon-sim/tests/signal_conformance.rs`.
12. Event staging/insertion permutations produce canonical calendars and hashes:
    `crates/aon-sim/tests/event_calendar.rs` and `crates/aon-sim/src/canonical.rs`.
13. Signal/event hash sensitivity and non-canonical layout/cache exclusions:
    `crates/aon-sim/src/canonical.rs`.
14. C-01 proves NOT `t=1` and 8-WU downstream arrival `t=4` in
    `crates/aon-sim/tests/signal_conformance.rs`.
15. C-02 proves a two-Tick pulse is filtered by delay three with exact canceled heat in
    `crates/aon-sim/tests/signal_conformance.rs`.
16. C-03 proves a one-Tick pulse arrives exactly five Ticks later in
    `crates/aon-sim/tests/signal_conformance.rs`.
17. Reversed equivalent public command batches preserve reports, observations, and per-Tick hashes:
    `crates/aon-sim/tests/signal_determinism.rs`.
18. Stateful signal fuzz and retained cases cover valid/stale/wrong-kind/event/checked-arithmetic
    paths:
    `crates/aon-fuzz-harness/src/lib.rs`, `crates/aon-fuzz-harness/tests/regression_corpus.rs`, and
    `crates/aon-fuzz-harness/corpus/signal-runtime/`.
19. Workspace and clean-checkout verification uses:
    `cargo fmt --all -- --check`,
    `cargo metadata --format-version 1 --no-deps --locked --offline`,
    `cargo check --workspace --all-targets --locked --offline`,
    `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`,
    `cargo test --workspace --locked --offline`, and
    `cargo tree -p aon-sim --edges all --prefix none --locked --offline` with no Bevy/winit/wgpu
    dependency in the canonical core.

## Completed slice — S0-M4

S0-M4 implements Route Diff, Driver Revision, TopologySyncArrival, revision-aware Sink Slots,
PathCertificateArena, stamped connection-generation validation, and stale-arrival rejection after
an in-flight topology edit. Implementation authority:
`docs/AON_S0_M4_Canonical_Decisions_v1.0.md`.

- [x] Route identity, Revision, synchronization, Certificate, V3 encoder, and completion decisions
  are versioned.
- [x] Driver Revision, revision-aware Sink Slots, four-way Route Diff, TopologySync, certified
  staging/consumption, and V3 canonical bytes/hash are implemented.
- [x] C-18, C-19, destroy/rebuild invalidation, deterministic topology fuzz, and retained-corpus
  replica agreement have executable coverage.
- [x] Final-workspace and fresh clean-checkout offline verification pass without warnings.

The 21 completion gates have the following executable evidence.

1. Revision no-op/change semantics: `crates/aon-sim/src/signal.rs`
   (`driver_revision_advances_only_for_a_real_sample_change`) and
   `crates/aon-sim/src/simulation.rs` (`driver_revision_overflow_rolls_back_event_and_certificate_frontiers`).
2. Slot table, stored-r3 conflict plus r4, and permutation rollback:
   `crates/aon-sim/src/signal.rs` (`slot_revision_table_is_applied_without_partial_mutation`) and
   `crates/aon-sim/src/simulation.rs` (`stored_revision_conflict_is_fatal_even_with_a_higher_winner_and_permutations`).
3. Four-way, generation-sensitive Route Diff and layout-order independence:
   `crates/aon-sim/src/signal_topology.rs` (`compiled_route_ties_ignore_command_and_store_layout`,
   `route_diff_is_four_way_pair_ordered_and_generation_sensitive`).
4. Same-Tick sync N versus propagation N+1 winner:
   `crates/aon-sim/tests/topology_sync.rs`
   (`replaced_shorter_route_sync_and_same_tick_revision_win_preserve_c19`).
5. Local zero-delay and positive physical sync delay:
   `crates/aon-sim/tests/topology_sync.rs`
   (`local_zero_sync_is_same_tick_while_c18_physical_sync_waits_exact_delay`) and
   `crates/aon-sim/src/signal_topology.rs` (`physical_delay_is_positive_and_local_delay_is_selected_by_compiler`).
6. C-18 passive-Low until exact new-route delay:
   `crates/aon-sim/tests/topology_sync.rs`
   (`local_zero_sync_is_same_tick_while_c18_physical_sync_waits_exact_delay`).
7. C-19 old revision cannot revert a newer Slot:
   `crates/aon-sim/tests/topology_sync.rs`
   (`replaced_shorter_route_sync_and_same_tick_revision_win_preserve_c19`).
8. Shorter replacement produces sync without changing the old arrival:
   `crates/aon-sim/tests/topology_sync.rs`
   (`replaced_shorter_route_sync_and_same_tick_revision_win_preserve_c19`).
9. Removed Route deletes its Slot and resolves passive Low without an arrival:
   `crates/aon-sim/tests/topology_sync.rs`
   (`removing_a_route_deletes_its_slot_and_resolves_passive_low_without_an_arrival`,
   `removing_an_in_flight_route_resolves_a_live_sink_even_before_its_slot_exists`).
10. Remove, rebind, and bind-away/back invalidate stamped paths only:
    `crates/aon-sim/tests/topology_sync.rs`
    (`wire_remove_rebind_bind_away_back_and_rebuild_invalidate_old_certificates`,
    `binding_another_incident_wire_advances_the_stamped_junction_and_invalidates_old_arrivals`).
11. Identical-geometry rebuild with a new EntityId invalidates the old path:
    `crates/aon-sim/tests/topology_sync.rs`
    (`wire_remove_rebind_bind_away_back_and_rebuild_invalidate_old_certificates`).
12. Unrelated edit retains a valid pending Certificate:
    `crates/aon-sim/tests/topology_sync.rs`
    (`unrelated_topology_edit_keeps_a_pending_certificate_valid`).
13. Monotonic/tombstoned Certificate IDs and canonical candidate allocation:
    `crates/aon-sim/src/path_certificate.rs` tests and
    `crates/aon-sim/src/event.rs` (`candidate_permutations_produce_identical_calendars_and_arenas`).
14. Exact empty, single-Wire, adjacent-Wire, and Junction sequences:
    `crates/aon-sim/src/signal_topology.rs`
    (`compiled_certificates_cover_empty_single_and_adjacent_gate_port_wires_exactly`).
15. Pending-invalid is permitted while orphan, consumed, and duplicate ownership is fatal:
    `crates/aon-sim/src/simulation.rs`
    (`committed_validator_rejects_orphan_consumed_and_duplicate_certificates` and the committed
    registry/store/calendar key-consistency regressions) plus `crates/aon-sim/tests/topology_sync.rs`
    invalidation cases.
16. Certificate/payload/Revision exhaustion and preflight failures roll back transactionally:
    `crates/aon-sim/src/simulation.rs`
    (`driver_revision_overflow_rolls_back_event_and_certificate_frontiers`,
    `certificate_and_payload_exhaustion_roll_back_phase0_allocations`,
    `topology_sync_due_tick_overflow_rolls_back_phase0_and_both_allocators`),
    `crates/aon-sim/src/event.rs`, and `crates/aon-sim/src/path_certificate.rs` tests.
17. V3 exact empty/populated bytes, sensitivity, tombstones, and raw-layout exclusions:
    `crates/aon-sim/src/canonical.rs` Path Certificate tests,
    `crates/aon-sim/tests/bootstrap_simulation.rs` (`s0m4_empty_state_v3_hash_has_a_golden_value`),
    and `crates/aon-sim/tests/structural_lifecycle.rs` V3 golden tests.
18. Certificate canonical sensitivity and observation/layout non-mutation:
    `crates/aon-sim/src/canonical.rs`
    (`path_certificate_fields_frontier_and_tombstones_are_hash_sensitive`,
    `path_certificate_raw_ranges_orphan_bytes_and_capacity_are_not_canonical`),
    `crates/aon-sim/tests/topology_sync.rs`
    (`route_diagnostics_and_public_observations_do_not_mutate_state_hash`), and public replica
    observations in `crates/aon-fuzz-harness/src/topology_runtime.rs`.
19. Stateful in-flight topology fuzz reaches add/remove/rebind/rebuild, stale revision, invalid
    generation, same-Tick sync/propagation, and checked boundaries:
    `crates/aon-fuzz-harness/src/topology_runtime.rs` and retained cases in
    `crates/aon-fuzz-harness/corpus/topology-runtime/`.
20. Forward/reversed public streams and retained corpus have identical reports, observations, and
    per-Tick hashes: `crates/aon-fuzz-harness/src/topology_runtime.rs` and
    `crates/aon-fuzz-harness/tests/regression_corpus.rs`.
21. Final-workspace and fresh clean-checkout proof uses `cargo fmt --all -- --check`,
    `cargo metadata --format-version 1 --no-deps --locked --offline`,
    `cargo check --workspace --all-targets --locked --offline`,
    `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`,
    `cargo test --workspace --locked --offline`, the canonical-core dependency boundary check,
    and the same commands from a fresh clean checkout of the final commit.

Passing these gates completes only S0-M4. S0-M5 completion evidence follows below; the remaining
Stage 0 milestones and gates are still mandatory.

## Completed slice — S0-M5

S0-M5 completed on 2026-08-12. Completion authority is
`docs/AON_S0_M5_Canonical_Decisions_v1.0.md`. This completes only the feedback/replay slice; it does
not complete Stage 0 or the game engine.

The 18 completion gates have the following executable evidence.

1. Feedback-capable Gate startup is Low/zero-strength/Revision-zero with no preexisting Event in
   `crates/aon-sim/src/signal.rs` (`every_gate_type_activates_with_the_frozen_quiescent_signal_state`);
   ordinary Phase 3/6 delayed activation is asserted by C-05 in
   `crates/aon-sim/tests/feedback_conformance.rs`.
2. Cycles compile through ordinary Driver/Route/Arrival and Path Certificate state with no
   feedback or Latch canonical type: `crates/aon-sim/tests/feedback_conformance.rs` and
   `crates/aon-sim/src/simulation.rs`.
3. C-05 proves the one-NOT self-loop's exact rising/falling `2D=4` period and retained edges in
   `crates/aon-sim/tests/feedback_conformance.rs`.
4. C-06 proves exchange-symmetric two-NOT startup across reversed command slices without choosing
   a complementary EntityId branch in `crates/aon-sim/tests/feedback_conformance.rs`.
5. Set, release, hold, Reset, release, and clear emerge from OR/NOT/Wire only in
   `crates/aon-sim/tests/feedback_conformance.rs`
   (`explicit_nor_style_set_reset_emerges_from_only_or_not_and_wire`).
6. Replay Header immutably binds format, semantics, three Profile Hashes, State Hash version,
   generator, zero Seed, initial hash, and algorithm in `crates/aon-sim/src/replay.rs`,
   `crates/aon-sim/src/simulation.rs`, and their Replay unit tests.
7. Strict JSON canonical round-trip and typed rejection of unknown fields, numeric width/float,
   invalid hash/Seed, unsupported versions, nonzero Empty seed, and World Inputs are covered by
   `crates/aon-sim/src/replay.rs` tests and `crates/aon-sim/tests/replay_golden.rs`.
8. All eight Stage 0 Command variants round-trip with extreme coordinates, typed IDs, domains,
   endpoints, ports, and levels in `crates/aon-sim/src/replay.rs`
   (`command_json_round_trips_all_stage_zero_variants`).
9. Reversed typed/JSON arrays, duplicate ordinals, canonical byte normalization, Tick batches,
   reports, rejections, and hashes are invariant in `crates/aon-sim/src/replay.rs`
   (`duplicate_ordinal_replay_normalization_is_input_json_and_execution_invariant`).
10. Checkpoint zero, strict duplicate/descending rejection, sparse success, final boundary,
    `u64::MAX` trace preflight, and exact first divergence are covered by Replay unit tests and
    `apps/aon-headless/tests/replay_cli.rs`.
11. Unsupported semantics, State Hash version, generator, Seed, and hash algorithm are typed
    strict-decode rejections; the three Profile Hashes and initial hash return their exact
    `ContractMismatch` field before Tick 0 without mutation in `crates/aon-sim/src/replay.rs`
    (`profile_and_initial_hash_mismatches_report_exact_fields_without_mutation` and the decoder
    rejection matrix).
12. `fixtures/replays/feedback-ring-v1.json` is exact canonical encoder output and its complete
    retained trace/checkpoints run headlessly in `crates/aon-sim/tests/replay_golden.rs` and
    `apps/aon-app/tests/replay_host.rs`.
13. That retained feedback Replay produces identical Headless and Bevy `FixedUpdate` traces with
    presenter on/off and 0/1/7 presentation updates in `apps/aon-app/tests/replay_host.rs`.
14. One long frame carrying Tick debt and 21 fixed frames preserve the same retained trace in
    `apps/aon-app/tests/replay_host.rs`
    (`retained_feedback_replay_preserves_trace_across_frame_partitioning`).
15. `fixtures/replays/stage0-100k-v1.json` runs 100,000 Ticks, matches its final golden, and compares
    all 100,001 Headless/Bevy hashes in `apps/aon-app/tests/replay_host.rs`
    (`retained_stage0_100k_replay_matches_headless_and_bevy_complete_trace`); the final workspace
    run passed in 207.65 seconds.
16. Bounded no-panic Replay decode fuzzing and retained valid/truncated/unknown-field cases live in
    `crates/aon-fuzz-harness/src/lib.rs`, `crates/aon-fuzz-harness/tests/regression_corpus.rs`, and
    `crates/aon-fuzz-harness/corpus/replay/`.
17. Replay/Header/Command/checkpoint/report and public observation reads preserve State Hash and
    Tick in `crates/aon-sim/src/replay.rs`
    (`replay_report_and_public_observation_reads_do_not_mutate_simulation`).
18. Final-workspace metadata, format, all-target check, workspace strict Clippy, workspace tests,
    canonical-core dependency boundary, and repository-zone checks pass offline; the independent
    audit reports P0/P1 findings 0 and the trace-length overflow P2 is fixed. Fresh clone commit
    `d45af47` passed the same offline gates; its 100,001-state Headless/Bevy comparison completed in
    207.96 seconds.

Passing these gates completes only the formally committed S0-M5 slice. The current workspace has
subsequently implemented S0-M6 and S0-M7, but their authority documents still require final
committed fresh clean-checkout evidence.

## Stage 0 closure status

As of 2026-08-12, Stage 0 is complete. The Windows-native workspace has executable S0-M6/S0-M7 evidence, matched
current-input-only and retained-state V4 Mobility Replays, an F5/F6 direct-play A/B probe, native
probe smoke evidence, and a passing fail-closed Stage 0 technical gate. Implementation commit
`bf651c9e63aed41e6db7e0f7b2767345834ff94e` passed an independent `git clone --no-local`
detached Windows-native offline run: format, metadata, all-target check, strict Clippy, every
workspace test, 25 exact technical-gate tests, the canonical dependency boundary, native link,
and a clean post-run status. The same clean binary passed the default editor and F5/F6 Network/
Circuit Mobile GUI smokes. The Stage 0 product gate passed by explicit user direct-play verdict
on 2026-08-12 using `docs/AON_Stage0_Product_Gate_Playtest_v1.0.md`. Stage 1 may begin.

## Completed slice — S1-M0

S1-M0 completed on 2026-08-12 at implementation commit
`fe616fc6d9ffc81fd37864cf3d018343b327106b`. This completes only the Physical Scale
Experiment Baseline; S1-M1 through S1-M6 and both Stage 1 gates remain open.

- [x] The Physical Scale generator validates and publishes a canonical hash-sorted 2 x 2 x 2
  matrix with eight pairwise-distinct semantic Profile hashes.
- [x] Long-wire distance is exact absolute Design geometry rather than a Physical Profile field;
  two distances reuse the eight Physical hashes and produce sixteen distinct canonical Run IDs.
- [x] Strict Experiment plan v1 resolution verifies Scenario and Profile IDs, kinds, schemas,
  invariants, declared hashes, axes, ordering, limits, and retained Run ID goldens.
- [x] Module v1 stores exact absolute Fixed geometry, validates the Semantics/Numeric/Physical
  contract and structural laws, and never scales, snaps, rotates, or mutates its source artifact.
- [x] Generated-profile Replay round trips with an identical full trace, while a different
  Physical hash is rejected before Tick execution.
- [x] Headless materialization validates before atomic publication and emits eight canonical
  Physical Profile files plus one sixteen-run manifest; bounded Experiment and Module decoder
  fuzz targets replay retained valid and invalid corpora without panic.
- [x] The fail-closed `scripts/s1-m0-technical-gate.ps1` runs 47 exact tests, and the complete
  Stage 0 technical gate remains green.
- [x] An independent Windows-native `git clone --no-local` of the implementation commit passed
  locked/offline metadata, formatting, all-target workspace check, strict Clippy, the complete
  workspace test suite, both technical gates, actual headless materialization, and a clean
  post-run Git status. WSL was not used.

Retained closure goldens:

- Experiment plan file SHA-256:
  `9229F65C6DD81605FB912EF03FBEA832A7828FA48675E0DE260D6A51F96872F4`.
- Module fixture file SHA-256:
  `2C2B27B14C5D75EF90EA2CAD075CA66D35CDC90A7129F575446161D65A5CABA3`;
  Module semantic `ArtifactHash`:
  `e7130605cbaebd753f8f338be7a633d8006bc6f85b14bbd5c74e44ecd0a06172`.
- Materialized `runs.json` SHA-256:
  `6F16DE35480066D8B7DCBC1006AC3F27CA30E219012F4F404D7E896391ACD371`;
  its sixteen retained Run IDs are ordered and pairwise distinct.

## Completed slice — S1-M1

S1-M1 completed on 2026-08-13 at implementation commit
`5554f76266467d9112acdb2bad3ba5fcba4ed011`. This completes Main Core / Capacity
Accounting only; S1-M2 through S1-M6 and both Stage 1 gates remain open.

- [x] Scenario schema v2 creates exactly one protected Main Core as EntityId 1 with exact Fixed
  position, implicit MainCoreAnchor, Capacity, Integrity, and HeatEnergy; Scenario v1 Empty bytes
  and semantic hash remain preserved.
- [x] The capacity feature, Main Core initial world, and `capacityProbe` profile form a strict
  dependency triad. Capacity coefficients remain inert when that feature is absent, and active
  whole-NCU to raw-Fixed conversion is checked and atomic.
- [x] Phase 4 sums every live Wire body exactly once in WireId order across OpenWorld,
  FixedSubstrate, and MobileSubstrate domains. It reports `U` and Main Core `S` as a soft limit;
  over-capacity construction remains accepted.
- [x] C-21 is retained as 10 NCU for one body, 10 NCU after an exact four-Wire split, and 12 NCU
  after a 2 NCU internal Wire. Per-Wire integer Euclidean rounding, including the 46,341/46,342
  diagonal boundary, is independently frozen.
- [x] Network Analyzer rows are WireId-sorted, sum exactly to Phase 4 usage, include the Main Core
  contribution, and are read-only derived observations excluded from State Hash.
- [x] MainCoreAnchor has exact command, Replay JSON, topology, Signal-isolation, and State encoder
  tags; live identity, position, routing-domain, binding, removal, and registry invariants are
  validated fail-closed.
- [x] State Hash V5 globally encodes Main Core presence and fields while retaining strict V3/V4
  Replay decode/re-encode compatibility and rejecting legacy execution before Tick 0.
- [x] Headless and Bevy hosts reproduce identical per-Tick reports and V5 hashes; Network View,
  selection, inspector, Analyzer, probes, and presentation partitions are non-intervening.
- [x] `scripts/s1-m1-technical-gate.ps1` runs 45 fail-closed exact tests for Gates 1–12, while the
  Stage 0 25-exact-test and S1-M0 47-exact-test technical gates remain green.
- [x] An independent Windows-native `git clone --no-local` of the implementation commit passed
  locked/offline metadata, formatting, all-target workspace check, strict Clippy, all 514
  registered workspace tests, all three technical gates, and a clean post-run Git status. WSL
  was not used.

Retained closure goldens:

- Scenario v2 file SHA-256:
  `F377128795D98B661D2BCEDC685DF5F193EE69C417C763F7486B3EAE2048251F`;
  semantic `ArtifactHash`:
  `f81b15ab86e4c172275b2e2c1c9a13289c04997e3fc1e80f14deedcd76d964ae`.
- Capacity Replay file SHA-256:
  `205BE93F4A848FD50189B9565841F0631B62541BB66EB520B51A3FAD4A46256B`.
- Retained Main Core initial State Hash:
  `d240a7ed885698c6d3197d7df0da1b9d741d702cdfd37a40df4e57f21659d87b`.
- Retained C-21 final State Hash:
  `cbe2f28ada7d5b969de8e220e694996a76391c2d5bf5605c55f28dab803150df`.

## Completed slice — S1-M2

S1-M2 completed on 2026-08-13 at implementation commit
`22d6ccd89cb0e1fa422111f98f99c9d371122695`. This completes Sensing / Power /
Brownout only; S1-M3 through S1-M6 and both Stage 1 gates remain open.

- [x] Balance schema v3 and Scenario schema v3 strictly bind the complete Power Probe,
  Main Core, and canonical Power Sources. Replay v2 supplies complete nonpersistent
  `HostileFrame` inputs, and the current global State encoder is V6.
- [x] Exact capsule sensing samples every live Wire in Phase 1. Its A/B Sense ports are isolated
  from the Wire signal surface and Junction OR behavior, preserve non-inertial delayed changes,
  and become passive Low without a Power grant.
- [x] Source anchors compile deterministic cross-domain Power regions and routes. Intrinsic Gate,
  Wire, Sense, and Movement demands are collected before a 17-step fixed-point common-ratio solve;
  route, rounding, zero-length, overflow, and permutation boundaries are fail-closed.
- [x] Brownout scales Gate delay/drive, Sense drive, Movement budget, and the pure Work-grant seam.
  Gate retention expires on the exact third under-threshold Tick, while cancellation, same-due
  event merging, and next-edge movement boundaries remain deterministic and atomic.
- [x] Leakage and transmission Heat are derived only from granted energy and are published by
  Phase 8 without adding premature thermal state. Power/Sense and Network Analyzers recompute
  sorted read-only observations without changing State Hash or identity frontiers.
- [x] Retained C-07 proves complete hostile counts `0 -> 3 -> 0`, independent A/B Sense output,
  exact delayed `Low -> High -> Low`, and multiplicity/order invariance.
- [x] Retained C-08 compares identical circuits at generation 51 and 24: total nominal demand 51,
  exact common ratios 1 and 1/2, Gate due Ticks 3 and 4, drive 400 and 200, Movement grants 65,536
  and 32,768, and pure Work grants 8 and 4.
- [x] Headless and Bevy hosts reproduce every retained C-07/C-08 per-Tick report and V6 hash.
  Bounded Scenario, Balance, Replay, topology, geometry, and solver corpora remain panic-free.
- [x] `scripts/s1-m2-technical-gate.ps1` runs 70 fail-closed exact tests for Gates 1–15, while the
  Stage 0 25-test, S1-M0 47-test, and S1-M1 45-test technical gates remain green.
- [x] An independent Windows-native `git clone --no-local` of the implementation commit passed
  locked/offline metadata, formatting, all-target workspace check, strict Clippy, all 629
  registered workspace tests, all four technical gates, and a clean post-run Git status. WSL was
  not used.

Retained closure goldens:

- S1-M2 authority SHA-256:
  `BBE7D7C45752B53714691FD391B77C9FBD52143B65760E9D1BD2E6CDAD555A9D`.
- Balance v3 file SHA-256:
  `6E2C5319F0103D731FCF419B5EF970DF12CF3FB41B1B9C0E93FEDEAD0E6177AA`;
  semantic `ProfileHash`:
  `96d89224a7edc9b2bbd82b092891465d42b0c8e3954ebed6f9693af216cdcc63`.
- C-07 Scenario file SHA-256:
  `C87CF13C383FAB6125DE5A0758A1F91A9B949A1D2E439A72385DE7D4FF383295`;
  semantic `ArtifactHash`:
  `5770222301e36fd352a859b4adce2907eac167ed233155ecfafa227f5cc59fef`.
- C-08 full/half Scenario file SHA-256 values:
  `3E286FA136028FED6A7D5FBFC9ADDF72032A40F8EB15B1BFD746D37FEFB945E9` and
  `FDC3A1B3FEBCE918BFFD82AD56E2EC690C6E9CC0AEF32DEC6659BCFACEA51D6C`;
  semantic `ArtifactHash` values:
  `98f73f4e267f1c1ddd706a1aafff2f075192592c5ce30dba1cbe17eb3f7af4d2` and
  `d28c5a918675bd4e00d0b8c62c4cd12cff145f4e09bf1415a8002c508cc066a1`.
- C-07 Replay file SHA-256:
  `3588209A8422121F55CA658AD1FFDFDDD80422816E5A62411270C468BC9A2DE6`;
  initial/final V6 State Hashes:
  `8b0c8f872b033be3f8c7a33f78c5ead7503cb9f29e39ba5eb9a737ffec0bf5c5` and
  `f7e3c45129336c4f018e63ad942500701efd98c2963c903fdd0c4e6df6b70d47`.
- C-08 full Replay file SHA-256:
  `7403378123A7A24479DF303DA3CD32753337926C037D568AEDDD212D554370EC`;
  initial/final V6 State Hashes:
  `086b4b71ba63d0795c1bda727e06ff3b949c1daac60b22cd59da9df27fe46db7` and
  `516070270ef1ef46bf312d2c2e906a0597974b6e3afa4546c7642a5e6224b3f3`.
- C-08 half Replay file SHA-256:
  `75485C5CDE0C5BCAA0CC4635D792C066E1D8AA870911AD81BF7BC6D222E346D6`;
  initial/final V6 State Hashes:
  `72a6eb1f246d18b7059e4a4d9efc6394890edd8360582d7f6b12beac3a13a5ed` and
  `8565e47f3a2a9d652956a9ca692b7cc3c3baaaf5f2dbb07b334acfd25ee7cace`.

## Completed slice — S1-M3

S1-M3 completed on 2026-08-13 at implementation commit
`f59fe50b6b19af0696e4f4fd0e2523f12889f973`. This completes Capacity Support Load only;
S1-M4 through S1-M6 and both Stage 1 gates remain open.

- [x] Balance schema v4 strictly requires Capacity, Power, and Capacity Support probes, selects the
  outer schema before strict version-body faults, and adds one independently hash-sensitive exact
  `supportPowerPerNCU` Rational. Balance v2/v3 validation, encodings, hashes, and runtime behavior
  remain unchanged.
- [x] The public exact-`u128` kernel computes `E=max(0,U-S)` and the frozen linear/quadratic support
  curve with one final ceiling. It rejects coefficient, denominator-floor, conversion, and overflow
  boundaries with exact typed errors and no saturation, wrapping, panic, or partial output.
- [x] Positive Support Demand is distributed conservatively by measured Wire length with remainder
  assigned in ascending `WireId` order. Phase 4 reuses those lengths and inserts positive shares as
  intrinsic `DemandKind::OvercapacitySupport` loads before the common-ratio Power solve.
- [x] Support loads receive the same regional ratio as ordinary loads, including partial and
  source-less cases. Phase 8 derives report-only Support Heat from actual grants with
  nearest-ties-even rounding; it adds no canonical thermal state, Phase-9 integration, damage, or
  destruction.
- [x] Balance v2/v3 reports retain `None`, active v4 zero is `Some(0)`, and overcapacity publishes
  exact positive excess, total demand, and per-Wire shares. Network and Power/Sense analyzers
  recompute sorted read-only observations without changing State V6, ticks, events, identities, or
  allocator frontiers.
- [x] Retained C-22 proves `U=120`, `S=100`, `E=20`, Support Demand `28`, ascending-Wire shares
  `17/11`, ordinary leakage+sensing demand `240`, total nominal demand and Source generation `268`,
  common ratio `1`, and Support Heat `4+3=7`, with no Wire deletion, build rejection, or direct
  Capacity delay/damage.
- [x] Headless and Bevy hosts reproduce every C-22 completed-Tick State V6 hash, Capacity report,
  Power load/grant/ratio, and Phase-8 Heat row through `nextTick=3`.
- [x] Bounded independent exact-`u128` oracles freeze the curve, one-final-ceiling rule, typed
  numeric boundaries, monotonicity (`U=121` produces `D=30 > 28`), conservation, and permutation
  stability; bounded strict-decoder and Capacity Support corpora remain panic-free.
- [x] `scripts/s1-m3-technical-gate.ps1` runs 29 unique fail-closed exact tests covering executable
  Gates 1–15, while the Stage 0 25-test, S1-M0 47-test, S1-M1 45-test, and S1-M2 70-test gates
  remain green.
- [x] An independent Windows-native `git clone --no-local` of the implementation commit passed
  locked/offline metadata, formatting, all-target workspace check, strict Clippy, all 653
  registered workspace tests, all five technical gates, and a clean post-run Git status with zero
  tracked or untracked differences. WSL was not used.

Retained closure goldens:

- S1-M3 authority SHA-256:
  `C725E03D2E7790056F32DE29D83D190CF01784617388646756886B6C77DBAF9B`.
- Balance v4 file SHA-256:
  `73A3B3512D0809808469BE614426867789D17F1BE86C5E9513893A4DE686624B`;
  semantic `ProfileHash`:
  `a0a8974aebc87e30d602ffa019340e59c908912c0b36e0e0634e51214afc45ef`.
- C-22 Scenario file SHA-256:
  `609CE583577F0E65084A75A27644C1D8D58FD054B966D99E98947081D0BCE992`;
  semantic `ArtifactHash`:
  `bdebfe491a2f3a31dfdcd7c2470cf447415137459de5e4d65095d3d38f0e01a5`.
- C-22 Replay file SHA-256:
  `8BB79ED60AE5CAFBC46F7A077549773BD9C117738D99982E5354EFA8DA777C9C`;
  initial/final V6 State Hashes:
  `47cddc7a4a1a1371d6600953bb7c0acc7c7e5e465741869375026e7efcab9369` and
  `7f687d752df7146141be826dbb74668866494c1a024ec6f157bb3eb264c3445c`.

## Completed slice — S1-M4

S1-M4 completed on 2026-08-14 at verified implementation tree
`1ce66372e7520c770f08b676cadba2837ceb753b`. The primary feature implementation is
`161f3fc0ae88cd6683d6fa01310f8f6d520229e0`; the follow-up commit makes canonical Replay and
Balance bytes clone-stable under Windows checkout. This completes Construction / Contact / Damage
only; S1-M5, S1-M6, and both complete Stage 1 gates remain open.

- [x] Balance schema v5 appends exact Construction and Contact/Damage probes while retaining v2–v4
  bytes, hashes, validation, and behavior. Scenario v4 owns sorted canonical Enemy initial states,
  and State V7 encodes Enemy, Site, optional Damage State/BUILD, pending destruction, and RunStatus.
- [x] `PlaceConstructionSite` reserves exact Gate/Wire/Junction/Fixed-Substrate geometry. BUILD is a
  real Mobile-owned Construction Power load, uses the regional common ratio and retained
  `scale_work`, supports source-less/partial/full and stable multi-builder reduction, and activates
  a completed Site only on the next Phase 0 with a fresh EntityId.
- [x] Live Wire demand uses resolved HIGH strength and actual granted Power. Exact swept Wire-body
  contact and deterministic EntityId remainder allocation conserve every granted Energy unit; C-10
  freezes grant `20` as Enemy absorption `5+5` plus Wire Heat `10`.
- [x] Phase 8 Heat sources integrate exactly once in Phase 9. Electrical and prior-Phase-1 thermal
  damage reduce Integrity simultaneously, mark sorted pending destruction in Phase 10, and remove
  objects on the next Phase 0 without cancelling their current-Tick work or actions.
- [x] C-09 freezes lethal Wire damage at Tick 45, all-surface removal and fresh EntityId `13` at
  Tick 46, and stale in-flight Signal Arrival rejection at Tick 51. Capacity, Signal, Sense, Power,
  Track, sink-slot, topology generation, region split, tombstone, and no-reconstruction facts are
  asserted explicitly.
- [x] Main Core lethal damage commits the complete fatal Tick and terminal State V7 hash before
  `RunStatus::Ended`; every later step is a typed, non-mutating `RunEnded` boundary in Simulation,
  Headless, Bevy, and the interactive Laboratory.
- [x] Five retained Replay v2 artifacts cover partial multi-builder Construction, all four target
  activations, C-10, C-09, and terminal behavior. Headless and Bevy reproduce every completed-Tick
  report and V7 checkpoint exactly.
- [x] Bounded exact kernels, independent oracles, stateful public-runtime corpora, strict
  Balance/Scenario/Replay/Command decoders, HostileFrame non-contact, mutual lethal actors,
  two-Tick thermal timing, and Mobile local-to-world geometry remain deterministic and panic-free.
- [x] `scripts/s1-m4-technical-gate.ps1` runs 95 unique fail-closed exact tests plus two
  executable/static invariants covering Gates 1–15. The Stage 0 25-test, S1-M0 47-test, S1-M1
  45-test, S1-M2 70-test, and S1-M3 29-test gates remain green.
- [x] An independent Windows-native `git clone --no-local` of the verified implementation tree
  passed locked/offline metadata, formatting, all-target workspace check, strict Clippy, all 761
  registered workspace tests, all six technical gates, canonical generator byte comparison, and a
  clean post-run Git status with zero tracked or untracked differences. WSL was not used.

Retained closure goldens:

- S1-M4 authority SHA-256:
  `00C17ADD8C1DDC5839F88FDE405E8EE5F3BFD6589AF9F414179417A2E5587667`.
- Balance v5 file SHA-256:
  `CEB4B24D81E94D85EF5535D1438E8C7A075BC718D4BC4AE5A8458134373E5C05`;
  semantic `ProfileHash`:
  `88b8fdc40dae59563699a0f611adae21c40d770d3d1c9076f8262a756107311a`.
- S1-M4 Scenario file SHA-256:
  `0A48404886D661DC2B991B7A904B2B6E7D9008F6D2193C7A7C182B18A284FC5E`;
  semantic `ArtifactHash`:
  `a9770d7afc466087664f44846d65f56e93d479738705975c10ab6527b59817cd`;
  common initial V7 State Hash:
  `51ace8554724d927c81c68716d15cc58a4115959076d031688ef85915e960111`.
- Partial multi-builder Replay SHA-256:
  `BF42606466998A3481E5A11CC2AADE7B0C2AED0CDB053B56E273CEACE84622D0`;
  final `nextTick=5` V7 State Hash:
  `31a1089b727c09776cd66796f116b1e0397286604bf2ef261ac38c0bec68efe1`.
- Four-target Construction Replay SHA-256:
  `99124936C9513C3EE56801AA114DF41B02FA78411CE6D26F0D46D35483C48D0C`;
  final `nextTick=21` V7 State Hash:
  `0670210744f55ef99e67d4170546bcd88f8a90a9dbe506d54163601c93fee3de`.
- C-10 Replay SHA-256:
  `61C4BBBF8B247E48031388D61ABBE8E6C1396D025A9A67D62565AAED1B65F62A`;
  final `nextTick=9` V7 State Hash:
  `5ba4d9a856a765cd59a98592b82b9de256a389ece2aecfe6cd34ef0e26c4b420`.
- C-09 Replay SHA-256:
  `27CB7C0AC4C4D3180B66CFB2B2AB53EB1DEAB291B3F4ED5EC0490D451FF96093`;
  final `nextTick=52` V7 State Hash:
  `7452a1b72aa6622f8d894cd64866707ce6c7fdb3c2faf8efdcc4c6ee0c7a0bd4`.
- Terminal Replay SHA-256:
  `4CC1303EDA2B1742F9076A7ED56FA6C546D41333FCAEFB7E8653F3A3EA876490`;
  final `nextTick=56` V7 State Hash:
  `fe1000209769a38c50440dd1bbcfe70d19d2cb09529343125590b06e4e129777`.
