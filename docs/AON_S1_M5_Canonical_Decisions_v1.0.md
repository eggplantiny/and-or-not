# A/O/N — S1-M5 Canonical Decisions

**Status:** implementation authority
**Applies to:** `S1-M5 — Reference Architecture Fixture`
**Source baseline:** PRD v1.0 / SSS v1.0 / TRD v1.0 / S1-M0 and S1-M4 Canonical Decisions

This document freezes the smallest reproducible Brute/Computed comparison fixture. The PRD owns
the product question, the SSS owns observable simulation behavior, the TRD owns artifact and
experiment structure, S1-M0 owns the retained Experiment v1 contract, and S1-M4 owns the current
simulation. This document resolves only the representation, fairness, measurement, and evidence
gaps required by S1-M5.

S1-M5 does not execute a profile sweep, find a crossover, or close either complete Stage 1 gate.
Those are S1-M6 and product-gate work.

## 0. Source conflicts and resolutions

1. TRD §33.6 names Brute and Computed but does not freeze exact coordinates. S1-M5 therefore
   freezes one transparent reference pair and treats it as content, not a balance law.
2. Module v1 is retained unchanged. It has no runtime materializer and cannot represent Main Core,
   Power Source, Wire Sense, or Mobile endpoints. The executable designs use a new same-schema
   `ReferenceArchitectureArtifact` v2 composed only of existing public primitives. Architecture
   v1 remains the byte-, hash-, and behavior-frozen immediate-binding form.
3. Experiment v1 derives one `LongWireDesign` from distance. S1-M0 explicitly forbids silently
   adding the S1-M5 axis. S1-M5 introduces Experiment/Run identity v2 while retaining every v1
   byte and ID.
4. The SSS describes a later multi-profile comparison, but absolute geometry cannot be silently
   rescaled. S1-M5 freezes one profile triad. S1-M6 must publish explicit profile-compatible
   derived design artifacts before it compares scales.
5. Construction cargo is not present. “A/O/N consumed” means exact active AND/OR/NOT primitive
   counts, not an invented inventory resource.
6. Runtime has no wall-clock latency or kill counter. S1-M5 derives both from stable Tick reports,
   semantic role bindings, damage, contact, and destruction observations.

No hidden multiplier, design-specific Enemy key, privileged runtime primitive, or bespoke AI rule
is permitted. Both designs use the identical simulator and profile values.

## 1. Ownership and exclusions

S1-M5 owns exactly:

- a strict `ReferenceArchitectureArtifact` v2 used by both architecture classes, with v1 retained;
- deterministic validation and local-symbol materialization to existing Command-v1 operations;
- a strict reference-problem/pair manifest that binds fairness inputs and observation roles;
- Experiment manifest v2 with an explicit two-design axis and Run ID v2;
- one exact profile/scenario/seed/boundary pair;
- a derived `ReferenceMetricArtifact` v1 and pure checked reducer;
- retained Brute and Computed design, Replay, metric, and pair result goldens;
- direct/headless/Bevy equality and an S1-M5 fail-closed gate.

The retained v2 pair uses exactly four binding stages, the smallest proven sequence that survives
the exact alpha thermal rules while preserving the required topology and response behavior. Brute
stage cardinalities are `[48, 0, 0, 32]`; Computed stage cardinalities are
`[156, 8, 4, 16]`. Their shared command-Tick to quiescent-boundary evidence is
`3 -> 8`, `8 -> 11`, `11 -> 14`, and `14 -> 18`. The exact endpoint rows are ordinary
design content and are frozen by the retained artifacts and structural oracle. No architecture
name or role-name prefix changes the interpreter, and the two empty Brute stages exist only because
paired v2 execution requires the same four-stage grammar as Computed. Section 5 freezes the complete
partition, geometry, and Tick evidence; no seven-stage retained schedule remains authoritative.

S1-M5 excludes:

- Physical/Balance sweeps, Early/Crossover/Large classification, or selection of a winner;
- new Semantics, Numeric, Physical, Balance, Scenario, Replay, State, Command, or generator
  versions;
- Relay, Radiation, Payload/cargo, repair/reconstruction, Quartz, waves, pathfinding, four new
  Enemy types, or any new canonical World state;
