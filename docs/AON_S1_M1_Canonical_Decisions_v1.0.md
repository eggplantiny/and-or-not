# A/O/N — S1-M1 Canonical Decisions

**Status:** implementation authority
**Applies to:** `S1-M1 — Main Core / Capacity Accounting`
**Source baseline:** PRD v1.0 GO Candidate / SSS v1.0 Draft / TRD v1.0 Draft /
S1-M0 Canonical Decisions

This document freezes the representation, artifact, versioning, validation, and test choices needed
to implement S1-M1. It is subordinate to the source documents in their respective areas of
authority:

- the PRD determines the product question, scope, and product invariants;
- the SSS determines observable World behavior, phase timing, numeric laws, and contract ownership;
- the TRD determines data structures, encoders, APIs, ordering, and test structure;
- the S1-M0 authority freezes the retained Physical Scale experiment and artifact baseline;
- this document closes only the gaps required by S1-M1.

If a later PRD or SSS revision changes an observable law, the compatibility review required by TRD
section 41 applies. Closing S1-M1 does not close the Stage 1 technical or product gate.

## 0. Source authority map

The decisions below apply the frozen priority, not document recency. The relevant source anchors are:

| Subject | Authority lines |
|---|---|
| Document responsibility and conflict priority | SSS lines 53-75; TRD lines 14-35 |
| Product Core invariants | PRD lines 289-306 |
| Global pool, physical-Wire usage, and one-time multi-role accounting | PRD lines 371-439 |
| Soft limit, not a build permission | PRD lines 443-476 |
| Stage 1 question and system boundary | PRD lines 1465-1493 |
| Numeric types, Fixed scale, Euclidean length, and per-Wire maximal runs | SSS lines 261-350 |
| Main Core canonical World state and derived accounting boundary | SSS lines 390-428 |
| Phase 0 activation/removal and Phase 4 accounting | SSS lines 470-548 |
| Exact `U`, one-time roles, split condition, and `S` laws | SSS lines 1146-1216 |
| Main Core roles, non-Power status, and later run end | SSS lines 1316-1330 |
| C-21 expected 10/10/12 NCU | SSS lines 2608-2622 |
| Canonical World/Main Core data shape | TRD lines 1243-1262 and 1325-1338 |
| Capacity scratch, units, inclusion, order, and preservation | TRD lines 2783-2868 |
| Analyzer observability and conformance | TRD lines 2973-2996 |
| StepReport/Analyzer host boundary | TRD lines 4514-4540 |
| Exact S1-M1 through S1-M4 ownership | TRD lines 5345-5416 |
| Full Stage 1 technical gate | TRD lines 5474-5486 |
| Body Connectivity/Core reachability remains S2-M1 | TRD lines 5531-5539 |
| Tracker status and full Stage 1 requirements | Tracker lines 27-40 |
| S1-M0 exclusions and version boundary | S1-M0 authority lines 77-132 |
| S1-M0 closure applies only to M0 | S1-M0 authority lines 913-949 |

Where the TRD's broad end-state structs contain fields owned by a later milestone, this document
exposes only the S1-M1-owned observable subset. It does not manufacture placeholder zeros.

## 1. Scope and ownership

S1-M1 owns:

- one canonical `MainCoreState` created by the initial-world generator;
- Main Core capacity sourced from the Balance Profile;
- the Main Core's implicit Open World network-anchor endpoint;
- Phase 4 calculation of Used Capacity `U` and Supported Capacity `S`;
- active Wire length accounting across every existing routing domain;
- one-time accounting of a multi-role Wire Body;
- read-only `StepReport` and Network Analyzer observations;
- C-21, including the exact 10/10/12 NCU evidence set;
- the artifact, State Hash, Replay-header, error, and migration changes required by those items.

S1-M1 does not implement:

- sensing, Power solving, Brownout, or C-07/C-08 (S1-M2);
- Excess `E`, the soft support curve, support-demand distribution, Support Heat, Relay capacity, or
  C-22 (S1-M3);
