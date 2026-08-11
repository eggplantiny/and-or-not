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
- [ ] S0-M4 — Topology Sync / Path Certificate
- [ ] S0-M5 — Feedback / Replay
- [ ] S0-M6 — Bevy ASCII Probe
- [ ] S0-M7 — Mobility
- [ ] Stage 0 technical gate
- [ ] Stage 0 product gate

Required conformance: C-01, C-02, C-03, C-05, C-06, C-14, C-16, C-17,
C-18, C-19, C-20, C-25.

## Stage 1 — Capacity Economy Probe

- [ ] S1-M0 — Physical Scale Experiment Baseline
- [ ] S1-M1 — Main Core / Capacity Accounting
- [ ] S1-M2 — Sensing / Power / Brownout
- [ ] S1-M3 — Capacity Support Load
- [ ] S1-M4 — Construction / Contact / Damage
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
- [ ] Golden replay and 100,000-Tick Stage 0 fixture pass
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

## Next implementation slice — S0-M4

S0-M4 owns Route Diff, Driver Revision, TopologySyncArrival, Sink Slot revision comparison,
PathCertificateArena, connection-generation validation, and stale-arrival rejection after an
in-flight topology edit.

Implementation authority: `docs/AON_S0_M4_Canonical_Decisions_v1.0.md`.

- [x] Route identity, Revision, synchronization, Certificate, V3 encoder, and completion decisions
  are versioned
- [ ] Driver Revision and revision-aware Sink Slots are implemented
- [ ] Added/Removed/Retained/Replaced Route Diff and TopologySync are implemented
- [ ] PathCertificateArena allocation, canonical staging, consumption, and validation are
  implemented
- [ ] V3 canonical bytes/hash and invariants are fixed
- [ ] C-18, C-19, destroy/rebuild invalidation, fuzz, and clean-checkout gates pass
