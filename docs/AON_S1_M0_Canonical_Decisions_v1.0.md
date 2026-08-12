# A/O/N — S1-M0 Canonical Decisions

**Status:** implementation authority
**Applies to:** `S1-M0 — Physical Scale Experiment Baseline`
**Source baseline:** PRD v1.0 GO Candidate / SSS v1.0 Draft / TRD v1.0 Draft

This document freezes representation and validation choices required to implement S1-M0 without
silently changing Stage 0 semantics or prematurely implementing the Stage 1 economy. It is
subordinate to the source documents in their respective areas of authority:

- the PRD determines the product question, required experiment axes, and product gate;
- the SSS determines observable simulation behavior and which contract owns a value;
- the TRD determines artifact, API, hashing, ordering, and test structure;
- this document closes gaps and resolves draft wording only for S1-M0.

If a later PRD or SSS revision changes an observable law, the compatibility review required by
TRD section 41 applies. Passing S1-M0 does not pass the Stage 1 technical or product gate.

## 1. Authority conflict resolutions

### 1.1 Physical Scale Profile Hash is not a Run ID

The PRD requires a sweep over Gate Footprint, Circuit Routing Pitch, Long-wire Distance, and
Network Capacity. The SSS assigns these values to different sources of truth:

| Value | Canonical owner |
|---|---|
| Gate footprint and Gate port anchors | `PhysicalScaleProfile` |
| Circuit routing pitch | `PhysicalScaleProfile` |
| World routing pitch | `PhysicalScaleProfile` |
| Wire geometry quantum, body radius, substrate clearance | `PhysicalScaleProfile` |
| Long-wire distance | Scenario or Design absolute geometry |
| Main Core capacity and support-curve coefficients | `BalanceProfile` |
| Seed | Experiment Run input |

Therefore:

- changing only a Physical Scale field MUST change `physicalScaleProfileHash`;
- changing only Long-wire Distance MUST NOT change `physicalScaleProfileHash`;
- changing only Network Capacity MUST change `balanceProfileHash`, not the Physical Scale hash;
- changing only Seed MUST change Run identity, not any Profile hash.

TRD S1-M0's completion phrase "each Run has a unique Physical Scale Profile Hash" and the SSS
phrase "each Sweep run has a unique profile hash" MUST be read as follows:

1. each semantically distinct Physical Scale variant has one distinct canonical Physical Scale
   Profile Hash;
2. each Run records the exact Numeric, Physical Scale, and Balance Profile hashes it uses;
3. the identity of the whole Run is `ExperimentRunId`, not a Physical Scale Profile Hash.

Literal per-Run Physical hash uniqueness would make distance and seed replication impossible and
would place Scenario geometry in the wrong contract. An implementation MUST NOT manufacture
unique Physical hashes by changing `profileId`, file path, JSON formatting, or unrelated fields.
Those metadata are excluded from canonical Profile hashes by the Stage 0 contract.

### 1.2 PRD `v0-alpha` is a product label, not schema version zero

The PRD's `Physical Scale Profile v0-alpha` names the provisional product-balance baseline. The
implemented artifact contract already uses Physical Scale `schemaVersion = 1` and Profile
canonical encoder version `1`. S1-M0 retains that schema and encoder. It MUST NOT introduce or
accept a schema-zero artifact merely to reproduce the PRD label.

### 1.3 Stage 0 alpha remains a baseline, not the Stage 1 answer

`profiles/physical-scale/stage0-alpha.json` is the required loaded baseline. Its values remain
valid inputs and retained golden evidence. They are not declared to be the Crossover answer.
S1-M0 creates versioned variants and the mechanism that will later evaluate them; it does not
select the final values required by PRD OPEN-C.

### 1.4 Absolute Module geometry forbids implicit migration

"Absolute geometry" means exact fixed-point world-unit geometry in a Module-local coordinate
frame. It does not mean screen pixels, zoom-relative coordinates, grid indices, or proportions of
the current Gate footprint. A Module made under one Physical Scale contract MUST NOT be resized,
snapped, re-anchored, or otherwise reinterpreted to fit another contract.

## 2. S1-M0 scope

S1-M0 owns:

- loading and retaining the Stage 0 Physical Scale alpha artifact;
- generating validated Physical Scale semantic variants;
- deterministic Numeric Profile Hash, generated Physical Scale Profile, and Balance Profile Hash
  axes corresponding to the TRD `ProfileMatrix` contract;
- deterministic expansion of the baseline experiment axes into `ExperimentRunSpec` values;
- an explicit Long-wire Distance axis represented as absolute `Fixed` geometry;
- explicit Scenario and Design `ArtifactHash` inputs and deterministic `ExperimentRunId`
  generation;
