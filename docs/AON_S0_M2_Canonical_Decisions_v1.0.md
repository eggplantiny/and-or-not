# A/O/N — S0-M2 Canonical Decisions

**Status:** implementation baseline
**Applies to:** `S0-M2 — Command / Geometry / Structural Phase`

This document closes the S0-M2 representation and boundary decisions that are left open by the
PRD, SSS, and TRD drafts. Within S0-M2, the rules below are normative. Changing an observable
rule requires a semantics/schema version review and new golden fixtures.

## Scope and stage boundary

- S0-M2 placement is a Laboratory direct-active structural edit. A successful placement creates
  an active Gate, Wire, Junction, or Fixed Substrate in Phase 0.
- Construction cost, cargo, work, progress, Construction Sites, and delayed activation are Stage 1
  semantics and are not simulated by S0-M2 placement.
- S0-M2 executes `PlaceGate`, `PlaceWire`, `PlaceJunction`, `PlaceFixedSubstrate`,
  `RemoveEntity`, and `BindPort`.
- `PlaceMobileSubstrate` remains present in the public `Command` type, but every invocation is
  rejected with `UnsupportedPlacement` until S0-M7.
- `SetExternalDriver` remains present in the public `Command` type, but every invocation is
  rejected with `UnsupportedCommand` until S0-M3.
- The unsupported variants do not allocate an EntityId, mutate a store, advance a generation, or
  increment `topology_revision`.
- Their payloads remain serializable and canonically encodable so a rejected input log remains
  reproducible. S0-M7 and S0-M3 own the later state-transition semantics, not new tag numbers.

## Public command and topology model

The following shapes are the S0-M2 semantic API. Field names may follow Rust naming conventions;
their meaning and ordering are fixed here.

```rust
pub struct CommandEnvelope {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub command: Command,
}

pub enum Command {
    PlaceGate(PlaceGateCommand),
    PlaceWire(PlaceWireCommand),
    PlaceJunction(PlaceJunctionCommand),
    PlaceFixedSubstrate(PlaceFixedSubstrateCommand),
    PlaceMobileSubstrate(PlaceMobileSubstrateCommand),
    RemoveEntity(RemoveEntityCommand),
    BindPort(BindPortCommand),
    SetExternalDriver(SetExternalDriverCommand),
}

pub enum GateType {
    And,
    Or,
    Not,
}

pub enum RoutingDomain {
    OpenWorld,
    FixedSubstrate(EntityId),
    MobileSubstrate(EntityId),
}

pub enum GatePort {
    InputA,
    InputB,
    Output,
    Power,
}

pub enum WireEnd {
    A,
    B,
}

pub enum LogicLevel {
    Low,
    High,
    X,
}

pub struct GatePortRef {
    pub gate: GateId,
    pub port: GatePort,
}

pub enum EndpointTarget {
    Free,
    Junction(JunctionId),
    GatePort(GatePortRef),
}

pub struct FixedAabb {
    pub min: FixedVec2,
    pub max: FixedVec2,
}
```

An AABB is non-empty: `min.x < max.x` and `min.y < max.y`. It is axis-aligned and has no rotation.

The frozen command payloads are:

```rust
pub struct PlaceGateCommand {
    pub gate_type: GateType,
    pub origin: FixedVec2,             // world-space
    pub routing_domain: RoutingDomain,
}

pub struct PlaceWireCommand {
    pub routing_domain: RoutingDomain,
    pub points: Vec<FixedVec2>,        // raw world-space vertices
    pub endpoint_a: EndpointTarget,
    pub endpoint_b: EndpointTarget,
}

pub struct PlaceJunctionCommand {
    pub routing_domain: RoutingDomain,
    pub position: FixedVec2,           // world-space
}

pub struct PlaceFixedSubstrateCommand {
    pub origin: FixedVec2,             // world-space
    pub routing_area: FixedAabb,       // substrate-local
    pub footprint: FixedAabb,          // substrate-local
}

pub struct PlaceMobileSubstrateCommand {
    pub origin: FixedVec2,             // world-space
    pub routing_area: FixedAabb,       // substrate-local
    pub footprint: FixedAabb,          // substrate-local
}

pub struct RemoveEntityCommand {
    pub target: EntityId,
}

pub struct BindPortCommand {
    pub wire: WireId,
    pub end: WireEnd,
    pub target: EndpointTarget,
}

pub struct SetExternalDriverCommand {
    pub driver: DriverId,
    pub level: LogicLevel,
    pub strength: DriveStrength,
}
```