- Module placement/flattening or mutation of Module v1;
- Encoder, MUX, FSM, Memory, targeting, or architecture-specific runtime classes;
- commands after the frozen build boundary, except the identical shared input stream;
- JSONL/CSV/Markdown sweep reporting, which belongs to S1-M6.

## 2. Frozen retained versions

| Contract | S1-M5 value |
|---|---|
| Semantics | `aon-semantics-v1` |
| Numeric Profile | schema `1` |
| Physical Scale Profile | schema `1`, `physical-scale-stage0-alpha` |
| Balance Profile | schema `5`, S1-M4 alpha |
| Scenario | schema `4`, `main-core-power-enemy-v1` |
| Replay | format `2` |
| Canonical State | V7 |
| Command encoder | v1, tags 0–8 retained |
| Module | format v1 retained and not materialized |
| Existing Experiment | format/Run ID v1 retained |
| Reference Architecture | format v1 retained; format v2 used by S1-M5 |
| Reference Pair / Metric | format v1, new |
| S1-M5 Experiment / Run ID | format/encoder v2, new |

The new artifacts are experiment content. They are not members of canonical State V7 and cannot
change a Tick, allocator frontier, cache, report, or State hash merely by being read or measured.

## 3. Reference Architecture Artifact v1 and v2

### 3.1 Public shape

The public nominal types are:

```rust
pub struct ReferenceArchitectureLocalId(u32); // nonzero

pub enum ReferenceArchitectureOperation {
    PlaceFixedSubstrate(ReferenceFixedSubstrate),
    PlaceMobileSubstrate(ReferenceMobileSubstrate),
    PlaceGate(ReferenceGate),
    PlaceJunction(ReferenceJunction),
    PlaceWire(ReferenceWire),
}

pub enum ReferenceArchitectureEndpoint {
    Free,
    Junction(ReferenceArchitectureLocalId),
    GatePort { gate: ReferenceArchitectureLocalId, port: GatePort },
    MobilePort { mobile: ReferenceArchitectureLocalId, port: MobilePort },
    MainCore,
    PowerSource { ordinal: u32 },
    WireSensePort { wire: ReferenceArchitectureLocalId, end: WireEnd },
}

pub struct ReferenceArchitectureArtifact {
    pub format_version: ReferenceArchitectureFormatVersion, // v1 or v2
    pub hash_algorithm_id: HashAlgorithmId,                 // blake3-v1
    pub display_name: String,                               // nonsemantic
    pub contract: SimulationContract,
    pub operations: Vec<ReferenceArchitectureOperation>,
    pub role_bindings: Vec<ReferenceArchitectureRoleBinding>,
    pub observation_bindings: Vec<ReferenceArchitectureObservationBinding>,
    // absent in v1; required in v2
    pub materialization_schedule: Option<ReferenceArchitectureMaterializationSchedule>,
}

pub struct ReferenceArchitectureMaterializationSchedule {
    // Ordered binding batches. Each entry names an existing Wire and end only.
    pub binding_batches: Vec<Vec<ReferenceArchitectureBindingEndpoint>>,
}
```

Routing domains are `OpenWorld`, local `FixedSubstrate`, or local `MobileSubstrate`. A semantic
target may additionally name a local entity/port, Main Core, a sorted Scenario Power Source
ordinal, or a sorted Scenario Enemy ordinal. Ordinals are zero-based in Scenario semantic order.

The admitted operation set is closed. It does not include Remove, direct driver mutation,
Construction Sites, hidden primitives, arbitrary bytes, architecture-class conditionals, raw
wait counts, or caller-selected quiescence predicates. A schedule can choose only when an already
declared final Wire endpoint is bound; it cannot change a target, geometry, or command payload.

### 3.2 Strict JSON and hash

JSON is UTF-8, `deny_unknown_fields`, integer-only, no duplicate keys, no trailing document, and
uses exact tagged unions. Decode selects `formatVersion` and `hashAlgorithmId` before strict body
validation. Canonical encode sorts operations by local ID and bindings by UTF-8 name; input
permutation produces identical bytes. Duplicate local IDs, role names, or observation names fail.

The retained v1 semantic `ArtifactHash` remains BLAKE3 over:

```text
"AON\0REFERENCE-ARCHITECTURE\0V1\0"
u16 encoderVersion = 1
u32 formatVersion = 1
text hashAlgorithmId
text semanticsVersion
32-byte Numeric hash
32-byte Physical hash
32-byte Balance hash
canonical operations
canonical role bindings
canonical observation bindings
```

