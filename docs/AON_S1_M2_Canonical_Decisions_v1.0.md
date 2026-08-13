# A/O/N — S1-M2 Canonical Decisions

**Status:** implementation authority
**Applies to:** `S1-M2 — Sensing / Power / Brownout`
**Source baseline:** PRD v1.0 GO Candidate / SSS v1.0 Draft / TRD v1.0 Draft /
S1-M1 Canonical Decisions

This document freezes only the representation, arithmetic, phase ownership, artifact migration, and
evidence choices needed to implement S1-M2. It is subordinate to the source documents in their
respective areas of authority:

- the PRD determines the product question, scope, and product invariants;
- the SSS determines observable World behavior, phase timing, numeric laws, and contract ownership;
- the TRD determines data structures, encoders, APIs, ordering, and test structure;
- the S1-M1 authority freezes the retained Main Core, Capacity, Scenario v1/v2, and State V5
  baseline;
- this document closes only gaps required by S1-M2.

The coefficients frozen below are transparent conformance constants. They are not a claim that the
Stage 1 product optimum has been found. S1-M6 still owns the parameter sweep and both Stage 1 gates
remain open after S1-M2.

## 0. Source authority map

| Subject | Authority lines |
|---|---|
| Document responsibility and conflict priority | SSS lines 53-75; TRD lines 14-35 |
| Absolute invariants and deterministic numeric domain | SSS lines 150-169 and 261-306; TRD lines 391-451 and 504-525 |
| Pre-existing Power Sources and Enemies | PRD lines 234-248 |
| Stage 1 product boundary | PRD lines 1465-1493 |
| Tick phase ownership | SSS lines 470-620; TRD lines 2292-2514 |
| Wire Body/surfaces, Sense ports, and Junction connectivity | SSS lines 761-849; TRD lines 1400-1456 and 1694-1792 |
| Gate Power, delay, drive, strength response, and retention | SSS lines 944-1057; TRD lines 2635-2715 |
| Power flow, demands, route, loss, common ratio, and grants | SSS lines 1466-1569; TRD lines 3186-3356 |
| Sensing geometry, information boundary, delay, and passive LOW | SSS lines 1603-1640; TRD lines 3503-3542 |
| Movement scaling and Power boundary | SSS lines 1702-1709 and 1733-1737; TRD lines 3436-3444 and 3472-3481 |
| Exact C-07 and C-08 observations | SSS lines 2499-2509 |
| Canonical/cache/scratch split | TRD lines 432-451 and 1278-1320 |
| Replay World inputs and deterministic hash boundary | SSS lines 2239-2285; TRD lines 4641-4717 |
| Read-only reports and analyzers | TRD lines 4289-4330 and 4430-4616 |
| S1-M2 exact ownership | TRD lines 5361-5377 |
| S1-M3 support-load ownership | TRD lines 5379-5396 |
| S1-M4 Construction/Enemy/Contact/Damage ownership | TRD lines 5398-5416 |
| Current tracker boundary | Tracker lines 27-40 |
| Retained S1-M1 versions and closure | S1-M1 authority lines 79-100 and 717-750 |

The end-state TRD names Construction and Enemy stores, but its milestone table assigns those
runtimes to S1-M4. S1-M2 therefore uses a typed replay World-input snapshot for hostile geometry and
a pure Work-grant seam. It MUST NOT create a fake Enemy or Construction entity merely to make C-07
or C-08 appear complete.

## 1. Scope and milestone ownership

S1-M2 owns:

- an ordered, reconstructible spatial index and deterministic capsule/circle narrow phase;
- one-bit, segment-local Wire sensing through two identical read-only Sense outputs;
- Phase 1 hostile snapshots and Phase 6 delayed Sense Driver transitions;
- static scenario Power Sources, Power attachments, Power graph/regions, canonical source routes,
  nominal demands, the common-region `rho` solver, and `PowerGrant` records;
- Gate delay/drive/retention, Sense drive, and Movement grant integration;
- a pure Construction Work scaling seam, without Construction state;
- derived Phase 8 leakage and transmission `HeatContribution` observations, without thermal state;
- C-07 and C-08 and all artifact, schema, Replay, State Hash, error, and migration work needed by
  those conformance tests.

S1-M2 does not implement:

- Excess `E`, the soft support curve, Overcapacity Support Demand/Heat, Relay demand, Relay Capacity,
  or C-22; those remain S1-M3;
- Construction Site, required Work by primitive, BUILD/progress/activation, Enemy canonical state or
  movement, Live Wire demand, Contact, thermal integration, Damage, destruction, Main Core run end,
  C-09, or C-10; those remain S1-M4;
- runtime Power Source placement/removal/replication or player-selectable Wire surfaces;
- a Sensor Health bit, hostile count/identity/type/position/distance/direction/velocity/HP/target
  output, Junction Sense OR, batteries, inertia, or priority load shedding;
- Support Heat, accumulated temperature/heat state, thermal delay/drive feedback, or thermal damage;
- S1-M5 reference architectures, S1-M6 tuning, or either complete Stage 1 gate.

Passing S1-M2 closes only C-07 and C-08 under the exact boundary below. The full technical gate still
requires C-09, C-10, C-21, C-22, every Stage 0 regression, and later milestone evidence.

## 2. Frozen versions

| Contract or artifact | S1-M2 value |
|---|---|
| Semantics Version | `aon-semantics-v1` |
| Scenario schema | v1 Empty retained; v2 MainCoreV1 retained; v3 MainCorePowerV1 added |
| Scenario semantic hash encoder | v1/v2 retained; v3 added |
| Numeric Profile schema | `1` |
| Physical Scale Profile schema | `1` |
| Balance Profile schema | v2 retained; v3 Power probe added |
| Profile canonical encoder | `1` retained |
| Command canonical encoder | `1` retained; new endpoint tags append without changing retained bytes |
| Canonical State Hash | `aon-state-v6` for every new session |
| Replay format | v1 retained for decode; v2 current |
| Empty World generator | `aon-empty-v1` retained |
| Main Core World generator | `aon-main-core-v1` retained |
| Main Core + Power World generator | `aon-main-core-power-v1` added |

