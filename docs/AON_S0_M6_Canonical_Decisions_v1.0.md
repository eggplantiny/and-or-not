# A/O/N S0-M6 Canonical Decisions v1.0

Status: implementation authority for S0-M6

This document resolves the implementation choices required by `S0-M6 — Bevy ASCII Probe`. It
refines the Simulation Semantics Specification and TRD where their presentation and host-control
prose leaves more than one reproducible implementation possible.

S0-M6 completes neither Stage 0 nor the game engine. Mobility, the Stage 0 technical and product
gates, Stage 1/2, MVP integration, and the supported release-platform matrix remain later work.

## Scope and exclusions

S0-M6 owns:

- an exact, read-only Stage 0 structural/signal projection from `aon-sim`;
- due Signal Arrival observations sufficient to distinguish ordinary propagation from topology
  synchronization;
- a Bevy-host action queue and Laboratory command scheduler;
- Paused, Single Step, and rational `1/4x`, `1x`, and `4x` host pacing;
- a presentation-only `CellBuffer`, deterministic rasterization, picking, and two discrete LODs;
- interactive placement of the currently supported fixed Stage 0 primitives, delete, bind, and
  external-input drive;
- at most eight signal probes, a 256-Tick waveform, revision/arrival markers, and a snapshot-only
  inspector;
- interactive reset and read-only Replay playback/restart;
- a native renderer with a repository-owned monospace font asset.

S0-M6 does **not** add or implement:

- `PlaceMobileSubstrate`, Track Graph, TrackPosition, STOP/LEFT/RIGHT, movement, mobile rendering,
  or mobile inspection; these belong to S0-M7;
- Network/Capacity, Power, Heat gameplay, Radiation, Sensing overlays, Relay, construction,
  damage, payload, or enemies;
- `AnalyzerSnapshot`, behavioral classification, automatic circuit names, modules, save states,
  Replay branching, or Replay editing;
- continuous zoom, camera polish, a final art renderer, a final accessibility pass, or web
  packaging.

The current `Command::PlaceMobileSubstrate` wire tag remains valid, but the S0-M6 editor never
offers it and Core continues to reject it as `UnsupportedPlacement` until S0-M7. No mobility-shaped
placeholder is added to the S0-M6 snapshot or CellBuffer.

## Authority and version boundary

The SSS remains authoritative for Canonical State and Tick semantics. The S0-M1 through S0-M5
decision documents remain authoritative for numeric, identity, command, signal, topology, and
Replay contracts. This document freezes their S0-M6 observation and host presentation.

`RenderSnapshot`, `StepReport`, probe selection, waveform history, CellBuffer contents, HostAction,
camera/view state, and Bevy entities are not Canonical State. They are excluded from State Hash and
Replay bodies. Adding the projections below therefore does not change `aon-semantics-v1`, State
Hash encoder V3, Scenario schema v1, or Replay format v1.

No presenter code, Bevy type, font handle, color, CellBuffer cell, probe registry, selection,
preview, or wall-clock value may enter `aon-sim` Canonical State. `aon-sim` remains Pure Rust with
no Bevy dependency.

## Single-owner host boundary

The frozen Bevy schedule is:

```text
PreUpdate
  raw input -> ordered HostAction values
  HostAction values -> host-only intent/command queues

FixedUpdate
  advance_canonical_simulation (the sole mutable owner of Simulation)
    reset/restart if requested
    obtain rational Tick credit or consume one Single Step request
    call Simulation::step zero or more times
    after every successful step: retain StepReport, refresh RenderSnapshot,
                                 sample probes, append trace/history

Update
  read LatestRenderSnapshot, reports, host state, selection, and waveform only
  build CellBuffer, inspector, and text batches
```

`advance_canonical_simulation` is the only system that requests `ResMut<CanonicalSimulation>`.
PreUpdate and Update do not access a mutable `Simulation`. Update does not re-read Core stores; it
uses the latest owned snapshot. An initial snapshot is written when a session is installed and a
new snapshot is written immediately after reset, so Paused Tick 0 is renderable without stepping.