- a strict, versioned Module Blueprint artifact sufficient to retain absolute primitive geometry;
- exact Module contract compatibility and no-silent-scaling validation;
- retained artifact, hash, replay, ordering, and Windows-native evidence for this slice.

S1-M0 does not implement:

- Main Core state or active Network Capacity accounting;
- the overcapacity support-load runtime;
- sensing, Power, Brownout, Heat, Construction, Contact, Enemy, or Damage behavior;
- the Brute and Computed reference Design artifacts owned by S1-M5;
- execution of the complete parameter sweep or production of metrics and Crossover reports owned
  by S1-M6;
- a product verdict or final Physical Scale numeric selection;
- Module placement, Construction Site creation, partial activation, migration, or the persistent
  Module Library;
- a runtime Module entity or black-box Module execution;
- a new Canonical World store or a State Hash field.

The S1-M0 Module artifact is a compatibility and geometry baseline. Later milestones may add
placement and library operations without changing the no-scaling rule.

## 3. Frozen versions

| Contract or artifact | S1-M0 value |
|---|---|
| Semantics Version | `aon-semantics-v1` |
| Numeric Profile schema | `1` |
| Physical Scale Profile schema | `1` |
| Balance Profile schema | `2` |
| Profile canonical encoder | `1` |
| Hash algorithm | BLAKE3, id `blake3-v1` |
| Canonical State Hash | `aon-state-v4` |
| Replay format | `1` |
| Experiment manifest format | `1` |
| Experiment Run ID encoder | `1` |
| Long-wire Design hash encoder | `1` |
| Module format | `1` |
| Module semantic hash encoder | `1` |

S1-M0 changes no observable Tick transition and adds no Canonical World state. It therefore MUST
NOT bump Semantics Version, State Hash Version, or Replay Format Version. A later change to a
Physical Scale coefficient continues to change only its canonical Profile hash. A change to the
meaning or formula of a field requires a Semantics Version review; a change to the serialized
shape or canonical field set requires the corresponding artifact or encoder version review.

## 4. Physical Scale variant generation

### 4.1 Existing Physical Scale v1 field set

The v1 semantic field set remains exactly:

```text
wireGeometryQuantum
circuitRoutingPitch
worldRoutingPitch
wireBodyRadius
gateFootprints.and / or / not
gatePortAnchors.and / or / not
substrateClearance
```

`profileId` and `kind` remain validated artifact fields. `profileId` is not a semantic hash input;
`kind` and `schemaVersion` are.

S1-M0 MUST NOT add Long-wire Distance, Network Capacity, Experiment ID, Seed, or a sweep ordinal
to `PhysicalScaleProfile`.

### 4.2 Physical variant axes

The S1-M0 Physical variant generator takes one validated base Physical Scale profile and these
axes:

1. Gate geometry variants;
2. Circuit routing pitches;
3. World routing pitches.

A Gate geometry variant contains the complete AND, OR, and NOT footprint tables and the complete
AND, OR, and NOT port-anchor tables. Footprint variation MUST NOT implicitly multiply, divide, or
move an anchor. Every resulting anchor coordinate is explicit input and is validated against its
corresponding footprint.

Unless explicitly supplied by a future versioned axis, the generator copies these fields exactly
from the base profile:

```text
wireGeometryQuantum
wireBodyRadius
substrateClearance
```

Copying is exact raw `Fixed` assignment. It is not rescaling.

### 4.3 Candidate validation

Every Cartesian candidate MUST pass the existing `PhysicalScaleProfile::validate()` contract
before it can enter a matrix. This includes:

- supported schema and expected Profile kind;
- nonempty metadata `profileId`;
- positive geometry quantum, routing pitches, body radius, and footprint extents;
- nonnegative substrate clearance;
- exact geometry-quantum alignment;
- centerable Gate footprints;
- every anchor inside and on the boundary of its Gate footprint.

No candidate is rounded, clamped, repaired, or silently skipped. One invalid candidate fails the
generation request with a typed error and returns no partial matrix.

The generated `profileId` is descriptive metadata only. It MAY contain a canonical hash prefix
for operator convenience, but neither uniqueness nor lookup correctness may depend on that text.
S1-M0 uses the exact generated form `s1m0-physical-{hash}`, where `{hash}` is the full lowercase
canonical Physical Scale Profile Hash. The ID remains excluded from that hash.

### 4.4 Semantic duplicate rule

After validation, each candidate's existing canonical Physical Scale Profile Hash is computed.
Two candidates with the same semantic hash are a duplicate even if they have different
`profileId` values or came from different axis positions. Duplicate semantic variants MUST cause
`ExperimentPlanError::DuplicatePhysicalScaleProfile`. They MUST NOT be silently deduplicated and
MUST NOT be made distinct by salting metadata.

### 4.5 Canonical variant order