V2 uses domain `AON\0REFERENCE-ARCHITECTURE\0V2\0`, encoder `u16(2)`, `formatVersion = 2`,
the same canonical body, then the binding-batch count and every canonical `(wireLocalId, WireEnd)`
row in semantic batch order. A schedule-only change therefore changes the Design Artifact hash.
The v1 domain, encoder, JSON shape, bytes, hash, plan, and execution remain exactly unchanged.
V1 rejects a schedule field; v2 requires it.

`displayName`, file path, provenance, JSON whitespace, and key order are excluded. Every operation,
coordinate, endpoint, contract hash, binding name, and binding target is independently sensitive.

### 3.3 Validation and materialization

Validation order is:

1. format and hash algorithm;
2. bounded counts and text lengths;
3. nonzero/unique local IDs;
4. reference existence, kind, valid ports, and Scenario ordinal preflight;
5. duplicate binding names and invalid targets;
6. SimulationContract equality;
7. Physical Profile hash/body and exact geometry validation;
8. canonical materialization plan and, for v2, the complete binding schedule.

The materializer is atomic at artifact scope: it executes against a private Simulation candidate
and publishes the candidate only if every step is accepted and every expected local ID is bound.
Failure returns a typed error and preserves State hash, Tick, events, revisions, and every frontier.

V1 materialization uses deterministic dependency batches:

1. Fixed Substrates;
2. Fixed-Substrate Gates and Open-World or Fixed-Substrate Junctions;
3. Open-World or Fixed-Substrate Wires, initially Free;
4. Mobile Substrates;
5. Mobile-Substrate Gates and Junctions;
6. Mobile-Substrate Wires, initially Free;
7. every non-Free Wire end ascending `(wireLocalId, WireEnd)`.

Each nonempty batch occupies one consecutive Tick. Commands inside a placement batch use ascending
local ID and ordinals `0..n-1`; the binding batch uses ascending `(wireLocalId, WireEnd)` and the
same contiguous ordinal rule. Empty batches consume no Tick. Every reference therefore resolves to
an entity created by an earlier batch, while no design ages one Tick per primitive or accumulates
bootstrap Heat merely because it contains more operations. The first Tick after the final binding
batch is `buildEndTick`. A batch with any rejection fails the complete private candidate; a
rejection is never skipped, salted, or converted to a different primitive.

V2 retains placement batches 1 through 6 exactly and replaces v1's single binding batch with an
explicit, hash-bound sequence of one to sixteen binding batches. Every final non-Free Wire end
appears exactly once; a Free end appears zero times. Every non-PowerSource end must be in the first
binding batch. Later batches may contain only PowerSource ends. Rows inside a batch are canonical
ascending `(wireLocalId, WireEnd)`; duplicate, missing, dangling, wrong-kind, or noncanonical rows
fail before execution. The first and final batches are nonempty; intermediate empty batches are
permitted so a paired design can share the same stage grammar without inventing a command.

After each v2 binding batch, including the final batch, the materializer advances owned private
candidates with empty Command lists to the earliest boundary where the signal driver calendar,
signal-arrival calendar, and pending Gate-transition tuples are all empty. The bound is 256 empty
Ticks per barrier. The artifact stores neither the number of empty Ticks nor another predicate.
Timeout, terminal Run status, any command rejection or acceptance mismatch, or any destruction or
contact during materialization is a typed atomic failure. Profile-mandated thermal damage may
accumulate and is retained exactly; damage alone is not a build failure, but no entity may be
destroyed before the build boundary. The first common quiescent boundary after the final binding
batch is `buildEndTick`.

Retained Brute and Computed materialize through the paired v2 interpreter. Placement and binding
batch indices advance in lockstep. At every barrier the earlier-ready candidate receives empty
Ticks until both are quiescent at the same boundary; the next binding batch is then submitted to
both at that same Tick. Empty batches still consume that shared command Tick through `step([])`.
Consequently both final activation batches and both per-design `buildEndTick` values are equal;
one design can never receive earlier defense Power merely because its own queues drained first.
Materialization evidence records every actually consumed v2 placement/binding batch kind and
command Tick on both sides, including an empty side, plus every binding stage's empty barrier Ticks
and final quiescent boundary. V1 evidence remains empty. Static metric derivation rejects any
command, acceptance, empty shared batch, stage, barrier, or build-boundary mismatch.

