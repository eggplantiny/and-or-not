# A/O/N S0-M4 Canonical Decisions v1.0

Status: implementation authority for S0-M4

This document resolves the implementation choices required by `S0-M4 — Topology Sync / Path
Certificate`. It refines the Simulation Semantics Specification and TRD where their prose leaves
more than one deterministic implementation possible.

S0-M4 does **not** complete Stage 0 or the game engine. Feedback/startup replay, the Bevy ASCII
probe, mobility, later stages, MVP integration, and global verification remain subsequent gates.

## Scope

S0-M4 owns:

- four-way pre/post Signal Route Diff;
- Driver Sample Revision advancement;
- revision-aware Sink Driver Slots;
- TopologySync Signal Arrivals;
- per-arrival Path Certificates;
- Connection Generation validation at arrival time;
- stale revision and invalid path discard;
- canonical certificate state and deterministic diagnostics;
- C-18, C-19, and destroy/rebuild invalidation.

S0-M4 does not own replay files/checkpoints, feedback startup policy, mobility, power, capacity,
damage, construction, radiation, or presentation beyond immutable observations needed by its
conformance tests.

## Driver Revision

Every newly allocated Driver starts with this complete Sample:

```text
level       = Low
strength    = 0
revision    = 0
emitted_at  = activation Tick
driver_id   = allocated DriverId
```

A DriverTransition compares only the requested Level and Strength with the committed Sample.

```text
Level changed OR Strength changed
→ revision = old revision + 1 with checked arithmetic
→ emitted_at = current Tick

Level unchanged AND Strength unchanged
→ complete Sample unchanged
→ revision and emitted_at unchanged
→ no propagation Arrival
```

Revision overflow is a Run Error and rolls back the whole Tick. DriverTransition EventKey revision
remains zero because the resulting Revision is not known until simultaneous Phase 2 application.
Every SignalArrival EventKey revision equals its complete `sample.revision`.

## Revision-aware Sink Driver Slots

Slots remain keyed by `(SinkId, DriverId)`. Applying one complete incoming DriverSample has these
outcomes:

```text
slot absent
→ Applied, including incoming Revision 0

incoming Revision > stored Revision
→ Applied and Sink dirty

incoming Revision == stored Revision AND complete Sample identical
→ Idempotent, no mutation

incoming Revision == stored Revision AND complete Sample differs
→ fatal canonical invariant violation

incoming Revision < stored Revision
→ Stale, no mutation
```

Complete Sample equality includes Level, Strength, Revision, emitted Tick, and DriverId.

All valid due Arrivals for one `(Sink, Driver)` are grouped before mutation. Every Revision bucket,
including a bucket that will later be classified stale, must contain one identical complete Sample;
equal-Revision/different-Sample is always fatal. The greatest Revision is then selected. A lower
Revision is counted stale. The selected Sample is compared with the existing Slot using the table
above. This grouping, not Event insertion order, defines the result.

Before selecting a winner, an Arrival bucket whose Revision equals the stored Slot Revision must
also equal the stored complete Sample. A higher Revision in the same due group does not hide or
legalize an equal-Revision conflict.

Raw-event counters use this exact rule:

- if the stored Revision is greater than the group's maximum, every Event in the group is stale;
- if the stored Revision equals the group maximum, lower Events are stale and equal identical
  Events are idempotent;
- if the stored Revision is lower or the Slot is absent, lower Events are stale, one maximum Sample
  is Applied, and additional equal maximum Events are idempotent.

## Stamped compiled routes

Route selection continues to use the S0-M3 total order:

```text
1. total Euclidean Wire length
2. segment count
3. ordered EntityId path key
4. final canonical node key as the worklist totalizer
```

Connection Generation does not participate in this tie-break. After all Phase 0 generation
advances are coalesced, the selected ID-only path is stamped in Driver-to-Sink direction:

```rust
pub enum PathElementStamp {
    Wire {
        id: WireId,
        generation: ConnectionGeneration,
    },
    Junction {
        id: JunctionId,
        generation: ConnectionGeneration,
    },
}
```

GatePort and FreeEnd nodes are not certificate elements. A local zero-Wire route has an empty
element sequence. Paths can contain adjacent Wire stamps when they traverse a shared GatePort;
the implementation must not require strict Wire/Junction alternation.

A Route Fingerprint is exact equality of:

```text
ordered PathElementStamp sequence
total length
segment count
route delay
```

DriverId and SinkId are the Route map key, not duplicated in the Fingerprint. Component ID,
adjacency layout, priority-queue layout, total component load, and compiled cache indices are not
Route identity.

## Four-way Route Diff

Route Diff compares the compiled topology immediately before Phase 0 with the topology compiled
from the final Phase 0 candidate. Intermediate command states never stage topology events.

If Phase 0 reports no topology change, no Route Diff diagnostic work is reported and all four route
counters plus `topology_sync_arrivals_staged` are zero. The current topology is still compiled as
needed by signal propagation. If topology changed, the four counters cover every old/new pair
exactly once and:

```text
topology_sync_arrivals_staged = routes_added + routes_replaced
```

For each `(DriverId, SinkId)` in canonical pair order:

```text
old absent, new present             → Added
old present, new absent             → Removed
both present, Fingerprint identical → Retained
both present, Fingerprint different → Replaced
```

The effects are:

- Added: keep the absent Slot absent and stage current-Sample synchronization on the new Route;
- Removed: remove the Slot immediately and mark the Sink dirty if that Sink remains live; endpoint
  lifecycle cleanup already owns a Sink tombstoned in the same Phase;
- Retained: preserve the Slot and all existing in-flight due Ticks; stage no synchronization;
- Replaced: preserve the Slot and stage current-Sample synchronization on the new Route.

Replaced is not modeled as Removed plus Added because a still-reachable Driver must not create a
spurious passive-Low interval. A shorter newly selected path is Replaced even while every element
of the old path remains physically alive. Numeric CompiledRoute IDs are reconstructible cache
identities and are not allocated or made canonical.

No Route Diff class deletes, retimes, or reroutes an existing Event or consumes its Certificate.
This is true for Added, Removed, Retained, and Replaced. Only normal due-time draining consumes a
Certificate.

Bind-away/bind-back in one Phase can finish with the same endpoint geometry, but its coalesced
Connection Generation advance changes the Fingerprint and therefore produces Replaced.

## Phase 0 and Phase 2 order

The S0-M4 portion of a Tick is:

```text
1. compile old Signal topology from the committed candidate
2. apply all Phase 0 structural commands and coalesce Connection Generations
3. compile the final new Signal topology
4. compute Added / Removed / Retained / Replaced
5. remove Removed Slots and mark their Sinks dirty
6. snapshot current Driver Samples for Added and Replaced Routes
7. stage TopologySync Arrivals and their Path Certificates
8. stage accepted external Driver requests
9. drain and simultaneously apply due DriverTransitions
10. advance changed Driver Revisions and stage Propagation Arrivals on the new topology
11. drain all due SignalArrivals
12. consume and validate their Path Certificates
13. apply the revision-aware Slot grouping
14. resolve each dirty Sink once
```

Topology Sync samples the Driver before same-Tick Phase 2 DriverTransitions. If the same Tick also
changes that Driver, the sync carries Revision N and same-Tick propagation carries Revision N+1;
revision grouping makes N+1 win regardless of staging or EventKey order.

## TopologySync timing

TopologySync uses the new Route's exact delay:

```text
due Tick = current Tick + new Route delay
```

The addition is checked and failure rolls back the whole Tick.

- a route containing at least one physical Wire has delay at least one Tick;
- an inherent zero-Wire local Gate-input route has delay zero and may sync in the same Phase 2;
- a zero-Wire sync carries a live empty Path Certificate;
- ordinary zero-Wire propagation remains same-Tick;
- no physical route may bypass its positive Wire delay.

The C-18 “not immediate” rule applies to the newly attached physical route. Gate activation creates
only Low/zero local state. Its absent local Slot is therefore Applied and dirtied, but the resolved
Level remains neutrally Low; it is not classified as a Slot-idempotent Arrival.

TopologySync and Propagation remain variants of SignalArrival. Both use
`SIGNAL_ARRIVAL_KIND_ORDER`; `SignalArrivalKind` is the stable payload tag. Their globally unique
payload orders keep complete EventKeys distinct.

## Path Certificate namespace and arena

PathCertificateId is an independent monotonic `u64` namespace:

```text
0 reserved
first allocated ID = 1
IDs never reused
consumed IDs remain tombstones
```