Validated generated variants are ordered by their 32 raw canonical Profile Hash bytes in
ascending lexicographic order. Reversing or permuting input axes therefore MUST produce the same
semantic variant sequence. File path, `profileId`, input ordinal, map iteration order, and locale
MUST NOT affect the order.

`PhysicalScaleMatrix::resolve` accepts at most `MAX_PHYSICAL_SCALE_PROFILES = 4_096` resolved
semantic variants. The Cartesian product is checked before publication. Exceeding the limit is
`TooManyPhysicalScaleProfiles`; arithmetic exhaustion is `CardinalityOverflow`.

## 5. Profile Matrix and Run expansion

### 5.1 Profile Matrix contract and S1-M0 API

The TRD `ProfileMatrix` contract remains:

```rust
pub struct ProfileMatrix {
    pub numeric_profiles: Vec<ProfileRef>,
    pub physical_scale_profiles: Vec<ProfileRef>,
    pub balance_profiles: Vec<ProfileRef>,
}
```

It denotes the Cartesian product:

```text
Numeric × Physical Scale × Balance
```

Numeric is normally a singleton v1 axis, but the implementation MUST treat it as an explicit
axis. S1-M0 represents the generated Physical portion as `PhysicalScaleMatrix` and resolves it to
`ResolvedPhysicalScaleProfile` values. `ExperimentPlan` accepts Numeric and Balance semantic hash
axes and one resolved Physical matrix. Every axis MUST be nonempty. An artifact-backed
`ProfileRef` resolver MUST require that its artifact's:

- schema and kind are supported;
- internal invariants are valid;
- declared `profileId` matches the reference metadata;
- declared Profile hash matches the canonical semantic hash.

References are sorted by raw semantic Profile Hash within each axis. A duplicate hash in one axis
is rejected. Paths and IDs locate or label an artifact; they do not define matrix identity.

### 5.2 Axes outside `ProfileMatrix`

For one resolved `ExperimentPlan`, Scenario Artifact Hash, `maxTicks`, and Metric Set ID are fixed.
Each distance derives its own `LongWireDesign` and therefore its own Design Artifact Hash. The
exact S1-M0 Run product is:

```text
Numeric Profile
× Physical Scale Profile
× Balance Profile
× Long-wire Distance
× Seed
```

Long-wire Distance and Seed are Experiment axes, not Profile axes. Scenario Artifact Hash,
`maxTicks`, and Metric Set ID are fixed attributes of one plan and remain part of every Run
identity. Design Artifact Hash is derived separately for each distance. There is no independent
Design vector axis in S1-M0. Adding the S1-M5 Brute/Computed Design axis requires an Experiment
format and identity review rather than silently extending this v1 `ExperimentPlan` seam.

### 5.3 Empty and overflowing products

An empty Profile, distance, or seed axis is invalid. The implementation MUST compute the
Run count with checked arithmetic before allocating or returning Runs. A count that cannot fit the
canonical unsigned 32-bit collection length fails with `CardinalityOverflow`. More than
`MAX_EXPERIMENT_RUNS = 65_536` resolved Runs fails with `TooManyExperimentRuns`. It MUST NOT
truncate, wrap, lazily omit the tail, or return a partial list. Tests may exercise a smaller limit
through `ExperimentPlan::resolve_with_run_limit`; production resolution uses the frozen maximum.

### 5.4 Canonical Run order

Runs are emitted in ascending lexicographic order of this tuple:

```text
NumericProfileHash raw bytes
PhysicalScaleProfileHash raw bytes
BalanceProfileHash raw bytes
LongWireDistance raw signed Fixed value
Seed raw bytes
```

All input axes are semantically normalized before expansion. Manifest JSON array order, paths,
display names, hash-map order, and parallel scheduling MUST NOT alter output order.

## 6. Long-wire Distance

### 6.1 Ownership

Long-wire Distance is the problem scale expressed by actual Wire geometry. It is not a timing
coefficient and not a Profile field. The reference S1-M0 distance fixture is a straight,
axis-aligned Open World segment:

```text
start = (0, 0)
end   = (L, 0)
```

where all values are raw fixed-point world coordinates. This makes `L` both the declared axis
value and the exact canonical polyline length under the existing Euclidean length law.

### 6.2 Validation

For every Physical Scale combination, `L` MUST:

- be strictly positive;
- be represented as raw signed `Fixed`, never JSON floating point;
- be an exact multiple of that Run's `worldRoutingPitch` for the axis-aligned reference fixture;
- produce quantized endpoints without rounding;
- fit all checked coordinate additions and canonical geometry calculations.

If one distance is invalid under one selected Physical Scale profile, expansion fails for the
manifest. It MUST NOT round `L`, move an endpoint, omit that Run, or substitute a nearby pitch.