The exact canonical Command Log hash is BLAKE3 over domain
`AON\0REFERENCE-COMMAND-LOG\0V1\0`, encoder `u16(1)`, command count, then each complete
Command-v1 canonical byte stream in execution order.

## 4. Shared reference problem and fairness manifest

`ReferenceArchitecturePairManifest` v1 binds:

- pair ID and two distinct design Artifact hashes, labeled `brute` and `computed` only in the pair;
- Scenario artifact hash and exact Scenario ID;
- SimulationContract and selected profile IDs/hashes;
- `Seed::ZERO`; current generator draws no randomness;
- `maxTicks`, the common build-window end `buildEndTick`, and `measurementStartTick`;
- one exact territory AABB and named cardinal sector anchors;
- sorted Scenario Enemy initial trajectory sequence hash;
- shared-build Command Log hash (empty in v1 unless later explicitly frozen);
- Metric Set ID/hash;
- response-latency observation pairs.

Pair validation requires byte/semantic equality for Scenario, all three profiles, Main Core capacity,
Power Source sequence and generation, territory, Enemy initial rows, Seed, versions, boundaries,
metric set, and shared inputs. Only design bytes/hash and the materialized design Command Log may
differ. The two designs must have distinct Artifact and Command Log hashes.

The v1 reference problem uses Scenario-owned deterministic Enemy trajectories; no HostileFrame can be a
contact or damage input. Territory is a derived, hash-bound AABB/anchor contract and is not added to
canonical State.

The retained Scenario's four moving Enemy rows use the same translated per-sector
trajectory. Let `q = WU / 64 = 1,024` raw Fixed units and let `O` be that sector's canonical Power
Source position. Every Enemy starts at `O + q(34,-35)` and has velocity per Tick
`q(-1,+1)`. The sector origins and semantic Enemy ordinals are `west=(-64 WU,0), ordinal 0`,
`south=(0,-64 WU), ordinal 1`, `north=(0,+64 WU), ordinal 2`, and
`east=(+64 WU,0), ordinal 3`. This translated sequence, including velocity, is hash-bound by the
Scenario and Pair; no design-specific trajectory is permitted.

## 5. Exact architecture definition

Both artifacts use the same schema, absolute coordinates, contract, Scenario anchors, and public
primitive rules. The retained generator freezes exact coordinates and IDs; these inventory laws are
the semantic architecture oracle.

The per-sector geometry below is written relative to the sector origin `O`. `q` is defined above,
and `CP = 16q = 16,384` raw Fixed units. The four sectors are translations only; they do not rotate
the local graph.

### 5.1 Brute

Brute must visibly contain:

- 16 independent sensor Wire bodies, four per cardinal sector;
- 16 dedicated signal trunks, one per sensor channel, with no shared long-trunk segment;
- 16 armed defense Wire ribs providing blanket sector coverage;
- zero AND, zero OR, and zero NOT Gates;
- no feedback state and no local processing.

Each sensor has one named `sensor.<sector>.<0..3>` observation. Each dedicated trunk has one named
role. Each defense rib has one named `defense.<sector>.<0..3>` role.

For representative channel 0 in every sector, the ordered Brute defense centerline is exactly
`[q(128,128),q(80,-48),O]` in sector-local notation. These are local IDs `103`, `203`, `303`, and
`403` in west, south, north, and east order respectively. The first point is the channel-0 branch
Junction, and the final endpoint binds that sector's Power Source. Reversing, adding, deleting, or
moving a point changes the retained design.

### 5.2 Computed

Computed must visibly contain:

- 16 sensor Wire bodies grouped four per cardinal sector;
- per sector, three OR Gates reducing four sensors to one sector-presence value;
- per sector, one state cell made only from the retained A/O/N feedback pattern;
- four shared long trunks, one per sector;
- four selectively armed defense ribs, one per sector, named `defense.<sector>.0` so each shares
  the exact representative response-binding name with Brute channel 0;
- exact total Gate inventory: 20 OR, 8 NOT, 0 AND.

The state cell is a primitive-only cross-coupled `OR + NOT` construction derived from the retained
feedback conformance pattern; it is not a Memory/FSM runtime class. The retained design artifact
must bind the sector sensor, state, trunk, and defense roles so the structural oracle can inspect
them without relying on global Entity IDs.