When one FixedUpdate pulse advances multiple Ticks at `4x`, the host refreshes and samples after
**each** Core step. It may publish only the latest world image to Update, but it must not collapse
the intermediate hash trace, StepReports, or waveform samples.

## Core RenderSnapshot projection

S0-M6 expands the existing scalar snapshot into the following semantic shape. Rust fields may
remain private behind read-only getters, but their meaning is fixed.

```rust
pub struct RenderSnapshot {
    scenario_id: String,
    next_tick: Tick,
    contract: SimulationContract,
    topology_revision: Revision,
    primitive_count: u64,
    state_hash: StateHash,
    fixed_substrates: Vec<FixedSubstrateRenderRecord>,
    gates: Vec<GateRenderRecord>,
    wires: Vec<WireRenderRecord>,
    junctions: Vec<JunctionRenderRecord>,
}

pub struct GateRenderRecord {
    pub id: GateId,
    pub gate_type: GateType,
    pub origin: FixedVec2,
    pub routing_domain: RoutingDomain,
    pub ports: GateSignalPorts,
    pub input_a_level: LogicLevel,
    pub input_b_level: Option<LogicLevel>,
    pub input_a_external_sample: DriverSample,
    pub input_b_external_sample: Option<DriverSample>,
    pub output_sample: DriverSample,
    pub current_output: LogicLevel,
    pub desired_output: LogicLevel,
    pub pending_generation: u32,
    pub pending_due_tick: Option<Tick>,
    pub pending_level: Option<LogicLevel>,
    pub pending_switch_energy: Option<Energy>,
    pub cancelled_switching_heat: HeatEnergy,
}

pub struct WireRenderRecord {
    pub id: WireId,
    pub routing_domain: RoutingDomain,
    pub points: Vec<FixedVec2>,         // exact accepted raw world-space vertices
    pub endpoint_a: EndpointTarget,
    pub endpoint_b: EndpointTarget,
    pub connection_generation: ConnectionGeneration,
    pub active_drive: DriveVector,
    pub previous_drive: DriveVector,
    pub active_level: LogicLevel,
    pub previous_level: LogicLevel,
}

pub struct JunctionRenderRecord {
    pub id: JunctionId,
    pub routing_domain: RoutingDomain,
    pub position: FixedVec2,
    pub connection_generation: ConnectionGeneration,
}

pub struct FixedSubstrateRenderRecord {
    pub id: EntityId,
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,        // substrate-local
    pub footprint: FixedAabb,           // substrate-local
}
```

The projection contains only live entities. Each vector is strictly ascending by its record's
underlying `EntityId`, independent of store-slot order or insertion layout. `primitive_count` is
the Core-validated live count and equals the checked sum of these four vector lengths; its getter
remains available for the bootstrap title.

Gate `output_sample.level == current_output`. `ports` provides the exact Sink/external-Driver/output
Driver identities; the level/sample fields provide their current values without exposing mutable
endpoint stores. For NOT, all input-B fields are `None`; for AND/OR they are `Some`.

The three pending option fields are either all `Some` or all `None`. `pending_generation` and
`cancelled_switching_heat` remain observable even when no transition is pending. The already
validated Canonical World guarantees these relationships; snapshot code does not recover by
emitting a partially populated or invented record.

Wire `active_level` and `previous_level` are resolved by Core from their corresponding
`DriveVector` with the loaded Balance Profile's existing `logicThreshold` rule. The host must not
reimplement signal resolution. The exact accepted point list is copied without snapping,
simplification, coordinate conversion, or endpoint substitution.

`Simulation::write_render_snapshot(&self, &mut RenderSnapshot)` clears and reuses owned buffers
where practical. Calling it any number of times between steps must leave `next_tick`, all Core
observations, pending events, allocator frontiers, and State Hash unchanged. Snapshot format is a
debug API, not an artifact serialization format.

## Due Signal Arrival observations

Counters alone cannot draw a truthful Topology Sync marker. S0-M6 adds this minimal read-only
projection to `StepReport`:

```rust
pub struct SignalArrivalObservation {
    pub due_tick: Tick,
    pub source_driver: DriverId,
    pub sink: SinkId,
    pub sample: DriverSample,
    pub kind: SignalArrivalKind,        // Propagation or TopologySync
}

pub struct StepReport {
    // existing fields remain
    pub signal_arrivals: Vec<SignalArrivalObservation>,
}
```

