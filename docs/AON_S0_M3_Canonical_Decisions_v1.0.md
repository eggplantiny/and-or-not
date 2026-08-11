# A/O/N — S0-M3 Canonical Decisions

**Status:** implementation baseline  
**Applies to:** `S0-M3 — Signal Topology / Event Runtime`

This document closes the S0-M3 representation, identity, ordering, and arithmetic decisions that
are left open by the PRD, SSS, and TRD drafts. Within S0-M3, the rules below are normative.
Changing an observable rule requires a semantics/schema version review and new golden fixtures.

## Scope and milestone boundary

S0-M3 implements the first deterministic signal runtime:

- Driver and Sink identity and canonical stores
- Gate signal state and lossless Wire excitation state
- explicit-binding Signal Graph compilation
- one canonical Driver-to-Sink route
- DriverTransition and SignalArrival calendars
- Gate inertial generation tokens
- transport Wire delay
- simultaneous Sink resolution
- scheduled-event and signal-state hashing

S0-M3 does **not** claim that the engine or even Stage 0 is complete. The following remain owned by
S0-M4 and must not be reported as S0-M3 evidence:

- Route Diff and route-add synchronization
- Driver Revision advancement
- TopologySyncArrival generation
- Sink Slot revision comparison
- PathCertificateArena
- connection-generation validation of arrivals
- stale arrival rejection after an in-flight topology edit

Feedback/startup replay fixtures, replay artifacts/checkpoints, the Bevy probe, and mobility remain
owned by S0-M5 through S0-M7. C-01 through C-03 use a static topology after their first observed
input transition. A signal run with an in-flight topology edit is therefore explicitly incomplete
until S0-M4.

All twelve simulation phases remain conceptually present. A phase with no implemented Stage 0
work is a no-op boundary; it is not merged into another phase.

## Signal endpoint identity

`DriverId` and `SinkId` are typed IDs in independent monotonic namespaces. Their `u64` payload is
not looked up in `EntityRegistry`, even though the Rust newtypes continue to wrap `EntityId` for
the frozen public and command-encoding shape.

Each namespace follows these rules:

- numeric ID `0` is reserved;
- the first allocated ID is `1`;
- IDs increase monotonically and never wrap or reuse a tombstone;
- allocator frontier, live slots, and tombstones are canonical state;
- dense indices, capacity, and free-list layout are not canonical;
- exhaustion is a fatal `NumericOverflow` and rolls back the whole Tick.

This preserves the S0-M2 rule that a Gate placement consumes exactly one global structural
`EntityId`. Signal endpoint allocation never changes the structural ID trace.

### Gate endpoint allocation

An accepted Gate creation allocates endpoint IDs immediately in the following role order.

Driver namespace:

1. `ExternalInputA`
2. `ExternalInputB` for AND/OR only
3. `GateOutput`

Sink namespace:

1. `InputA`
2. `InputB` for AND/OR only

The Gate owns all of these endpoints. NOT has no InputB Driver or Sink. Gate removal tombstones all
owned endpoints, removes owned Sink slots, and never rewinds either frontier. A Gate created and
removed within one Phase 0 still consumes and tombstones its endpoint IDs.

```rust
pub struct GateInputSignalPort {
    pub sink: SinkId,
    pub external_driver: DriverId,
}

pub struct GateSignalPorts {
    pub input_a: GateInputSignalPort,
    pub input_b: Option<GateInputSignalPort>,
    pub output: DriverId,
}
```

The external input Drivers are Laboratory injection points. They begin as `Low` with strength
zero, so their mere existence does not drive a net. A Gate Output Driver begins as `Low` with
strength zero, matching SSS startup semantics.

## SetExternalDriver semantics

`SetExternalDriver` becomes supported in S0-M3 without changing command tag `7` or its frozen
payload.

Only a live `ExternalInputA` or `ExternalInputB` Driver accepts the command. A Gate Output Driver
cannot be overwritten by Laboratory input.