The exact compact Computed graph is repeated in each sector. With local bases
`gateBase=1000+100s`, `sensorBase=2000+100s`, `junctionBase=3000+100s`, and
`wireBase=4000+100s`, its seven Gates and two Junctions are:

| Symbol / local offset | Primitive | Sector-local origin |
|---|---|---|
| `G0 / +0` | OR | `(2,3) CP` |
| `G1 / +1` | OR | `(2,-3) CP` |
| `G2 / +2` | OR | `(5,3) CP` |
| `G3 / +3` | OR | `(8,-3) CP` |
| `G4 / +4` | NOT | `(11,-3) CP` |
| `G5 / +5` | OR | `(8,3) CP` |
| `G6 / +6` | NOT | `(11,3) CP` |
| `J0 / +0` | Junction | `(6,0) CP` |
| `J1 / +1` | Junction | `(4,-1) CP` |

Sensor bodies `S0..S3` are respectively
`O -> q(-1,2)`, `O -> q(-16,-48)`, `O -> q(-3,8)`, and `O -> q(-5,13)`; each
sensor's `A` end binds the sector Power Source and its `B` end remains Free. Data and feedback
Wires use these exact endpoint paths, where `InA`, `InB`, `Out`, and `Power` name ordinary Gate
ports:

```text
W0  S0.A -> G0.InA       W1  S1.A -> G0.InB
W2  S2.A -> G1.InA       W3  S3.A -> G1.InB
W4  G0.Out -> G2.InA     W5  G1.Out -> G2.InB
W6  G2.Out -> G5.InB
W10 G3.Out -> G4.InA     W11 G5.Out -> G6.InA
W12 G4.Out -> (13,-3)CP -> (13,0)CP -> J0
W13 J0 -> G5.InA
W14 G6.Out -> (14,3)CP -> (14,1)CP -> (6,-2)CP -> G3.InB
W20 J0 -> J1
W21 J1(4,-1)CP -> (5,-3)CP -> O
```

`W21` is the sole `defense.<sector>.0` Wire and its endpoint at `O` binds the sector Power Source.
The Gate Power-source paths are frozen consistently with the Gate offsets:

```text
W30 O -> G0.Power
W31 O -> (0,-5)CP -> G1.Power
W32 O -> G2.Power
W33 O -> (-2,-5)CP -> G3.Power
W34 O -> (1,-5)CP -> (6,-5)CP -> G4.Power
W35 O -> G5.Power
W36 O -> G6.Power
```

Thus `W30..W36` power `G0..G6` in the same numeric order. In particular, `W35` powers the
upper state OR `G5`; `W33`, `W34`, and `W36` power `G3`, `G4`, and `G6`; none of those names may be
reassigned to a different Gate merely to describe the schedule.

### 5.3 Autonomy and costs

The retained endpoint partition and paired timeline are exactly:

| Stage | Brute binding rows | Computed binding rows | Command Tick | Empty barrier Ticks | Common quiescent boundary |
|---|---:|---:|---:|---|---:|
| 0 | `48` all non-Source ends | `156`: all `136` non-Source ends, all `16` sensor Source ends, and four `W30` Source ends | `3` | `4,5,6,7` | `8` |
| 1 | `0` | `8`: `W31` and `W32` Source ends in four sectors | `8` | `9,10` | `11` |
| 2 | `0` | `4`: `W35` Source ends in four sectors | `11` | `12,13` | `14` |
| 3 | `32`: all 16 sensor and 16 defense Source ends | `16`: `W21`, `W33`, `W34`, and `W36` Source ends in four sectors | `14` | `15,16,17` | `18` |

Both candidates execute every one of these four indices on the listed command Tick; Brute stages 1
and 2 are shared `step([])` command Ticks. The paired materializer records the listed barrier Ticks
and reaches the common first signal-quiescent boundary at Tick 18. Therefore
`buildEndTick = measurementStartTick = 18`; `maxTicks = finalNextTick = 20`. These derived empty
Ticks are retained in Replay checkpoints and are not user-authored commands or separately stored
schedule inputs.

