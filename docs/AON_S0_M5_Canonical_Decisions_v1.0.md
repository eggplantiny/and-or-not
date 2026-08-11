# A/O/N S0-M5 Canonical Decisions v1.0

Status: implementation authority for S0-M5

This document resolves the implementation choices required by `S0-M5 — Feedback / Replay`. It
refines the Simulation Semantics Specification and TRD where their prose leaves more than one
deterministic implementation possible.

S0-M5 does **not** complete Stage 0 or the game engine. The Bevy ASCII probe, mobility, later
stages, MVP integration, product gates, and the supported cross-platform release matrix remain
subsequent gates.

## Scope

S0-M5 owns:

- ordinary signal routes that form directed feedback cycles;
- exact LOW/zero-strength/revision-zero Gate startup behavior under feedback;
- C-05 odd-NOT ring and C-06 symmetric latch fixtures;
- an explicit Set/Reset NOR-style emergence fixture with no Latch runtime type;
- replay format v1, strict decode/encode, Header validation, Command Log scheduling, and hash
  checkpoints;
- a single-file headless replay command and per-Tick trace;
- the same Replay Command Log in the headless loop and Bevy `FixedUpdate` host;
- a retained golden replay and a 100,000-Tick headless/cross-host fixture.

S0-M5 does not add a feedback, oscillator, latch, memory, counter, analyzer, or module canonical
entity. It does not add random world generation, WorldInput semantics, save-state restoration,
mobility, presentation controls, or a platform-dependent scheduler.

## Feedback semantics

Feedback is an ordinary directed Driver-to-Sink route compiled from Gate ports, Wires, and
Junctions. A cycle is neither rejected nor given a special execution path. Every edge continues to
use the S0-M3/S0-M4 DriverTransition, SignalArrival, Revision, delay, and Path Certificate rules.

New Gate signal state is exactly:

```text
current output = Low
desired output = Low
output strength = 0
Driver Revision = 0
pending transition = none
cancelled switching heat = 0
```

Phase 3 may derive a different desired output during the placement Tick and Phase 6 may reserve the
ordinary delayed transition. The engine never searches for, chooses, or writes a fixed point when a
cycle is created. It never deletes pulses or Events merely because a component is cyclic.

C-05 uses a one-NOT physical self-loop, which is the smallest odd-NOT ring and avoids a different
synchronous mode that a completely symmetric multi-Gate startup could produce. With circuit pitch
`P=16384`, the NOT is at `(0,0)` and its route points are
`[(P,0),(2P,0),(2P,2P),(-2P,2P),(-2P,0),(-P,0)]`. The route is `2.5 wu`, its reference-profile Wire
delay is 1, its stable Gate delay is 1, and therefore `D=2`. After construction, `StepReport` records
rising edges at completed Ticks 2/6/10/14 and falling edges at 4/8/12; the corresponding post-state
`nextTick` values are one greater. Each same-polarity period is exactly `2D=4` for at least three
periods. Every expected edge is present in `driver_changes`; no transition is synthesized by a
fixture. Larger odd rings may be added separately but do not redefine this oracle.

C-06 creates the two symmetric Gate branches in one command batch and the two equal feedback paths
in one later command batch. The assertion is symmetry, not a preferred binary state: both branches
must remain exchange-equivalent at every observed Tick and the engine must not choose one
complementary branch solely from EntityId, command insertion order, or store layout. Symmetric
oscillation or symmetric `X` is valid.

The C-06 fixture varies command-slice insertion order across independent replicas. Generic
structural store-layout and route-compiler independence remain inherited S0-M2/S0-M4 regression
gates; S0-M5 does not add a private layout-mutation path to the feedback fixture.

The C-06 exchange mapping compares current/desired Level, pending due offset, pending target,
output Sample Level/Strength/Revision, and input Sink Level after swapping the declared branch
labels. It repeats the fixture with command-slice insertion order reversed while preserving
ordinals. Raw
EntityIds remain identity and are not exchange-equivalence fields.

The explicit Set/Reset fixture is two cross-coupled NOR constructions made only from OR, NOT, and
Wire primitives. Its startup sequence and input pulses are part of the Command Log. A Set pulse
must make `Q` hold after the pulse ends, and a Reset pulse must clear `Q` and hold after that pulse
ends. No `Latch` command, store, event, or state-hash tag is introduced.

## Replay format and host locator

Replay JSON format version 1 is strict UTF-8 JSON with unknown fields rejected. Its top-level shape
is:

```json
{
  "scenarioPath": "../scenarios/empty.json",
  "header": {},
  "commands": [],
  "worldInputs": [],
  "checkpoints": []
}
```

The Rust boundary is explicit:

```rust
pub struct ReplayArtifact {
    scenario_path: String,
    replay: Replay,
}

pub fn decode_replay_artifact(bytes: &[u8])
    -> Result<ReplayArtifact, ReplayError>;

pub fn encode_replay_artifact(replay: &ReplayArtifact)
    -> Result<Vec<u8>, ReplayError>;
```

The wire format flattens `Replay` beside `scenarioPath`; it does not nest another `replay` object.
`scenarioPath` must be a nonempty relative UTF-8 path using `/`. Absolute POSIX paths, Windows
drive/UNC forms, backslashes, and NUL are rejected; `..` is allowed so `fixtures/replays` can refer
to sibling `fixtures/scenarios`. It is resolved relative to the Replay file. It is not a
canonical simulation field, is not hashed, and is not part of `ReplayHeader` or `Replay`. Moving a
Replay and its Scenario together cannot change simulation meaning. The Header binds the loaded
Scenario package to the Replay.

Replay JSON integers are JSON integers and must fit their declared Rust width exactly. Hashes and
the Seed use fixed-length lowercase hexadecimal strings. Floating-point JSON values, alternate
integer strings, uppercase hex, duplicate semantic aliases, and unknown fields are rejected.

## Replay Header v1

```rust
pub struct ReplayHeader {
    pub format_version: ReplayFormatVersion,
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
    pub balance_profile_hash: ProfileHash,
    pub state_hash_version: StateHashVersion,
    pub world_generator_version: WorldGeneratorVersion,
    pub seed: Seed,
    pub initial_state_hash: StateHash,
    pub hash_algorithm_id: HashAlgorithmId,
}
```

The v1 JSON field names are `formatVersion`, `semanticsVersion`, `numericProfileHash`,
`physicalScaleProfileHash`, `balanceProfileHash`, `stateHashVersion`, `worldGeneratorVersion`,
`seed`, `initialStateHash`, and `hashAlgorithmId` in that order when encoded by this engine.
`stateHashVersion` is an S0-M5 refinement to the TRD Header: the hash algorithm alone cannot
identify which canonical State encoder produced a checkpoint.

Supported values are:

```text
formatVersion         = 1
semanticsVersion      = aon-semantics-v1
stateHashVersion      = aon-state-v3
worldGeneratorVersion = aon-empty-v1
seed                   = 64 lowercase zero hex characters
hashAlgorithmId        = blake3-v1
```

`Seed` is exactly 32 bytes. Empty world generation consumes no random draw and requires `Seed::ZERO`;
a nonzero Seed with `aon-empty-v1` is rejected rather than silently ignored. Later generators must
add an explicit supported generator version and deterministic PRNG contract.

`Simulation::replay_header()` returns the Header that would reproduce the Simulation's initial
state. The initial hash is captured after `Simulation::new` validation and remains unchanged in the
Header as the Simulation advances. Header metadata is noncanonical because the contract and actual
world are already canonical; reading it cannot change State Hash.

Before Tick 0, static Replay-body shape is checked first. Header comparison then follows the printed
Header field order. Finally the runner requires current `next_tick == 0` and current State Hash
equal to `header.initialStateHash`. The first mismatch in this precedence is returned. There is no
best-effort migration or partial playback.

## Command Log JSON and scheduling

Replay v1 supports exactly the eight current Stage 0 command variants. The tagged JSON command
names are:

```text
place-gate
place-wire
place-junction
place-fixed-substrate
place-mobile-substrate
remove-entity
bind-port
set-external-driver
```

Each envelope has `targetTick`, `ordinal`, and `command`. Points are `{ "x": i64, "y": i64 }`.
AABBs are `{ "min": Point, "max": Point }`. Routing domains are tagged `open-world`,
`fixed-substrate`, or `mobile-substrate`; endpoints are tagged `free`, `junction`, or `gate-port`.
Gate, port, Wire-end, and Logic-level names are lowercase kebab-case. IDs and strengths are `u64`.
Point `x`/`y` values are raw `Fixed.0` units, not world-unit floating point.

The exact v1 payload fields are:

| Command `type` | fields after `type` |
|---|---|
| `place-gate` | `gateType`, `origin`, `routingDomain` |
| `place-wire` | `routingDomain`, `points`, `endpointA`, `endpointB` |
| `place-junction` | `routingDomain`, `position` |
| `place-fixed-substrate` | `origin`, `routingArea`, `footprint` |
| `place-mobile-substrate` | `origin`, `routingArea`, `footprint` |
| `remove-entity` | `target` |
| `bind-port` | `wire`, `end`, `target` |
| `set-external-driver` | `driver`, `level`, `strength` |

Tagged subobjects are exact:

```text
RoutingDomain = { kind: open-world }
              | { kind: fixed-substrate, substrate: u64 }
              | { kind: mobile-substrate, substrate: u64 }

Endpoint = { kind: free }
         | { kind: junction, junction: u64 }
         | { kind: gate-port, gate: u64, port: input-a|input-b|output|power }
```

The decoder converts the complete JSON payload into the same typed `CommandEnvelope` used by live
play. No replay-only semantic command exists. The encoder emits this one spelling.

The Command Log is normalized by `(target_tick, ordinal, canonical command bytes)`. On canonical
Tick `t`, the host submits every envelope whose `target_tick == t` exactly once in one slice. Replay
v1 records only canonical on-time player intent. A live request submitted in a Tick different from
its `target_tick` is host noise that may produce `WrongTick`, but it is not recordable in this v1
`Vec<CommandEnvelope>` because the TRD body has no submitted-Tick field. No future command is
submitted early, no command is retried, and no wall-clock/frame grouping is observable. Duplicate
ordinals remain in the log so the ordinary deterministic command-rejection rule is replayed; the
canonical command-byte tie-break only stabilizes their artifact order.

Every command target Tick must be less than the Replay's final `nextTick`. A Replay cannot contain
commands after its declared run boundary.

## World Inputs v1

`WorldInputEvent` is an explicitly reserved replay-body type. Replay v1 with `aon-empty-v1`
requires `worldInputs` to be an empty array. A nonempty array is rejected as an unsupported replay
feature, not ignored. Later world-input variants require a replay format/version decision before
use.

## Hash Checkpoints and run boundary

```rust
pub struct HashCheckpoint {
    pub next_tick: Tick,
    pub state_hash: StateHash,
}
```

The JSON fields are `nextTick` and `stateHash`. A checkpoint names the canonical State whose
`Simulation::next_tick()` equals `nextTick`. Thus checkpoint 0 is the initial state, and stepping
canonical Tick `t` produces the candidate for checkpoint `t + 1`.

Checkpoints must be nonempty and strictly increasing. The first checkpoint is exactly `nextTick=0`
and its hash equals `header.initialStateHash`. The last checkpoint defines the Replay run boundary.
Sparse checkpoints are valid; the runner still computes and exposes every intervening per-Tick
hash. Every declared checkpoint is compared immediately at that Tick and the first divergence
returns a typed error containing `nextTick`, expected hash, and actual hash.

The retained golden feedback replay contains every checkpoint for its bounded conformance run.
The 100,000-Tick fixture uses the explicit Set/Reset latch after it reaches a quiescent held state;
it does not keep allocating periodic SignalArrival Certificates. It may retain sparse artifact
checkpoints, including the final golden, while the in-process headless and Bevy comparison still
compares the complete 100,001-state trace. The bounded oscillator stays separate because V3's
monotonic Certificate frontier would otherwise make a long debug hash run quadratic.

## Replay execution and host boundary

Core owns Replay types, strict decode/encode, shape validation, Header validation, command lookup,
and checkpoint/trace verification. Core performs no filesystem access.

```rust
impl Replay {
    pub fn validate_against(&self, simulation: &Simulation)
        -> Result<(), ReplayError>;
    pub fn commands_for_tick(&self, tick: Tick)
        -> impl Iterator<Item = &CommandEnvelope>;
    pub fn verify_trace(&self, trace: &[StateHash])
        -> Result<(), ReplayError>;
}
```

The headless host owns file reads and resolution of `scenarioPath`. Its stable command is:

```text
aon-headless replay <replay-path>
```

It loads the Scenario package, constructs a new Simulation, validates the Header, runs through the
last checkpoint, and prints the scenario ID, completed Tick count, and final hash. Scenario mode
remains supported independently.

The runner owns this fresh Simulation. If a declared checkpoint diverges after a successful Tick,
the run aborts and discards its private partially advanced instance; no caller receives it. This is
distinct from a Simulation Run Error, for which the attempted Tick remains transactionally rolled
back.