SSS v1 already specifies sensing, Power flow, Brownout, and C-07/C-08, so implementing those laws
does not change the Semantics Version. Balance v3 is necessary because the exact M2 coefficients are
new contract inputs. The canonical Profile stream already encodes the schema tag; its domain and
encoder remain V1, and all Balance v2 bytes and hashes remain exact.

Replay v1 explicitly rejects every nonempty `worldInputs` array. Relaxing that behavior under the
same format would silently reinterpret old artifacts, so typed hostile input requires Replay v2.
Power Sources, Sense Drivers, sampled presence, gate `unpowered_ticks`, and pending Sense transitions
are new canonical state, so the State encoder advances globally to V6.

## 3. Scenario v3 and static Power Sources

### 3.1 Exact InitialWorld shape

Scenario v3 supports exactly this new world kind:

```json
"initialWorld": {
  "kind": "main-core-power-v1",
  "mainCore": {
    "position": { "x": 0, "y": 0 },
    "integrity": 1000,
    "heatEnergy": 0
  },
  "powerSources": [
    {
      "position": { "x": 655360, "y": 0 },
      "generationPerTick": 12
    }
  ]
}
```

Coordinates are raw signed `i64` Fixed values. Core `integrity` is positive `u64`; Core
`heatEnergy` is `u64`; each `generationPerTick` is positive `u64 Energy`. The Core fields retain
their v2 meaning. A Power Source has no implicit Integrity or Heat field in S1-M2 because those
values cannot change before S1-M4; adding placeholder state would violate the no-fake-results rule.

The v3 decoder:

1. validates the envelope/schema before its version-specific payload;
2. requires exactly `main-core-power-v1`; `powerSources` may be empty;
3. validates every position against `wireGeometryQuantum`, with no snap or rounding;
4. requires distinct Source positions; sharing the Main Core point is allowed because the Core is a
   separate non-Power body/anchor;
5. canonical-sorts Sources by `(position.x, position.y, generationPerTick)` before generation;
6. allocates Core `EntityId(1)`, then Sources in that order as `EntityId(2..)`;
7. requires Capacity, Power, and Sensing features enabled, and later-stage features disabled.

Duplicate positions are rejected even if Generation differs. Input array order therefore cannot
choose Source identity. The Core remains only a Capacity/network anchor and MUST NOT generate Power.
There is no Source placement, removal, duplication, replication, damage, or generation-update
Command in S1-M2.

The strict schema/world matrix is:

| Scenario schema | Allowed InitialWorld | Current feature coherence |
|---|---|---|
| v1 | `empty` | Capacity/Sensing/Power disabled |
| v2 | `main-core-v1` | Capacity enabled; Sensing/Power disabled |
| v3 | `main-core-power-v1` | Capacity/Sensing/Power enabled |

### 3.2 Power Source canonical state and anchor

```rust
#[repr(transparent)]
pub struct PowerSourceId(pub EntityId);

pub struct PowerSourceState {
    id: PowerSourceId,
    position: FixedVec2,
    generation_per_tick: Energy,
}

pub enum EndpointTarget {
    // retained tags 0..=4
    PowerSourceAnchor(PowerSourceId), // tag 5
    WireSensePort(WireSensePortRef),  // tag 6
}
```

`PowerSourceAnchor` is a Power-only attachment at the exact Source position and is the sole explicit
Power bridge across routing domains. The same `PowerSourceAnchor(PowerSourceId)` node key may be
referenced by Open World, FixedSubstrate, and MobileSubstrate Wires, so those otherwise isolated
Power surfaces union at that Source node. In a substrate domain the endpoint must additionally lie
inside that substrate's routing area under the retained inclusive containment rule. It is not a
Signal node, Sense node, Track Junction, or body-connectivity source. A Wire endpoint bound to it
joins the Power graph only; its other surface views terminate there. The Source anchor and store
relation, registry kind, sorted IDs, positions, and generations are canonical and V6-sensitive.

Removing or placing a Power Source is `UnsupportedCommand`. A missing Source, wrong registry kind,
duplicate ID/position, mismatched anchor, non-sorted generator allocation, or nonpositive generation
is `InvalidCanonicalState` after construction. Ordinary command references to an unknown, removed,
wrong-kind, wrong-position, or wrong-domain Source anchor reject without mutation using the existing
endpoint-rejection taxonomy.

### 3.3 Scenario v3 semantic encoder

```text
domain                       ASCII AON\0SCENARIO\0V3\0
encoderVersion               u16 little-endian = 3
schemaVersion                u32 little-endian = 3
common fields                same semantic order as v2
initialWorld tag             u8 = 2 (MainCorePowerV1)
mainCore fields              v2 position/integrity/heat order
powerSource count            u32 little-endian
each sorted Source           position.x i64, position.y i64, generation u64
requiredFeatures             retained eight booleans in v2 order
profile hashes               Numeric, Physical, Balance raw 32-byte hashes
```

Paths and display Profile IDs remain excluded. Independent v1, v2, and v3 encoders must prove the
retained/new lowercase BLAKE3 goldens and array-permutation equivalence.

## 4. Balance v3 Power probe

Balance v3 requires exactly one `powerProbe`; Balance v2 forbids it. Generation is Scenario state,
not a Profile coefficient.

```json
"powerProbe": {
  "gateIdleDemand": 1,
  "gateDriveDemand": 1,
  "gateSwitchDemandPerEnergy": { "numerator": 1, "denominator": 1 },
  "wireLeakagePerWU": { "numerator": 1, "denominator": 1 },
  "wireSenseDemandPerWU": { "numerator": 1, "denominator": 1 },
  "movementDemandPerWU": { "numerator": 1, "denominator": 1 },
  "powerLossK": { "numerator": 0, "denominator": 1 },
  "senseNominalDrive": 400,
  "gateStateRetentionTicks": 3
}
```

The first two, `senseNominalDrive`, and `gateStateRetentionTicks` are positive `u64`.
`gateSwitchDemandPerEnergy`, `wireLeakagePerWU`, `wireSenseDemandPerWU`, and
`movementDemandPerWU` are positive reduced `Rational`; `powerLossK` is nonnegative. Denominators are
positive. Existing `logicThreshold=100`, `nominalGateDrive=400`,
`logicOperateThreshold=1/5`, `brownoutDelayFloor=1/5`, `senseDelay=1`, and
`senseRadius=1.25 wu` retain their Balance meaning.

