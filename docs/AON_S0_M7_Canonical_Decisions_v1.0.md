# A/O/N S0-M7 Canonical Decisions v1.0

Status: implementation authority for `S0-M7 — Mobility`

This document freezes the choices required to implement Stage 0 mobility without adding an FSM,
destination, pathfinder, or free two-dimensional canonical transform. The SSS and TRD remain the
authority for the 12-Phase Tick, identity, signal, command, Replay, and hashing contracts.

Passing S0-M7 completes neither the Stage 0 technical gate nor the Stage 0 product gate. Both
gates require their complete conformance and manual evidence after this slice.

## Scope

S0-M7 owns:

- a derived Track Graph over live OpenWorld Wire and Junction records;
- canonical `MobileSubstrateStore` records and `TrackPosition`;
- STOP, LEFT, and RIGHT signal sinks exposed as bindable Mobile ports;
- deterministic Stage 0 movement budget and Phase 3/6/7/11 movement staging;
- exact fixed-integer junction turn ordering, reverse, and dead-end behavior;
- mobile-domain Gate/Wire/Junction placement and signal routing;
- mobile snapshot, ASCII rendering, picking, probing, and inspector projection;
- C-14 and a retained State-driven STOP/RETURN scenario.

S0-M7 does not add Capacity, a Power Graph, mass/cargo economy, LOAD/UNLOAD/BUILD behavior,
collision traffic, inertia, battery coast, construction, damage, sensing, a destination command,
or a RoutePlanner/FSM runtime class.

## Canonical types

```rust
pub enum Heading {
    Forward,
    Reverse,
}

pub enum TrackPosition {
    Edge {
        edge: WireId,
        offset: Fixed,       // measured from endpoint A along canonical raw point order
        heading: Heading,
    },
    Junction {
        junction: JunctionId,
        incoming_edge: WireId,
    },
}

pub enum MobilePort {
    Stop,
    Left,
    Right,
}

pub struct MobilePortRef {
    pub mobile: MobileId,
    pub port: MobilePort,
}

pub struct MobileControlPorts {
    pub stop: SinkId,
    pub left: SinkId,
    pub right: SinkId,
}
```

`Heading::Forward` increases edge offset and `Heading::Reverse` decreases it. An edge offset is in
the closed interval `[0, edge_length]`. A Junction position retains the incoming edge so reverse
is defined without a hidden previous-position cache.

Mobile ID, control Sink IDs, committed TrackPosition, and local substrate geometry are Canonical
State. Phase 7 movement staging is transaction-local and becomes Canonical State only at the
Phase 11 commit. Derived Track adjacency, turn candidates, world position, render direction, and
the Track Graph cache are not independent truth.

## Track Graph

Every live `RoutingDomain::OpenWorld` Wire is one Track edge. Fixed- or mobile-domain Circuit Wires
are not Track edges. The edge orientation is endpoint A/raw point zero toward endpoint B/raw last
point. Edge length is the existing checked canonical polyline length.

Track connectivity is explicit:

- a Wire end participates in a Track junction only when its `EndpointTarget` is that live
  `JunctionId`;
- an unbound geometric coincidence is not connected;
- a Wire crossing is not connected;
- a Junction's incident edges are ordered by `WireId`, then Wire end;
- rebuilding identical geometry produces a new edge identity;
- Track Graph compilation is a deterministic derived cache and does not allocate canonical IDs.

The endpoint tangent is the first nonzero segment when leaving that endpoint. Accepted Wire
geometry already excludes zero-length segments, so every live edge has two defined endpoint
tangents.

## PlaceMobileSubstrate

`PlaceMobileSubstrateCommand.origin` is a requested Track point, not a free transform. Placement
searches live OpenWorld edges in `(WireId, segment index)` order. A segment is a candidate only if
the origin is exactly collinear, lies inside the closed segment bounds, and maps back to the same
fixed coordinate through the canonical edge-offset projection below.