For each distance, S1-M0 constructs a `LongWireDesign` with exact geometry:

```text
start = (0, 0)
end   = (L, 0)
```

Its semantic artifact hash is BLAKE3 over:

```text
domain:                 ASCII bytes AON\0LONG-WIRE-DESIGN\0V1\0
encoderVersion:         u16 little-endian = 1
start.x, start.y:       raw i64 Fixed, little-endian
end.x, end.y:           raw i64 Fixed, little-endian
```

Changing `L` alone leaves all three Profile hashes and the fixed Scenario Artifact Hash
unchanged, changes the Long-wire Design Artifact Hash, and MUST change `ExperimentRunId`.

## 7. Scenario and Design artifact identity

S1-M0 uses the distinct nominal 32-byte type `ArtifactHash` for the resolved Scenario and Design
artifact identities. It MUST NOT reuse `ProfileHash`, `StateHash`, or `ExperimentRunId` for these
fields merely because they have the same byte width.

The Run planner receives a declared Scenario `ArtifactHash`; defining a new generic Scenario
schema or replacing the existing Scenario loader is outside this slice. The producer of that hash
MUST use semantic artifact content, not a filesystem path, working directory, JSON formatting, or
display label. The S1-M0 retained baseline freezes its expected lowercase hash as a golden.

The Design `ArtifactHash` for the S1-M0 reference distance is the `LongWireDesign` semantic hash
defined in section 6. Multiple later Design formats may share the `ArtifactHash` nominal wrapper,
but each format MUST have its own domain separator and encoder version.

Neither Artifact Hash is added to Canonical World State. Two differently identified artifacts may
still produce equal per-Tick State hashes if their constructed initial state and input log are
semantically equal.

## 8. Experiment artifact and Run identity

### 8.1 Minimum S1-M0 manifest

The later persisted S1-M0 Experiment manifest will be strict, versioned JSON containing at least:

```text
formatVersion = 1
hashAlgorithmId = "blake3-v1"
experimentId
stage
Scenario reference + declared ArtifactHash
Long-wire Design derivation version
Numeric Profile Hash axis
PhysicalScaleMatrix
Balance Profile Hash axis
Long-wire Distance axis
Seed axis
maxTicks
Metric Set identity
```

S1-M0 freezes its typed planning API and canonical identities first. If JSON decode/encode is
exposed in this milestone, unknown fields, duplicate struct fields, trailing data, floating-point
numbers for canonical numeric fields, unsupported versions, and unsupported algorithms are
rejected. Human-facing CSV or Markdown is never a canonical input.

For the exact S1-M0 distance axis, the Design Artifact Hash is always derived from
`LongWireDesign`; it is never a path, display name, caller-supplied placeholder, or independent
input. The full Brute and Computed Design Artifact schema remains owned by S1-M5.

### 8.2 `ExperimentRunId`

`ExperimentRunId` is a separate 32-byte nominal type. It is BLAKE3 over:

```text
domain:                       ASCII bytes AON\0EXPERIMENT-RUN\0V1\0
encoderVersion:               u16 little-endian = 1
experimentId:                 u32 UTF-8 byte length + bytes
scenarioArtifactHash:         32 raw bytes
designArtifactHash:           32 raw bytes
semanticsVersion:             u32 UTF-8 byte length + bytes
numericProfileHash:           32 raw bytes
physicalScaleProfileHash:     32 raw bytes
balanceProfileHash:           32 raw bytes
longWireDistance:             raw i64 Fixed, little-endian
seed:                         32 raw bytes
maxTicks:                     u64 little-endian
metricSetId:                  u32 UTF-8 byte length + bytes
```

Strings are encoded as UTF-8 bytes and byte lengths, never character counts or null-terminated
host strings. Hashes are raw bytes, never their hexadecimal display form.

Every field above is semantically significant. Changing any one MUST change Run ID. The following
are excluded:

- file path and working directory;
- Profile ID or display label;
- matrix/input ordinal;
- JSON formatting and key order;
- wall-clock time;
- host OS or renderer;
- thread scheduling;
- build commit ID;
- Command Log Hash;
- Final State Hash;
- Metric result artifact hash.

Build commit, Command Log Hash, Final State Hash, and Metric Artifact Hash are Run result lineage
recorded beside the Run ID under TRD section 36.4. They are not inputs to the identity of the Run
that produces them. Including an output hash would be circular; including a build commit would
make semantically identical Runs implementation-dependent.

After expansion, duplicate `ExperimentRunId` values are a manifest error. The implementation MUST
NOT add an ordinal or random salt to make them distinct.

### 8.3 Metric Set identity

