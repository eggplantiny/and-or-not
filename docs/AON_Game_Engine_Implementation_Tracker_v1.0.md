# A/O/N — Game Engine Implementation Tracker

**Version:** v1.0
**Source baseline:** PRD v1.0 GO / SSS v1.0 Draft / TRD v1.0 Draft
**Completion boundary:** TRD §40.4 MVP Definition of Done

This tracker distinguishes an implemented system from an experimentally proven product gate.
The game engine is complete only after every milestone, conformance test, regression gate,
cross-host/replay gate, and MVP scenario below has authoritative evidence.

## Stage 0 — Emergence Probe

- [x] S0-M0 — Bootstrap
- [ ] S0-M1 — Contract / Numeric / Identity
- [ ] S0-M2 — Command / Geometry / Structural Phase
- [ ] S0-M3 — Signal Topology / Event Runtime
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

## Current implementation slice

S0-M1 is in progress. Its acceptance evidence is:

- [ ] `SimulationContract` validates all three canonical profile hashes
- [ ] Numeric v1 and Stage 0 physical/balance profile artifacts load and validate
- [ ] Same semantic profile content produces the same hash regardless of JSON formatting or ID
- [ ] `floor_div`, `ceil_div_nonnegative`, and ties-to-even division pass edge cases
- [ ] `ceil_isqrt`, segment/polyline length, cell coordinate, and quantization pass C-17
- [ ] Stable `EntityId` allocation is monotonic and never reuses a destroyed ID
- [ ] `ConnectionGeneration` increments with checked overflow
- [ ] Canonical state encoding includes contract, Tick, topology revision, and identity state
- [ ] All workspace and clean-checkout quality gates pass