The first candidate wins. The initial heading is `Forward`. At endpoint B, where Forward cannot
leave the edge, initial heading is `Reverse`. If the origin is also an explicitly bound Junction,
the lowest `(WireId, WireEnd)` that leaves that Junction wins. No candidate is
`UnsupportedPlacement`; arithmetic exhaustion is a fatal checked Simulation error.

Placement allocates, in one transactional command:

1. one stable Mobile EntityId;
2. STOP, LEFT, and RIGHT Sink IDs in that order;
3. one Mobile store record with the exact local routing area and footprint;
4. the derived initial TrackPosition.

The identity allocator and all stores roll back on fatal failure. A normal invalid placement
consumes neither Mobile nor Sink identity. Mobile IDs and control Sink IDs are never reused.

`RemoveEntity(mobile)` is accepted only when its mobile-domain Gate/Wire/Junction stores are empty.
Removal tombstones the Mobile and its three Sink identities. A Wire or Junction occupied by a live
Mobile, or retained as a Junction position's incoming edge, cannot be removed in the same Phase 0;
the command returns a deterministic `TrackOccupied` rejection.

## Mobile-domain circuit geometry

Gate origins, Wire points, and Junction positions in `RoutingDomain::MobileSubstrate(id)` are
substrate-local coordinates. They are validated against that Mobile's local routing area using the
existing circuit pitch, geometry, spacing, and overlap rules. They do not change as the Mobile
moves.

`EndpointTarget` gains `MobilePort(MobilePortRef)` after the existing Free/Junction/GatePort tags.
Only a Wire in the same `MobileSubstrate(id)` domain may bind to that Mobile's STOP/LEFT/RIGHT port.
Mobile control ports are Sinks; binding them never invents a Driver or a direct boolean field.

Stage 0 control ports are logical intrinsic terminals rather than profile-defined geometric
anchors. A bound Wire endpoint may be any geometry-quantized point inside the Mobile's local
routing area. Every endpoint bound to the same intrinsic port joins the same logical Signal node;
the endpoint coordinate remains ordinary Wire geometry and does not add separate port state.

Adding or removing a mobile-domain route uses the same Driver Revision, TopologySync, Path
Certificate, and passive-LOW rules as every other signal route.

## Stage 0 movement budget

The general SSS formula remains:

```text
baseMovePerTick × powerRatio ÷ massFactor(totalMass)
```

Stage 0 freezes the unavailable later-economy terms as follows:

```text
baseMovePerTick = PhysicalScaleProfile.worldRoutingPitch
powerRatio      = 1
massFactor      = 1
```

Therefore every non-stopped Mobile receives exactly one world routing pitch of movement budget per
Tick. Stage 1+ must version the responsible Profile before replacing either unity term; it may not
silently reinterpret a Stage 0 Replay.

Stage 0 Mobility supports
`worldRoutingPitch / wireGeometryQuantum <= 65,536`. Every accepted Track segment is at least one
geometry quantum long, so this bounds the number of edge/Junction observations a single Mobile can
produce in one Tick. The reference Stage 0 profile ratio is 64. A larger ratio does not invalidate
an otherwise valid Physical Scale v1 profile, preserving pre-Mobility profile compatibility;
`PlaceMobileSubstrate` rejects it as `UnsupportedPlacement` before allocating Mobile or Sink
identity. A later Profile schema may version and replace this Stage 0 limit.

Phase behavior is:

1. Phase 1 snapshots the starting TrackPosition and derived world point.
2. Phase 3 samples STOP/LEFT/RIGHT once from resolved Sink state.
3. Phase 6 grants either zero or the exact Stage 0 budget.
4. Phase 7 consumes the budget into a staged TrackPosition and trajectory.
5. Phase 11 commits all staged Mobile positions in ascending Mobile EntityId order.

No position changes in Phase 3, 6, or 7. Signal or presentation reads cannot alter the staging.