`points[0]` is Wire End A and `points[points.len() - 1]` is Wire End B. The stored points are the
exact accepted input points; placement does not round, simplify, snap, or reorder them.

Every accepted command produces a `CommandAcceptance`. The supported creation commands
`PlaceGate`, `PlaceWire`, `PlaceJunction`, and `PlaceFixedSubstrate` return `Some(id)`.
`RemoveEntity`, `BindPort`, and an accepted no-op binding return `None`. Unsupported commands never
produce an acceptance. A rejection carries the envelope's `target_tick`, `ordinal`, and a stable
rejection reason.

```rust
pub struct CommandAcceptance {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub created_entity: Option<EntityId>,
}

pub struct CommandRejection {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub reason: CommandRejectionReason,
}
```

## Batch ordering, rejection, and identity

For the command slice supplied to Tick `t` Phase 0:

1. An envelope whose `target_tick != t` is rejected as `WrongTick`.
2. Among envelopes targeting `t`, every envelope whose ordinal occurs more than once is rejected
   as `DuplicateOrdinal`. No member of a duplicate-ordinal group is applied.
3. Remaining envelopes are processed in ascending ordinal order.
4. Each command observes structural changes made by earlier accepted commands. This includes
   geometry occupancy and overlap conflicts.
5. A command is fully validated before it mutates the cloned world or allocates an EntityId.

Command result records are deterministically ordered by `(target_tick, ordinal)`. Duplicate
rejections with the same key have identical canonical fields, so permuting the input slice cannot
change the returned result sequence, accepted world, or state hash.

Only entities live at the beginning of the command batch may be referenced by an EntityId in that
batch. An EntityId returned by a successful placement cannot be referenced by another command in
the same batch, even if the caller predicts its numeric value. It becomes referenceable beginning
with the next Tick. A predicted ID at or beyond the batch-start allocation frontier is rejected as
`UnknownEntity`. Earlier accepted placements still participate in geometry validation.

A rejected command consumes no EntityId. Accepted creation allocates exactly one monotonically
increasing EntityId after validation succeeds. Guessing a future ID never turns into a valid
same-batch reference.

Command rejection is not a Run Error. The exhaustive S0-M2 rejection-reason surface, in stable tag
order `0..=17`, is:

```text
DuplicateOrdinal, WrongTick, UnknownEntity, RemovedEntity,
InvalidGeometryQuantum, InvalidRoutingPitch, InvalidGeometryShape,
ZeroLengthSegment, GeometryOverlap, InsufficientSpacing,
UnsupportedPlacement, UnsupportedCommand, InvalidRoutingDomain,
InvalidEndpoint, InvalidPort, InvalidPortBinding,
SubstrateBoundsViolation, SubstrateInUse
```

Checked canonical arithmetic overflow and Tick, EntityId, ConnectionGeneration, or
topology-revision exhaustion are deterministic Run Errors, not player command rejections. A Run
Error aborts Phase 0 without swapping the cloned world into canonical state.

### Command validation precedence

When one command violates multiple rules, the first applicable stage below fixes its public
rejection reason. Implementations do not choose a reason based on container iteration order:

1. command schema/shape preflight: a Wire point count must fit `u32` and be at least two; Fixed
   Substrate routing/footprint AABBs must be non-empty;
2. referenced EntityId existence and lifecycle, in payload order: routing-domain substrate, Wire
   endpoint A, Wire endpoint B; for `BindPort`, the Wire then the replacement target;
3. S0-M2 command and placement support (`MobileSubstrate` routing remains unsupported);
4. `wireGeometryQuantum` validation;
5. remaining geometry shape such as zero-length segments;
6. routing pitch, routing-domain entity kind, and substrate footprint/containment;
7. checked recomputation of the complete derived Wire length; overflow is a fatal Run Error that
   aborts the Phase 0 transaction rather than a command rejection;