The Metric Set used in S1-M0 Run identity is a nonempty stable `metricSetId` UTF-8 string. It is
encoded by byte length and bytes, exactly as shown above. Its later metric-definition artifact is
owned by the experiment harness; changing the selected ID changes the Run ID. A path is not a
Metric Set ID.

## 9. Module Blueprint baseline

### 9.1 Public contract surface

The S1-M0 baseline uses these versioned concepts:

```text
MODULE_FORMAT_VERSION_V1
ModuleFormatVersion
ModuleLocalId
ModuleContract
ModuleBlueprint
AbsoluteModuleGeometry
SubstrateBlueprint
GateBlueprint
JunctionBlueprint
WireBlueprint
ModuleRoutingDomain
ModuleEndpoint
ModuleIoBinding
ModuleProvenance
Module semantic `ArtifactHash`
ModuleError
```

The strict artifact operations are:

```text
decode_module_artifact
encode_module_artifact
validate_module_against
ModuleBlueprint::semantic_hash
```

The exact signatures are:

```rust
pub fn decode_module_artifact(source: &str) -> Result<ModuleBlueprint, ModuleError>;
pub fn encode_module_artifact(module: &ModuleBlueprint) -> Result<String, ModuleError>;
pub fn validate_module_against(
    module: &ModuleBlueprint,
    contract: &SimulationContract,
    physical_scale: &PhysicalScaleProfile,
) -> Result<(), ModuleError>;

impl ModuleBlueprint {
    pub fn semantic_hash(&self) -> Result<ArtifactHash, ModuleError>;
}
```

These names define an artifact and validator seam, not a runtime Module entity or placement
command.

### 9.2 `ModuleContract`

Exact placement compatibility requires:

```text
semanticsVersion
numericProfileHash
physicalScaleProfileHash
```

An optional `balanceProfileHash` MAY be retained as analysis/provenance metadata, but it MUST NOT
be required for exact geometry compatibility and MUST NOT be used to resize geometry. It is
excluded from the Module semantic hash for S1-M0.

Two Profile artifacts with different `profileId` or paths but the same canonical semantic hashes
are compatible. Two artifacts with different Physical Scale hashes are incompatible even if the
stored Module geometry happens to validate under both.

### 9.3 Absolute geometry representation

`AbsoluteModuleGeometry` stores exact Module-local fixed-point coordinates and extents for every
primitive. At minimum it retains:

- Substrate origin, routing area, and physical footprint;
- Gate local ID, Gate kind, origin, and routing-domain reference;
- Junction local ID, position, and routing-domain reference;
- Wire local ID, exact ordered polyline vertices, routing-domain reference, and both endpoints;
- I/O bindings to stable Module-local endpoints.

All Module-local IDs are nonzero, stable within the artifact, and unique in the applicable
namespace. They are not Runtime `EntityId` values. References MUST resolve to a live record of the
required kind. Wire vertices retain their exact stored order and redundant live vertices, just as
Canonical World geometry does.

Geometry uses raw signed `Fixed` values. The artifact MUST NOT store:

- UI pixels, zoom, viewport, or grid-cell indices;
- percentages of a footprint or routing area;
- normalized coordinates;
- floating-point geometry;
- a hidden scale factor;
- Runtime Entity IDs or dense store indices.

The only transform a later exact placement may apply is checked translation of the entire
Module-local coordinate frame to an explicitly requested world origin. S1-M0 freezes identity
orientation. Implicit rotation, reflection, uniform scale, nonuniform scale, pitch snapping, and
automatic port-anchor relocation are forbidden.

### 9.4 Compatibility before geometry reinterpretation

`validate_module_against` performs exact compatibility in this order:

1. Semantics Version;
2. Numeric Profile Hash;
3. Physical Scale Profile Hash.

If any field differs, it returns a typed compatibility error equivalent to
`ExplicitMigrationRequired`. It MUST return before attempting target-profile geometry repair or
transformation. Geometry that is coincidentally valid under the target does not override a
contract mismatch.

For an exact match, the validator first recomputes the supplied `PhysicalScaleProfile` hash and
requires that it equal the already-matched contract hash. It then checks the stored raw geometry
unchanged using the referenced Numeric and Physical Scale contracts: checked numeric range,
geometry quantum, routing pitch, routing domain, footprint and routing-area containment,
endpoint/reference integrity, overlap, and the existing primitive geometry laws.

Validation is pure. Success or failure MUST NOT mutate the source Module, allocate Runtime IDs,
create Construction Sites, or consume a canonical identity frontier.

### 9.5 Explicit migration boundary

Migration is outside S1-M0. A future migration operation MUST:

1. load the source Module immutably;
2. select an explicit target contract;
3. construct and validate new geometry without mutating the source;
4. assign new Module identity and provenance;
5. write a new artifact.

Migration is never a fallback branch inside exact validation.

## 10. Module artifact and semantic hash