Each SignalArrival owns exactly one Certificate. Certificates are not shared or reference-counted
in Stage 0. Propagation, TopologySync, physical routes, and local empty routes all follow this rule.

The implementation uses an append-only element arena and no-compaction certificate slots:

```rust
pub struct PathCertificateArena {
    next_id: u64,
    certificates: Vec<Option<PathCertificate>>,
    elements: Vec<PathElementStamp>,
}

pub struct PathCertificate {
    pub id: PathCertificateId,
    pub element_range: Range<u32>,
}
```

Raw element offsets/ranges, allocation capacity, and unreachable bytes left by consumed
Certificates are implementation layout. The canonical view is the PathCertificateId-ordered slot
sequence and each live slot's logical ordered elements.

## Canonical Arrival staging

Certificate ID and Event payload order are separate namespaces and must not be forced equal.
Driver events share the payload namespace but do not allocate Certificates.

Signal staging uses an unassigned candidate containing:

```text
due Tick
source Driver
target Sink
complete Driver Sample
SignalArrivalKind
ordered PathElementStamp sequence
```

The transaction is:

```text
validate unassigned candidates
→ canonical sort by complete semantic candidate
→ exact duplicate removal
→ preflight Certificate frontier and u32 element ranges
→ preflight shared payload frontier and calendar insertion
→ allocate Certificate IDs in candidate order
→ attach Certificate IDs to SignalArrival payloads
→ allocate payload orders in the same candidate order
→ insert Events
```

Certificate IDs, payload IDs, ranges, and calendars remain unchanged if any preflight or insertion
fails. The surrounding CandidateWorld transaction is still the final rollback boundary.

Certificate ID is not part of the pre-allocation sort key. The ordered element contents are. This
prevents input iteration order from choosing IDs.

The exact candidate comparator is lexicographic over:

```text
due Tick
Event kind order (= SignalArrival)
target SinkId
source DriverId
Sample Revision
Event generation (= 0)
payload source DriverId
payload SinkId
Sample Level tag
Sample Strength
Sample Revision
Sample emitted Tick
Sample DriverId
SignalArrivalKind tag
ordered PathElementStamp sequence
```

Each PathElementStamp compares `(kind tag, EntityId, ConnectionGeneration)`, where Wire is tag 0
and Junction is tag 1. Exact duplicate removal uses equality of this complete sequence before any
Certificate or payload ID is assigned.

## Arrival shape and Certificate lifetime

Every committed pending SignalArrival has:

- nonzero payload order;
- EventKey target/source matching Sink/Driver;
- EventKey revision matching `sample.revision`;
- EventKey generation zero;
- `sample.driver_id` matching source Driver;
- `path_certificate = Some(nonzero live PathCertificateId)`;
- exactly one live Certificate reference;
- a Certificate referenced by no other Event.

Every live Certificate is referenced by exactly one pending SignalArrival. A missing, already
consumed, duplicate, or out-of-frontier Certificate reference is a fatal canonical invariant
violation.

Topology edits are allowed to make the elements of a pending Certificate invalid. Committed-state
validation must **not** require pending paths to match the current World. Validity is checked only
when the Event becomes due.

Draining a due SignalArrival consumes its Certificate before lifecycle/path/revision
classification. Consumption tombstones the Certificate slot regardless of whether the Arrival is
applied, idempotent, stale, endpoint-invalid, or path-invalid. A later Tick failure rolls this back
with the whole CandidateWorld.

## Path validation

At due time, each Certificate element is checked in stored order:

```text
typed entity is missing
OR EntityId does not identify the stamped type
OR current Connection Generation differs
→ invalid path discard
```

The runtime does not compare the Certificate with the current compiled Route and never reroutes an
old Event. An old route that remains physically valid can still deliver even after a shorter route
becomes canonical. Revision comparison, not rerouting, prevents it from restoring stale state.

An unrelated topology edit does not invalidate a Certificate. Removing or rebinding a stamped
Wire/Junction does. Destroying and rebuilding the same geometry creates a new EntityId and cannot
inherit an old Event.

If the source Driver or target Sink has been removed, the Arrival is classified with invalid-path
diagnostics after its Certificate is consumed. Removed endpoint IDs are not canonical-shape errors.

## Simultaneous due-Arrival classification

The classification precedence is:

```text
canonical Event/Certificate reference shape violation → fatal Run Error
Certificate consume
removed endpoint or invalid stamped path              → invalid-path discard
valid path, lower Revision                             → stale-revision discard
equal Revision and identical Sample                   → idempotent
equal Revision and different Sample                   → fatal Run Error
higher Revision or absent Slot                         → apply
```

Events are first grouped by `(SinkId, DriverId)` after path validation, then resolved by the
revision table. Sequential BTreeMap/EventKey iteration must not become observable semantics.

## Diagnostics and public observation

StepReport remains non-canonical. S0-M4 extends its deterministic counters with:

```text
routes_added
routes_removed
routes_retained
routes_replaced
topology_sync_arrivals_staged
stale_revision_arrivals
invalid_path_arrivals
idempotent_signal_arrivals
```

`signal_arrivals_applied` counts Slot creations/replacements, not raw due Events. A lower-revision
raw Event counts once as stale. Equal identical duplicates count as idempotent. Each dirty Sink
still resolves once.

DriverSample Revision is already exposed by `Simulation::driver_sample`. S0-M4 additionally exposes
the immutable applied Slot Sample needed by C-18/C-19:

```rust
Simulation::sink_driver_sample(SinkId, DriverId) -> Option<DriverSample>
```

No mutable Certificate or compiled topology API becomes public. Tests observe due behavior through
StepReport, Driver/Slot Samples, Sink levels, hashes, and existing immutable Gate/Wire snapshots.

All counters and observations are excluded from State Hash input.

## Errors and whole-Tick rollback

The following are checked before commit and roll back structural/signal state, all ID frontiers,
Certificate slots/elements, both Event calendars, payload frontier, topology revision, Tick, and
hash:

- Driver Revision overflow;
- sync due-Tick overflow;
- PathCertificateId exhaustion;
- certificate slot/index overflow or failed `u32` element-range preflight;
- Event payload exhaustion;
- equal-Revision/different-Sample conflict;
- malformed Event/Certificate ownership;
- any existing canonical topology numeric error.

The public Run Error taxonomy follows TRD §37.1:

- arithmetic/frontier/range exhaustion maps to `SimulationError::NumericOverflow`;
- equal-Revision/different-Sample and impossible Revision relations map to
  `SimulationError::DriverRevisionInvariantViolation`;
- missing, consumed, duplicate, malformed, or orphan Certificate ownership and arena-shape errors
  map to `SimulationError::PathCertificateInvariantViolation`;
- malformed non-certificate EventKey/calendar relations map to
  `SimulationError::EventQueueInvariantViolation`;
- other cross-store structural inconsistencies remain `SimulationError::InvalidCanonicalState`.

An Arrival whose formerly valid stamped path was changed or destroyed is not a Run Error; it is an
`invalid_path_arrivals` discard.

Player command rejection remains nonfatal and cannot partially consume any canonical ID.

Dense in-memory storage makes a real `u64` slot frontier or `u32` element arena limit impractical to
reach by allocating every preceding record in a test. Test-only frontier seams may exercise the
whole CandidateWorld rollback for ID exhaustion. Pure range/frontier preflight tests must exercise
the exact boundary and prove no local mutation; tests must not allocate terabytes or claim an
invalid pre-seeded World is a valid public fixture.

## Canonical state encoding v3

S0-M4 changes the state domain and encoder to `AON\0STATE\0V3\0`, version `3`. Although S0-M3
reserved a final empty path-certificate marker, a nonempty arena introduces new canonical field
meaning and variable records. A new version prevents an older V2 reader from silently
reinterpreting those bytes.

V3 preserves every V2 section through the SignalArrival calendar. The tail is:

```text
Mobile reserved count      u64 = 0
destruction reserved count u64 = 0
radiation reserved count   u64 = 0
relay reserved count       u64 = 0

PathCertificate frontier        u64
PathCertificate allocated_count u64
for every ID where 1 <= ID < frontier:
    id                 u64
    alive              u8
    if alive:
        element_count  u32
        for each ordered element:
            kind       u8  (Wire=0, Junction=1)
            entity_id  u64
            generation u64
```

Frontier is at least one and `allocated_count = frontier - 1`. Tombstones encode only ID and alive
tag. Live raw range offsets are excluded. Every integer is fixed-width little-endian.