Every raw Signal Arrival drained as due for `completed_tick` produces exactly one observation,
including events that validation/grouping will later count as invalid-path, stale, or idempotent.
`due_tick == completed_tick` for every returned record. Observations preserve the drain's ascending
full `EventKey` order, but they do not expose EventKey payload order, Path Certificate identity,
path elements, or a guessed per-event disposition.

That omission is intentional. Multiple identical arrivals are grouped by `(SinkId, DriverId,
Revision)` before slot application, so assigning one of those indistinguishable raw events a
presentation-only `Applied` label would add an unnecessary convention. The existing aggregate
`SignalStepCounters` remain the authority for applied, invalid-path, stale-revision, and idempotent
counts. The raw observation remains the authority for whether each due event was Propagation or
TopologySync. Thus even an event later counted stale or invalid still truthfully contributes its
`A` or `S` due marker.

An equal revision carrying a different sample, a missing certificate, or an invalid event shape
remains a Canonical State invariant error. The Core transaction rolls back and returns no
`StepReport`. Constructing or reading successful observations does not add events, retain
certificates in Canonical State, alter event ordering, or change State Hash.

## Probe projection and non-intervention

The Core-facing target namespace is explicit:

```rust
pub enum SignalProbeTarget {
    Driver(DriverId),
    Sink(SinkId),
    GateInputA(GateId),
    GateInputB(GateId),
    GateOutput(GateId),
    Wire(WireId),
}

pub enum SignalProbeValue {
    Driver(DriverSample),
    Sink {
        sink: SinkId,
        level: LogicLevel,
    },
    Wire {
        active_drive: DriveVector,
        previous_drive: DriveVector,
        active_level: LogicLevel,
        previous_level: LogicLevel,
    },
}

pub struct SignalProbeSample {
    pub target: SignalProbeTarget,
    pub next_tick: Tick,
    pub value: SignalProbeValue,
}
```

`Simulation::signal_probe(target)` is read-only and returns `None` for an unknown/removed target or
for InputB on NOT. Driver and Sink numeric IDs are separate namespaces and are never inferred from
one another. GateInputA/B and GateOutput are stable convenience aliases resolved through the
Gate's projected ports to the same underlying Sink or Driver values. A Wire sample carries both
DriveVectors and Core-resolved levels; the host must not apply a nonzero or palette-based threshold.

Core owns no probe registry and does not filter event execution by selected probes. Adding,
removing, or reordering host probes cannot change Canonical State, event ordering, route choice,
allocation that affects results, State Hash, Replay checkpoints, or StepReport content.

## HostAction and editor command scheduling

Raw keys, pointer events, and widget callbacks are presentation bindings. They emit the following
ordered host intents; raw platform input is never passed to Core.

```rust
pub enum HostAction {
    Pause,
    Resume,
    SetRate(HostRate),
    SingleStep,
    Reset,
    QueueEdit(Command),
    SetView(ViewMode),
    Select(PickTarget),
    ClearSelection,
    AddProbe(SignalProbeTarget),
    RemoveProbe(SignalProbeTarget),
    ClearPreview,
}

pub enum HostRate { Quarter, One, Four }
pub enum ViewMode { Network, Circuit { substrate: EntityId } }
```

One PreUpdate system drains actions in their event insertion order. Determinism is conditional on
the same ordered HostAction/Command Log, not on two humans producing identical OS event timing.

`QueueEdit` accepts only `PlaceGate`, `PlaceWire`, `PlaceJunction`, `PlaceFixedSubstrate`,
`RemoveEntity`, `BindPort`, and `SetExternalDriver`. A complete wire polyline is previewed in host
state and becomes one `PlaceWire` only when committed. `PlaceMobileSubstrate` and any future command
are rejected by the host as out of S0-M6 scope without consuming an ordinal.

In an interactive session, the host converts each accepted `QueueEdit(command)` to:

```rust
CommandEnvelope {
    target_tick: simulation.next_tick(),
    ordinal: next_session_ordinal,
    command,
}
```