Module JSON is strict RFC 8259 JSON with:

```text
formatVersion = 1
hashAlgorithmId = "blake3-v1"
ModuleContract
Module Blueprint payload
optional analysis metadata
ModuleProvenance
```

Unknown fields, duplicate struct fields, trailing data, unsupported versions, unsupported hash
algorithms, floating-point geometry, malformed lowercase hashes, duplicate local IDs, unresolved
references, and over-limit collection counts are rejected.

The Module semantic `ArtifactHash` is BLAKE3 over the following canonical stream:

```text
domain:                       ASCII bytes AON\0MODULE\0V1\0
encoderVersion:               u16 little-endian = 1
moduleFormatVersion:          u32 little-endian = 1
hashAlgorithmId:              u32 UTF-8 byte length + bytes
semanticsVersion:             u32 UTF-8 byte length + bytes
numericProfileHash:           32 raw bytes
physicalScaleProfileHash:     32 raw bytes
substrateCount + records:     u32 count, records by ModuleLocalId
gateCount + records:          u32 count, records by ModuleLocalId
junctionCount + records:      u32 count, records by ModuleLocalId
wireCount + records:          u32 count, records by ModuleLocalId
ioBindingCount + records:     u32 count, records by canonical binding key
```

Each primitive record uses stable `u8` enum tags, fixed-width little-endian integers, exact raw
`Fixed` values, and `u32` collection/string lengths. A Wire's vertex order is semantic and is
encoded as stored. Endpoint references and routing-domain references are semantic and included.

The canonical I/O binding key is:

```text
UTF-8 port name bytes, then endpoint kind tag, then referenced ModuleLocalId, then sub-port tag
```

Names are compared by raw UTF-8 byte lexicographic order; locale collation is forbidden.

The following are artifact metadata and are excluded from the Module semantic `ArtifactHash`:

- display name;
- source path;
- `ModuleProvenance` authoring time or tool information;
- optional Balance Profile Hash;
- JSON key order and insignificant whitespace.

Excluding provenance makes byte-different copies of the same blueprint share semantic identity.
The strict canonical encoder still preserves and deterministically emits retained metadata in the
artifact representation; artifact byte equality and semantic Module equality are distinct tests.

## 11. Canonical encoding and ordering rules

All new S1-M0 semantic encoders obey these common rules:

- explicit ASCII domain separation;
- an explicit unsigned 16-bit encoder version;
- fixed-width little-endian integers;
- raw signed `i64` for `Fixed`;
- raw 32-byte hashes and Seeds;
- unsigned 32-bit byte lengths for UTF-8 strings;
- unsigned 32-bit collection counts;
- explicit stable `u8` enum tags and booleans;
- deterministic record ordering before encoding;
- checked conversion for every host `usize` to canonical count;
- BLAKE3 with algorithm id `blake3-v1`.

Rust enum discriminants, struct memory layout, pointer addresses, `HashMap` iteration, filesystem
enumeration, JSON source order, locale, and wall-clock values are never canonical encodings.

Canonical encoding is all-or-error. A failure MUST NOT return a prefix and label it a valid hash.
The public 32-byte wrappers are nominal at their actual API boundaries: `ProfileHash`,
`ArtifactHash`, `StateHash`, and `ExperimentRunId` MUST NOT be freely interchanged merely because
each contains 32 bytes. Scenario, Long-wire Design, and Module semantic identities deliberately
share the `ArtifactHash` wrapper in S1-M0; their distinct domain prefixes prevent cross-format
hash equality, while field names and typed artifact APIs preserve their roles. A later Metric Set
artifact receives its own frozen identity contract when that format is introduced.

## 12. Strict validation and error precedence

### 12.1 Experiment and matrix input

When more than one problem exists in the same artifact, the first reported class follows this
pipeline:

1. JSON syntax, duplicate/unknown field, type, floating-point, or trailing-data failure when using
   a serialized artifact;
2. unsupported Experiment format version or hash algorithm when using a serialized artifact;
3. empty identifier or axis (`EmptyTextField`, `EmptyAxis`), malformed hash, or malformed Seed;
4. text length beyond canonical `u32` (`TextFieldTooLong`);
5. zero `maxTicks` (`NonPositiveMaxTicks`);
6. referenced Profile schema, kind, and internal invariant (`Profile`);
7. declared Profile ID or canonical hash mismatch in an artifact-backed resolver;
8. duplicate generated Physical semantic hash (`DuplicatePhysicalScaleProfile`) or duplicate
   Numeric/Balance hash (`DuplicateProfileHash`);
9. nonpositive or duplicate Long-wire Distance (`NonPositiveLongWireDistance`,
   `DuplicateLongWireDistance`);