## Control interpretation

STOP is examined first:

- STOP HIGH or X grants zero movement;
- STOP LOW permits movement;
- when STOP prevents movement, LEFT/RIGHT are observationally irrelevant.

At a Junction with STOP LOW:

| LEFT | RIGHT | choice |
|---|---|---|
| LOW | LOW | smallest absolute turn (straight) |
| HIGH | LOW | greatest canonical left turn |
| LOW | HIGH | greatest canonical right turn |
| HIGH | HIGH | reverse on the incoming edge |
| X | any | stop at the Junction |
| any | X | stop at the Junction |

The incoming edge is excluded from straight/left/right candidates and is reserved for reverse.
If no non-reverse candidate exists, LOW/LOW reverses (the degree-one dead-end rule); a requested
LEFT or RIGHT with no candidate on that side stops at the Junction. An absent incoming edge is a
Canonical State invariant error.

The Phase 3 sample is reused if one Tick's budget crosses more than one Junction. It is never
re-read after movement begins.

## Exact turn ordering

Let `incoming` be the travel vector into the Junction and `outgoing` the candidate vector leaving
it. All vectors use exact `i128` widening from fixed coordinates. Runtime trigonometry and floats
are forbidden.

Candidates are classified by `cross(incoming, outgoing)`:

- positive: left;
- negative: right;
- zero with positive dot: straight;
- zero with negative dot: reverse direction, but a different edge remains a non-incoming
  candidate.

Absolute angle comparisons use exact cross multiplication of dot/cross pairs after half-plane
classification. No division or square root participates. Equal angular directions tie by
`WireId`, then Wire end. Straight chooses the minimum absolute angle; LEFT chooses the greatest
counter-clockwise angle; RIGHT chooses the greatest clockwise angle.

All cross, dot, and comparison products are checked/widened. Any value outside the proven `i128`
range is a deterministic fatal numeric error before Phase 11 mutation.

## Budget traversal

An Edge position consumes distance toward the headed endpoint. If budget ends before the endpoint,
only offset changes. Reaching an endpoint bound to no live Junction reverses and may consume the
remaining budget on the same edge. Reaching a bound Junction creates a transient Junction position,
selects the next edge, and continues if a candidate exists.

An edge with two free ends uses exact reflecting-period reduction (`2 * edge_length`) to obtain the
same final offset and heading without iterating once per bounce. It emits no Junction decisions
because that topology contains none.

At an exact endpoint or Junction with no remaining budget, the staged position preserves that exact
boundary; it does not perform a zero-cost turn. Every traversed edge has positive length. A
Junction-to-edge transition may consume zero budget, but the following edge transition consumes
positive budget or terminates; same-Junction self-loop binding is rejected, preventing a zero-cost
topology loop.

Polyline world projection locates the segment by canonical cumulative segment length. Interior
coordinates use checked ties-to-even interpolation of the segment delta by
`segment_offset / segment_length`. Endpoint offsets always return exact raw endpoints.

## Hash, Replay, and observations

The State Hash encoder advances from V3 to V4. V4 includes, in Mobile EntityId order:

- Mobile ID/alive tombstone state and allocator frontier;
- exact TrackPosition discriminant and fields;
- local routing area and footprint;
- STOP/LEFT/RIGHT Sink IDs;
- the Phase-11 committed TrackPosition; transient Phase-7 staging is never encoded separately.

Derived Track adjacency, cached world position, render direction, candidate arrays, and trajectory
diagnostics are excluded. V4 also encodes `EndpointTarget::MobilePort` and mobile-domain structural
records through their existing canonical sections.

Replay format v1 remains the container format, but its Header must declare State Hash V4 and the
updated initial hash. Existing V3 Replays remain strictly decodable and produce an explicit
unsupported State Hash version when executed by a V4-only session; retained Stage 0 fixtures are
regenerated from their authoritative command streams.