8. topology, overlap, crossing, spacing, and Gate-contact validation;
9. endpoint entity kind, exact position/domain, Gate port availability, and binding semantics.

An ID at or beyond the batch-start frontier is `UnknownEntity`; a previously allocated tombstone
is `RemovedEntity`. Lifecycle preflight checks existence only. A live entity of the wrong kind is
reported later by the relevant routing or endpoint semantic stage. The two public S0-M2 placeholder
variants are exceptions: `PlaceMobileSubstrate` and `SetExternalDriver` unconditionally return
their documented unsupported reason without interpreting inactive payload semantics.

## Coordinate spaces, quantization, and pitch

All accepted points and AABB bounds first satisfy `wireGeometryQuantum`; silent rounding is
forbidden. More specific routing-pitch rules then apply:

- A Fixed Substrate world origin is aligned to `worldRoutingPitch`.
- Each bound of its local `routing_area` is aligned to `circuitRoutingPitch`.
- Each bound of its local `footprint` is aligned to `wireGeometryQuantum`.
- `routing_area` is contained by `footprint`, including coincident boundaries.
- Open World Wire and Junction coordinates are aligned to `worldRoutingPitch`.
- For a Fixed Substrate entity `s`, internal world point `p` is converted by checked subtraction
  to `local = p - s.origin`. The local coordinate is aligned to `circuitRoutingPitch`.
- A Wire's two physical endpoints satisfy `wireGeometryQuantum` and routing-domain containment,
  but are not required to satisfy the domain routing pitch. Every non-endpoint Wire vertex remains
  aligned to `worldRoutingPitch` or local `circuitRoutingPitch`. This permits exact profile Gate
  anchors and keeps physical geometry valid when an endpoint is bound, unbound, or its target is
  removed. The endpoint's connectivity target does not alter its geometry validation.
- S0-M2 Gates are permitted only in `RoutingDomain::FixedSubstrate`; Open World and Mobile
  Substrate Gate placement is `UnsupportedPlacement`.
- `RoutingDomain::MobileSubstrate` placement is `UnsupportedPlacement` throughout S0-M2.

Gate rotation is not part of S0-M2. Its profile footprint is axis-aligned and centered on the Gate
origin. Each full footprint extent is therefore an even multiple of `wireGeometryQuantum`, making
both half extents exact quantum multiples. Every profile Gate anchor lies on the footprint boundary;
a profile that cannot satisfy either condition is invalid. A Gate
footprint, every Wire segment, and every Junction in a Fixed Substrate domain must be contained by
that substrate's local `routing_area` after the checked world-to-local translation.

Two AABBs have an interior overlap exactly when both of these are true:

```text
a.min.x < b.max.x && b.min.x < a.max.x
a.min.y < b.max.y && b.min.y < a.max.y
```

Interior overlap is a conflict. Boundary-only contact is allowed unless a more specific Wire/Gate
rule below forbids it. Fixed Substrate footprints may not interior-overlap one another, and Gate
footprints in the same structural world may not interior-overlap one another.

## Wire, crossing, spacing, and endpoint rules

- A Wire has at least two raw points.
- Every segment has non-zero length.
- No two segments of one Wire may share a positive-length collinear interval. This includes
  immediate retracing and non-adjacent self-overlap.
- Two distinct Wires may not share a positive-length collinear interval.
- Point intersections and point crossings are allowed. They create no Junction, incidence, signal
  connection, power connection, or track connection unless explicit endpoint binding says so.
- Parallel centerlines in one routing domain maintain at least that domain's routing pitch.
- The only spacing exception is between incident first/last segments whose physical Wire endpoints
  have exactly the same coordinate in the same routing domain. It applies independently of whether
  either endpoint target is `Free`, a Junction, or a Gate port. Coordinate equality does not imply
  connectivity.
- The spacing exception does not permit positive-length overlap.
- A Wire may not intersect a Gate interior. Its only permitted Gate-boundary contact is its own
  physical endpoint at any exact, valid profile anchor of a Gate in the same routing domain.
  This contact is permitted independently of binding; running along a Gate boundary or touching a
  non-anchor boundary point remains invalid.