10. duplicate Seed (`DuplicateSeed`);
11. Long-wire Distance misaligned to one selected World pitch
    (`LongWireDistanceNotWorldPitchAligned`);
12. checked Cartesian count overflow (`CardinalityOverflow`), followed by the frozen physical or
    Run limit (`TooManyPhysicalScaleProfiles`, `TooManyExperimentRuns`);
13. duplicate `ExperimentRunId` (`DuplicateExperimentRun`).

Errors at or after semantic validation MUST be typed and identify the axis and offending semantic
value or hash. Failure returns no partial variant or Run collection.

### 12.2 Module input and compatibility

Module validation precedence is:

1. JSON syntax, duplicate/unknown field, type, floating-point, or trailing-data failure;
2. unsupported Module format version;
3. unsupported hash algorithm;
4. malformed contract version or canonical hash text;
5. invalid or duplicate Module-local ID and collection-count shape;
6. unresolved or wrong-kind local reference;
7. Semantics Version compatibility mismatch;
8. Numeric Profile Hash compatibility mismatch;
9. Physical Scale Profile Hash compatibility mismatch;
10. supplied Physical Scale Profile canonical hash mismatch against the matched contract;
11. checked arithmetic or coordinate overflow;
12. geometry-quantum alignment;
13. routing-pitch and routing-domain validity;
14. footprint, bounds, endpoint, overlap, and spacing validity.

The exact public `ModuleError` variants may group JSON shape failures, but they MUST preserve this
fail-closed behavior. A compatibility mismatch MUST NOT be hidden by a later geometry error and
MUST NOT invoke an implicit migration.

## 13. Replay and State Hash boundary

The same-profile Replay requirement means:

1. the generated Run constructs a `SimulationContract` from the exact selected Profile hashes;
2. its Replay header records those same hashes;
3. replay under semantically identical Profile artifacts produces the same hash at every declared
   checkpoint;
4. changing only file path, JSON formatting, or `profileId` while keeping semantic Profile hashes
   equal remains replay-compatible;
5. changing the Physical Scale semantic hash is rejected as a Replay contract mismatch before
   stepping.

S1-M0 Experiment manifests, Run IDs, Module Blueprints, and Module provenance are outside
Canonical World State. Merely loading, hashing, validating, or enumerating them MUST NOT change a
Simulation State Hash, identity frontier, Tick, topology revision, event queue, or render snapshot.

## 14. Completion gates

S1-M0 is complete only when every gate below has direct executable or retained evidence.

### Gate 1 — Baseline retention

- Stage 0 Physical Scale alpha strictly loads under schema v1.
- Its existing canonical Profile hash golden is unchanged.
- Existing Stage 0 Replay and State Hash goldens remain unchanged.

### Gate 2 — Physical variant product

- At least two explicit Gate geometry variants, two Circuit pitches, and two World pitches prove
  Cartesian cardinality.
- Every candidate is fully validated before publication.
- Reversing every input axis yields the same hash-ordered variants.

### Gate 3 — Physical semantic hash ownership

- Changing each footprint, port anchor, Circuit pitch, and World pitch independently changes the
  Physical Scale Profile Hash.
- Changing only `profileId`, path, JSON whitespace, or JSON key order does not.
- Duplicate semantic variants with different metadata are rejected.

### Gate 4 — Matrix strictness

- Numeric, Physical Scale, and Balance axes are nonempty and strict.
- Wrong schema, kind, invariant, ID reference, or declared hash is rejected with typed evidence.
- Empty axes and checked Cartesian count overflow fail without partial output.

### Gate 5 — Distance ownership

- Two Runs differing only in `L` have the same three Profile hashes and Scenario Artifact Hash,
  but different Long-wire Design Artifact Hashes and different Run IDs.
- Nonpositive, non-pitch-aligned, or overflowing reference distances are rejected.
- No rounding, snapping, skipped combination, or hidden Profile mutation occurs.

### Gate 6 — Balance ownership

- Two Runs differing only in Main Core Capacity or support coefficients have equal Physical Scale
  hashes, different Balance hashes, and different Run IDs.

### Gate 7 — Run product and order

- Numeric × Physical × Balance × Distance × Seed cardinality is exact.
- Design Artifact Hash is derived from Distance and is not a separate Cartesian axis.
- Input permutation does not change emitted Run order or IDs.
- Duplicate Run IDs are rejected rather than salted.

### Gate 8 — Independent Run ID golden

- A test builds the canonical Run byte stream independently of the production encoder.
- Every included Run field changes the golden ID when changed alone.
- Paths, Profile IDs, input order, and build commit do not change it.
- The retained expected lowercase BLAKE3 hex is fixed in a golden fixture.

### Gate 9 — Strict Experiment plan

- The typed `ExperimentPlan` resolves to exact `ExperimentRunSpec` values and retained ID
  goldens.