Validation uses the Driver allocator frontier captured at the start of the command batch:

1. ID `0` or an ID at/after the batch-start frontier -> `UnknownDriver`
2. allocated tombstone -> `RemovedDriver`
3. live non-external Driver -> `InvalidDriverKind`
4. live external Driver -> accepted

The new stable rejection tags append to the S0-M2 range:

```text
UnknownDriver     = 18
RemovedDriver     = 19
InvalidDriverKind = 20
```

An endpoint allocated earlier in the same Tick therefore cannot be guessed and referenced. A
later Tick may obtain its ID through the public Gate-port observation API.

Same-Tick commands for one external Driver are all accepted in ordinal order and coalesce to the
last ordinal's `(level, strength)`. Only that final request can stage a DriverTransition. If the
final request equals the active sample, all commands remain accepted but no event or payload ID is
allocated. If the owner Gate is removed by a later command in the same Phase, the accepted request
is discarded with the endpoint lifecycle; it does not resurrect a tombstone.

The coalesced transition is due in the current Tick. Phase 2 applies it before SignalArrival
processing and before Phase 3 Gate evaluation.

## Signal nodes and connectivity

The reconstructible Signal Graph uses canonical node keys:

```rust
pub enum SignalNodeKey {
    GatePort(GateId, GatePort),
    Junction(JunctionId),
    FreeEnd(WireId, WireEnd),
}
```

Connectivity comes only from accepted `EndpointTarget` bindings:

- a Wire connects its End A node to its End B node;
- a Junction target maps every incident Wire end to the same Junction node;
- Gate Input/Output targets map every bound Wire end to that exact Gate-port node;
- every Free endpoint maps to its own `(WireId, WireEnd)` node;
- coordinate equality, crossing, or Gate-boundary contact without an explicit target never adds
  connectivity;
- a Gate never connects Input A, Input B, and Output through its body.

`GatePort::Power` is not a Signal node shared between Wires. For the Signal Graph, each Wire end
bound to Power is a unique terminal, because a Power attachment belongs only to the Power Graph.

An external input Driver and its own Sink share the same Gate input node. Their route has no Wire
and has delay zero, so a Laboratory input changes its local Sink in the same Phase 2. Any route
containing at least one Wire has delay of at least one Tick.

Compiled adjacency, components, routes, and route-load summaries are reconstructible cache. They
must not enter state hashing.

## Canonical route selection

For every live Driver/Sink pair in one Signal component, compile at most one route using this total
priority:

1. total canonical Euclidean Wire length
2. segment count, defined as `sum(points.len() - 1)` over traversed Wires
3. ordered path elements, lexicographically
4. final canonical node key

An ordered path element is `(kind, EntityId)` with `Wire=0`, `Junction=1`. The sequence follows
physical traversal and therefore has the form `Wire, Junction, Wire, ...` when Junctions occur.
Gate IDs at the fixed source/target ports are not repeated in the path key. All adjacency
iteration and queue tie-breaking uses canonical node and entity order; insertion order, SoA order,
and heap order cannot select a route.

For load calculation:

- `reachableSinkCount` is the number of unique live Sinks in the Driver's connected component;
- `totalConnectedWireLength` is the sum of each unique live Wire length in that component exactly
  once;
- a Gate resets load because its Output is a distinct Driver and no Gate body edge exists.

## Logic and Sink resolution

The logic domain remains `Low`, `High`, and `X`; it is never reduced to `bool`.

For each Sink, Phase 2 first applies every due slot update simultaneously, then resolves that Sink
exactly once with wide non-wrapping accumulators:

```text
H = sum(strength of High slots)
L = sum(strength of Low slots)
U = sum(strength of X slots)
T = logicThreshold

U >= T                 -> X
H >= T and L >= T      -> X
H >= T                 -> High
L >= T                 -> Low
otherwise              -> Low
```