- A Junction may not be placed in the strict interior of a Wire segment. A Wire endpoint at a
  Junction position is connected only when its `EndpointTarget` explicitly names that Junction.

For `EndpointTarget::Junction(id)`, the endpoint coordinate must exactly equal the Junction world
position and both entities must have the same routing domain. For
`EndpointTarget::GatePort(reference)`, it must exactly equal the Gate's world origin plus the
profile-defined anchor and must share the Gate's Fixed Substrate domain. `InputB` is invalid for a
NOT Gate. Hosts cannot supply custom anchors.

Multiple Wires may bind to the same Gate port. Connectivity is derived exclusively from explicit
endpoint targets, never from coordinate equality or permitted physical contact. A free endpoint has
no incident target even if it lies at the same coordinate as a Gate anchor, Junction, or another
free endpoint.

## Bind and remove semantics

- `BindPort` replaces exactly one Wire endpoint target without changing Wire geometry.
- Binding to `EndpointTarget::Free` unbinds the selected endpoint.
- Rebinding an endpoint to its current target is an accepted no-op. It does not advance a
  generation and does not change `topology_revision`.
- Every effective bind or unbind revalidates endpoint coordinate, entity kind, lifecycle, routing
  domain, and port availability rules against the current cloned world. Physical endpoint pitch,
  spacing, and Gate contact validity are independent of the endpoint target and therefore cannot be
  invalidated by a connectivity-only change.
- Removing a Wire removes its endpoint incidence from any referenced Junction or Gate port.
- Removing a Gate or Junction changes every incident Wire endpoint to `Free` before removing the
  target. The affected live Wire generations are advanced as described below.
- A Fixed Substrate can be removed only when no live Gate, Wire, or Junction belongs to its
  routing domain. A non-empty substrate removal is rejected as `SubstrateInUse`.
- Removing an empty Fixed Substrate is a substrate-only change and does not increment
  `topology_revision`.

Remove and bind operations are individually atomic. Unknown, tombstoned, or wrong-kind targets
are rejected without partial incidence changes.

## Connection generation and topology revision

Wire and Junction `ConnectionGeneration` values begin at `0`.

- If one or more effective connectivity changes affect a live Wire during one Phase 0, its
  generation advances exactly once in that Phase.
- If one or more incident endpoint changes affect a live Junction during one Phase 0, its
  generation advances exactly once in that Phase.
- Multiple changes to the same live Wire or Junction in one Phase coalesce into one checked
  advance. Creation initializes generation `0`; an accepted no-op does not advance it.
- Generation overflow is a Run Error and leaves the original canonical world unchanged.

`topology_revision` advances at most once per Phase 0. It advances when at least one accepted
command does any of the following:

- adds or removes a Gate;
- adds or removes a Wire;
- adds or removes a Junction; or
- performs an effective `BindPort`, including an effective unbind caused by removal.

A substrate-only add or remove does not advance it. If structural topology changed at any point
in the Phase, the revision advances even when later commands make the final geometry or
connectivity observationally equal to the starting arrangement. Revision overflow is detected
before canonical state is swapped.

## Phase 0 transaction boundary

The S0-M2 command batch is applied to a clone of the complete structural world. Commands are
validated and applied to that clone in the ordering above. The clone is swapped into canonical
state only after the entire Phase 0 structural operation completes without a Run Error.

Ordinary command rejections remain records in the successful Phase result and do not prevent
accepted commands from being swapped. A fatal checked overflow or internal invariant failure
discards all cloned mutations and returns a Run Error. Tick increment overflow and any required
topology-revision increment overflow are checked before mutating canonical state.

The remaining Phase 0 steps described by the SSS—pending destruction, Reconstruction Sites,
Relay transitions, Construction activation, module flattening, and topology synchronization—are
introduced by their owning milestones. S0-M2 establishes their transaction boundary but does not
invent partial semantics for them.

## Structural storage boundary

- `GateStore`, `WireStore`, `JunctionStore`, and `FixedSubstrateStore` use no-compaction
  structure-of-arrays storage. Removal leaves stable internal tombstones for this milestone.