Balance v3 appends the nine fields above after the retained v2 canonical bytes in displayed order;
there is no optional-presence byte because schema v3 requires the section. Every field has a
single-field hash-sensitivity test. The unit coefficients intentionally make conformance arithmetic
visible; S1-M6, not M2, decides product-optimal tuning. A separate nonzero-`powerLossK` in-memory
fixture proves route loss and heat without changing the reference artifact.

## 5. Replay v2 hostile input

### 5.1 Exact typed event

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldInputEvent {
    HostileFrame {
        target_tick: Tick,
        hostiles: Vec<HostileCircle>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostileCircle {
    pub id: u64,
    pub center: FixedVec2,
    pub radius: Fixed,
}
```

Replay JSON uses the existing tagged-body convention: `type: "hostile-frame"`, camelCase
`targetTick`, and a `hostiles` array containing `id`, `center`, and raw Fixed `radius`. IDs are
World-input-local observation identities, not global Entity IDs and not entries in `EnemyStore`.
Hostile ID zero is reserved and rejected. Radius is nonnegative and every checked capsule/circle
intermediate must fit the numeric policy.

Each `HostileFrame` is the complete hostile collider snapshot for exactly `targetTick`. It does not
persist. A missing frame means an empty snapshot. Exactly zero or one frame is allowed per executed
Tick; duplicate ticks and duplicate IDs in one frame reject. Hostiles are normalized by ascending
ID; permutation does not affect sensing, Replay identity, or State Hash trace. An explicit empty
frame remains a valid round-trippable Replay record, while its simulation effect is identical to an
omitted frame; no persistent hostile state is created by either representation.

All World inputs must target a Tick before the Replay's final `nextTick`. Replay v2 normalizes frames
by `targetTick`, then Hostile ID. Unknown event kinds, unknown/duplicate fields, floats, negative
radius, duplicate Tick/ID, overflow, and out-of-run events fail before Tick 0. Replay v1 continues to
reject every nonempty `worldInputs` array with its retained error.

### 5.2 Step boundary and ownership

```rust
pub fn step(
    &mut self,
    commands: &[CommandEnvelope],
) -> Result<StepReport, SimulationError>;

pub fn step_with_world_inputs(
    &mut self,
    commands: &[CommandEnvelope],
    world_inputs: &[WorldInputEvent],
) -> Result<StepReport, SimulationError>;
```

The retained one-input `step` delegates to `step_with_world_inputs(commands, &[])`; Rust API
overloading is not assumed. Replay execution selects only the current Tick's frame and supplies it
to `step_with_world_inputs`. The frame is
validated and fixed before cloning/mutating the CandidateWorld, then enters Phase 1 immediately
after Phase 0 topology commit. Phase 1 copies the sorted circles into immutable scratch; no later
phase may mutate or resample them. Movement committed in Phase 11 is therefore visible to sensing
only through the next Tick's World input, as required by the snapshot law.

World-input frames are deterministic inputs, not canonical state. They are in Replay v2 but not V6.
Their observable effects—sampled Wire presence and scheduled Driver events—are canonical and hashed.
A State checkpoint cannot resume without the remaining Replay input log, matching the existing
Command-input model.

## 6. Wire sensing and isolated Sense ports

### 6.1 Geometry and spatial ordering

Every straight pair in the canonical polyline contributes a closed capsule with radius
`senseRadius`. A Wire's presence is HIGH iff any hostile circle intersects any of its capsules;
otherwise it is LOW. Tangency counts as intersection. The narrow phase uses squared checked integer
distance and MUST NOT use floating point or square roots.

The spatial index is a reconstructible cache excluded from Replay/State Hash. The initial
implementation uses ordered sparse chunk keys. It collects candidate Hostile IDs, sorts/deduplicates
them, then performs WireId/segment-index/Hostile-ID narrow phase. Hash-map bucket order is never an
observable order. A brute-force independent oracle, insertion permutation tests, and clear/rebuild
equivalence must match exactly, including negative cell coordinates, cell boundaries, tangency,
zero-radius circles, bends, and multiple capsules hitting the same circle.

Presence reveals only one bit. Count `1` and count `3` are indistinguishable, and neither the Driver,
Sink, report, analyzer, nor render snapshot may expose the prohibited hostile data listed in section
1.

### 6.2 Sense driver identity and port isolation

```rust
pub struct WireSensePortRef {
    pub wire: WireId,
    pub end: WireEnd,
}

pub struct WireSenseState {
    pub driver_a: DriverId,
    pub driver_b: DriverId,
    pub sampled_presence: bool,
    pub last_intended_level: LogicLevel,
    pub last_intended_strength: SignalStrength,
}
```

Each active Wire allocates exactly two Drivers, in A then B order, after the Wire Entity is
allocated. `DriverRole::WireSenseA` and `WireSenseB` append stable role tags. Both Drivers begin
`LOW/strength 0/initial revision`; `sampled_presence` begins `false`. Both observe the same level and
effective strength, but they are independent Driver identities and independent fan-out sources.
The two `last_intended_*` fields also begin LOW/zero and describe the most recent pair enqueued by
Phase 6, not merely the currently active Driver sample.

`EndpointTarget::WireSensePort({wire,end})` refers to the selected Driver's own Signal node. It does
not refer to the target Wire's main Signal surface, its Power surface, or the other Sense Driver.
Sense nodes never union through a Junction. Attaching both Drivers to one owner Signal node would
sum their strengths and is forbidden.

A Sense binding requires a live different target Wire, the same routing domain, and an endpoint
coordinate exactly equal to the referenced target Wire endpoint. Binding a Wire endpoint to its own
Sense port, binding both ends of one Wire to the same Sense node, or a wrong/removing target is
rejected. Removing a sensed target Wire detaches all inbound Sense references to `Free` in ascending
referencing WireId/end order in the same Phase 0 transaction; it never leaves a dangling port.

The endpoint target is one physical binding with purpose-specific projections. For a
`WireSensePort`, only the binding Wire's Signal projection joins the Sense Driver node; its Power and
Track projections terminate. For `PowerSourceAnchor` and `GatePort::Power`, only Power joins. These
rules avoid accidental cross-surface connectivity without introducing four player-selectable Wire
bodies.

### 6.3 Sample, delay, power, and event law

Phase 1 computes occupancy from that Tick's immutable frame. Phase 3 records the Sense sample intent.
Phase 5 supplies the sensed Wire's Power ratio. Phase 6 computes:

```text
senseStrength = RNE(senseNominalDrive * rhoRaw / FIXED_ONE)
```

If either intended level or effective strength differs from `last_intended_*`, Phase 6 updates that
pair and schedules A and B `DriverTransition` events at `t + senseDelay`. A/B share the same due Tick,
level, and strength but consume distinct stable Event payload identities. Sense delay is transport,
not Gate-style inertial supersession: a later sample MUST NOT invalidate an earlier queued sample.
Every changed sample becomes due in `(dueTick, WireId, A-before-B, EventId)` order, so a one-Tick HIGH
pulse remains a one-Tick HIGH pulse after any fixed `senseDelay`. If the event format requires a
generation token, it is a Wire/Driver-lifecycle token only; it is not advanced per sample.

On due Phase 2 application, each actual Driver sample change advances that Driver's own Revision and
uses the ordinary current-topology Signal route and Wire transport delay. A level-preserving strength
change is a real sample. At a Sink, strength below `logicThreshold` resolves as passive LOW. There is
no Sensor Health bit. The exact active/scheduled target needed for deterministic replacement,
sampled presence, last intended pair, Driver samples/revisions, pending events, and downstream slots are
V6 canonical state.

## 7. Power graph, regions, and demand attachment

### 7.1 Compiled graph boundary

Power nodes and edges are compiled from active physical state after Phase 0:

- each Wire is a bidirectional edge carrying its exact canonical Euclidean polyline length and
  straight segment count;
- incident Wire Power surfaces union at a live Junction in the same routing domain;
- `GatePort::Power` is one device node and never a Signal Sink;
- `PowerSourceAnchor` is one Source node across all routing domains and is the only routing-domain
  bridge in the Power graph; every other endpoint union remains domain-local;
- Free, Main Core, Signal Gate ports, Mobile control ports, and Sense ports terminate the Power
  projection rather than joining it.

A connected component is a `PowerRegion`. Region IDs, membership, Source routes, ratios, demands,
grants, and solver scratch are derived cache/scratch, not independent canonical truth. Cache clear
and rebuild must yield identical ordered records and reports. A region with no Source has `G=0` and
`rho=0`. Otherwise `G` is the checked sum of every Source generation in Source EntityId order.

### 7.2 Virtual intrinsic Wire attachment

Wire leakage and sensing are orientation-neutral intrinsic loads attached at a derived
`PowerNodeKey::WireBody(WireId)` at one half of total canonical arclength `L`. The compiler replaces
the conceptual whole edge with two pseudo-edges from this virtual node to the physical endpoint
nodes; their lengths sum exactly to `L`. Pseudo-edges carry no independently counted route-segment
metric. Segment count is calculated later from the coalesced physical-Wire traversal.

The two incident semantic endpoint descriptors are ordered by:

```text
(EndpointTarget canonical tag, referenced EntityId, referenced port/end tag,
 endpoint position.x, endpoint position.y)
```

The pseudo-edge incident to the lower descriptor has raw length `floor(L.raw / 2)`; the pseudo-edge
incident to the higher descriptor has raw length `ceil(L.raw / 2)`. Thus the derived attachment may
be one raw Fixed unit nearer the lower descriptor when `L.raw` is odd. Distinct physical endpoints
guarantee a strict final position tie-break for a valid Wire; a relation still indistinguishable
after the full descriptor is `InvalidCanonicalState`.
Swapping stored A/B orientation together with its semantic targets and geometry therefore cannot
change region, route, loss, or heat. If `floor(L.raw/2) == 0`, the compiler coalesces the virtual
node with that endpoint and omits the zero-length pseudo-edge. Public `PowerRouteWire` records remain
strictly positive; zero-length derived edges never enter route identity, loss, or heat.

For a polyline, the virtual point is the exact arclength split, not the coordinate midpoint. A route
coalesces all traversed pieces of the same physical Wire into one positive `PowerRouteWire` whose
length is the exact arclength actually traversed. Its segment metric is the count of original
canonical straight segments with positive overlap with that traversal; the segment containing a
virtual attachment is counted once, not once per pseudo-edge. Ordered EntityId route identity names
the physical Wire once, and Capacity never counts the split.

### 7.3 Mobile attachment

Movement demand does not attach at the Wire midpoint. For each Mobile, Phase 4 attaches its demand
to its immutable Phase 1 tick-start `TrackPosition` on the occupied Track Wire. The derived load node
has exact distances `offset` and `L - offset` to the headed Wire's canonical endpoints. A zero side
at an exact endpoint is coalesced and omitted. At a Junction-position state, offset is the exact
incoming-edge endpoint. Routes emit only the positive partial arclength actually traversed for that
physical Wire. Invalid edge/offset/heading or a Track/Power projection mismatch is
`InvalidCanonicalState`.

Phase 6 scales the budget for the current attachment. Phase 7 accepts the derived set of powered
Track edges, may traverse only members of that set, and stops exactly at the first unpowered edge
boundary. For this boundary only, `powered` means the Track Wire's common region ratio is strictly
greater than zero; the Gate-only `logicOperateThreshold` is not reused. The ratio scales the budget
exactly once at the Tick-start attachment and is not applied again on each entered edge. There is no
coast, inertia, or battery.

M2 cannot construct a connected powered-to-unpowered boundary through the public Simulation graph:
Track adjacency requires a shared Junction, and that same Junction participates in Power, so all
incident Track Wires belong to one Power component and share one `rho`. The exact Phase 7 boundary
algorithm is nevertheless mandatory and is proven at the derived TrackGraph seam by
`mobility::tests::powered_movement_stops_at_an_unpowered_junction_edge_boundary`. Full-Simulation
evidence with different adjacent-edge ratios is deferred until a later canonical version introduces
an explicit Power-disconnect or Power-switch topology mechanism. That later version must add the
integration evidence; M2 must not fake it with coincident geometry, adapter-only regions, or a change
to Junction Power participation.

### 7.4 Nominal M2 demands and IDs

`DemandId` is a derived, stable composite `(owner EntityId, DemandKind canonical tag)`, not an
allocation from the global Entity space. Records sort by that key. The end-state TRD tag order is
retained; M2 instantiates only:

- `GateIdle = gateIdleDemand` for every active Gate;
- `GateDrive = gateDriveDemand` when its output has a reachable load;
- `GateSwitch = ceil(gateSwitchDemandPerEnergy * switchEnergy)` for an M2 switching intent;
- `WireLeakage = ceil(wireLeakagePerWU * wireLengthWU)`;
- `WireSensing = ceil(wireSenseDemandPerWU * wireLengthWU)`;
- `Movement = ceil(movementDemandPerWU * baseMovePerTickWU)` for each non-STOP Mobile intent.

Here `wireLengthWU = wireLength.raw / FIXED_ONE` is evaluated as one rational expression before one
final `ceil_div_nonnegative`; it is not truncated first. `baseMovePerTick` remains the S0-M7
`worldRoutingPitch` and mass factor remains `1`. A disabled intent creates no Movement demand and is
granted zero. Positive demand coefficients and positive physical lengths therefore cannot round to
zero. Multiple kinds for one owner remain distinct.

Construction, LiveWire, support, Relay, extraction, transfer, and radiation demand records are not
created in M2. `gateSwitchBaseEnergy * (1 + load)` retains its existing calculation; the new
coefficient converts that energy intent into nominal GateSwitch demand. All demands are collected
before the solver, and collection/store permutation cannot choose a grant.

`GateSwitch` collection sees only an ordinary Phase 3 new/replacement schedule intent:
`desired_output != current_output` and no complete pending transition already targets that same
desired level. A Tick that merely retains an identical pending transition is not charged again. The
retention-expiry LOW decision cannot exist until Phase 6 has read Phase 5 `rho`, so it MUST NOT
retroactively add a Phase 4 demand or re-run the solver. Its reset transition carries no switch-energy
claim for that Tick; consequently cancelling that reset cannot create switching Heat from Energy that
was never granted.

## 8. Canonical source route and common-ratio solver

### 8.1 Route choice

Each demand with at least one reachable Source chooses exactly one source route by this tuple:

1. total exact Fixed Euclidean Power-edge length;
2. total original canonical straight-segment overlap count after coalescing each physical Wire;
3. lexicographic path token sequence, each token `(EntityKindTag, EntityId, local subtag)`.

The path token sequence includes the Source, every physical Wire/Junction/device traversed, and the
load attachment, so two equal metric paths cannot depend on adjacency insertion order. A physical
Wire appears once with its exact positive traversed arclength even when a virtual attachment lies
inside it. The region may contain several
Sources; route selection chooses the loss distance, while Generation `G` still sums every Source in
the region as the SSS requires. No reachable Source means no route and the source-less rule applies.

### 8.2 Exact numeric kernel

`PowerRatio` is a validated Fixed raw integer in `0..=FIXED_ONE`; `FIXED_ONE=65_536`.
For candidate raw ratio `r` and demand `D_i`:

```text
P_i(r) = round_div_nearest_even(D_i * r, FIXED_ONE)

loss_i(r) = ceil_div_nonnegative(
  powerLossK.numerator * distanceRaw_i * P_i(r)^2,
  powerLossK.denominator * FIXED_ONE
)

sourceCost_i(r) = P_i(r) + loss_i(r)
```

`powerLossK=0` or `P_i=0` produces zero loss. All products use checked, sufficiently wide unsigned
intermediates; conversion, square, sum, or denominator overflow is `NumericOverflow` and rolls back
the entire Tick. Implementations may use factored GCD cancellation before multiplication, but must
match an arbitrary-precision oracle over the supported input range.

The solver returns the greatest raw `r` satisfying:

```text
sum_i sourceCost_i(r) <= G
```

It uses upper-mid integer binary search over inclusive `[0, FIXED_ONE]` and exactly
`ceil(log2(FIXED_ONE + 1)) = 17` narrowing comparisons for this Numeric Profile. Each predicate
reduces demands by stable `DemandId`; early exit may optimize a checked cost above `G` but may not
alter errors or the result. `G=0` fast-paths to zero only after validation. All loads in one region
receive the same exact `r`; there is no traversal-order monopoly or priority scheduler.

The grant record is:

```rust
pub struct PowerGrant {
    pub demand_id: DemandId,
    pub granted: Energy,           // P_i(rho)
    pub ratio: PowerRatio,
    pub transmission_loss: Energy,
}
```

### 8.3 Brownout scaling helpers

The exact M2 helpers are:

```text
scaleDrive(nominal, rho)   = RNE(nominal * rhoRaw / FIXED_ONE)
scaleMovement(base, rho)   = RNE(base.raw * rhoRaw / FIXED_ONE)
scaleWork(nominal, rho)    = RNE(nominal * rhoRaw / FIXED_ONE)

effectiveGateDelay(baseTicks, rho) = max(
  1,
  ceil_div_nonnegative(
    baseTicks * FIXED_ONE,
    max(rhoRaw, brownoutDelayFloorRaw)
  )
)
```

`brownoutDelayFloorRaw` is the exact nearest-even conversion of the reduced Profile Rational and
must be in `1..=FIXED_ONE`. `baseTicks` is the already checked
`gateBaseDelay + fanoutPenalty(load)`; M2 thermal factor is exactly unity. At `rho=1`, all helpers
preserve nominal values. The reference C-08 values are chosen to make `rho=1/2` exact, so nearest-even
ties do not obscure the required strict decreases.

`scaleWork` is a public/pure Power-grant seam used by C-08 and later S1-M4 Construction. It creates
no Demand, Site, progress, report fiction, or canonical state in M2. S1-M4 must call this exact helper
after it owns Construction nominal demand and progress.

## 9. Gate, Sense, and Movement Brownout timing

### 9.1 Gate scheduling and drive

Phase 3 evaluates desired Gate output from the resolved Tick-start Signal view. Phase 5 finds the
Gate's Power-port region ratio. In Phase 6:

- if `rho < logicOperateThreshold`, no new logic-level transition is scheduled;
- otherwise a newly scheduled transition freezes `effectiveGateDelay(base,rho)` at scheduling time;
- a due Tick already stored before the current Power solve is never retimed;
- output strength is `scaleDrive(nominalGateDrive,rho)` and any level-preserving difference schedules
  the retained `t+1` strength-response path;
- supersession/cancellation retains existing inertial generation and switching-heat behavior.

GateIdle, GateDrive, and GateSwitch demands all attach to `GatePort::Power`. A missing or disconnected
Power port belongs to a source-less region and gets `rho=0`; it is not silently treated as full
Power. Existing Stage 0/v1/v2 sessions do not enable Power and retain their exact unity behavior.

### 9.2 Retention boundary

`unpowered_ticks` is canonical `u64`, initially zero. At each Phase 6, after the current ratio is
known:

```text
rho >= logicOperateThreshold  -> unpowered_ticks = 0
rho <  logicOperateThreshold  -> unpowered_ticks += 1 (checked)
```

The Gate preserves internal output while the updated count is `< gateStateRetentionTicks`. With the
reference value `3`, the third consecutive under-threshold Phase 6 expires retention. Expiry sets
the desired retained state to LOW through the ordinary inertial scheduling path: it advances the
pending generation/cancels any conflicting event as usual and schedules LOW no earlier than
`t + effectiveGateDelay`. It never edits the Driver or internal output immediately and never bypasses
Revision or Wire delay. A Gate already LOW schedules no redundant level transition, but its strength
still follows Power. Power recovery before expiry resets the count without a hidden transition.
This retention-expiry reset is the sole under-threshold LOW-scheduling exception to the first rule in
section 9.1; input-driven Gate evaluation remains blocked below `logicOperateThreshold`.
It also obeys section 7.4's no-retroactive-demand rule: the Phase 6 reset schedules with no new
`GateSwitch` demand, no second Power solve, and no synthetic granted or cancelled switching Energy.
To preserve the retained complete pending tuple, this exceptional transition stores
`pending_switch_energy = Some(Energy(0))`. Canonical validation permits zero only for a
retention-expiry pending LOW; every ordinary Gate switch pending tuple still requires positive
energy. Cancellation therefore adds exactly zero Heat.

### 9.3 Sense and Movement

Sense uses section 6's delayed Driver path and scales `senseNominalDrive`; a source-less sensed Wire
therefore becomes passive LOW after the scheduled strength response, not a health signal.

Movement uses the Phase 1 position and Phase 3 control exactly once. Phase 6 grants
`scaleMovement(worldRoutingPitch,rho)` for an enabled intent, Phase 7 stages traversal, and Phase 11
commits positions in Mobile EntityId order. At `rho=1/2`, the reference budget is exactly half one
world routing pitch. A later control or Power change in the same Tick cannot alter that staged budget.

## 10. Leakage and transmission HeatContribution boundary

M2 computes heat production but does not create accumulated thermal state. Phase 8 emits derived,
read-only records:

```rust
pub enum HeatContributionKind {
    LeakageDissipation,
    TransmissionLoss,
}

pub struct HeatContribution {
    pub owner: WireId,
    pub kind: HeatContributionKind,
    pub energy: HeatEnergy,
}
```

For baseline leakage, the granted `WireLeakage` Energy becomes `LeakageDissipation` on the owner
Wire. `nominal - granted` is unmet demand, not transferred Energy, and MUST NOT create heat. Thus a
source-less region cannot create leakage heat from no source. This preserves the no-energy-creation
invariant while exposing the M2-owned leakage result; later thermal state merely consumes this
derived contribution.

Each demand's transmission loss is distributed over physical route Wires proportional to each
Wire's exact positive raw arclength traversed by that route, including a partial terminal Wire.
First allocate `floor(totalLoss * traversedWireLength / totalRouteLength)` to each Wire; then
give one raw Energy unit of the remaining exact remainder at a time in ascending Wire EntityId order
among route Wires until exhausted. Coalesced traversed lengths sum exactly to total route length, so
the remainder is smaller than the number of route Wires. Repeated pieces of one physical Wire are
coalesced before the division. A zero-length route with nonzero loss is invalid. Reductions sort by
`(owner WireId, kind, demandId)` and check every accumulator.

`HeatContribution` is Tick scratch copied into `StepReport.power.heat_contributions`; it is excluded
from V6 and repeated analyzer/report reads cannot mutate state. Phase 9 remains an explicit no-op for
these records. Accumulated `heat_energy`, exchange/cooling, thermal factors, damage, and destruction
remain S1-M4 and will require their own canonical-state decision. This narrow boundary satisfies the
M2 milestone's leakage/transmission-heat requirement without inventing incomplete thermal truth.

## 11. State Hash V6 and Replay migration

### 11.1 Global V6 policy

State Hash version is a schema identifier, not a feature flag. Every new session uses
`aon-state-v6`, including retained Empty and MainCoreV1 sessions. V3/V4/V5 identifiers remain
strictly decodable header values, but a current V6 Simulation rejects them before Tick 0 and never
reinterprets old checkpoints. Retained Replay fixtures are regenerated from their authoritative
packages and Command streams as Replay v2 + V6; Scenario v1/v2 semantic hashes, Module/Design hashes,
and Experiment Run IDs remain exact.

### 11.2 Exact V6 extension order

V6 starts with the exact V5 logical field order, under:

```text
domain                         ASCII AON\0STATE\0V6\0
encoderVersion                 u16 little-endian = 6
```

The V5 reserved stores are replaced/extended in this order at their logical canonical locations:

```text
PowerSourceStore live count u32, then immutable generator records by EntityId
  id, position.x, position.y, anchor tag/id, generationPerTick
Wire Sense extension after retained drive vectors within each WireId signal record
  sensePresent u8
  if 1: driverA, driverB, sampledPresence, lastIntendedLevel, lastIntendedStrength
Gate unpoweredTicks within each GateId record
Driver/Sink/Event stores using appended Sense roles/causes
retained remaining reserved empty stores
```

The implementation may preserve dense physical layout, but its independent encoder must produce
the logical order above in EntityId/EventKey order. `EndpointTarget` tags 5/6 and all nested IDs/end
tags are encoded wherever endpoints appear. Source anchor derivation and A/B Sense ownership are
encoded explicitly and validated rather than trusted from duplicate mutable values.
Power Sources are immutable generator-owned entities in M2, so their dedicated store has neither a
frontier nor tombstones; the global entity registry remains the sole allocator/liveness authority.
`sensePresent=0` ends that Wire's Sense extension and is required for retained feature-off Wires;
`sensePresent=1` is required exactly when sensing was activated for that live Wire. Sense-sample
events are non-inertial and encode `pending_generation=0`; lifecycle validity comes from the live
Driver/tombstone relation, not a per-sample generation counter.
`sampledPresence` encodes as exact `u8 0|1`. `lastIntendedLevel` uses the retained LogicLevel byte
tag but accepts only LOW/HIGH for Sense state; X or any other tag is `InvalidCanonicalState`.

Excluded from V6: hostile input frames/circles, spatial index, Power regions/nodes/routes, Demand and
Grant buffers, ratios, route heat allocation, StepReport/Analyzer data, presentation, telemetry, and
all other reconstructible cache/scratch. Pending Sense Driver transitions and their payload/order,
however, remain ordinary canonical events and are included.

### 11.3 Replay v2 policy

Replay v2 retains the header field order and adds typed World-input body semantics. Current headers
declare V6 and the generator matching the Scenario schema. `Seed::ZERO` is required for all three
deterministic generators. Body validation precedes Header comparison; after normalized Commands and
World inputs are valid, Header fields compare in printed order, then `next_tick==0`, then exact
initial State Hash. No Command or input executes on any mismatch.

Decode-only Replay v1 fixtures prove retained empty-world-input behavior. All executable retained
fixtures use v2/V6. C-07 includes explicit hostile frames; C-08 may use empty `worldInputs`.

## 12. Reports, analyzers, and exact C-07/C-08 fixtures

### 12.1 Derived observations

`StepReport` adds an optional Power section containing, in stable order:

- each region's derived stable report key, Generation, total nominal demand, and `PowerRatio`;
- `PowerDemand` and `PowerGrant` records by `DemandId`;
- canonical source route observations and transmission losses;
- Sense sample/Driver observations by WireId/end;
- Gate delay/drive/unpowered counter observations;
- Mobile nominal/granted budgets;
- Phase 8 Heat Contributions.

The Power/Sense Analyzer recomputes from the current state without mutation. It may expose player-
legal occupancy bits and circuit/power facts, but never hostile circle fields. Region report keys are
derived from the lexicographically least Power node key and are explicitly not durable IDs.

### 12.2 C-07 — Sensing

The authoritative Replay uses one powered straight sensed Wire, one downstream independent probe per
Sense end, and consecutive complete frames whose counts are `0 -> 3 -> 0`. The three HIGH circles
all intersect at least one capsule; their IDs/order and overlapping geometry are permuted in a paired
fixture.

Assertions are exact:

1. sampled occupancy changes only `LOW -> HIGH -> LOW` and never exposes count;
2. A and B schedule the same levels/strength at exactly `sampleTick + senseDelay`;
3. each Driver advances its own Revision only on actual sample change;
4. downstream observations arrive only after the ordinary route transport delay;
5. the pulse remains HIGH for exactly the input pulse width after delay;
6. a same-frame count change `1 -> 3` produces no additional level transition;
7. disconnected Power lowers strength through the delayed path and appears passive LOW without a
   health bit;
8. replay restart, input permutation, spatial-index rebuild, and both hosts produce the same V6
   trace.

### 12.3 C-08 — Brownout

One exact circuit is run twice with only Scenario Source Generation changed so its solved ratios are
exactly `rho=1` and `rho=1/2`; demand totals and the expected generation values are asserted rather
than assigning a ratio directly. Baseline `powerLossK=0` isolates common-ratio effects. A separate
nonzero-loss fixture proves the cost solver and heat path.

For carefully even nominal values, C-08 asserts:

```text
rho 1:   gate delay = base, gate/sense drive = nominal,
         movement = worldRoutingPitch, workGrant = nominal
rho 1/2: gate delay = 2 * base, gate/sense drive = nominal / 2,
         movement = worldRoutingPitch / 2, workGrant = nominal / 2
```

The low-Power run must therefore have strictly greater Delay and strictly lower Drive, Work grant,
and Movement. It also proves all demands in the region receive the same ratio, demand permutation
does not change results, an unpowered region produces zero, an overprovisioned region clamps to one,
and the retention boundary expires on exactly the third consecutive under-threshold Tick.

The Work assertion calls only the production `scaleWork` helper with an even nominal conformance
value. It is a complete test of the M2-owned grant seam, not a claim that Construction exists. Actual
Construction demand/progress integration remains a mandatory S1-M4 test.

## 13. Strict errors and atomicity

Artifact/package precedence remains envelope-first and fail-closed:

1. JSON syntax/category;
2. outer schema/format version;
3. version-specific strict shape, unknown/duplicate fields, and numeric representation;
4. Profile validity in Numeric, Physical, Balance order;
5. declared Profile hashes and Simulation contract;
6. Scenario feature/InitialWorld triad and generator coherence;
7. initial canonical-state validation.

`Simulation::new` removes Sensing and Power from `first_unsupported` only after the whole M2 feature
surface exists. Remaining unsupported-feature precedence is Relay, Payload, Radiation. A compound
fixture proves an unsupported later feature wins before invalid Profiles, then each fault is removed
to reveal Profile/hash/world errors in turn.

Before each Tick mutation, validate current-Tick Command/input ticks, duplicate ordinals/frames/IDs,
circle numeric bounds, and canonical World invariants. The existing CandidateWorld transaction is
the rollback boundary. Numeric overflow, allocator/event exhaustion, invalid Source/Sense ownership,
invalid Power route arithmetic, retention overflow, and report reduction overflow are fatal typed
Run errors and leave the World, hash, frontiers, calendars, and caches observationally unchanged.
Ordinary bad player bindings use existing command rejections and do not abort the Tick.

## 14. Executable completion gates

### Gate 1 — retained regressions and identity

- Every Stage 0, S1-M0, and S1-M1 test/gate passes.
- Scenario v1/v2, retained Profile v2, Module/Design, and Experiment Run ID goldens remain exact.
- Only Replay format/header/checkpoints and State hashes migrate where specified.

### Gate 2 — strict Balance v3 and Scenario v3

- v2 forbids and v3 requires the complete `powerProbe`; all nine fields validate and hash.
- v3 accepts only MainCorePowerV1 with Capacity/Sensing/Power enabled.
- Source permutation normalizes, duplicate positions and invalid/overflow values reject, and Core is
  never a Source.
- independent v1/v2/v3 Scenario and v2/v3 Balance encoders prove retained/new goldens.

### Gate 3 — Source state and attachment

- Core is ID 1; sorted Sources receive consecutive stable IDs and exact anchor relations.
- PowerSourceAnchor joins only Power at its exact Source position; its one node key explicitly
  bridges OpenWorld, FixedSubstrate, and MobileSubstrate Power surfaces.
- Fixed/Mobile bindings additionally require the Source position inside the selected substrate's
  routing area; wrong kind/ID/position, wrong substrate/area, mutation Commands, malformed
  store/registry, and duplicate Sources fail deterministically without mutation.

### Gate 4 — Replay v2 World inputs

- Hostile frames are complete, one-Tick, nonpersistent snapshots applied before Phase 1.
- missing/empty equivalence, Tick/ID sorting, permutation, duplicate rejection, bounds, and final-run
  boundary are exact.
- Replay v1 still rejects nonempty World inputs; v2 round-trips the typed form.

### Gate 5 — sensing geometry and spatial cache

- capsule/circle interior, exterior, tangency, zero radius, bend, negative cell, and boundary cases
  match an independent brute-force oracle.
- index insertion order and clear/rebuild do not change candidates, reports, or hashes.
- count/type/position and every prohibited hostile attribute remain unobservable.

### Gate 6 — Sense port identity and event timing

- every Wire has exactly two isolated A/B Drivers with deterministic IDs and tags.
- main Signal, A, B, Junction, Power, and Track projections never alias accidentally.
- binding/removal lifecycle rejects self/tie/dangling cases and detaches deterministically.
- Phase 1 sample, Phase 3 intent, Phase 6 `t+senseDelay`, Phase 2 Revision, and subsequent transport
  delay are proven separately; strength-only changes follow the same canonical route.

### Gate 7 — Power graph, virtual attachments, and cache

- all allowed projections compile exact components; source-less components have `G=0,rho=0`.
- intrinsic midpoint halves sum to exact `L`, reversal is invariant, odd raw remainders obey endpoint
  key order, and Capacity still counts one body once.
- Mobile tick-start offsets attach with exact `offset/L-offset`; the derived TrackGraph test named in
  section 7.3 proves the unpowered-next-edge stop algorithm. Full-Simulation different-ratio evidence
  is deferred for the explicit Power-disconnect/switch topology reason frozen there.
- graph/store/adjacency permutations and cache rebuild yield identical regions/routes.

### Gate 8 — demands, routes, and solver

- each M2 demand formula, positive-demand ceil rule, DemandId/tag, and stable collection order is exact.
- route ties exercise length, segment count, and lexicographic tokens independently.
- upper-mid 17-step maximal-ratio search matches exhaustive/arbitrary-precision oracles across
  boundaries, rounding ties, multiple Sources/loads, `G=0`, saturation, and overflow.
- every load gets the common ratio; no first-load monopoly exists.

### Gate 9 — Brownout timing and retention

- Gate operate threshold blocks only new logic scheduling; due events are never retimed.
- delay/drive/Sense/Movement/Work helpers match exact formulas at zero, floor, half, threshold, and one.
- level-preserving strength changes schedule at `t+1` and propagate after Wire delay.
- retention preserves before Tick 3, expires exactly on Tick 3 through normal scheduling, and resets
  on recovery.

### Gate 10 — heat contribution boundary

- granted leakage dissipation and nonzero transmission loss create exact stable Phase 8 records;
  unmet leakage creates none.
- route-length allocation conserves loss exactly and distributes remainder by WireId.
- reports/analyzers are read-only; no temperature, accumulated heat, thermal factor, damage, or S1-M3
  Support Heat state appears.

### Gate 11 — C-07

- the authoritative `0 -> 3 -> 0` Replay produces only the exact delayed `LOW -> HIGH -> LOW` result
  at both Sense ends, plus every assertion in section 12.2.

### Gate 12 — C-08

- the same circuit solves exact ratios one and one-half from Generation/Demand, not injected ratios.
- the fixture reaches at least one actual `GatePort::Power` from a Scenario PowerSource through the
  explicit Source-anchor routing-domain bridge; a pure topology-only Gate ratio does not satisfy it.
- lower ratio gives strict Delay increase and Drive/Work/Movement decrease.
- Work evidence is the production pure grant seam; no fake Construction state is present.

### Gate 13 — V6, Replay migration, hosts, and fuzz

- independent Empty, MainCoreV1, and MainCorePowerV1 V6 encoders prove exact bytes/goldens and
  single-field sensitivity.
- V5/current mismatch rejects before Tick 0; retained streams regenerate as v2/V6 exactly.
- Headless and Bevy match every per-Tick V6 hash/report for C-07/C-08.
- decoder, spatial, Power kernel, event-order, topology mutation, replay restart, and numeric-boundary
  fuzz/property corpora run without panic or order dependence.

### Gate 14 — negative scope

- no S1-M3 Support formula/Heat/Relay behavior and no S1-M4 Construction Site/Enemy runtime/Live
  Wire/Contact/Thermal/Damage/destruction/run-end behavior appears.
- C-09/C-10/C-22 and both Stage 1 gates remain open, not fake-passed.

### Gate 15 — Windows-native fail-closed evidence

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
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\s1-m2-technical-gate.ps1
```

The S1-M2 gate covers every gate above, retains exact test counts/goldens, fails on skipped evidence,
and leaves `git status --short` empty in the verification clone.

## 15. Closure boundary

S1-M2 may be marked complete only after all fifteen gates pass on the committed tree and fresh
Windows-native clone. The closure record must name the implementation commit, exact Scenario v3,
Balance v3, Replay v2, and V6 goldens, C-07/C-08 report values, registered suite/gate counts, host
equivalence, and clean-clone evidence.

That closure advances the tracker only for S1-M2. S1-M3 through S1-M6, C-09/C-10/C-22, and both
Stage 1 gates remain open.