The Bevy harness receives the already decoded Replay and loaded package. Its `FixedUpdate` system
selects commands by the canonical Simulation Tick, never by frame count. Presentation updates,
presenter presence, frame delta grouping, CPU scheduling, and run speed may change how quickly
FixedUpdate is called but cannot change the command batch or resulting trace.

Both hosts expose the initial hash followed by one hash for every completed Tick. Cross-host tests
compare length and every index, report the first divergence, and independently validate all golden
checkpoints.

## Replay errors

Replay decode and execution use typed non-panicking errors. The stable categories are:

- invalid JSON and unsupported Replay format;
- invalid lowercase hash/Seed encoding;
- unsupported generator or nonzero Empty-world Seed;
- unsupported nonempty WorldInput log;
- malformed checkpoint order/boundary or command outside the run boundary;
- Replay contract mismatch, identifying the mismatched Header field;
- hash-algorithm mismatch;
- initial-state mismatch;
- checkpoint divergence with Tick/expected/actual;
- underlying package, file, or Simulation error at the host boundary.

Malformed replay input never mutates a Simulation. A runtime Simulation error retains the existing
whole-Tick rollback guarantee. Command rejection remains an ordinary replayed result and is not a
Replay error.

## Artifact canonicality and compatibility

JSON whitespace and object member order are not semantic. Decode followed by engine encode emits
`serde_json` pretty output with two-space indentation, LF line endings, and exactly one final LF,
plus normalized command order. Header field values, typed
commands, WorldInput sequence, and checkpoints are semantic Replay content; `scenarioPath` is only
location metadata.

Replay format version, Simulation semantics version, State-hash encoder version, Scenario schema,
and Profile schemas are separate contracts. S0-M5 adds Replay format v1 without changing
`aon-semantics-v1`, Scenario schema 1, Balance schema 2, or State encoder V3.

## S0-M5 completion gates

S0-M5 is not complete until all of the following are executable deterministic tests:

1. Gate activation under a feedback-capable topology starts Low, strength zero, Revision zero, and
   no preexisting pending transition; Phase 3/6 changes use ordinary delay Events;
2. cyclic routes compile without a feedback-specific canonical type or event path;
3. C-05 one-NOT physical self-loop has exact rising/falling period `2D=4` for at least three
   periods and its expected Driver Events are not deleted;
4. C-06 symmetric latch startup remains exchange-symmetric across command insertion permutations
   and independent replica executions and never selects one complementary branch arbitrarily;
5. explicit Set, release, hold, Reset, release, and clear behavior emerges from OR/NOT/Wire only;
6. Replay Header captures immutable initial state and exactly binds generator, zero Seed,
   semantics, three Profile hashes, State-hash encoder version, hash algorithm, and initial hash;
7. strict Replay JSON round-trips to one canonical spelling and rejects unknown fields, invalid
   widths/hex, unsupported versions, nonzero Empty seed, and nonempty WorldInput logs without panic;
8. all eight Command variants round-trip through Replay JSON with extreme signed coordinates,
   typed IDs, routing domains, endpoints, ports, and Logic levels;
9. command normalization and Tick grouping are independent of JSON insertion order; duplicate
   ordinals reproduce the same deterministic rejections;
10. checkpoint 0, strict ordering, sparse checkpoints, final boundary, out-of-range commands, and
    first-divergence errors follow the frozen rules;
11. mismatched semantics, each Profile hash, State-hash encoder version, generator, Seed, hash
    algorithm, and initial hash are rejected before Tick 0 and leave the Simulation unchanged;
12. a bounded retained feedback golden Replay verifies every declared checkpoint and complete
    per-Tick trace in headless execution;
13. the same Replay produces identical complete traces through the headless loop and actual Bevy
    `FixedUpdate`, with/without presenter and across presentation-update counts;
14. variable frame deltas, including one long frame carrying Tick debt, do not change the Replay
    command schedule or per-Tick hashes;
15. a retained Stage 0 Replay runs 100,000 Ticks headlessly, matches its final golden, and produces
    the same 100,001 hashes in the Bevy harness;
16. replay decode/command mapping has a bounded no-panic fuzz target and retained malformed corpus;
17. reading Replay Header, checkpoints, traces, reports, and public observations does not mutate
    State Hash;
18. format, metadata, check, strict Clippy, workspace tests, dependency boundary, and fresh clean
    checkout offline gates pass.

Passing this list completes only S0-M5. S0-M6 presentation and laboratory controls, S0-M7
mobility, the Stage 0 technical/product gates, later stages, and MVP remain mandatory.