- Raw Wire vertices live in a `GeometryArena`. Arena offsets, allocation order, spare capacity,
  and storage ranges are implementation details.
- The Entity Registry remains the authority for stable EntityId and lifecycle. Dense indices,
  registry locations, store slot numbers, Geometry Arena ranges, and free-list state are excluded
  from the semantic hash.
- Canonical traversal never uses SoA or arena iteration order. Live structural records are emitted
  in ascending EntityId order.
- Wire length is recomputed from raw points using the S0-M1 numeric rule and is not stored in the
  canonical encoding.
- Gate-port incidence and Junction incidence are derived from Wire endpoint targets and are not
  stored in the canonical encoding.

No-compaction is an S0 implementation choice, not permission to expose dense indices as semantic
identity. A future storage rewrite must preserve the exact EntityId-ordered encoding below.

## Canonical command encoding

The S0-M2 command byte encoding is streaming and uses:

- domain separator `AON\0COMMAND\0V1\0`;
- encoder version `u16` value `1`;
- fixed-width little-endian integers;
- `Fixed` as its signed `i64` bit pattern in little-endian order;
- `EntityId`, `Tick`, and `ordinal` as `u64` little-endian values;
- vector counts as `u32` little-endian values; and
- explicit `u8` enum tags, never Rust discriminants or memory layout.

An envelope is encoded in this order:

```text
domain | encoderVersion | targetTick | ordinal | commandTag | commandPayload
```

Stable tags are:

| Type | Tag mapping |
| --- | --- |
| `Command` | `PlaceGate=0`, `PlaceWire=1`, `PlaceJunction=2`, `PlaceFixedSubstrate=3`, `PlaceMobileSubstrate=4` (unsupported until S0-M7), `RemoveEntity=5`, `BindPort=6`, `SetExternalDriver=7` (unsupported until S0-M3) |
| `GateType` | `And=0`, `Or=1`, `Not=2` |
| `RoutingDomain` | `OpenWorld=0`, `FixedSubstrate=1`, `MobileSubstrate=2` |
| `GatePort` | `InputA=0`, `InputB=1`, `Output=2`, `Power=3` |
| `WireEnd` | `A=0`, `B=1` |
| `EndpointTarget` | `Free=0`, `Junction=1`, `GatePort=2` |
| `LogicLevel` | `Low=0`, `High=1`, `X=2` |

Compound encodings are:

```text
FixedVec2       := x:i64 | y:i64
FixedAabb       := min:FixedVec2 | max:FixedVec2
RoutingDomain   := tag:u8 | substrateId:u64 only for tags 1 or 2
GatePortRef     := gateId:u64 | gatePortTag:u8
EndpointTarget  := tag:u8
                   | junctionId:u64 for tag 1
                   | GatePortRef for tag 2

PlaceGate       := gateTypeTag | origin | routingDomain
PlaceWire       := routingDomain | pointCount:u32 | points[pointCount]
                   | endpointA | endpointB
PlaceJunction   := routingDomain | position
PlaceFixedSubstrate := origin | routingArea | footprint
PlaceMobileSubstrate := origin | routingArea | footprint
RemoveEntity    := targetId:u64
BindPort        := wireId:u64 | wireEndTag | endpointTarget
SetExternalDriver := driverId:u64 | logicLevelTag:u8 | strength:u64
```

Tags `4` and `7` and the payload bytes above are fixed so later milestones cannot renumber or
reinterpret the command log. S0-M2 encodes them normally, then deterministically rejects them
without a state transition. Command results, rejection text, command history, and rejection history
are not part of the canonical state hash.

## Canonical structural state encoding

The state encoder keeps the existing `AON\0STATE\0V1\0` prefix and header. The first four
previously empty structural store sections are Gate, Wire, Junction, and Fixed Substrate in that
order. Each section begins with its live-record count as `u64`, followed by records in ascending
EntityId order:

```text
GateRecord :=
    entityId:u64 | gateTypeTag:u8 | origin:FixedVec2 | routingDomain

WireRecord :=
    entityId:u64 | routingDomain | connectionGeneration:u64
    | pointCount:u32 | rawPoints[pointCount]
    | endpointA | endpointB

JunctionRecord :=
    entityId:u64 | routingDomain | position:FixedVec2
    | connectionGeneration:u64

FixedSubstrateRecord :=
    entityId:u64 | origin:FixedVec2 | routingArea:FixedAabb
    | footprint:FixedAabb
```