It then increments `next_session_ordinal` with checked arithmetic and appends the envelope to both
the pending queue and the immutable session Edit Command Log. Ordinals begin at zero, increase
across the entire session, and reset only with a new session. Exhaustion is a typed Host error and
does not enqueue or mutate Core.

While paused, `next_tick` does not change, so all queued edits target that same upcoming Phase 0.
On the next Single Step or Resume, the sole FixedUpdate owner submits that complete batch exactly
once. Core retains authority over ordinal sorting, conflicts, geometry, and command rejection.
After the step, those envelopes leave the pending queue whether accepted or rejected; the
StepReport result and Edit Command Log remain inspectable.

Running at `4x` may perform four Core steps in one FixedUpdate pulse. A newly queued edit is
consumed only by the first step whose Tick equals its captured `target_tick`; it is never repeated
on the remaining steps. UI helpers may snap a preview using the validated Physical Scale Profile,
but they must submit the exact resulting fixed-point coordinates to Core. A preview is advisory
and may still be rejected by Phase 0.

Selection, a ghost wire/gate, drag state, hover, and tentative bindings live only in host resources.
Creating or changing them may update CellBuffer pixels but may not call `Simulation::step`, clone
and replace Canonical State, or directly change a store. The Core's command result is the only
acceptance authority.

## Pause, Single Step, and rational pacing

The native Laboratory starts `Paused` at `nextTick = 0` with rate `1x`. The validated Balance
Profile supplies `simulationHz` (20 Hz for Stage 0). The host never passes frame delta or a speed
multiplier into Core; those values are consumed only by the host pacer before it chooses how many
times to call the one-Tick `Simulation::step` API.

Pacing uses a checked `u128` rational credit with nanoseconds and a common quarter-speed
denominator:

```text
NANOS_PER_SECOND = 1_000_000_000
RATE_UNITS        = { 1/4x: 1, 1x: 4, 4x: 16 }
CREDIT_DENOMINATOR = 4 * NANOS_PER_SECOND

added_credit = elapsed_nanoseconds * simulationHz * RATE_UNITS[current_rate]
accumulated_credit += added_credit
ticks_due = accumulated_credit / CREDIT_DENOMINATOR
accumulated_credit %= CREDIT_DENOMINATOR
```

All multiplication/addition and the conversion of `ticks_due` to the host loop count are checked;
overflow is a typed host pacing error before Core mutation. There is no floating-point speed
accumulator. Partitioning one elapsed duration across frames produces the same total Tick count and
remainder. Changing rate preserves fractional credit. Pausing preserves already earned fractional
credit but adds no elapsed credit; wall time spent paused is never converted into later Tick debt.

Bevy Virtual Time's maximum-delta clamp is set to `Duration::MAX`, and the complete host-observed
elapsed duration is offered to the pacer. A long render frame may delay work but cannot delete Tick
debt. S0-M6 places no additional drop-on-overload cap on canonical steps. If a future
responsiveness cap is added, unprocessed whole-Tick debt must be retained rather than discarded and
must produce the same eventual per-Tick trace.

`SingleStep` is valid only while paused. Repeated requests before the next FixedUpdate coalesce into
one boolean request. The next FixedUpdate bypasses rate credit, runs exactly one Core Tick with the
pending current-Tick command batch, refreshes all observations, clears the request, and remains
paused. It neither adds nor consumes pacing credit. A request while Running is a typed host-action
rejection and has no effect.

A `SimulationError` or Replay checkpoint divergence puts the host in `Faulted/Paused`; it performs
no more steps until reset/restart. Core's transactional error behavior applies. A normal
`CommandRejection` is displayed and does not fault the host.

## Laboratory reset and session boundary

Reset is a host request, never a `Command`. The FixedUpdate owner performs it before any step:

```text
dispose current Simulation
clone the validated original SimulationPackage
Simulation::new(package)
write initial RenderSnapshot at nextTick 0
start a new Replay/Edit session
```

Interactive Reset atomically clears pending commands, the Edit Command Log, next ordinal, hash
trace, retained StepReports, selection, hover, ghost/preview, probes, 256-Tick histories, arrival
markers, FixedUpdate credit, Single Step request, and Host fault. It restores `Paused`, `1x`, and
Network View. The new initial hash is the first trace entry. No pending event or identity frontier
from the disposed session survives.