After boundary 18, neither design receives an architecture-specific command or any World input;
both retained input streams are empty. On completed Tick 18, every bound channel-0 sensor has
`sampled_presence = true`, and neither design reports a contact or destruction. On completed Tick
19, each design reports exactly four bound Enemy/defense contacts, one per sector, each with
`absorbed = 1`. No entity is destroyed anywhere in the retained trace through `finalNextTick = 20`.
A raw `desiredOutput != currentOutput` mismatch is not itself pending work: below the frozen
operate threshold it schedules no transition and therefore does not delay `measurementStartTick`.

All artifact operations are direct bootstrap placements, so observed applied Construction Work is
zero. `plannedConstructionWork` is nevertheless computed by the exact S1-M4 public Work kernels
for the complete primitive inventory. These two values are reported separately and never conflated.

The retained Balance v5 values are not altered to keep a design alive. All four response rows are
complete on Tick 19, so the bounded retained trace ends at boundary 20 before cumulative
alpha-profile Heat destroys infrastructure. S1-M6 owns any new balance profile used for long sweep
runs.

## 6. Experiment v2 and Run ID v2

Experiment manifest v1, `ExperimentStage::S1M0`, `LongWireDesign`, and Run ID v1 remain byte- and
behavior-identical.

Experiment manifest v2 adds `stage: "s1-m5"`, one exact reference-pair artifact, two design
references, one selected Numeric/Physical/Balance triad, `Seed::ZERO`, boundaries, and Metric Set
reference. It is not a matrix or sweep. Canonical design order is ascending Artifact hash and must
still contain exactly one Brute and one Computed role.

Run ID v2 uses domain `AON\0EXPERIMENT-RUN\0V2\0`, encoder `u16(2)`, and includes in order:

1. experiment ID;
2. pair artifact hash;
3. Scenario artifact hash;
4. shared-build Command Log hash;
5. selected Design Artifact hash;
6. selected Design Command Log hash;
7. semantics text and Numeric/Physical/Balance hashes;
8. Seed bytes;
9. build-end, measurement-start, and final/max-Tick boundaries;
10. Metric Set ID and semantic hash.

Paths, display labels, output directory, build commit, JSON formatting, and run ordinal are excluded.
The two reference runs have distinct Run IDs. Duplicate Run IDs fail rather than being salted.

## 7. Reference Metric Set v1

Metrics are derived, noncanonical, read-only observations. Collection cannot change State, Tick,
events, IDs, revisions, caches, reports, or subsequent results. All sums use checked `u128`; JSON
uses canonical base-10 strings without sign or leading zero (except `"0"`). Overflow or missing
required observation fails the whole result with no partial artifact.

The full Run trace is retained. Comparison accumulators cover `[measurementStartTick, finalNextTick)`.

### 7.1 Static inventory

- planned total Wire centerline length in raw Fixed units and NCU;
- shared, sensor, trunk, defense, and other Wire length subtotals;
- Gate total, AND, OR, and NOT counts;
- planned Construction Work from the exact Work kernel;
- build Command count and Command Log hash.

Static values are recomputed from the validated design artifact. Runtime peak/final Capacity used is
reported separately and must not replace planned length.

### 7.2 Runtime reductions

- `survivedBoundary`: final status is Running at the requested boundary;
- `completedTicks`, terminal status/Tick/cause, measurement-start Core integrity, final Core
  integrity, and total Core damage;
- Power sums: region generation, load nominal, granted, source cost, and transmission loss;
- `brownoutTicks`: count of Ticks containing any positive load with ratio below one;
- Construction requested, nominal Power, granted Work, and applied Work sums;
- Heat: sum `PowerStepReport.heat_contributions` plus `StepReport.interaction_heat` exactly once;
- Network peak/final/integral used NCU and total support-demand integral;
- enemy kills: Damage destruction of a bound Scenario Enemy, unique per Enemy;
- per-observation response latency rows.

Power Heat rows are not also re-counted from canonical heat deltas. Interaction Heat contains only
its distinct kinds. Rows are reduced by their stable tuple keys before accumulation.

### 7.3 Response latency

The retained Metric Set has exactly four response rows, one per canonical cardinal-sector Enemy.
Each row uses that sector's representative channel `0` in both designs:
`sensor.<sector>.0 -> defense.<sector>.0`. The remaining Brute channels are still structural
coverage evidence, not additional observations of the same Enemy. Each row name is `<sector>.0`;
Scenario semantic Enemy ordinals are `west=0`, `south=1`, `north=2`, and `east=3` (while canonical
row order by name is east, north, south, west). Each pair binding names:

- stimulus: the first completed Tick at or after measurement start where the bound sensor Wire/end
  `PowerSenseReport.sampled_presence` becomes true;
- response: the first completed Tick at or after stimulus where the bound defense Wire has a
  positive `ContactEnergyReport.absorbed` against the bound Enemy;
- latency: checked `responseTick - stimulusTick`.

Missing stimulus, missing response, response-before-stimulus, duplicate names, ambiguous Enemy, or
an observation outside the window is a typed failure. Latency is Tick-based, never wall clock.

For the retained pair, all four stimulus Ticks are exactly 18 and all four response Ticks are
exactly 19 in both designs. Consequently every retained response row has latency `19 - 18 = 1`.
Each response is backed by its bound contact row with `absorbed = 1`, not merely by an active Wire
level, and neither role records a destruction before boundary 20.

### 7.4 Metric artifact hash

The semantic hash domain is `AON\0REFERENCE-METRICS\0V1\0`, encoder `u16(1)`, followed by format,
Metric Set ID, Run ID v2, exact boundaries, static inventory, runtime scalar fields, and sorted
latency rows. File SHA-256, build commit, paths, and presentation formatting are provenance only.

## 8. Retained artifacts and generator

The canonical retained set is:

```text
fixtures/scenarios/s1-m5-reference-architectures-v1.json
fixtures/designs/s1-m5-brute-v2.json
fixtures/designs/s1-m5-computed-v2.json
fixtures/experiments/s1-m5-reference-pair-v1.json
fixtures/experiments/s1-m5-reference-plan-v2.json
fixtures/metrics/s1-m5/reference-metric-set-v1.json
fixtures/replays/s1-m5/brute-v1.json
fixtures/replays/s1-m5/computed-v1.json
fixtures/metrics/s1-m5/brute-v1.json
fixtures/metrics/s1-m5/computed-v1.json
```

The pair references one strict Scenario-v4 artifact and the existing selected profiles. Replay v2
does not learn design paths; a separate verifier requires its Command stream to equal the
materialized design Command Log and its checkpoints to match execution.

`generate_s1m5_reference_architectures` builds all artifacts twice in memory, requires byte
equality, and by default writes nothing. Non-writing mode byte-compares every checked-in artifact,
strictly decodes/re-encodes it, resolves locators canonically, materializes both designs, runs both
traces, reduces metrics, and checks every frozen inventory/hash/fact. `--write` publishes only after
the same double-build succeeds, then performs the same read-back checks.

File SHA-256, semantic hashes, Command Log hashes, Run IDs, initial/final/every-Tick V7 hashes,
static inventories, and complete metric bytes are closure goldens.

## 9. Public API boundary

The minimum public surface is:

```text
decode_reference_architecture_artifact / encode_reference_architecture_artifact
validate_reference_architecture_against / semantic_hash
ReferenceArchitectureMaterializationPlan and typed resolver
materialize_reference_architecture (atomic candidate execution)
materialize_reference_architecture_pair (atomic lockstep v2 execution)
decode/encode/validate ReferenceArchitecturePairManifest
decode/encode/resolve ExperimentPlanArtifactV2
experiment_run_id_v2
reduce_reference_metrics / decode/encode metric artifact
```

Materialization and reduction may live in the headless/experiment layer when they require a complete
Run, but validation, canonical encoders, hashes, and pure reductions live in `aon-sim`.

## 10. Error precedence and atomicity

For each strict artifact, envelope/version/hash selection precedes body typing. Then structural
shape precedes referenced semantic hashes; reference existence/kind precedes geometry; pair fairness
precedes Run resolution; Run resolution precedes execution; execution/checkpoint faults precede
metric reduction; metric structural/order faults precede numeric overflow.

Errors are exact typed variants. No saturating/wrapping arithmetic, silent sorting of duplicate
semantic keys, fallback binding, partial metric output, partial materialization, lossy ID cast,
panic, or design-specific exception is permitted.

## 11. Executable gates

### Gate 1 — Retained baseline

All Stage 0 and S1-M0 through S1-M4 exact suites, artifacts, hashes, and gates remain green. Module
v1, Experiment v1, and Run ID v1 bytes/goldens are unchanged.

### Gate 2 — Strict schemas and versions