- Every `ExperimentPlanError` class frozen in section 12.1 has direct executable evidence.
- If format-v1 JSON is exposed, it strictly decodes/re-encodes and rejects unknown and duplicate
  fields, trailing data, floats, malformed hashes/Seeds, unsupported format, and unsupported
  algorithm.

### Gate 10 — Module exact-contract compatibility

- Exact Semantics, Numeric, and Physical Scale contracts validate.
- Different Profile IDs with equal semantic hashes validate.
- Each of the three compatibility-axis mismatches is typed and requires explicit migration.
- A different Physical hash is rejected even when the geometry would also be legal in the target.

### Gate 11 — Module absolute geometry retention

- Every decoded raw point, AABB, vertex order, endpoint, routing-domain reference, and I/O binding
  equals the retained fixture exactly.
- Validation success and failure leave source semantic hash and encoded source bytes unchanged.
- No scale, ratio, automatic anchor motion, pitch snap, rotation, or reflection exists in the
  exact-validation path.

### Gate 12 — Module strictness and hash golden

- Retained Module format-v1 JSON strictly decodes and canonically re-encodes.
- An independent encoder proves the Module semantic hash golden.
- Reordering primitive arrays does not change semantic hash or canonical order.
- Changing exact geometry, primitive kind, endpoint, routing domain, or I/O binding changes it.
- Changing excluded provenance, display name, path, or optional Balance metadata does not.
- Unknown/duplicate fields, float geometry, malformed hashes, unsupported version/algorithm,
  duplicate local IDs, unresolved references, invalid geometry, and count overflow are rejected.

### Gate 13 — Same-profile Replay

- A generated Physical variant records its exact hash in `SimulationContract` and Replay header.
- The same semantic Profile content replays with identical per-Tick hashes.
- A different Physical hash fails with the existing typed Replay contract mismatch before Tick
  execution.

### Gate 14 — Noninterference and regressions

- Matrix generation, artifact hashing, and Module validation do not mutate Simulation state.
- The complete Stage 0 test and technical-gate suite still passes.
- `aon-sim` retains its Bevy-free dependency boundary.

### Gate 15 — Windows-native clean-checkout evidence

- formatting, diff whitespace, workspace check, strict Clippy, full workspace tests, S1-M0 exact
  tests, and the Stage 0 regression gate pass using native Windows PowerShell;
- all dependency and test execution uses the pinned `Cargo.lock` and offline mode where the
  existing project gate requires it;
- a committed fresh clean checkout repeats the S1-M0 exact tests and required regression gates;
- the evidence records commit ID, profile/module/Run goldens, command lines, and exit status;
- WSL output is not accepted as evidence for this milestone.

## 15. Required implementation boundaries

The expected ownership is:

```text
crates/aon-sim/src/experiment.rs
    ArtifactHash and ExperimentRunId
    GateGeometryVariant, PhysicalScaleMatrix, and ResolvedPhysicalScaleProfile
    ExperimentPlan, ResolvedExperimentPlan, and ExperimentRunSpec
    LongWireDesign and typed ExperimentPlanError
    Physical variant generation
    Profile-axis validation and Run expansion
    Long-wire Design and ExperimentRunId canonical encoders
    deterministic ordering and typed errors

crates/aon-sim/src/module.rs
    Module format v1
    ModuleContract and AbsoluteModuleGeometry
    strict decode/encode
    exact compatibility and geometry validation
    Module semantic ArtifactHash canonical encoder and typed ModuleError

crates/aon-sim/src/hash.rs
    existing Profile and State hash types; new nominal types may instead remain in their owning
    artifact modules
```

These modules remain Pure Rust Canonical Core code and MUST NOT depend on Bevy, a window, GPU,
wall clock, or host filesystem enumeration. A host may resolve declared artifact paths before
calling the Core; path resolution never enters canonical identity.

Retained artifacts belong under versioned `fixtures/experiments/` and `fixtures/modules/` paths.
Generated Physical profiles belong under `profiles/physical-scale/` only when intentionally
retained as named experiment inputs. Tests MUST NOT overwrite retained goldens implicitly.

## 16. Final S1-M0 invariant

```text
same semantic Scenario and Design
+ same Semantics Version
+ same Numeric Profile
+ same Physical Scale Profile
+ same Balance Profile
+ same Long-wire Distance
+ same Seed
+ same maxTicks and Metric Set

= same ExperimentRunId

and

same Module primitive composition
+ same absolute Fixed geometry
+ same Semantics / Numeric / Physical contract

= same Module semantic ArtifactHash and exact compatibility
```

Conversely, metadata cannot be used to fake semantic uniqueness, and semantic incompatibility
cannot be hidden by metadata or silent geometry transformation.