Reset before and after states are different Replay sessions even when their initial hashes match.
Reset cannot be encoded in Replay v1 and is not appended to the old Command Log.

## CellBuffer coordinate and layer contract

`CellBuffer` is a Pure Rust type in `aon-app`; it contains no Bevy `Entity`, `Transform`, or Core
store reference.

```rust
pub struct CellBuffer {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Cell>,              // row-major screen order
}

pub struct Cell {
    pub glyph: char,
    pub foreground: CellColor,
    pub background: CellColor,
    pub layer: CellLayer,
    pub picks: Vec<PickTarget>,
}
```

The world/grid convention is `+x` right and `+y` up. Screen row zero is the top row. For an integer
view origin `(left, bottom)`, grid cell `(gx, gy)` maps to:

```text
column = gx - left
row    = (bottom + height - 1) - gy
```

All conversion, clipping, and buffer-size multiplication use checked integer arithmetic. An
off-screen or unrepresentable presentation primitive is clipped/skipped with a typed host
diagnostic; it never panics or becomes a Core rejection.

The back-to-front layer order is fixed:

```text
Empty
< FixedSubstrate
< Wire
< Junction
< GateAndPort
< Selection
< GhostAndDebug
```

Within one ordinary layer, higher `EntityId` wins visual/pick ties after multi-stroke Wire cells
have been combined. This is only a presentation tie-break and does not imply canonical priority.
Selection changes style without replacing the underlying glyph. Ghost/debug cells do not add pick
targets and cannot hide the selected canonical identity from the inspector.

Wire rasterization converts endpoints to the active view grid with mathematical floor division,
then walks every polyline segment with this integer supercover rule. Let `nx = abs(x1-x0)`,
`ny = abs(y1-y0)`, `sx/sign(dx)`, `sy/sign(dy)`, and start `ix = iy = 0` at `(x0,y0)`:

```text
while ix < nx or iy < ny:
    if ix == nx: advance y
    else if iy == ny: advance x
    else compare (1 + 2*ix)*ny with (1 + 2*iy)*nx
         left  < right: advance x
         left  > right: advance y
         equal: emit the x-neighbor, then the y-neighbor, then advance/emit the diagonal;
                increment both ix and iy
```

Products use checked unsigned wide integers. The endpoint cells are included, duplicate cell
emissions are coalesced, and reversing a segment produces the same cell/stroke set. Consecutive
cardinal occupied cells contribute N/E/S/W connection bits; an exact-corner supercover retains
both touched strokes. Golden CellBuffer tests freeze horizontal, vertical, turn, T, four-way,
negative-coordinate, clipped, reverse, exact-corner, and crossing cases.

Two physically crossing strokes without a live Junction render as `╳`, never `●`, even when they
share a CellBuffer cell. A live Junction record at that exact cell renders `●` on the higher layer.
This is a topology distinction, not decoration.

## Discrete Network and Circuit views

Continuous zoom is not part of S0-M6. Both views use the same world-space records and stable IDs.

### Network View

- one CellBuffer grid unit is `worldRoutingPitch`;
- OpenWorld Wires and Junctions are rasterized;
- each Fixed Substrate footprint is translated by its world origin and drawn at substrate layer;
- a Fixed Substrate origin carries `■` and is pickable by its EntityId;
- internal FixedSubstrate Gates/Wires/Junctions are collapsed into that substrate and are not
  individually drawn or picked.

Network View does not invent an aggregate circuit Logic Level. A substrate stays neutral because
no such canonical signal exists.

### Circuit View

- the view requires one selected live Fixed Substrate ID;
- one CellBuffer grid unit is `circuitRoutingPitch`;
- world position `p` maps through checked `local = p - substrate.origin` before grid conversion;
- only Gate/Wire/Junction records whose domain is `FixedSubstrate(selected_id)` are drawn;
- the selected substrate's local routing area/footprint is background context;
- OpenWorld and other-substrate primitives are not drawn.