SignalArrival keeps its existing optional-certificate tag for explicit encoding compatibility,
but committed V3 pending events require the tag to be present. Driver/Sink Revision fields already
exist in their V2 positions and now admit nonzero values. TopologySync uses its already reserved
arrival-kind tag.

The V2 empty and populated goldens are intentionally replaced by V3 goldens. This is an explicit
state-hash migration, not a silent semantic change.

State encoder version and Simulation semantics version are distinct contracts. S0-M4 keeps
`SEMANTICS_VERSION_V1` / `aon-semantics-v1`, the Scenario schema, and all Profile contracts
unchanged. Only the canonical state byte domain/version migrates from V2 to V3.

## Canonical invariants

In addition to S0-M3 invariants, mutation tests and commit/hash validation enforce:

- all Driver mutation paths use checked monotonic Revision advancement; a current-state validator
  does not pretend that it can reconstruct prior history;
- each Slot key and embedded IDs agree and both its Driver and Sink are live;
- a pending Arrival or Slot Sample Revision is not greater than its live source Driver's current
  Revision; a removed source is handled by due-time lifecycle classification;
- pending SignalArrival key revision equals Sample revision;
- pending Event ↔ live Certificate ownership is one-to-one;
- Certificate frontier is nonzero, slot zero is empty, and `slots.len() == frontier` through checked
  conversion;
- every live Certificate ID equals its slot ID and has `start <= end <= elements.len()`;
- live Certificate ranges do not overlap; tombstoned ranges and orphan element bytes are not
  validated or hashed;
- live Certificate elements have nonzero typed IDs below the structural allocation frontier;
- path validity against current generations is deliberately not a commit invariant;
- all due Ticks are at least committed `next_tick`;
- diagnostic/cache/route-diff layout remains outside canonical state.

## S0-M4 completion gates

S0-M4 is not complete until all of the following are executable deterministic tests:

1. Driver Revision starts at zero, advances only on Level/Strength change, preserves emitted Tick
   on no-op, and overflows transactionally;
2. absent/greater/equal-identical/equal-conflict/lower Slot cases, including stored-r3 conflict plus
   a valid r4 winner in one group, and insertion permutations follow the frozen table;
3. Added, Removed, Retained, and Replaced Route Diff is independent of command/store/adjacency
   order;
4. sync sampling before same-Tick Driver change lets Revision N+1 win over sync Revision N;
5. local empty-route sync is same-Tick and physical sync always keeps positive delay;
6. C-18 keeps a newly attached Sink passive Low until the exact new-route delay, then applies the
   current Driver Revision;
7. C-19 lets a still-valid old Revision 3 path Event arrive after Revision 4 without reverting the
   Slot;
8. a shorter newly selected Route produces Replaced + sync without changing an old Event's due
   Tick or Certificate;
9. Removed Route Slot deletion resolves a Sink passive Low even with no due Arrival;
10. Wire/Junction remove, rebind, and bind-away/back invalidate only stamped-generation Events;
11. destroying and rebuilding identical geometry with a new ID invalidates the old Event;
12. unrelated topology edits leave pending Certificates valid;
13. Certificate IDs are monotonic/tombstoned/non-reused and candidate permutations allocate the
    same Certificate and payload IDs;
14. local empty, single-Wire, adjacent Wire/Wire through a GatePort, and Wire/Junction/Wire
    Certificate element sequences are exact;
15. invalid current paths may remain pending, while missing/consumed/duplicate Certificate
    ownership is fatal;
16. Certificate/payload/due/Revision exhaustion rolls back the whole Tick through bounded
    test seams, while exact slot/range preflight boundaries fail without local mutation;
17. V3 exact empty/populated bytes, field sensitivity, tombstone sensitivity, and raw arena
    layout/capacity exclusions are fixed;
18. Certificate fields are hash-sensitive, while reading route-diff/counter/public observations
    does not mutate or change State Hash;
19. stateful fuzz reaches in-flight add/remove/rebind/rebuild, stale Revision, invalid generation,
    same-Tick sync/propagation, and checked arithmetic without panic;
20. identical public streams and retained corpus cases produce identical per-Tick hashes;
21. format, metadata, check, strict Clippy, workspace tests, dependency boundary, and fresh clean
    checkout offline gates pass.

Passing this list completes only S0-M4. S0-M5 feedback and replay fixtures remain mandatory before
the Stage 0 technical gate.