- Construction, Contact, Damage, Main Core destruction/run-end behavior, or C-09/C-10 (S1-M4);
- runtime Module placement or the S1-M5 reference architectures;
- the S1-M6 parameter sweep or either Stage 1 gate;
- Body Connectivity compilation or Core-to-Relay reachability, which remain S2-M1 work.

The complete Stage 1 technical gate still requires C-07, C-08, C-09, C-10, C-21, C-22, and every
Stage 0 regression. Passing C-21 alone therefore closes S1-M1 only.

## 2. Frozen versions

| Contract or artifact | S1-M1 value |
|---|---|
| Semantics Version | `aon-semantics-v1` |
| Scenario schema | v1 Empty retained; v2 Main Core added |
| Scenario semantic hash encoder | v1 retained; v2 added |
| Numeric Profile schema | `1` |
| Physical Scale Profile schema | `1` |
| Balance Profile schema | `2` |
| Profile canonical encoder | `1` |
| Canonical State Hash | `aon-state-v5` for every new session |
| Replay format | `1` |
| Empty World generator | `aon-empty-v1` |
| Main Core World generator | `aon-main-core-v1` |

The existing SSS v1 already defines Main Core state, the Phase 4 capacity laws, the fixed-point
Capacity unit, and C-21. Implementing those laws does not change the Semantics Version. Main Core is
new canonical state, so the State Hash encoder advances globally from V4 to V5. Replay format v1
already carries explicit state-hash and world-generator versions, so its container format remains
unchanged. The optional `capacityProbe` section already belongs to Balance schema v2; its use does
not create Balance schema v3.

## 3. Scenario v2 and InitialWorld

### 3.1 Version/world pairing

Scenario schema v1 remains exactly the retained Empty contract:

```json
"initialWorld": {
  "kind": "empty"
}
```

Scenario schema v2 supports exactly the Main Core v1 contract:

```json
"initialWorld": {
  "kind": "main-core-v1",
  "position": { "x": 0, "y": 0 },
  "integrity": 1000,
  "heatEnergy": 0
}
```

`position.x` and `position.y` are raw signed `i64` Fixed coordinates. `integrity` and `heatEnergy`
are unsigned `u64` values. `integrity` MUST be positive. `heatEnergy` is explicit so the generated
canonical state and Scenario identity cannot depend on an implicit host default. S1-M1 does not
change it during a Tick.

The Scenario does not contain Main Core ID, anchor ID, or Capacity. IDs and the anchor identity are
deterministically generated. Capacity belongs to the referenced Balance Profile.

The Scenario decoder accepts only these schema/world-kind pairs:

| `schemaVersion` | `initialWorld.kind` |
|---|---|
| `1` | `empty` |
| `2` | `main-core-v1` |

Schema v1 with Main Core or schema v2 with Empty fails during Scenario decoding. Capacity-feature
coherence is deliberately enforced later by `Simulation::new`, preserving the existing artifact
feature-boundary pattern: a strictly decodable v1 Empty Scenario may declare Capacity, but cannot
start a Simulation. Either failure occurs before Tick 0. A raw/envelope-first decode MUST identify
the Scenario schema before decoding its version-specific payload, so an unsupported schema is not
hidden by a payload-shape error from another version.

### 3.2 Position validation

The Main Core position MUST be aligned to `wireGeometryQuantum`. It is not required to be aligned to
`worldRoutingPitch`. An Open World Junction is a placed body and retains its stricter routing-pitch
rule; `MainCoreAnchor` is a distinct fixed endpoint and follows the ordinary Wire geometry quantum.
No snapping or rounding is allowed.

### 3.3 Scenario semantic hashes

The schema-v1 Empty encoder and hash remain byte-for-byte exact:

```text
domain                  ASCII AON\0SCENARIO\0V1\0
encoderVersion          u16 little-endian = 1
schemaVersion           u32 little-endian = 1
common fields           exactly the existing v1 order and encoding
initialWorld tag        u8 = 0 (Empty)
```

Its retained golden hash, the S1-M0 Scenario `ArtifactHash`, and all retained Experiment Run IDs
MUST NOT change.

Schema v2 uses a separate encoder:

```text
domain                  ASCII AON\0SCENARIO\0V2\0
encoderVersion          u16 little-endian = 2
schemaVersion           u32 little-endian = 2
scenarioId              u32 UTF-8 byte length, then bytes
semanticsVersion        u32 UTF-8 byte length, then bytes
hashAlgorithm           u32 UTF-8 byte length, then bytes
initialWorld tag        u8 = 1 (MainCoreV1)
position.x              raw i64 Fixed, little-endian
position.y              raw i64 Fixed, little-endian
integrity               u64 little-endian
heatEnergy              u64 little-endian
requiredFeatures        signal, mobility, capacity, sensing, power,
                        relay, payload, radiation as eight u8 booleans
profile hashes          Numeric, Physical Scale, Balance raw 32-byte hashes
```

Paths and display Profile IDs remain excluded. Every listed v2 field is significant. An independent
encoder and a fixed lowercase BLAKE3 golden MUST prove the v2 hash.

## 4. Main Core and implicit anchor

### 4.1 Exact canonical types

```rust
#[repr(transparent)]
pub struct MainCoreId(pub EntityId);

pub enum TopologyNodeId {
    MainCoreAnchor(MainCoreId),
}

pub struct MainCoreState {
    id: MainCoreId,
    position: FixedVec2,
    capacity: Capacity,
    integrity: Integrity,
    heat_energy: HeatEnergy,
}
```

The private fields have exact read-only getters `id()`, `position()`, `anchor_node()`, `capacity()`,
`integrity()`, and `heat_energy()`. `anchor_node()` returns
`TopologyNodeId::MainCoreAnchor(self.id)`. The anchor is therefore a logical canonical
`MainCoreState` field required by the TRD, but is derived rather than stored as a second independently
mutable value. V5 nevertheless encodes its tag and ID explicitly and validates the relation.

`MainCoreId` is nominally distinct but uses the global Entity ID space. During MainCoreV1 world
generation, the Core is the first allocation: `MainCoreId(EntityId(1))`, registered as
`EntityLocation::MainCore`; the next Entity allocation frontier is `EntityId(2)`.

The anchor is exactly `TopologyNodeId::MainCoreAnchor(core.id)`. It is not a Junction, does not have
a dense Junction slot, and does not consume another Entity ID. Generating a synthetic protected
Junction would create an extra physical body and identity not required by the PRD or SSS.

### 4.2 Physical attachment API

The existing endpoint union adds:

```rust
pub enum EndpointTarget {
    Free,
    Junction(JunctionId),
    GatePort(GatePortRef),
    MobilePort(MobilePortRef),
    MainCoreAnchor(MainCoreId),
}
```

Binding a Wire end to `MainCoreAnchor(id)` is valid only when all of the following hold:

1. the Wire routing domain is `OpenWorld`;
2. `id` resolves to the one live `EntityLocation::MainCore`;
3. the Wire endpoint coordinate equals `MainCoreState.position` exactly;
4. `MainCoreState.anchor_node == TopologyNodeId::MainCoreAnchor(id)`.

The new endpoint has an explicit stable canonical tag and is encoded in Command, Replay, structural
State Hash, topology validation, and immutable observations wherever `EndpointTarget` is encoded.
It is an actual attachment seam now; S1-M1 does not yet compile Body Connectivity or attach Power,
Signal, Sense, or Track behavior to the Core.

### 4.3 Field sources and protection

- `id` and `anchor_node` come from deterministic world generation.
- `position`, `integrity`, and `heat_energy` come exactly from Scenario v2.
- `capacity` is the checked Balance conversion frozen in section 5.
- The Main Core is not a Power Source and creates no Driver, Sink, Power region, or automatic route.
- There is no Main Core placement, duplication, or replication Command.
- `RemoveEntity(core.id)` deterministically returns `UnsupportedCommand` without mutation.
- A malformed duplicate Core, mismatched anchor ID, wrong registry location, or non-first Core ID is
  `InvalidCanonicalState`.
- Main Core destruction and Run termination remain inert until S1-M4; the complete fields are
  stored now because they are already part of the TRD `MainCoreState` contract.

## 5. Capacity units and Balance ownership

Canonical `Capacity(pub u64)` stores nonnegative Fixed-NCU quanta:

```text
Capacity(65_536) = 1 NCU
Capacity(16_384) = 0.25 NCU
```

An active Wire's canonical nonnegative `Fixed` length is converted to `Capacity` by an exact checked
raw-integer conversion; it is not rounded to a whole NCU. The Balance JSON fields
`capacityProbe.mainCoreCapacity`, `relayCapacity`, and `capacityDenominatorFloor` are whole NCU.
They convert by checked multiplication by `FIXED_ONE`. Therefore the alpha Main Core value `1000`
becomes `Capacity(65_536_000)`.

Conversion bounds are validated only when a field is consumed by its owning runtime. S1-M1 checks
`mainCoreCapacity <= u64::MAX / FIXED_ONE` while constructing a capacity-enabled MainCoreV1 world.
An overflow is `SimulationError::NumericOverflow`. It MUST NOT add a global Profile-validation
range restriction to `relayCapacity` or `capacityDenominatorFloor`; those fields retain their
existing Balance-schema-v2 positivity and rational validation until S1-M3/Relay work consumes them.
Likewise an Empty/capacity-disabled package with a previously valid large optional Capacity value
remains decodable and constructible. This preserves Balance schema v2 compatibility.

The existing Balance schema v2 hash encoder continues to include `capacityProbe`. No capacity value
moves into the Physical Scale Profile or Scenario.

Feature/profile/world coherence is exact:

- `requiredFeatures.capacity == true` requires Scenario v2 MainCoreV1 and
  `balance.capacityProbe == Some`;
- Scenario v2 MainCoreV1 requires `requiredFeatures.capacity == true`;
- Scenario v1 Empty requires `requiredFeatures.capacity == false`;
- a `capacityProbe` section by itself remains valid but inert in an Empty/capacity-disabled Stage 0
  package; it MUST NOT create a Core, run Phase 4 accounting, or alter Tick behavior. Its Balance
  hash still enters the contract and therefore the State Hash exactly as it already did;
- Relay fields may be retained and hashed, but Relay capacity is not used in S1-M1.

## 6. Phase 4 Capacity accounting

### 6.1 Timing and derived-state boundary

Phase 0 first applies structural changes. Phase 4 then calculates:

```text
U = sum(wireLength(e) for each alive Wire e)
S = MainCoreCapacity
```

A Wire accepted or removed in the current Tick's Phase 0 is respectively included or excluded in
that Tick's completed report. A Phase-10 pending destruction remains active until the next Phase 0,
when it is removed from `U`.

`U`, `S`, Wire contributions, and Analyzer output are derived Phase 4 scratch/report data. They are
not independent Canonical World truth, are not cached as correctness state, and are excluded from
State Hash. If a revision cache is later added, clear-and-recompute equivalence is mandatory.

S1-M1 does not calculate or expose `E`, support demand, per-region load, Support Heat, or Relay
contributions. Returning zeros for unimplemented S1-M3 quantities would create false observations.

### 6.2 Exact Wire length algorithm

For each alive Wire independently:

1. retain the stored polyline for state identity;
2. collapse only consecutive same-direction collinear segments into maximal runs for length;
3. compute each run as `ceil_isqrt(dx^2 + dy^2)` in raw Fixed units;
4. checked-sum those runs to the Wire's `Capacity` contribution;
5. checked-sum Wire contributions in ascending `WireId` order.

All Wire bodies are included once regardless of routing domain or enabled surface: Open World,
Fixed Substrate, Mobile Substrate, and circuit-internal Wires all count. Gate, Junction, Substrate,
and Main Core bodies do not count. Signal/Power/Sense/Track roles do not create additional charges.

Maximal-run canonicalization is per Wire. S1-M1 MUST NOT merge collinear runs across two Wire
entities and MUST NOT redistribute Fixed-unit length remainders between Wires. The SSS defines
`U` as the sum of per-Wire lengths and requires split invariance only when the split representation
has equal total canonical length.

Consequently diagonal ceil rounding can make two separate Wires one Fixed unit longer than one
direct Wire. With raw routing pitch `P = 16_384`:

```text
(0,0) -> (P,P) direct              = 46_341 Fixed-NCU
(0,0) -> (P/2,P/2)                 = 23_171 Fixed-NCU
(P/2,P/2) -> (P,P)                 = 23_171 Fixed-NCU
two-Wire sum                        = 46_342 Fixed-NCU
```

That difference is canonical, not an accounting defect. C-21's split fixture uses geometry whose
per-Wire lengths total exactly 10 NCU; the reference fixture MUST use axis-aligned segments.

### 6.3 Overflow and atomicity

Every coordinate delta, square, run sum, Wire sum, and Capacity conversion uses checked `i128` or
`u128` intermediates and checked final conversion. Phase 4 overflow returns
`SimulationError::NumericOverflow`. `Simulation::step` operates on its candidate clone, so the
entire Tick is discarded and the prior canonical state remains byte-for-byte unchanged.

Capacity excess never rejects a build, removes a Wire, adds direct Signal delay, or causes a
capacity-specific Damage type.

## 7. Public report and Analyzer shape

The S1-M1 public surface is exactly:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkAccounting {
    used: Capacity,
    supported: Capacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireCapacityUsage {
    wire: WireId,
    length: Capacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MainCoreCapacityContribution {
    main_core: MainCoreId,
    capacity: Capacity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkAnalyzerSnapshot {
    next_tick: Tick,
    accounting: NetworkAccounting,
    main_core_contribution: MainCoreCapacityContribution,
    wires: Vec<WireCapacityUsage>,
}
```

The private fields have copy/read-only getters with the same names as their values:

```rust
NetworkAccounting::used() -> Capacity
NetworkAccounting::supported() -> Capacity
WireCapacityUsage::wire() -> WireId
WireCapacityUsage::length() -> Capacity
MainCoreCapacityContribution::main_core() -> MainCoreId
MainCoreCapacityContribution::capacity() -> Capacity
NetworkAnalyzerSnapshot::next_tick() -> Tick
NetworkAnalyzerSnapshot::accounting() -> NetworkAccounting
NetworkAnalyzerSnapshot::main_core_contribution() -> MainCoreCapacityContribution
NetworkAnalyzerSnapshot::wires() -> &[WireCapacityUsage]
```

`StepReport` adds:

```rust
pub network_accounting: Option<NetworkAccounting>
```

It is `Some` for a valid capacity-enabled MainCoreV1 session and `None` for a retained
capacity-disabled Empty session. `NetworkAnalyzerSnapshot.wires` contains every alive Wire exactly
once in ascending `WireId` order, including zero only if a future valid zero-length Wire law exists;
S1-M1 placement still rejects zero-length segments.

`Simulation::network_analyzer_snapshot(&self) -> Result<Option<NetworkAnalyzerSnapshot>,
SimulationError>` recomputes the same derived result without mutating canonical state, advancing
allocators, emitting events, or changing State Hash. It returns `None` for an Empty session and
`Some` for a MainCoreV1 session. `next_tick` identifies the state observed: immediately after a
successful step it is one greater than that report's `completed_tick`. Repeated reads are identical.
Analyzer and StepReport accounting for the same post-step state MUST be equal.

## 8. State Hash V5 and Replay migration

### 8.1 Global V5 policy

State Hash version is a schema identifier, not a feature flag. Every S1-M1 engine session uses
`aon-state-v5`, including Empty/capacity-disabled sessions. There is no runtime choice where Empty
uses V4 and Main Core uses V5. This follows the retained V3-to-V4 precedent: Replay format remained
v1 while one global state encoder advanced for a newly representable canonical store.

V3 and V4 identifiers remain strictly decodable as header values. A current V5 session rejects a
Replay declaring V3 or V4 before Tick 0 with expected `aon-state-v5`; it does not reinterpret an old
checkpoint under V5. Retained Stage 0 and S1-M0 Replay fixtures are regenerated from their
authoritative initial packages and Command streams with V5 headers/checkpoints. Their Scenario v1
hashes, experiment inputs, Module/Design hashes, and Experiment Run IDs do not change.

### 8.2 Exact V5 field order

The encoder is:

```text
domain                         ASCII AON\0STATE\0V5\0
encoderVersion                 u16 little-endian = 5
semanticsVersion tag           u8
numericProfileHash             raw 32 bytes
physicalScaleProfileHash       raw 32 bytes
balanceProfileHash             raw 32 bytes
nextTick                       u64 little-endian
topologyRevision               u64 little-endian
EntityRegistry frontier/count/slots
Main Core presence             u8
if present:
  Main Core EntityId           u64 little-endian
  position.x                   raw i64 Fixed, little-endian
  position.y                   raw i64 Fixed, little-endian
  anchor kind                  u8 = MainCoreAnchor tag
  anchor MainCore EntityId     u64 little-endian
  capacity                     raw Fixed-NCU u64, little-endian
  integrity                    u64 little-endian
  heatEnergy                   u64 little-endian
existing structural stores     existing V4 order, with the new endpoint tag where present
existing signal stores         existing V4 order
event payload frontier         existing V4 order
Driver events                  existing V4 order
Signal events                  existing V4 order
reserved destruction store     existing zero/count encoding
reserved radiation store       existing zero/count encoding
reserved Relay store           existing zero/count encoding
Path Certificates              existing V4 order
```

The EntityRegistry already assigns the stable `MainCore` kind tag; V5 additionally encodes the Core
store fields so registry identity cannot alias different Core state. `used`, `supported`, Wire
contributions, reports, Analyzer snapshots, caches, and presentation state are excluded.

An Empty V5 session writes `Main Core presence = 0` and otherwise retains the exact listed V5
order. MainCoreV1 writes presence `1`; any present/missing Core disagreement with the registry,
anchor, feature set, or world generator is invalid canonical state.

### 8.3 Replay world generator

`Simulation::replay_header()` selects:

- `aon-empty-v1` for Scenario v1 Empty;
- `aon-main-core-v1` for Scenario v2 MainCoreV1.

`aon-empty-v1` retains the zero-Seed/no-random-draw contract. `aon-main-core-v1` also consumes no
random draw and requires `Seed::ZERO`; its complete output is determined by the decoded v2 initial
world plus validated Profile bundle. The Replay header stores `aon-state-v5` in both cases.

Replay validation retains format-v1 precedence: body shape first, then Header fields in printed
order, then `next_tick == 0`, then exact initial State Hash. A generator/version mismatch fails
before any Command executes.

### 8.4 Migration fixtures

The retained suite contains:

1. an independent Empty V5 encoder and fixed initial-hash golden;
2. an independent MainCoreV1 V5 encoder and fixed initial-hash golden;
3. single-field sensitivity for Core presence, ID, both position coordinates, anchor tag/ID,
   Capacity, Integrity, and Heat Energy;
4. a malformed Core/anchor/registry fixture rejected before hashing or stepping;
5. regenerated Stage 0 and S1-M0 Replay v1 fixtures declaring V5;
6. at least one decode-only V4 Replay fixture that is rejected by a V5 session before Tick 0;
7. EmptyV1 and MainCoreV1 generator-header fixtures;
8. retained exact Scenario v1 hash and S1-M0 first/last Run ID goldens;
9. a new independent Scenario v2 hash golden and capacity Replay golden;
10. per-Tick Headless and Bevy State Hash/report equality for identical inputs.

## 9. Strict error precedence

### 9.1 Artifact/package decode

When multiple faults exist, Scenario/package decoding returns the first class in this order:

1. Scenario JSON syntax, EOF, trailing data, top-level type, or missing/invalid `schemaVersion` needed
   to read the minimal envelope;
2. unsupported Scenario schema;
3. selected-version strict full JSON shape, including duplicate/unknown field, field type, and float
   rejection;
4. empty `scenarioId`, unsupported Semantics Version, and unsupported hash algorithm, in that order;
5. selected-version InitialWorld semantic invariants, including schema/world-kind pairing and
   positive Integrity;
6. malformed Scenario Profile references and hashes in Numeric, Physical Scale, Balance order;
7. Numeric Profile strict decode and validation;
8. Physical Scale Profile strict decode and validation;
9. Balance Profile strict decode and existing schema-v2 validation; no S1-M1-only conversion bound
   is imposed on an unconsumed optional field;
10. referenced Profile ID mismatch in Numeric, Physical Scale, Balance order.

Contract hash comparison occurs in `Simulation::new`, because `decode_package` retains declared
hashes in the `SimulationContract`. No error returns a partial package.

### 9.2 Simulation construction

For direct or artifact-backed `SimulationPackage`, construction preserves the existing unsupported
feature precedence and validates in this order:

1. unsupported enabled features in `sensing`, `power`, `relay`, `payload`, `radiation` order;
2. Profile bundle validity in Numeric, Physical Scale, Balance order;
3. contract Profile hashes in Numeric, Physical Scale, Balance order;
4. InitialWorld/Capacity-feature pairing;
5. required `capacityProbe` presence;
6. direct-package Main Core Integrity and position-quantum validation;
7. checked `mainCoreCapacity` whole-NCU conversion when the active MainCoreV1 world consumes it;
8. deterministic identity, anchor, registry, and canonical-world links.

Capacity is no longer in the unsupported-feature list. Signal and Mobility remain supported Stage 0
features. A compound construction fixture with an unsupported feature plus invalid Profiles and a
world/Capacity mismatch MUST report the first unsupported feature. Further compound fixtures remove
that fault in turn and prove Profile validity, hash, and triad ordering. This preserves the Stage 0
observable precedence; S1-M1 MUST NOT silently reorder it.

The exact public typed additions are:

```text
PackageError::UnsupportedInitialWorld { schema_version, initial_world }
PackageError::NonPositiveInitialWorldField { field }
SimulationError::CapacityRequiresMainCore
SimulationError::MainCoreRequiresCapacity
SimulationError::CapacityRequiresProfile
SimulationError::InvalidMainCoreGeometryQuantum
SimulationError::InvalidMainCoreIntegrity
```

Exact payload fields MUST identify the version, feature, or field without embedding unstable host
paths. Runtime Phase 4 overflow remains `SimulationError::NumericOverflow`, not a Command
rejection.

## 10. C-21 retained fixture

C-21 is realized with already flattened primitive Wire bodies; runtime Module placement remains out
of scope. Its evidence is deliberately divided between one retained Replay and focused conformance
tests so mutually overlapping direct/split layouts do not have to coexist in one Structural World:

1. MainCoreV1 starts with a selected capacity greater than or equal to 12 NCU;
2. the retained capacity Replay builds a 10-wu path from four separate Wire entities joined through
   ordinary Open World Junctions;
3. that Replay adds one 2-wu Wire in a Fixed Substrate routing domain to represent an internal
   circuit Wire;
4. a focused conformance test separately builds one 10-wu Open World Wire and proves that accounting
   iterates that physical body once. Signal, Power, Sense, and Track are semantic roles on that same
   body; the test does not enable the S1-M2 Power/Sense runtimes or create four projected bodies.

Expected completed Phase 4 observations are exact:

```text
focused multi-role one-body test      U = 10 NCU
retained Replay four-Wire split       U = 10 NCU
retained Replay split + internal      U = 12 NCU
S                                     = selected Main Core Capacity
```

Each checkpoint asserts `StepReport.network_accounting`, sorted Analyzer Wire records, State Hash
V5, and Replay restart equality. The internal Wire counts because it is a physical Wire, not because
a runtime Module entity exists.

## 11. Executable completion gates

### Gate 1 — retained regressions and identity

- Full Stage 0 and S1-M0 suites pass.
- The retained Scenario v1 semantic hash, Module/Design hashes, and all S1-M0 Run IDs are exact.
- Only State Hash/Replay checkpoint values migrate to V5.

### Gate 2 — strict Scenario contracts

- v1 accepts only Empty/capacity-disabled; v2 accepts only MainCoreV1/capacity-enabled.
- Unknown, duplicate, cross-version, float, overflow, zero Integrity, and feature mismatch inputs
  fail closed with the frozen precedence.
- A compound fixture with an unsupported schema plus a malformed version-specific InitialWorld
  reports `UnsupportedSchema`, proving raw/envelope-first dispatch rather than one-shot payload
  decoding.
- Independent v1 and v2 encoders prove their exact retained/new hash goldens.

### Gate 3 — Main Core initialization and protection

- MainCoreV1 creates exactly EntityId 1 and leaves frontier 2.
- Every field and implicit-anchor relation is exact; position requires only geometry-quantum
  alignment.
- Removal, placement, duplication, replication, and malformed identity cases are rejected without
  mutation.
- The Core creates no Power or Signal behavior.

### Gate 4 — endpoint attachment

- Open World Wires bind to the exact live Core position through `MainCoreAnchor`.
- wrong ID, wrong position, wrong domain, removed/non-Core registry entries, and mismatched anchors
  reject deterministically.
- Command, Replay, State Hash, and topology encoders independently cover the new endpoint tag.

### Gate 5 — Capacity units and coherence

- fractional Wire lengths remain exact Fixed-NCU values.
- alpha `1000` converts exactly to raw `65_536_000`.
- active Main Core conversion boundary and `NumericOverflow` tests are atomic.
- large unconsumed Relay/floor fields do not acquire a premature S1-M1 range restriction.
- a disabled `capacityProbe` package remains behaviorally and hash noninterfering except for its
  already-declared Balance contract hash.

### Gate 6 — Phase timing and derived-only behavior

- same-Tick Phase 0 Wire placement/removal is reflected in Phase 4 and the completed report.
- accounting never enters Canonical State Hash and repeated Analyzer reads do not mutate state.
- clear/recompute, store-layout permutation, and equivalent-command-order tests agree.

### Gate 7 — C-21

- multi-role usage is exactly 10 NCU.
- the retained four-Wire split usage is exactly 10 NCU.
- adding the 2-NCU internal Wire produces exactly 12 NCU.
- no role surface is counted twice and every routing domain is covered.

### Gate 8 — length and rounding laws

- redundant same-direction vertices inside one Wire preserve length.
- bends remain separate maximal runs.
- per-Wire contributions and global accumulation are WireId-stable and checked.
- the diagonal `46_341` direct versus `46_342` two-Wire golden proves that no cross-Wire merge or
  remainder redistribution exists.
- property tests cover equal-total split representations and numeric boundaries.

### Gate 9 — reports and Analyzer

- capacity sessions return `Some(U,S)`; retained Empty sessions return `None`.
- Analyzer records are complete, unique, WireId-sorted, and sum exactly to report `U`.
- no S1-M3 field is exposed with a placeholder value.

### Gate 10 — V5 and Replay migration

- independent Empty and Main Core V5 encoders prove exact order and goldens.
- all Core fields and MainCoreAnchor endpoints have sensitivity tests; all derived values are proven
  excluded.
- current Replays use format v1 + V5 + the matching generator; V3/V4 sessions fail before Tick 0.
- regenerated retained Replays reproduce their authoritative Command streams exactly.

### Gate 11 — host equivalence and presentation isolation

- Headless and Bevy hosts produce identical per-Tick reports and V5 hashes.
- rendering, probes, selection, Analyzer reads, and enabled/disabled presentation paths cannot
  mutate accounting or canonical state.

### Gate 12 — negative scope

- no sensing, Power, Brownout, Relay, Excess/support formula, Heat, Construction, Damage, Core
  run-end, capacity build rejection, or direct capacity delay behavior appears in S1-M1.
- C-07/C-08/C-22/C-09/C-10 remain failing/not-run milestone requirements, not fake passes.

### Gate 13 — Windows-native fail-closed evidence

From a fresh Windows-native `git clone --no-local`, with no WSL use:

```powershell
cargo metadata --locked --offline --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline -- --test-threads=1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\stage0-technical-gate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\s1-m0-technical-gate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\s1-m1-technical-gate.ps1
```

The S1-M1 gate covers every gate above, retains exact counts/goldens, fails on skipped required
evidence, and leaves `git status --short` empty in the verification clone.

## 12. Closure boundary

S1-M1 may be marked complete only after all thirteen gates pass on the committed tree and the fresh
Windows-native clone. Its closure record MUST name the implementation commit, exact Scenario v2 and
State V5 goldens, C-21 report values, suite/gate counts, and clean-clone evidence.

That closure advances the tracker only for S1-M1. S1-M2 through S1-M6 and both Stage 1 gates remain
open.