If the selected substrate is removed, the host clears that selection and returns to Network View
on the next snapshot. View/LOD changes never change Core coordinates or State Hash.

## Glyph and signal-style contract

The S0-M6 glyph map is:

```text
empty                         ·
Fixed Substrate origin        ■
AND / OR / NOT                &  |  !
Junction                      ●
free Gate port                ○
bound Gate port               ◉
unconnected Wire crossing     ╳

E+W                           ─
N+S                           │
N+E / N+W / S+E / S+W         └  ┘  ┌  ┐
S+E+W / N+E+W                 ┬  ┴
N+S+E / N+S+W                 ├  ┤
N+E+S+W                       ┼
```

A one-sided terminal uses `─` or `│` according to its incident direction. Any cell whose stroke
cannot be represented by the cardinal map, including combined diagonal/ambiguous strokes, uses
`╳` and retains the exact entity candidates for picking.

Signal level changes style, not Wire geometry: LOW is dim neutral, HIGH is bright green, and X uses
a bright warning foreground **and** contrasting warning background. Gate input/output port style
comes from the projected Sink/Driver level; Wire style comes from `active_level`. Junction and
substrate have no invented signal and remain neutral. Selection uses inverse/highlight style;
ghosts use a dim/dashed warning style. Palette RGB values are presentation constants, not
semantics, but LOW/HIGH/X must remain visually distinguishable without relying on glyph shape.

## Deterministic picking and edit targeting

Picking reads `Cell.picks`, not Bevy render entities or floating transforms. The public target
shape is:

```rust
pub enum PickTarget {
    Entity(EntityId),
    GatePort(GatePortRef),
    WireEnd { wire: WireId, end: WireEnd },
}
```

Candidate order is higher visible layer first, then higher parent `EntityId`, then subtarget order
`GatePort < WireEnd < Entity`. A primary click selects the first candidate. Cycling overlapping
candidates is outside S0-M6; therefore repeated identical picks return the same target. At an
unconnected Wire crossing, the higher Wire EntityId is primary. At a Junction, the Junction is
primary because its layer is higher.

Delete converts the selected entity to `RemoveEntity`. Bind requires a selected Wire End followed
by a GatePort or Junction target and emits `BindPort`; explicit unbind emits `EndpointTarget::Free`.
Picking never guarantees command acceptance, and a stale target is reported through the ordinary
Core rejection path.

## Waveform and markers

The host supports at most eight distinct `SignalProbeTarget` values. Adding an existing target
focuses it without duplicating it. Adding a ninth returns a nonfatal `ProbeLimitReached` host
result. Removing a probe deletes its host history only.

Each probe owns a fixed-capacity ring of 256 successful completed-Tick samples. Adding a probe does
not reconstruct the past: while paused its current value is visible in the inspector, and its
waveform begins after the next successful step. After each Core step the host reads the refreshed
snapshot and appends `(completed_tick, LOW/HIGH/X)` for every live probe. A target that disappeared
is automatically removed after that snapshot; its existing displayed history may remain until the
row is dismissed.

Driver rows also retain the projected Driver Revision. The first stored Driver sample prints
`rN`; later samples print `rN` only when Revision changes. The decimal value is the exact `Revision`
and is never inferred from edge count.

The waveform has a shared arrival-marker band derived only from
`StepReport.signal_arrivals` for that completed Tick:

- `A` means at least one `SignalArrivalKind::Propagation` was due;
- `S` means at least one `SignalArrivalKind::TopologySync` was due;
- `AS` is shown when both kinds were due.

Driver and Sink probe rows additionally highlight observations matching `source_driver` or `sink`.
Wire rows use the shared band because Core does not project a presentation-specific Wire match for
an Arrival. Selecting a marker exposes due Tick, source, sink, sample, and kind in the inspector;
the same Tick's aggregate counters show applied/invalid/stale/idempotent totals. The UI never
silently hides a due event based on those aggregate outcomes.

Probe count, history length, marker selection, and presenter Update frequency cannot affect the
per-Tick hash trace.

## Snapshot-only inspector

The inspector reads only `LatestRenderSnapshot`, retained StepReports, and host resources. It never
holds a Core store reference.

Its minimum fields are:

- session: scenario ID, `nextTick`, completed Tick when present, State Hash, contract/profile
  hashes, Topology Revision, Paused/Running/Faulted, and rate;
- Gate: ID, type, origin/domain, both input Sink IDs/levels and external Driver samples, output
  Driver sample, desired output, pending generation/due/level/energy, and cancelled heat;
- Wire: ID, domain, exact points, endpoints, connection generation, active/previous DriveVector,
  and resolved levels;
- Junction: ID, domain, position, and connection generation;
- Fixed Substrate: ID, origin, local routing area, and local footprint;
- latest command acceptance/rejection for the selected target when available;
- selected arrival: due Tick, kind, source, sink, full sample, and that StepReport's aggregate
  Arrival counters.

The inspector does not label a circuit CPU, memory, latch, oscillator, router, controller, or any
other inferred role. Analyzer work and automatic behavioral classification remain later gates.

## Replay playback policy

The app has mutually exclusive `Interactive` and `ReplayPlayback` session modes. In playback,
Replay v1 is the sole command source. `QueueEdit`, delete/bind, external-drive changes, and ghost
commit are rejected at the host boundary; they are not merged with Replay commands and consume no
ordinal. S0-M6 does not fork or rewrite a Replay.

Pause, Single Step, rate, Network/Circuit view, selection, probes, waveform, and inspection remain
available during playback. They observe the same Replay execution without entering its hash.
Playback stops Paused at `Replay::final_next_tick()`. A checkpoint mismatch faults immediately and
does not present later states.

Reset in ReplayPlayback means **restart the same validated Replay**, not enter an editable branch.
It constructs a fresh Simulation from the retained package, validates the Header at Tick 0 again,
clears the same transient resources listed for Laboratory Reset, remains read-only, and starts
Paused at the initial checkpoint. To edit, the user must explicitly leave playback and start a new
Interactive scenario session; that new session has a new Edit Command Log.

## Native renderer, font, and assets

The authoritative presentation input is CellBuffer. The renderer pools one text row/run for each
contiguous sequence of equal foreground/background style; it does not create one persistent Bevy
Entity per Canonical Gate, Wire, Junction, or Cell. Background runs and glyph runs may be separate
presentation batches. Their entity IDs are caches and are never exposed as canonical identity or
pick targets.

S0-M6 checks in these repository-owned assets:

```text
apps/aon-app/assets/fonts/noto-sans-mono/NotoSansMono-Regular.ttf
apps/aon-app/assets/fonts/noto-sans-mono/OFL.txt
```

The font is Noto Sans Mono under the SIL Open Font License 1.1. Native code embeds both files with
`include_bytes!`; it does not depend on the process working directory, an OS-installed font,
network access, or an absolute developer path. Failure to decode the embedded font is a typed Host
startup error. There is no platform-dependent fallback font because fallback glyph metrics could
change the debug view.

The checked font must cover every frozen glyph above. Cell layout uses measured monospaced advance
and fixed line height. Resizing changes only the visible CellBuffer extent and clipping; it cannot
change view coordinates, picking order for still-visible cells, Tick pacing, commands, or hashes.

GPU/window construction is excluded from headless deterministic tests. CellBuffer, picking,
waveform, and action/pacer logic are testable with `MinimalPlugins` or pure functions; the native
window remains a manual smoke gate. Disabling the presenter must leave the same Core trace.

## Host errors

S0-M6 host failures are typed and non-panicking. At minimum they distinguish:

- out-of-scope or playback-read-only HostAction;
- Single Step requested while Running;
- session ordinal exhaustion;
- probe limit reached or unknown probe target;
- invalid/removed Circuit View substrate;
- checked CellBuffer dimension/coordinate failure;
- embedded-font decode failure;
- Core Simulation error and Replay validation/checkpoint error.

An input/action rejection is nonfatal and leaves Core unchanged. A Simulation or Replay execution
error faults and pauses the session. No malformed preview, resize, pick, or font failure may mutate
Canonical State.

## S0-M6 completion gates

S0-M6 is not complete until all of the following are executable tests or the explicitly identified
native smoke test:

1. `RenderSnapshot` projects contract, exact `nextTick`/Topology Revision/hash, and all live
   Gate/Wire/Junction/FixedSubstrate records in EntityId order, including the frozen port/sample,
   pending, raw-point, endpoint, generation, DriveVector, and resolved-level fields;
2. repeated snapshot/probe/inspector reads before and after store-layout permutations do not mutate
   State Hash, and no Bevy type/dependency enters `aon-sim`;
3. every raw due Propagation and TopologySync Arrival yields one full-EventKey-ordered observation
   with exact due Tick, source, sink, sample, and kind, including events later counted invalid,
   stale, or idempotent; existing aggregate counters remain exact;
4. PreUpdate preview/selection/actions cannot mutate Core, and FixedUpdate has exactly one mutable
   Simulation owner; presenter enabled/disabled and arbitrary Update counts produce identical
   traces;
5. the interactive editor queues and executes Gate/Wire/Junction/FixedSubstrate placement,
   delete, bind/unbind, and external drive solely as current-Tick CommandEnvelopes, while mobile or
   future commands are refused without consuming identity or ordinal;
6. Paused edits leave the hash unchanged until a step, session ordinals are checked/monotonic, and
   a retained C-25 fixture produces the same command results and final hash through Laboratory
   Single Step and direct headless `Simulation::step`;
7. Single Step from Paused performs exactly one Tick then remains Paused, coalesces repeated pending
   requests, preserves pacing credit, and is rejected without mutation while Running;
8. integer `1/4x`, `1x`, and `4x` pacing produces the exact expected Tick counts and identical
   per-Tick Replay hashes across frame-delta partitions, FPS/Update-count changes, and a long frame
   carrying Tick debt;
9. interactive Reset and Replay restart construct fresh Tick-0 Simulations and clear commands/log,
   ordinal, trace/reports, selection/preview, probes/history/markers, credit, step request, and
   faults; no old pending event or session log crosses the boundary;
10. golden CellBuffer tests cover +x/+y orientation, negative coordinates, checked clipping,
    profile-pitch conversion, fixed layers, every frozen Wire/gate/port glyph, LOW/HIGH/X styles,
    and `╳` crossing versus live `●` Junction;
11. Network View collapses fixed circuits at world pitch, Circuit View exposes only the selected
    fixed domain at local circuit pitch, and changing view/viewport cannot change State Hash;
12. picking is independent of Bevy entities/transforms and returns the frozen layer/EntityId/
    subtarget order for Gate ports, Wire ends, crossings, Junctions, selection, and ghosts;
13. at most eight probes retain exactly the last 256 completed-Tick LOW/HIGH/X samples without
    retroactive history, automatically handle removed targets, and never affect hashes or event
    order;
14. a retained feedback/topology-edit fixture displays exact `rN`, `A`, and `S` markers from Core
    revisions/arrival observations, exposes the selected raw Arrival and aggregate counters, and
    includes at least one due TopologySync marker;
15. the inspector exposes every minimum frozen field from snapshot/report data only and never
    invents Analyzer classifications or obtains a mutable Core reference;
16. Replay playback refuses all edits/ordinal consumption, permits observation and pacing controls,
    stops at the final boundary, faults at first checkpoint divergence, and restart remains a fresh
    read-only execution with the same complete hash trace;
17. the embedded OFL font/license are present and cwd-independent, all glyphs decode in a native
    window smoke test, then format/metadata/check, strict Clippy, workspace tests, dependency
    boundary, and fresh clean-checkout offline gates pass without warnings.

Passing this list completes only S0-M6. S0-M7 mobility, the remaining Stage 0 technical/product
gates, later stages, MVP integration, native UX acceptance, and release verification remain
mandatory.

## Non-normative implementation evidence

Implementation commit `bf651c9e63aed41e6db7e0f7b2767345834ff94e` passed the gate 17
Windows-native native-window smoke and independent `git clone --no-local` fresh clean-checkout
offline format/metadata/check, strict Clippy, workspace tests, dependency boundary, and native
link verification on 2026-08-12. S0-M6 is complete. S0-M7 subsequently extends the editor with
Mobile placement and Mobile Circuit View; that extension does not rewrite the frozen S0-M6 slice
boundary.