No valid Driver slot resolves to passive `Low`. A slot is identified by `(DriverId, SinkId)`.
Contention heat belongs to the later power/thermal milestone; S0-M3 preserves enough slot data to
compute it without inventing a partial thermal result.

Gate truth tables are exactly the SSS tables:

- AND: any Low -> Low; both High -> High; otherwise X
- OR: any High -> High; both Low -> Low; otherwise X
- NOT: Low -> High; High -> Low; X -> X

Phase 3 writes each Gate's `desired_output` from the Phase 2 resolved input snapshot.

## Gate inertial state

Canonical Gate signal state contains:

```text
ports
current_output
desired_output
pending_generation:u32
pending_due_tick:Option<Tick>
pending_level:Option<LogicLevel>
pending_switch_energy:Option<Energy>
cancelled_switching_heat:HeatEnergy
```

The Output Driver sample level and Gate `current_output` must agree at every committed Tick.

Phase 6 follows this state machine:

- no pending event and desired differs from current: advance generation once and schedule;
- a pending event already targets the same desired level: preserve its generation and due Tick;
- pending target changes to another non-current level: add its saved switching energy to canceled
  heat, advance generation once, and replace it;
- desired returns to current: add saved switching energy to canceled heat, advance generation
  once, and clear pending state;
- a due event matching Gate, generation, due Tick, and target applies simultaneously in Phase 2,
  updates Gate output and Driver sample, and clears pending state;
- a generation-mismatched event is stale and is drained without a state transition.

The queue never searches for and deletes an invalidated event. Generation overflow is fatal and
rolls back the Tick.

Stage 0 power and thermal factors are exactly `1/1`. A newly powered AND/OR whose logic remains Low
still receives a one-Tick `GateStrengthResponse` from strength zero to
`nominalGateDrive`; NOT normally combines startup level and strength in its first logic
transition. Strength-only response uses the same DriverTransition calendar and does not alter Gate
logic level.

## Exact load, delay, and energy arithmetic

Balance Profile schema v2 adds one required positive field:

```text
gateSwitchBaseEnergy:u64
```

All Stage 0 reference, capacity-probe, and radiation-reference Balance artifacts use schema v2.
Numeric and Physical Scale profiles remain schema v1. The profile schema number and the new field
participate in canonical profile hashing.

`wireQuadraticK` must be strictly positive. `wireLinearK` remains nonnegative.

Let `F = FIXED_ONE` and `L` be a nonnegative canonical Fixed length.

```text
wireLoad = ceil(
    wireLoadPerWU.numerator * L
    /
    (wireLoadPerWU.denominator * F)
)

load = inputLoad * reachableSinkCount + wireLoad

fanoutPenalty = ceil(
    max(0, load - fanoutFreeLoad)
    /
    fanoutStep
)

gateDelay = max(1, gateBaseDelay + fanoutPenalty)
```

For a physical route, evaluate the two Wire-delay rational terms without intermediate rounding and
ceil once at the end:

```text
k1 = n1 / d1
k2 = n2 / d2

N = n1 * L * d2 * F + n2 * L^2 * d1
D = d1 * d2 * F^2

wireDelay = max(1, ceil(N / D))
```

A zero-Wire local route is the only route with delay zero. Canonical Wire length already collapses
same-direction collinear runs for length calculation while preserving raw vertices in state.

Switch energy is fixed at reservation time:

```text
switchEnergy = gateSwitchBaseEnergy * (1 + load)
```

All products, sums, denominators, conversions, Tick additions, and final type conversions use
checked exact arithmetic. An unrepresentable result is `NumericOverflow`, never saturation or
wrapping.

## Wire excitation state

The TRD draft's signed `i64` Wire drive fields cannot losslessly represent X, multi-driver
contention, or a `u64` strength. S0-M3 therefore replaces that placeholder with:

```rust
pub struct DriveVector {
    pub high: u128,
    pub low: u128,
    pub unknown: u128,
}

pub struct WireSignalState {
    pub active: DriveVector,
    pub previous: DriveVector,
}
```

After Driver application in Phase 2, every Wire in a component receives the lossless sum of active
source Drivers attached to that component. `previous` is the Wire's `active` value at Tick entry;
`active` is the post-Driver-apply value. This is a canonical source-excitation observation for the
Wire body. It does not bypass transport delay: remote Sinks change only through SignalArrival.

The maximum number of Drivers and maximum `u64` strength fit their exact total in `u128`; checked
addition still guards invariant violations.

## Event representation and total order

```rust
pub struct EventKey {
    pub due_tick: Tick,
    pub kind_order: u8,
    pub target_id: u64,
    pub source_id: u64,
    pub revision: Revision,
    pub generation: u32,
    pub payload_order: u64,
}
```

Canonical tags:

```text
DriverTransition                 kind_order = 0
SignalArrival                    kind_order = 1

DriverTransitionCause:
ExternalDriver=0, GateOutput=1, GateStrengthResponse=2

SignalArrivalKind:
Propagation=0, TopologySync=1 (reserved; not generated in S0-M3)

LogicLevel: Low=0, High=1, X=2
```

Revision fields are present and encoded as zero in S0-M3. `TopologySync` and
`path_certificate=Some` are reserved but not generated until S0-M4.

Events are staged, never pushed while iterating a canonical store. Staged candidates are sorted by
their complete payload excluding `payload_order`, exact duplicates are removed, and then a
canonical monotonic payload allocator assigns IDs in that order. Payload ID zero is reserved; its
frontier and tombstones are canonical and never reused.

The calendar representation may be a heap or ordered map. Its internal order and capacity are not
canonical. Serialization and application always use ascending EventKey.

For Tick `t`:

- every event with `due_tick == t` is drained;
- a retained event with `due_tick < t` is an invariant violation;
- events with `due_tick > t` remain pending;
- all Driver events are grouped and applied simultaneously before any Signal event;
- different non-stale samples for one Driver in the same group are an invariant violation;
- Driver changes stage physical future arrivals and zero-Wire current-Tick arrivals;
- existing and newly staged current-Tick Signal arrivals are grouped together;
- different samples for the same `(Driver, Sink)` in one due group are an invariant violation;
- every dirty Sink resolves once after all slot writes.

A real Driver sample change stages one arrival for each currently compiled route. An unchanged
sample stages none. Wires use transport, not inertial, delay: High at `t` and Low at `t+1` produce
arrivals exactly one Tick apart after any fixed route delay.

## Phase order and transaction

One call to `Simulation::step` operates on a complete clone/candidate and commits only after every
phase succeeds:

```text
Phase 0  command ordering, structural edits, endpoint lifecycle,
         external request coalescing, topology revision, full Signal compile
Phase 1  immutable resolved-Sink and topology snapshot
Phase 2  DriverTransition apply, propagation staging, SignalArrival apply,
         Sink resolve, Wire excitation update
Phase 3  Gate desired-output evaluation
Phase 4  Stage 0 accounting no-op
Phase 5  Stage 0 power/thermal factors = 1/1
Phase 6  Gate inertial schedule/cancel and startup strength response
Phase 7-10 no-op boundaries for S0-M3
Phase 11 next Tick, post-step canonical hash, commit
```

Failure in command-independent canonical work rolls back structural state, signal state, both ID
frontiers, both calendars, payload frontier, topology revision, Tick, cache candidate, and state
hash. Player command rejection remains nonfatal and cannot partially mutate its command.

## Public observation surface

Conformance tests and later presentation code use public immutable observations, not test-only
state injection:

```rust
Simulation::gate_signal_ports(GateId) -> Option<GateSignalPorts>
Simulation::driver_sample(DriverId) -> Option<DriverSample>
Simulation::sink_level(SinkId) -> Option<LogicLevel>
Simulation::gate_signal_state(GateId) -> Option<GateSignalSnapshot>
Simulation::wire_signal_state(WireId) -> Option<WireSignalSnapshot>
```

`StepReport` adds canonically sorted Driver and Sink change records plus signal counters. These are
observations and are not themselves state-hash input. Gate snapshots expose pending generation,
due Tick, current/desired level, and canceled switching heat so C-02 can be asserted without
private access.

## Canonical state encoding v2

S0-M3 changes the state domain and encoder to `AON\0STATE\0V2\0`, version `2`. It does not silently
reinterpret the completed S0-M2 byte layout. The v2 section order is:

1. Simulation Contract, next Tick, topology revision
2. structural Entity registry frontier and slots
3. structural Gate, Wire, Junction, Fixed Substrate records
4. Driver frontier and allocation slots
5. Sink frontier and allocation slots
6. Gate signal records in Gate EntityId order
7. Wire excitation records in Wire EntityId order
8. Sink Driver slots in `(SinkId, DriverId)` order
9. event payload frontier
10. DriverTransition events in EventKey order
11. SignalArrival events in EventKey order
12. reserved later-stage Mobile, destruction, radiation, relay, and path-certificate sections

Live Driver records encode ID, owner Gate ID, role, and complete DriverSample. Tombstones encode ID
and alive tag only. Live Sink records encode ID, owner Gate ID, role, resolved level, and dirty tag;
dirty must be false at a committed Tick. Gate signal records encode every field listed above. Wire
records encode both DriveVectors as little-endian `u128` values. Slot records encode both IDs,
level, strength, revision, and emitted Tick.

Each event encodes its complete EventKey and payload, including cause/kind, sample, pending
generation, and absent path-certificate tag. Counts and all integer widths are fixed-width little
endian.

The hash excludes compiled components/routes, adjacency order, priority-queue layout, staging,
scratch, dense indices, SoA capacity, presentation records, and StepReport observations.

## S0-M3 completion gates

S0-M3 is not complete until all of the following are executable deterministic tests:

1. endpoint IDs are monotonic, typed, tombstoned, never reused, and independent of structural IDs;
2. same-batch predicted Driver IDs reject and next-Tick observed IDs accept;
3. external Driver wrong-kind, unknown, removed, same-value no-op, and ordinal-last coalescing are
   stable;
4. explicit bindings compile while crossings and equal Free coordinates stay disconnected;
5. route length, segment, and lexicographic ties are independent of store/adjacency order;
6. local routes have delay zero and every physical route has positive superlinear delay;
7. exact load, fanout, delay, energy, due-Tick, generation, and accumulator overflow roll back the
   whole Tick;
8. multi-driver Low/High/X resolution is permutation-invariant and each Sink resolves once;
9. inertial replacement/cancel leaves stale events harmless and canceled energy as heat;
10. transport preserves a one-Tick pulse;
11. unchanged Driver samples do not emit arrivals;
12. event insertion/staging permutations produce identical calendars and state hashes;
13. canonical event/signal sensitivity and cache/SoA/calendar-layout exclusions are fixed;
14. C-01 observes NOT internal transition at relative `t=1` and downstream Wire arrival at `t=4`;
15. C-02 observes no output pulse for a two-Tick input pulse against delay three and observes the
    exact canceled switching heat;
16. C-03 observes a one-Tick pulse exactly five Ticks later;
17. identical public command streams produce identical per-Tick hashes;
18. stateful fuzz reaches valid external updates, stale IDs, wrong kinds, event ordering, and
    checked arithmetic without panic;
19. workspace format, check, strict Clippy, tests, dependency boundary, and clean-checkout offline
    gates pass.

S0-M4 stale-route fixtures and S0-M5 replay/feedback fixtures remain mandatory subsequent gates;
passing this list is not permission to label the whole engine complete.