Architecture v1/v2, pair v1, metric v1, and Experiment v2 select envelopes first, strictly decode
and canonically re-encode, reject malformed/unknown/duplicate/trailing inputs, and preserve every
retained v1 byte, hash, plan, and execution behavior.

### Gate 3 — Design identity and resolver

Independent canonical bytes/hash goldens, full field sensitivity, input permutation normalization,
local/scenario reference checks, two-pass Command equality, Command Log hash, and atomic rollback.

### Gate 4 — Pair fairness

Scenario/profile/Core capacity/Power/territory/Enemy sequence/Seed/versions/boundaries/metric set
are exact equal; only the two design/Command Log hashes differ. Any compound mismatch fails with the
frozen typed precedence.

### Gate 5 — Brute structural oracle

Exact 16 sensors, 16 dedicated long trunks, 16 defense ribs, zero Gates, named roles, no shared
trunk, representative channel-0 defense path `[q(128,128),q(80,-48),O]`, the frozen
`[48,0,0,32]` endpoint partition, and no command after build boundary.

### Gate 6 — Computed structural oracle

Exact 16 sensors in four groups, 12 reduction OR, four primitive state cells adding eight OR/eight
NOT, four shared trunks, four defense ribs, total OR20/NOT8/AND0, named roles, and no privileged
runtime class or post-build command. The compact per-sector graph, sensor bodies, Gate/source
mapping `W30..W36 -> G0..G6`, defense path `J1 -> (5,-3)CP -> O`, and frozen
`[156,8,4,16]` endpoint partition are exact. Every non-Source endpoint is in stage 0.

### Gate 7 — Fair deterministic execution

Both designs materialize from fresh identical packages through the same paired v2 interpreter.
Every command is accepted, all retained batch indices and earliest-common-quiescence barriers are
lockstep, repeated builds produce identical ID maps/commands/hashes, rejections are atomic, shared
inputs are byte-identical, build-time contact/destruction/run-end is forbidden while ordinary
thermal damage is retained, and operational traces are autonomous. Exact stage evidence is
`3 -> 8`, `8 -> 11`, `11 -> 14`, `14 -> 18`; the common build and measurement boundary is 18
and the retained final boundary is 20.

### Gate 8 — Metrics

Independent checked oracle pins every static/runtime metric, window boundary, heat non-duplication,
Power and support reductions, enemy kill uniqueness, latency rules, overflow, ordering, and
read-only noninterference. Tick 18 supplies four true bound sensor samples per role with no contact
or destruction; Tick 19 supplies four bound contacts per role at `absorbed=1`; every latency is 1
and the retained trace has no destruction.

### Gate 9 — Retained host equivalence

Direct, headless, and Bevy hosts reproduce every command acceptance, complete StepReport, V7
checkpoint, final status/hash, role observation, and exact metric artifact for both designs.

### Gate 10 — Negative scope and anti-overfit

No Relay/Radiation/Payload/cargo/repair/Module runtime/sweep/crossover/new AI; no design-specific
profile/enemy/rule; no silent rescale; no failed-profile omission; no Stage-1 gate claim.

### Gate 11 — Generator and corpus

Non-writing generator double-builds and matches all checked-in bytes; strict bounded decoder,
resolver, metric, reference/order, and retained corpus remain deterministic and panic-free.

### Gate 12 — Native clean-clone closure

After the implementation commit, an independent Windows-native `git clone --no-local` runs locked
offline metadata, formatting, all-target check, strict Clippy, serial workspace tests, app build,
all prior gates, S1-M5 gate, generator verification, and leaves a clean status. WSL is not used.

Only after Gate 12 may README/tracker mark S1-M5 complete in a separate closure commit. S1-M6 and
both complete Stage 1 gates remain open.

## 12. Closure checklist

- [ ] authority file hash frozen;
- [ ] v1 experiment/module/design/run goldens retained;
- [ ] all new artifact hashes and file SHA-256s frozen;
- [ ] both exact architecture inventories frozen;
- [ ] pair fairness and two distinct Run IDs proven;
- [ ] direct/headless/Bevy complete-trace and metric equality proven;
- [ ] generator twice-generated and checked-in byte equality proven;
- [ ] Gates 1–11 green in the implementation tree;
- [ ] implementation commit pushed;
- [ ] Gate 12 fresh Windows-native clean clone green and clean;
- [ ] closure README/tracker commit pushed with S1-M6 still open.