`StepReport` adds read-only Mobile movement observations containing Mobile ID, starting and ending
TrackPosition, granted/consumed budget, and each Junction decision. Observations do not enter State
Hash.

## Snapshot and host projection

`RenderSnapshot` adds live Mobile records in EntityId order:

```rust
pub struct MobileRenderRecord {
    pub id: MobileId,
    pub track_position: TrackPosition,
    pub world_position: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
    pub ports: MobileControlPorts,
    pub stop: LogicLevel,
    pub left: LogicLevel,
    pub right: LogicLevel,
}
```

Network View draws a Mobile as `>`, `<`, `^`, or `v` from the dominant exact endpoint tangent;
ties prefer horizontal, then positive direction. Circuit View exposes its local Circuit at circuit
pitch and makes STOP/LEFT/RIGHT bindable and probeable. Rendering, picking, probing, inspection,
and view changes are read-only.

## S0-M7 completion gates

S0-M7 is complete only when all of the following have executable evidence:

1. Track Graph compilation uses only live OpenWorld Wires and explicit Junction bindings and is
   invariant to store layout and equivalent command permutations.
2. Mobile placement deterministically resolves the requested Track point, allocates Mobile plus
   STOP/LEFT/RIGHT identities transactionally, and rejects off-track placement without allocation.
3. Mobile-domain Gate/Wire/Junction geometry is local, bounds-checked, and can route ordinary
   Driver samples to each control Sink through MobilePort bindings.
4. TrackPosition edge offsets, headings, Junction incoming edges, projection, and boundary values
   survive exact canonical round-trip and V4 hash sensitivity tests.
5. Phase 3 samples controls once, Phase 6 grants the exact Stage 0 budget, Phase 7 only stages, and
   Phase 11 commits in Mobile ID order.
6. STOP HIGH/X, LEFT X, and RIGHT X stop without partial movement or hidden control re-reads.
7. C-14 covers all LOW/HIGH LEFT/RIGHT combinations plus X rows, with exact selected edge and
   position evidence.
8. Straight, left, right, reverse, angular ties, collinear alternatives, and EntityId ties pass
   fixed-integer permutation tests without float/trigonometric code.
9. A degree-one LOW/LOW dead end reverses; missing requested-side candidates stop; budget can cross
   multiple positive-length edges without a zero-cost loop.
10. Removing an occupied edge/Junction is deterministically rejected, unrelated topology edits do
    not move a Mobile, and identical-geometry rebuilds never inherit an old Track identity.
11. Multiple Mobiles produce the same positions and V4 hashes under reversed store/command layout
    and do not reserve Track capacity or collide.
12. Replay execution, restart, FPS partitions, presenter enabled/disabled, and Headless/Bevy hosts
    produce the same per-Tick V4 hashes.
13. Mobile snapshot, arrows, Circuit View, picking, STOP/LEFT/RIGHT probing, and inspector reads do
    not change Tick, event order, or State Hash.
14. A retained `A → Junction → B → State → STOP/RETURN` scenario demonstrates behavior generated
    only by AND/OR/NOT/Wire feedback and low-level ports, with no FSM, memory, destination, or route
    planner runtime type.
15. Stateful mobility fuzzing covers placement, bindings, turns, removal, checked arithmetic, and
    retained replica agreement without panic.
16. Format, metadata/check, strict Clippy, workspace tests, dependency boundary, fresh clean
    checkout, and native mobile probe smoke all pass without warnings.

Passing these gates completes S0-M7 only. Stage 0 technical conformance and the manual product gate
remain separate required evidence.

## Non-normative implementation evidence

The current Windows-native workspace contains executable evidence for gates 1–15, the retained V4
Mobility Replay, and native mobile-probe smoke. Formal completion remains pending the final
committed fresh clean-checkout portion of gate 16. Stage 0 product acceptance remains a separate
user-direct play verdict documented in `AON_Stage0_Product_Gate_Playtest_v1.0.md`.