The existing Mobile Substrate, scheduled-event, pending-destruction, and path-certificate sections
remain zero-count sections until their owning milestones. Registry tombstones and allocation
frontier remain canonical under S0-M1, but their dense store location values remain excluded.

Raw live Wire vertices are emitted through their owning Wire record, not by Geometry Arena order.
Therefore redundant accepted collinear vertices change the state hash even when they do not change
derived Wire length. Derived length, incidence lists, spatial indexes, compiled topology, and other
caches are excluded and must be validated or recomputed from canonical records.

## S0-M2 completion tests

S0-M2 is not complete until all of the following are deterministic golden or property tests:

1. **C-20 Same-tick Command Ordering:** in a world with a valid Fixed Substrate, two Place Gate
   commands target the same origin with ordinals `1` and `2`; ordinal `1` is accepted, ordinal `2`
   is rejected for overlap, and only the first Gate exists.
2. **Input permutation:** every permutation of the same uniquely-ordinaled command slice produces
   the same acceptances, rejections, created IDs, structural state, revision, and state hash.
3. **Duplicate ordinal:** all current-Tick commands in a duplicate group receive
   `DuplicateOrdinal`; none mutate state or consume IDs. Input permutation does not choose a
   winner.
4. **Wrong Tick:** an envelope for another Tick receives `WrongTick`, does not participate in
   current-Tick duplicate detection, and cannot mutate state or consume an ID.
5. **Quantum and pitch:** off-quantum coordinates, an off-world-pitch substrate origin, an
   off-circuit-pitch local bound/internal point, malformed AABBs, and out-of-area geometry are
   rejected without rounding or panic. A quantum-aligned physical Wire endpoint is accepted off
   routing pitch while an identically positioned internal vertex is rejected. Negative coordinates
   use exact checked arithmetic.
6. **Wire shape:** fewer than two points, every form of zero segment, immediate retracing, and
   positive-length self-overlap are rejected without panic.
7. **Overlap and spacing:** Gate interior overlap, distinct-Wire positive-length overlap, Gate
   interior traversal, non-anchor Gate-boundary contact, and insufficient parallel spacing reject;
   AABB boundary touch and the exact shared-physical-endpoint spacing exception accept. Equal
   physical endpoints remain disconnected unless their explicit targets connect them.
8. **Crossing:** a point crossing of two Wires is accepted but produces no implicit Junction or
   connectivity. Coordinate-equal free endpoints remain disconnected.
9. **Endpoint binding:** Junction and Gate-anchor position/domain equality are exact; NOT InputB is
   invalid; multiple Wires on one valid Gate port are accepted; bind-to-Free unbinds; same-target
   rebind is an accepted no-op.
10. **Removal:** Gate/Junction removal frees incident Wire endpoints, Wire removal clears derived
    incidence, non-empty substrate removal rejects, and empty substrate removal accepts.
11. **EntityId:** each accepted creation returns its ID; rejected placement consumes none;
    same-batch predicted/new ID references reject; the returned ID is valid on the next Tick.
12. **Generation:** any number of effective connectivity changes advances each affected live
    Wire/Junction exactly once in the Phase; a no-op advances neither.
13. **Topology revision:** any topology mutation advances revision exactly once per Phase;
    substrate-only changes and no-op binding do not; remove-plus-add or other net-zero-looking
    topology work still advances once.
14. **Transactional overflow:** Tick, EntityId, generation, revision, single-segment Wire length,
    and multi-segment total Wire length overflow paths return a deterministic Run Error and leave
    the pre-Phase canonical world unchanged.
15. **Canonical hash:** input order, dense indices, SoA slots, arena ranges, capacity, and derived
    incidence/length do not affect the hash; EntityId-ordered records, endpoint targets,
    generations, and every raw live Wire vertex do affect it.

The S0-M1 C-16/C-17 identity, numeric, and state-hash fixtures remain regression gates. Invalid
geometry and malformed command input must return deterministic errors and must never panic.
