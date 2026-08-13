# A/O/N — S1-M4 Canonical Decisions

**Status:** implementation authority
**Applies to:** `S1-M4 — Construction / Contact / Damage`
**Source baseline:** PRD v1.0 GO Candidate / SSS v1.0 Draft / TRD v1.0 Draft /
S1-M3 Canonical Decisions

This document freezes only the representation, arithmetic, phase ownership, artifact policy, and
evidence required to implement S1-M4. It is subordinate to the source documents in their respective
areas of authority:

- the PRD determines the product question, scope, and absolute invariants;
- the SSS determines observable World behavior, Tick phases, numeric laws, and conformance;
- the TRD determines stores, encoders, APIs, stable ordering, and test structure;
- the S1-M3 authority freezes the retained Capacity, sensing, Power, Brownout, support, Scenario v3,
  Replay v2, and State V6 baseline;
- this document resolves only the representation and narrow-content gaps needed by S1-M4.

The reference numbers below are transparent conformance constants, not final balance. S1-M5 owns
the competing reference architectures and S1-M6 owns the parameter sweep. Neither complete Stage 1
gate closes in this milestone.

## 0. Source authority, conflicts, and resolutions

| Subject | Authority |
|---|---|
| Construction is delayed and length-sensitive | PRD §42; SSS §19; TRD §21 |
| Contact uses actual Wire Body and conserves granted Energy | SSS §20; TRD §21 |
| Thermal state, damage, destruction, and run end | SSS §§22–23; TRD §22 |
| Phase ownership | SSS §7; TRD §15 |
| C-09 and C-10 | SSS §28; TRD conformance table |
| S1-M4 implementation boundary | TRD §33.5; tracker Stage 1 |
| Retained Power/Work/Heat seams | S1-M2 §§8–10; S1-M3 §7 |

The end-state SSS describes cargo, transfer, reconstruction, full thermal exchange, several kinds of
damageable collider, and four Enemy pressures. The milestone table assigns only Construction Site,
one deterministic Enemy, Live Wire, Contact, Damage, and Main Core run end to S1-M4; Payload,
transfer, full reconstruction, Radiation, and the four-enemy pressure set belong to later milestones.
S1-M4 therefore uses these minimal canonical decisions:

1. Construction is Work-only. Construction cargo arrays are reserved empty. Inventory,
   `LOAD`/`UNLOAD`, required A/O/N cargo, and transfer remain MVP-M0.
2. `PlaceConstructionSite` creates Gate, Wire, Junction, or Fixed Substrate sites only. Direct
   `PlaceMobileSubstrate` remains the bootstrap builder path. Module flattening and automatic
   reconstruction-site creation remain MVP-M1.
3. One uniform canonical Enemy moves by an exact Scenario-owned velocity. No Enemy pathfinding,
   waves, implicit randomness, or content archetype is added.
4. Contact candidates are canonical Enemies only. Replay v2 `HostileFrame` remains a noncanonical
   sensing input and can never cause contact, damage, or destruction.
5. S1-M4 integrates locally produced Heat into the owning canonical object, but thermal edges,
   ambient exchange, and Heat-to-delay/drive/leakage modifiers remain later thermal breadth. Thermal
   damage itself is implemented from the Phase-1 temperature snapshot.

These choices preserve every required Stage 1 observation without fake cargo, reconstruction,
physics, or Enemy results. No unresolved contradiction remains inside this milestone boundary.

## 1. Exact ownership and exclusions

S1-M4 owns exactly:

- strict Balance schema v5 with Construction and Contact/Damage sections;
- strict Scenario schema v4 with canonical Enemy initial state;
- global State V7 and a new `aon-main-core-power-enemy-v1` generator;
- append-only `PlaceConstructionSite` Command tag 8 under Command encoder v1 and Replay v2;
- Work-only Construction Sites for Gate, Wire, Junction, and Fixed Substrate targets;
- canonical geometry reservation, Work-derived Power demand, retained `scale_work` Brownout grant,
  Phase-11 progress, and next-Phase-0 activation;
- one uniform canonical Enemy with deterministic straight-line trajectory;
- Live Wire Power demand, exact Wire-body contact, conservative Energy allocation, and Wire Heat;
- canonical Heat accumulation for currently represented damageable objects;
- Electrical/Thermal damage, simultaneous pending destruction, next-Phase-0 removal, C-09 path
  invalidation, and terminal Main Core run end;
- exact C-09/C-10 retained Scenario/Replay evidence and host equivalence.

S1-M4 does not implement:

- Relay, Relay reconstruction, C-23, or C-24;
- payload, inventory, cargo delivery, `LOAD`, `UNLOAD`, or Transfer Work;
- Module placement/flattening, automatic Gate/Wire Reconstruction Sites, or repair behavior;
- Radiation, Quartz, extraction, deposits, or four Enemy pressure types;
- faction immunity beyond the absence of non-Enemy contact candidates;
- arbitrary external damageable colliders or HostileFrame contact;
- Enemy pathfinding, collision avoidance, target selection beyond the frozen smallest-ID rule,
  spawning, waves, or random draws;
- thermal edges, ambient cooling, object-to-object heat transfer, thermal cells, or Heat-derived
  timing/drive/leakage changes;
- S1-M5 reference architectures, S1-M6 sweep, or either complete Stage 1 gate.

Passing S1-M4 closes C-09 and C-10 only. It retains C-07, C-08, C-21, and C-22 and does not
fake-pass the Stage 1 technical or product gate.

## 2. Frozen versions and compatibility

| Contract or artifact | S1-M4 value |
|---|---|
| Semantics Version | `aon-semantics-v1` retained |
| Numeric Profile schema | `1` retained |
| Physical Scale Profile schema | `1` retained |
| Balance Profile schema | v2/v3/v4 retained; v5 current |
| Profile canonical encoder | `1` retained |
| Scenario schema | v1/v2/v3 retained; v4 current |
| Replay format | v1 decode retained; v2 current and retained |
| Canonical State Hash | global `aon-state-v7` |
| Command canonical encoder | `1` retained; tag 8 appended |
| World generator | `aon-main-core-power-enemy-v1` current |

The formulae, phase order, division rules, and deterministic tie-breaks already exist in Semantics
v1. Implementing them does not create Semantics v2. Numeric and Physical Scale also retain their
exact laws and hashes.

Replay format remains v2 because its envelope and World-input ontology do not change. Replay v2's
strict tagged Command body learns the append-only Command variant that its canonical Command enum
owns; no new top-level field, WorldInput type, checkpoint form, or outcome field is introduced.
Replay v1 rejects `PlaceConstructionSite` as `ReplayError::UnsupportedCommandForFormat` before
header/session execution; it remains decode-only/currently non-executable under V7 as already
governed by header validation. Retained v2 Replays are regenerated with V7 headers/checkpoints but
preserve their authoritative Scenario packages, Command streams, and HostileFrames.

Legacy direct `PlaceGate`, `PlaceWire`, `PlaceJunction`, and `PlaceFixedSubstrate` Command meaning is
not changed or feature-gated. They remain valid bootstrap/world-edit primitives so old Command logs
are not silently reinterpreted and a builder Track can be created. Product Stage-1 run UI and the
S1-M4 measured-construction fixture use `PlaceConstructionSite` for measured expansion; that UI
policy is not a new canonical rejection path.

## 3. Strict Balance schema v5

### 3.1 Exact public profile shape

```rust
pub const BALANCE_SCHEMA_VERSION_V5: u32 = 5;

pub struct ConstructionProbeProfile {
    pub and_gate_work: u64,
    pub or_gate_work: u64,
    pub not_gate_work: u64,
    pub junction_base_work: u64,
    pub wire_endpoint_work: u64,
    pub wire_work_per_ncu: Rational,
    pub substrate_work_per_square_wu: Rational,
    pub construction_power_per_work: Rational,
    pub builder_work_per_tick: u64,
    pub construction_heat_fraction: Rational,
}

pub struct PrimitiveIntegrityProfile {
    pub main_core: u64,
    pub wire: u64,
    pub gate: u64,
    pub junction: u64,
    pub fixed_substrate: u64,
    pub mobile_substrate: u64,
    pub enemy: u64,
}

pub struct PrimitiveThermalCapacityProfile {
    pub main_core: u64,
    pub wire: u64,
    pub gate: u64,
    pub junction: u64,
    pub fixed_substrate: u64,
    pub mobile_substrate: u64,
    pub enemy: u64,
}

pub struct ElectricalToleranceProfile {
    pub main_core: u64,
    pub wire: u64,
    pub gate: u64,
    pub junction: u64,
    pub fixed_substrate: u64,
    pub mobile_substrate: u64,
    pub enemy: u64,
}

pub struct ContactDamageProbeProfile {
    pub live_energy_per_strength_wu: Rational,
    pub world_leak_weight: u64,
    pub enemy_conductivity: u64,
    pub initial_integrity: PrimitiveIntegrityProfile,
    pub thermal_capacity: PrimitiveThermalCapacityProfile,
    pub electrical_tolerance: ElectricalToleranceProfile,
    pub safe_temperature: Fixed,
    pub thermal_damage_rate: Rational,
    pub enemy_attack_energy_per_tick: u64,
    pub gate_power_heat_fraction: Rational,
    pub movement_heat_fraction: Rational,
}

pub struct BalanceProfile {
    // retained fields and v2-v4 probes
    pub construction_probe: Option<ConstructionProbeProfile>,
    pub contact_damage_probe: Option<ContactDamageProbeProfile>,
}

BalanceProfile::construction_contact_damage_alpha(profile_id)
```

The strict JSON names are their camelCase forms under `constructionProbe` and
`contactDamageProbe`. Nested kind fields are exactly `mainCore`, `wire`, `gate`, `junction`,
`fixedSubstrate`, `mobileSubstrate`, and `enemy`.

The schema matrix is exact:

| Balance schema | Power/support sections | Construction section | Contact/damage section |
|---|---:|---:|---:|
| v2 | retained v2 rules | forbidden | forbidden |
| v3 | retained v3 rules | forbidden | forbidden |
| v4 | retained v4 rules | forbidden | forbidden |
| v5 | Capacity + Power + support required | required | required |

All Work fields, `builderWorkPerTick`, all initial Integrity values, all thermal capacities, all
electrical tolerances, `worldLeakWeight`, `enemyConductivity`, and `enemyAttackEnergyPerTick` are
strictly positive. `wireWorkPerNCU`, `substrateWorkPerSquareWU`,
`constructionPowerPerWork`, `liveEnergyPerStrengthWU`, and `thermalDamageRate` are positive reduced
Rationals. `constructionHeatFraction`, `gatePowerHeatFraction`, and `movementHeatFraction` are
reduced Rationals in `(0,1]`. `safeTemperature.raw >= 0`. Unknown, duplicate, float,
zero-denominator, forbidden, missing, sign, and relation faults fail closed.

### 3.2 Reference v5 values

```text
Construction
AND work                              8 Energy
OR work                               8 Energy
NOT work                              6 Energy
Junction base work                    4 Energy
Wire endpoint work                    2 Energy
Wire work per whole NCU/WU          1/1 Energy
Substrate work per square WU        1/1 Energy
Construction Power per Work         1/1 Energy
Builder requested Work per Tick       8 Energy
Construction Heat fraction          1/4

Contact and Damage
Live Energy per strength-WU        1/400
World leak weight                     2
Enemy conductivity                    1
Initial Integrity:
  Main Core                          100
  Wire/Gate/Junction/Enemy            10 each
  Fixed/Mobile Substrate              20 each
Thermal Capacity                      10 for every supported kind
Electrical Tolerance                   1 for every supported kind
Safe Temperature                  65,536 raw Fixed = 1
Thermal Damage Rate                 1/1
Enemy attack Energy per Tick          10
Gate Power Heat fraction             1/4
Movement Heat fraction               1/4
```

Scenario values must use the matching v5 initial-Integrity values for Main Core and Enemies.
Newly activated primitives use the matching v5 values and zero Heat. A direct-package mismatch is
`SimulationError::InitialIntegrityProfileMismatch`; a strict Scenario mismatch is
`PackageError::InitialIntegrityProfileMismatch { field }` after Profile decoding.

Enemy radius and velocity are Scenario content, not Balance coefficients. The generator consumes no
random draw and creates no implicit speed or direction.

### 3.3 Exact v5 canonical suffix

Profile encoder version remains 1. Balance v5 writes the exact retained v4 stream, then:

```text
andGateWork, orGateWork, notGateWork                    u64 LE
junctionBaseWork, wireEndpointWork                      u64 LE
wireWorkPerNCU                                          normalized Rational i64/i64 LE
substrateWorkPerSquareWU                                normalized Rational i64/i64 LE
constructionPowerPerWork                               normalized Rational i64/i64 LE
builderWorkPerTick                                      u64 LE
constructionHeatFraction                                normalized Rational i64/i64 LE
liveEnergyPerStrengthWU                                 normalized Rational i64/i64 LE
worldLeakWeight, enemyConductivity                      u64 LE
initialIntegrity                                        seven u64 in listed kind order
thermalCapacity                                         seven u64 in listed kind order
electricalTolerance                                     seven u64 in listed kind order
safeTemperature.raw                                     i64 LE
thermalDamageRate                                       normalized Rational i64/i64 LE
enemyAttackEnergyPerTick                                u64 LE
gatePowerHeatFraction                                   normalized Rational i64/i64 LE
movementHeatFraction                                    normalized Rational i64/i64 LE
```

No v2/v3/v4 canonical byte or semantic hash changes. The v5 fixture's semantic and file hashes are
frozen by the artifact generator and closure record, never handwritten.

## 4. Scenario v4, canonical Enemy, and generator

### 4.1 Scenario shape and features

Scenario v4 accepts exactly:

```json
"initialWorld": {
  "kind": "main-core-power-enemy-v1",
  "mainCore": { "position": { "x": 0, "y": 0 }, "integrity": 100,
                "heatEnergy": 0 },
  "powerSources": [],
  "enemies": [
    { "position": { "x": 0, "y": 0 },
      "velocityPerTick": { "x": 0, "y": 0 },
      "radius": 65536, "integrity": 10, "heatEnergy": 0 }
  ]
}
```

`StageFeatureSet` appends these strict booleans after retained `radiation` in semantic/hash order:

```rust
pub construction: bool,
pub contact: bool,
pub damage: bool,
```

Scenario v4 requires retained `signal`, `mobility`, `capacity`, `sensing`, and `power`, plus all
three new features. It requires Balance v5 and `main-core-power-enemy-v1`. v1/v2/v3 forbid the new
feature fields by their selected strict wire types and retain their exact semantic hashes.

### 4.2 Enemy canonicalization and identity

```rust
pub struct EnemyInitialState {
    pub position: FixedVec2,
    pub velocity_per_tick: FixedVec2,
    pub radius: Fixed,
    pub integrity: Integrity,
    pub heat_energy: HeatEnergy,
}

pub struct EnemyState {
    id: EnemyId,
    position: FixedVec2,
    velocity_per_tick: FixedVec2,
    radius: Fixed,
    integrity: Integrity,
    heat_energy: HeatEnergy,
}

pub struct EnemyStore { /* EntityId-keyed canonical slots */ }
```

Radius is positive and aligned to `wireGeometryQuantum`; position, both velocity components, and the
checked next position are quantum-aligned. Integrity is positive and exactly v5 `enemy` initial
Integrity. A v4 world contains at least one Enemy. The complete semantic sort key is:

```text
(position.x, position.y, velocity.x, velocity.y, radius, integrity, heatEnergy)
```

Scenario decode normalizes by this key and rejects an exact duplicate. The generator allocates:

1. Main Core;
2. Power Sources in their retained semantic order;
3. Enemies in the normalized Enemy order.

The resulting IDs and allocator frontier are canonical. Enemy position/velocity/radius/integrity/
Heat are all State V7 truth. A stationary Enemy (`velocity = 0,0`) is valid.

Scenario v4 semantic hashing uses domain `AON\0SCENARIO\0V4\0` and encoder version 4. It writes the
retained header text fields, InitialWorld tag `3`, Main Core fields, sorted Source count/records,
sorted Enemy count/records in the complete key-field order above, the retained eight feature booleans
followed by `construction`, `contact`, `damage`, and then Numeric/Physical/Balance semantic hashes.
All integers use the existing fixed little-endian widths. Paths and display Profile IDs remain
excluded. No v1/v2/v3 Scenario byte stream or semantic hash changes.

### 4.3 Deterministic trajectory and attack

Phase 1 snapshots Enemy fields. Phase 7 stages one exact line segment from `position` to
`position + velocityPerTick` with checked Fixed arithmetic. It does not bounce, steer, accelerate,
collide with other Enemies, or read a random value. Phase 11 commits the staged endpoint for an
Enemy not removed at that Tick; pending-destroyed Enemies still commit because they complete the
current Tick.

Each Enemy has one Phase-3 attack intent for `enemyAttackEnergyPerTick`. Phase 8 considers Core and
active Wire bodies swept/intersected by the trajectory. If more than one target intersects, the
smallest target EntityId receives the complete attack Energy. If none intersects, the Energy is not
an exposure and creates no Heat. Enemy attack is direct Electrical exposure, not a new Damage type
or Power load.

## 5. Command v1 and Construction Site contract

### 5.1 Append-only Command

```rust
pub enum ConstructionTarget {
    Gate { gate_type: GateType, origin: FixedVec2, routing_domain: RoutingDomain },
    Wire { routing_domain: RoutingDomain, points: Vec<FixedVec2>,
           endpoint_a: EndpointTarget, endpoint_b: EndpointTarget },
    Junction { routing_domain: RoutingDomain, position: FixedVec2 },
    FixedSubstrate { origin: FixedVec2, routing_area: FixedAabb, footprint: FixedAabb },
}

pub struct PlaceConstructionSiteCommand {
    pub target: ConstructionTarget,
}

Command::PlaceConstructionSite(PlaceConstructionSiteCommand) // canonical tag 8
```

The target payload reuses the exact existing target Command encodings after a target-kind tag:
Gate `0`, Wire `1`, Junction `2`, Fixed Substrate `3`. Mobile Substrate is not a Construction target
in S1-M4. Command domain and encoder version remain exact v1; existing tags 0..7 do not move.

Validation runs after retained envelope/tick/ordinal/schema rules and applies the corresponding
existing primitive validation without allocating the target primitive. It additionally reserves
the target's exact geometry against active primitives and every live Construction Site. Any overlap,
spacing, domain, endpoint, routing-pitch, substrate-bound, or duplicate reservation fault produces
the corresponding retained `CommandRejectionReason`; malformed construction target shape is
`InvalidGeometryShape`; unsupported target is `UnsupportedPlacement`. Rejection consumes no Entity
ID and creates no partial Site.

All retained direct placement Commands perform the reciprocal reservation check; a direct primitive
cannot bypass a live Site. `RemoveEntity` may cancel a Construction Site, tombstoning only the Site
ID. Removing an active entity used as a Site routing domain or endpoint is rejected with the appended
`CommandRejectionReason::ConstructionDependencyInUse`. Pending destruction cannot be rejected: at
the next Phase 0 it cancels every transitively dependent Site in Site-ID order before activation,
with no Work refund and no target allocation. Those cancellations are reported as
`DestructionKind::ConstructionDependencyLost` and never create Reconstruction Sites.

### 5.2 Store and public API

```rust
pub struct ConstructionSiteId(pub EntityId);

pub struct ConstructionSite {
    pub id: ConstructionSiteId,
    pub target: ConstructionTarget,
    pub required_work: Energy,
    pub completed_work: Energy,
    pub activation_ready: bool,
}

pub struct ConstructionSiteStore { /* canonical slots; EntityId order */ }

pub fn required_construction_work(
    target: &ConstructionTarget,
    probe: &ConstructionProbeProfile,
) -> Result<Energy, ConstructionError>;

pub fn construction_nominal_demand(
    site: ConstructionSiteId,
    builder: MobileId,
    attachment: PowerNodeKey,
    probe: &ConstructionProbeProfile,
) -> Result<NominalPowerDemand, ConstructionError>;

pub fn grant_construction_work(
    nominal: Energy,
    ratio: PowerRatio,
) -> Result<Energy, ConstructionError>;

pub fn apply_construction_work(
    sites: &mut ConstructionSiteStore,
    contributions: &[ConstructionWorkContribution],
) -> Result<Vec<ConstructionProgressResult>, ConstructionError>;
```

`grant_construction_work` must delegate to the retained public `scale_work`; it may only translate
the typed error. It must not implement a second rounding path.

Required Work is:

```text
Gate       = table value
Junction   = junctionBaseWork
Wire       = wireEndpointWork
             + ceil(wireWorkPerNCU × canonicalRawLength / FIXED_ONE)
Substrate  = ceil(substrateWorkPerSquareWU
                  × rawWidth × rawHeight / FIXED_ONE²)
```

Each Rational expression uses checked/cancelled `u128` arithmetic and exactly one final
`ceil_div_nonnegative`. The Wire addition is checked. Width/height are exact positive AABB extents.
No per-segment rounding occurs; Wire length uses the existing maximal-collinear-run law. Required
Work is nondecreasing with canonical raw length. The one final ceiling means sub-boundary length
increases may retain the same Work, while every crossed ceiling boundary strictly increases it.
The frozen reference pair is `1 WU -> 3` and `1 WU + 1 raw unit -> 4`.

### 5.3 Builder selection and Power identity

`MobilePort` appends `Build = 3`; `MobileControlPorts` appends
`build: Option<SinkId>` after retained STOP/LEFT/RIGHT. Balance v5 construction-enabled placement
allocates the BUILD external Driver and Sink after all retained Mobile identities and stores
`Some(buildSink)`. Pre-v5 placement allocates no extra identity and stores `None`. Binding BUILD on
such a retained Mobile is `InvalidPort`. State V7 encodes the presence tag and optional Sink ID. A
v4 Scenario Mobile exposes BUILD in snapshots, rendering, Replay, and State V7.

At Phase 3, a Mobile whose resolved BUILD is HIGH selects the smallest `ConstructionSiteId` whose
reserved geometry intersects its Phase-1 footprint. LOW makes no intent; X is fail-safe STOP for
Construction and makes no intent. One Mobile selects at most one Site; any number of Mobiles may
select the same Site.

The interaction test is inclusive and exact. It uses the retained Phase-1 world Mobile footprint.
Gate sites use the target Gate footprint; Junction sites use the closed target point; Wire sites use
the union of target straight-segment capsules with `wireBodyRadius`; Fixed Substrate sites use the
translated world footprint. A Mobile footprint touching any of those closed shapes counts. A Wire
test uses the same checked integer segment/AABB distance kernel as Contact, without sensing radius,
float, or sample points.

Phase 4 creates one load for each selected builder:

```text
DemandId(owner = Mobile EntityId, kind = Construction tag 12)
Power attachment = builder's Phase-1 Track WireOffset
requested Work = min(builderWorkPerTick, remaining Site Work)
nominal Power = ceil(constructionPowerPerWork × requested Work)
```

The `DemandId` is per Mobile, not per Site, so multiple builders cannot collide. Nominal conversion
uses exact Rational arithmetic and one final ceiling. The load enters the complete Phase-4 set before
solve and gets the common region ratio without priority. Phase 6 obtains granted Work through
`scale_work(requestedWork, rho)`. Phase 8 publishes Work contributions and derives builder-owned
Construction Heat from actual granted Construction Power using `constructionHeatFraction` and
nearest-ties-even. Phase 11 sorts by `(site,builder)`. For each Site it starts with remaining Work
and gives each builder `applied=min(granted,remaining)` in ascending builder ID order, then subtracts
that amount. It sets `activation_ready` when exact progress reaches the requirement. Excess
same-Tick grant is reported as unapplied and is not carried forward; its consumed Power was already
eligible for Construction Heat.

### 5.4 Phase-0 activation

At the next Tick Phase 0, ready Sites activate in ascending Site EntityId before player Commands:

1. validate the still-reserved exact target against canonical invariants;
2. tombstone the Site registry slot without reusing its ID;
3. allocate a fresh EntityId for the active target;
4. create the corresponding active primitive with v5 initial Integrity and zero Heat;
5. update connection generations/topology revision and rebuild topologies once after the batch.

The active target never inherits the Site ID. A Wire has no Signal, Power, Sense, Track, or Capacity
usage before activation. From this Phase-0 activation Tick onward its entire canonical length enters
all four surfaces and Capacity accounting. Capacity shortage never rejects, delays, or removes it.
An impossible stale reservation is `SimulationError::InvalidCanonicalState` and aborts the Tick
atomically rather than partially activating the batch.

## 6. Live Wire demand and exact contact

### 6.1 Public kernels

```rust
pub struct LiveWireInput {
    pub wire: WireId,
    pub length: Fixed,
    pub high_drive_strength: u128,
}

pub struct ContactCandidate {
    pub target: EnemyId,
    pub weight: u128,
}

pub struct ContactAllocation {
    pub target: EnemyId,
    pub weight: u128,
    pub absorbed: Energy,
}

pub fn calculate_live_wire_demand(
    input: LiveWireInput,
    probe: &ContactDamageProbeProfile,
) -> Result<Energy, ContactError>;

pub fn allocate_contact_energy(
    granted_live_energy: Energy,
    candidates: &[ContactCandidate],
    world_leak_weight: u64,
) -> Result<(Vec<ContactAllocation>, HeatEnergy), ContactError>;
```

Only a Wire whose resolved signal is HIGH and whose checked aggregate active HIGH Drive Strength is
positive submits
Live Wire demand. LOW, X, or zero Strength submits none. Demand is independent of whether a contact
exists, so an armed Wire still pays while no Enemy touches it.

```text
liveDemand = ceil(
    liveEnergyPerStrengthWU
    × highDriveStrength
    × rawWireLength
    / FIXED_ONE
)
```

The exact reduced Rational, Strength, and length are combined with checked/cancelled `u128`
arithmetic and one final ceiling. A positive input cannot round to zero. The positive result is
`DemandKind::LiveWire` tag 5 at the owner `WireBody`; it participates in the same route/loss/common
ratio solve as every load.

### 6.2 Narrow phase and contact weight

The Phase-7 Enemy trajectory is a swept circle. An actual Wire Body is each canonical straight
segment thickened by `wireBodyRadius`. Contact exists when the swept Enemy center segment intersects
the Wire capsule expanded by Enemy radius. The implementation uses checked integer orientation,
projection, squared-distance cross multiplication, and inclusive boundary comparison. It does not
use float, runtime square root, a sensing capsule, spatial-index iteration order, or sampling.

Each `(WireId, EnemyId)` pair contributes at most once per Tick even if multiple straight segments
touch. For this one-Enemy-type milestone:

```text
contactDuration = 1
contactMeasure  = 1
conductivity    = enemyConductivity
weight          = conductivity
```

Candidates are de-duplicated and sorted by Enemy EntityId. Zero conductivity is invalid at Profile
load rather than a zero-weight runtime candidate.

### 6.3 Conservative allocation and remainder

Let `G` be the actual granted Live Wire Energy, `L` the positive `worldLeakWeight`, `W` the checked
sum of positive target weights, and `D=L+W`.

```text
targetBudget = floor(G × W / D)
base_i       = floor(G × weight_i / D)
remainder    = targetBudget - Σ base_i
```

Give one Energy unit to candidates in ascending Enemy EntityId until `remainder` is exhausted.
Because the bases share one denominator, `remainder < candidateCount`. Then:

```text
absorbed_i = base_i + possible remainder unit
wireHeat   = G - Σ absorbed_i
```

This freezes the ambiguous prose rule: target remainder may distribute only the target-side integer
budget; the world-leak share never leaks into targets. Empty candidates return no rows and all `G`
as Wire Heat. Zero `G` returns no rows and zero Heat. Duplicate targets, zero weights, accumulator
overflow, or impossible remainder return typed errors without partial output.

The exact C-10 conformance row is:

```text
G = 20, L = 2, target weights = 1 and 1
targetBudget = 10
absorbed = 5 and 5
Wire Heat = 10
```

An independent odd-budget case must give the one-unit target remainder to the smaller Enemy ID.

## 7. Heat, damage, destruction, and run end

### 7.1 Canonical thermal/integrity fields

S1-M4 adds this optional component to structural records:

```rust
pub struct DamageState {
    pub integrity: Integrity,
    pub heat_energy: HeatEnergy,
}
```

Balance-v5 Gate, Wire, Junction, Fixed Substrate, and Mobile Substrate creation stores
`Some(DamageState)` with the kind's initial Integrity and zero Heat. Pre-v5 creation stores `None`,
so retained sessions gain no fake sentinel Integrity, no Heat state, no new signal identities, and no
damage behavior. Enemy records always carry their Scenario-owned Integrity/Heat because they exist
only in v4/v5 worlds. Main Core retains both always-present existing fields, but only v5 enables its
thermal/damage runtime. Power Sources remain immutable generator infrastructure in S1-M4 and are not
damage targets.

```text
TemperatureRaw = floor(HeatEnergy × FIXED_ONE / ThermalCapacity)
```

Phase 1 snapshots Integrity, Heat, and this derived temperature with checked `u128` multiplication.
Thermal capacity is selected by object kind from Balance v5.

### 7.2 Heat integration boundary

The new public interaction-Heat report is separate from the retained Wire-only
`PowerHeatReport`, whose tags and API do not move:

```rust
#[repr(u8)]
pub enum InteractionHeatKind {
    GatePowerDissipation = 0,
    Movement = 1,
    Construction = 2,
    LiveWireRemainder = 3,
    CancelledGateSwitch = 4,
}

pub struct InteractionHeatReport {
    pub owner: EntityId,
    pub kind: InteractionHeatKind,
    pub demand: Option<DemandId>,
    pub energy: HeatEnergy,
}
```

Rows sort by `(owner,kind,demand)`. Demand-derived rows store `Some(exactDemandId)`;
`CancelledGateSwitch` stores `None`. Duplicate complete keys are reduced with checked addition
before publication.

Phase 8 reduces positive Heat contributions by `(owner, kind, source/demand)` and Phase 9 applies
their checked sums to present canonical `DamageState.heat_energy` or the retained Main Core Heat.
Pre-v5 sessions retain report-only Power Heat and Phase 9 no-op behavior. Active v5 consumes:

- retained leakage, transmission-loss, and overcapacity-support Heat on Wire owners;
- Live Wire unused/no-contact Energy and Contact remainder on the Live Wire;
- retained cancelled Gate switching Heat exactly once on its Gate owner;
- nearest-even `gatePowerHeatFraction` of actual granted GateIdle, GateDrive, and GateSwitch Power
  on the Gate owner;
- nearest-even `movementHeatFraction` of actual granted Movement Power on the Mobile owner;
- nearest-even `constructionHeatFraction` of actual granted Construction Power on the builder.

`cancelled_switching_heat` remains canonical staging until Phase 9 consumes it; consumption resets
the staging value to zero in the same candidate Tick. There is no ambient edge, pairwise transfer,
cooling, negative Heat, or implicit loss. A contribution names an existing Phase-1 object with a
present damage state (or the Main Core), or the Tick fails atomically. Each fraction is applied per
grant with checked multiplication and `round_div_nearest_even`; zero rounded Heat is omitted. Newly
generated Phase-8 Heat is in canonical state after Phase 9 but cannot affect the current Tick's
Phase-1 temperature or Phase-10 thermal damage.

### 7.3 Damage kernels

```rust
#[repr(u8)]
pub enum ThermalObjectKind {
    MainCore = 0,
    Wire = 1,
    Gate = 2,
    Junction = 3,
    FixedSubstrate = 4,
    MobileSubstrate = 5,
    Enemy = 6,
}

#[repr(u8)]
pub enum DamageKind { Electrical = 0, Thermal = 1 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeatContributionKey {
    pub kind: InteractionHeatKind,
    pub source: EntityId,
    pub demand: Option<DemandId>,
}

pub struct HeatContributionInput {
    pub key: HeatContributionKey,
    pub energy: HeatEnergy,
}

pub struct DamageSnapshot {
    pub target: EntityId,
    pub kind: ThermalObjectKind,
    pub integrity: Integrity,
    pub phase1_temperature: Fixed,
}

pub struct ElectricalExposure {
    pub target: EntityId,
    pub source: EntityId,
    pub energy: Energy,
}

pub struct DamageResolution {
    pub target: EntityId,
    pub integrity_before: Integrity,
    pub electrical_exposure: Energy,
    pub electrical_damage: Integrity,
    pub thermal_damage: Integrity,
    pub integrity_after: Integrity,
    pub pending_destruction: bool,
}

pub fn integrate_heat(
    owner: EntityId,
    current: HeatEnergy,
    contributions: &[HeatContributionInput],
) -> Result<HeatEnergy, ThermalDamageError>;

pub fn resolve_damage(
    snapshot: DamageSnapshot,
    electrical: &[ElectricalExposure],
    probe: &ContactDamageProbeProfile,
) -> Result<DamageResolution, ThermalDamageError>;

pub const fn thermal_capacity_for(
    kind: ThermalObjectKind,
    profile: &ContactDamageProbeProfile,
) -> u64;

pub const fn electrical_tolerance_for(
    kind: ThermalObjectKind,
    profile: &ContactDamageProbeProfile,
) -> u64;
```

`HeatContributionInput` is already owner-partitioned scratch: the caller selects exactly one
`owner`, filters every retained Power/interaction row for it, converts to the frozen
`HeatContributionKey`, and calls `integrate_heat` once. The slice must be strictly sorted by key and
contain no duplicate key; unsorted input is `NonCanonicalHeatOrder`, duplicate input is
`DuplicateHeatContribution`. `energy` must be positive. The kernel checked-sums the slice onto
`current`; it does not inspect World stores or publish a report. `owner` is carried only in its typed
error payload. `InteractionHeatReport` is a Phase-8 public projection built from the same reduced
scratch for new S1-M4 kinds; it is not fed back into the kernel and is not canonical state. Retained
`PowerHeatReport` rows are converted directly to input keys at Phase 9 without copying them into the
interaction report.

`DamageSnapshot.phase1_temperature` must be nonnegative and is computed before this public kernel as
`floor(phase1Heat × FIXED_ONE / selectedThermalCapacity)`. `resolve_damage` requires the exposure
slice to contain only `snapshot.target`, be strictly sorted by `(target,source)`, and contain no
duplicate pair. A different target is `ExposureTargetMismatch`, unsorted input is
`NonCanonicalExposureOrder`, and duplicate input is `DuplicateExposureSource`.

`ThermalObjectKind` selects both values from the exact nested v5 table with the same field names:

| Kind | `thermalCapacity` field | `electricalTolerance` field |
|---|---|---|
| MainCore | `mainCore` | `mainCore` |
| Wire | `wire` | `wire` |
| Gate | `gate` | `gate` |
| Junction | `junction` | `junction` |
| FixedSubstrate | `fixedSubstrate` | `fixedSubstrate` |
| MobileSubstrate | `mobileSubstrate` | `mobileSubstrate` |
| Enemy | `enemy` | `enemy` |

The caller and public kernel both use the same exhaustive selector helpers
`thermal_capacity_for(kind, profile)` and `electrical_tolerance_for(kind, profile)`; no default,
fallthrough, entity-kind numeric cast, or caller-supplied divisor exists.

Electrical exposures sort and reduce by `(target,source)` before per-target summation. Contact
absorption and Enemy attack are both Electrical delivery. There is no Capacity damage type.

```text
electricalDamage = floor(totalElectricalEnergy / electricalTolerance)
thermalExcessRaw = max(0, phase1TemperatureRaw - safeTemperatureRaw)
thermalDamage    = round_div_nearest_even(
                     thermalDamageRate.numerator × thermalExcessRaw,
                     thermalDamageRate.denominator × FIXED_ONE)
totalDamage      = checked electricalDamage + thermalDamage
integrityAfter   = max(0, integrityBefore - totalDamage)
```

The nearest-even thermal result must fit `u64`. Zero total damage produces a report row only when a
positive exposure existed; it never marks destruction. Unsupported kind, missing/duplicate object,
zero divisor, sign fault, reduction overflow, output overflow, and an exposure to a non-damageable
object are exact typed errors. A runtime arithmetic/invariant fault aborts the complete candidate
Tick and leaves the prior World, hash, Tick, events, identities, and analyzers unchanged.

### 7.4 Simultaneous destruction and Phase-0 removal

Phase 10 computes every target from Phase-1 Integrity and the complete Phase-8 exposure set. It
commits Integrity reductions together and inserts zero-Integrity targets into one unique ascending
`pending_destructions` set, except that a zero-Integrity Main Core stages the terminal intent and is
not inserted into the Phase-0 removal set. A target marked this Tick still completes its trajectory,
attack, Contact, Work, Heat production, and Phase-11 progress/position. Thus mutually lethal
same-Tick actors both complete their actions.

At the next Tick Phase 0, pending destructions apply in ascending EntityId before Construction
activations and Commands:

- Wire: remove Signal/Power/Sense/Track/Capacity body together, advance connection generation,
  remove routes/slots, and tombstone the identity;
- Enemy: tombstone the Enemy record and identity;
- Gate/Junction/Fixed/Mobile: remove through the deterministic damage-cascade path below;
- Main Core: is never Phase-0 removed because its fatal Tick terminates the Run after Phase 11.

Before mutation, Phase 0 computes a deterministic destruction closure from the start-of-phase
World. The dependency edges are exact: a Mobile depends on the Wire/Junction named by its committed
`TrackPosition`; every routed primitive depends on its Fixed/Mobile Substrate routing-domain owner;
a Site depends on each active routing-domain/endpoint entity in its target. A destroyed support
appends its dependents to a fixed point. Newly appended Mobile targets receive
`DestructionKind::TrackSupportLost`, routed primitives receive `SubstrateSupportLost`, and Sites
receive `ConstructionDependencyLost`; an original Phase-10 target keeps the lower canonical
`Damage=0` cause. Cause tags then follow in the listed order `0,1,2,3`.

Removal uses the exact dependency graph's strongly connected components, computed without iteration
order dependence. Condense them to a DAG; repeatedly choose a zero-incoming component (no remaining
dependent), tie-break by that component's smallest EntityId, and remove its members by ascending
EntityId. Thus Sites/hosted primitives/Mobiles are removed before their support owner, while a valid
cyclic support arrangement still has a deterministic result. This damage path bypasses
player-facing `TrackOccupied`/`SubstrateInUse` rejections; a valid damage outcome must not become a
Run error or leave dangling references. Gate/Junction removal retains the existing rule that
surviving attached Wire endpoints become Free. Wire removal likewise frees surviving
WireSensePort references through the retained rule.

S1-M4 does not create a Reconstruction Site. Already emitted Radiation does not exist in this
milestone. Signal arrivals retain their calendar entries; if their due-time Path Certificate names
the removed Wire/generation, Phase 2 discards them through the ordinary stale-certificate path.

### 7.5 Run status

```rust
pub enum RunStatus {
    Running,
    Ended { completed_tick: Tick, cause: RunEndCause },
}

pub enum RunEndCause { MainCoreDestroyed }
```

If Phase 10 reduces Main Core Integrity to zero, Phase 11 still commits every current-Tick result,
increments `next_tick`, stores terminal `RunStatus::Ended` with the completed Tick, validates the
complete canonical World, and computes State V7 including terminal status. `StepReport.run_status`
exposes that same post-commit status and its `state_hash` is the terminal hash.

Every subsequent `step` or `step_with_world_inputs` returns `SimulationError::RunEnded` before
validating commands or World inputs and without cloning/mutating state. Hash, Tick, events,
allocators, reports, and analyzers stay unchanged. Read-only snapshot/analyzer/replay verification
remains available. A Replay whose commands, World inputs, or checkpoints require stepping beyond
the terminal boundary returns `ReplayError::RunEndedBeforeReplayBoundary` with the terminal and
requested next Tick.

## 8. Exact 12-phase ownership

The implemented order is exact:

1. **Phase 0:** apply pending destructions by EntityId; activate ready Construction Sites by SiteId;
   apply Commands by ordinal; rebuild topology once; remove broken sink slots and stage sync.
2. **Phase 1:** snapshot all object Integrity/Heat/temperature and positions; sample retained
   sensing/world inputs. Canonical Enemies and `HostileFrame` circles are both hostile sensing
   colliders and their one-bit Wire occupancy is ORed; `HostileFrame` remains sensing-only and only
   canonical Enemies may enter Contact or Damage.
3. **Phase 2:** retained Driver/Signal due-event processing and certificate validation.
4. **Phase 3:** evaluate Gates, sensing, Mobile movement/BUILD, Live Wire intents, and Enemy attack.
5. **Phase 4:** Capacity/support first; collect every ordinary load, per-Mobile Construction load,
   and positive Live Wire load; freeze the complete nominal set.
6. **Phase 5:** retained region/common-ratio solve; keep all Heat private.
7. **Phase 6:** retained scheduling; call `scale_work` for each Construction request; freeze actual
   granted Live Energy from the Power report.
8. **Phase 7:** stage Mobile and Enemy trajectories from Phase-1 positions.
9. **Phase 8:** determine exact Contacts; allocate granted Live Energy; apply Enemy attack target
   selection; accumulate Electrical exposures, Construction Work, and Heat contributions.
10. **Phase 9:** integrate all owned Heat simultaneously from the Phase-8 contribution set.
11. **Phase 10:** resolve all Electrical and Phase-1-temperature Thermal damage simultaneously;
    stage pending destruction and Core terminal intent.
12. **Phase 11:** commit positions and Construction progress, increment Tick, commit terminal Run
    status when staged, then validate/hash the complete State V7.

No phase reads a current-Tick mutation that the SSS assigns to a later phase. In particular,
current-Tick Contact Heat affects next-Tick temperature/damage, newly completed Work activates next
Tick, and a newly destroyed Wire remains usable until next Phase 0.

## 9. State V7 exact policy and encoder order

### 9.1 Global migration

State Hash version is a global schema identifier. Every new session uses `aon-state-v7`, including
Empty and old Scenario kinds. V3/V4/V5/V6 identifiers remain strictly decodable header values, but
a current Simulation rejects them before Tick 0 and never reinterprets their checkpoints. Retained
Replays are regenerated with V7 headers/checkpoints. Scenario v1/v2/v3 semantic hashes,
Numeric/Physical/Balance v2-v4 hashes, Module/Design hashes, and Experiment Run IDs remain exact.

### 9.2 Encoder layout

V7 uses:

```text
domain                         ASCII AON\0STATE\0V7\0
encoderVersion                 u16 little-endian = 7
```

It then writes the exact V6 logical stream with these explicit extensions at their store locations:

- Main Core retained fields, then `RunStatus` only once at the root tail described below;
- each Gate, Wire, Junction, Fixed Substrate, and Mobile structural record appends
  `damageStatePresent u8`; presence `0` appends nothing, while `1` appends
  `integrity u64, heatEnergy u64` in that order;
- each Mobile signal record appends `buildPortPresent u8` after RIGHT; presence `0` appends
  nothing, while `1` appends BUILD Sink EntityId u64;
- retained signal/event stores follow;
- the former reserved destruction store becomes:
  `count u64`, then ascending pending EntityId u64;
- the reserved Radiation store remains zero/count encoding;
- the reserved Relay store remains zero/count encoding;
- new Enemy store: live count u64, then ascending Enemy ID with
  `id, position.x, position.y, velocity.x, velocity.y, radius.raw, integrity, heatEnergy`;
- new Construction Site store: live count u64, then ascending Site ID with `id`, exact target
  canonical payload, `requiredWork, completedWork u64`, and `activationReady u8`;
- `RunStatus`: tag `0=Running`, `1=Ended`; Ended appends `completedTick u64` and cause tag
  `0=MainCoreDestroyed`;
- retained Path Certificate arena follows last.

Entity kind tags 10 Enemy and 11 ConstructionSite were already reserved and do not move. The
former three-store V6 reservation is not blindly reused: independent encoder tests must prove every
V7 logical offset and every field's sensitivity. Reports, Power regions/grants, Construction
requests, Contacts, exposures, temperatures, and topology/spatial caches are derived and excluded.

## 10. Reports, analyzers, and host boundary

Public report rows are:

```rust
pub struct ConstructionWorkReport {
    pub site: ConstructionSiteId,
    pub builder: MobileId,
    pub requested: Energy,
    pub nominal_power: Energy,
    pub granted_work: Energy,
    pub applied_work: Energy,
    pub completed_work: Energy,
}

pub struct ContactEnergyReport {
    pub wire: WireId,
    pub target: EnemyId,
    pub weight: u128,
    pub absorbed: Energy,
}

pub struct DamageReport {
    pub target: EntityId,
    pub electrical_exposure: Energy,
    pub electrical_damage: Integrity,
    pub thermal_damage: Integrity,
    pub integrity_before: Integrity,
    pub integrity_after: Integrity,
    pub pending_destruction: bool,
}

pub struct DestructionReport {
    pub target: EntityId,
    pub kind: DestructionKind,
}

#[repr(u8)]
pub enum DestructionKind {
    Damage = 0,
    TrackSupportLost = 1,
    SubstrateSupportLost = 2,
    ConstructionDependencyLost = 3,
}
```

`StepReport` appends `construction_work`, `contacts`, `interaction_heat`, `damage`, `destructions`,
and `run_status`. Rows sort respectively by `(site,builder)`, `(wire,target)`,
`(owner,kind,demand)`, `target`, and `target`. A Live Wire with no target still has its ordinary Power
load and Wire Heat row but no Contact row. Report reductions are checked and never serve as
canonical truth.

`Simulation` exposes read-only `construction_sites()`, `enemies()`, and `run_status()`. The Network
analyzer remains derived. A new `construction_contact_damage_analyzer_snapshot()` recomputes Site
progress, Enemy state, current Integrity/Heat/temperature, armed-Wire nominal demand, and current
run status without trajectory/contact prediction and without mutation. Repeated reads cannot change
events, allocator frontiers, revisions, ticks, state hashes, or later reports.

RenderSnapshot includes Enemy and Construction Site records, all represented object
Integrity/Heat, BUILD state, and run status. Bevy presentation, selection, probes, overlays, and
enabled/disabled rendering cannot write canonical state. Headless and Bevy must reproduce identical
per-Tick State V7 hashes and complete reports.

## 11. C-09 and C-10 retained artifacts

The authoritative generator creates one Scenario v4, Balance v5, and Replay v2 fixture set. Exact
semantic/file/State hashes are generated twice independently and frozen in the fixture tests and
closure record.

### 11.1 C-10 exact facts

The retained C-10 Tick has one powered HIGH Wire whose actual Live grant is exactly 20 Energy and
two canonical Enemies with equal one-unit weights intersecting the actual Wire Body.

```text
granted Live Energy             20
world leak weight                2
target weights                 1,1
target budget                   10
lower-ID Enemy absorption        5
higher-ID Enemy absorption       5
Wire Heat                       10
total absorbed                  10 <= 20
```

Both Enemies receive exactly 5 Electrical exposure before tolerance. The report ordering is lower
Enemy ID first. A kernel test separately proves the lower-ID odd remainder.

### 11.2 C-09 exact timeline

The retained C-09 trace schedules a Signal Arrival whose Path Certificate includes the victim Wire,
then supplies exact lethal Enemy attack Energy while that Arrival is in flight:

```text
Tick t Phase 2       Arrival remains scheduled for a later Tick
Tick t Phase 7/8     Enemy trajectory intersects victim Wire; attack exposure = 10
Tick t Phase 10      Wire Integrity 10 -> 0; pending destruction inserted
Tick t Phase 11      Wire completes Tick; event still present; V7 commits
Tick t+1 Phase 0     Wire removed from Signal/Power/Sense/Track/Capacity together
later due Phase 2    Arrival fails Path Certificate and is stale-discarded
```

The fixture asserts topology revision/generation, sink-slot removal, region separation, sensing and
Track loss, exact Capacity decrease, absence of a Reconstruction Site, and no ID reuse. A separate
same-Tick lethal mutual-contact test proves both actors complete before simultaneous pending
destruction.

### 11.3 Construction timing facts

The retained Construction trace includes at least one Gate, Junction, Wire, and Fixed Substrate
Site. It proves exact required Work, partial Brownout Work, multi-builder reduction, Work completion
at Tick `u` Phase 11, no active target or Wire Capacity at `u`, activation with a fresh ID at
`u+1` Phase 0, and full Wire Capacity from that activation Tick. The frozen `1 WU` versus
`1 WU + 1 raw unit` pair proves strict boundary-crossing Work growth, while a redundant-vertex
encoding of the longer Wire proves no per-segment rounding.

## 12. Strict errors, precedence, and atomicity

### 12.1 Artifact/package precedence

The retained fail-closed order extends as follows:

1. JSON syntax/category;
2. outer Scenario schema / Replay format / Profile schema;
3. selected-version strict shape, unknown/duplicate/type/float faults;
4. Scenario ID, Semantics, hash algorithm;
5. Scenario v4 world-kind, Main Core, Source, then Enemy shape/positivity/duplicate invariants;
6. Profile references/hashes in Numeric, Physical, Balance order;
7. Numeric, Physical, then Balance strict validation; inside Balance v5 retained probes precede
   Construction, then Contact/Damage;
8. referenced Profile IDs/hashes;
9. Scenario/Profile coherence, including feature/world/generator and initial Integrity matches;
10. initial canonical State V7 validation.

Unsupported outer schema/format wins over malformed selected-body fields. A v4 Scenario shape fault
cannot be masked by a later Profile or geometry conversion error. A v5 missing section precedes a
coefficient relation inside the missing section only because there is no value to validate.

### 12.2 Public typed errors

New error enums contain stable variants for:

```text
ConstructionError:
  UnsupportedTarget, NonPositiveExtent, NegativeLength, ArithmeticOverflow,
  WorkOutOfRange, DuplicateContribution, UnknownSite, SiteAlreadyReady,
  InvalidConstructionAttachment, Power(PowerError)

ContactError:
  NegativeLength, ZeroDriveWithDemand, NonPositiveCoefficient,
  ZeroWorldLeakWeight, ZeroCandidateWeight, DuplicateTarget,
  ArithmeticOverflow, DemandOutOfRange, InvalidRemainder

ThermalDamageError:
  UnknownTarget, DuplicateTarget, UnsupportedTargetKind, NonPositiveThermalCapacity,
  NonPositiveElectricalTolerance, InvalidThermalCoefficient, ArithmeticOverflow,
  TemperatureOutOfRange, DamageOutOfRange, ExposureToNonDamageable

SimulationError:
  InitialIntegrityProfileMismatch, RunEnded, NumericOverflow, InvalidCanonicalState

ReplayError:
  RunEndedBeforeReplayBoundary { terminal_next_tick, requested_next_tick }
```

Variant payloads name stable typed IDs/fields, never host paths or iteration indexes.

### 12.3 Tick atomicity

`Simulation::step` continues to execute on a cloned candidate. Every Site row, reservation,
activation, Work/demand, Enemy trajectory, Contact pair, Energy allocation, Heat integration,
exposure reduction, damage result, destruction set, and post-commit invariant is validated before
the candidate replaces the current World. Any numeric, topology, geometry, reduction, Profile, or
canonical-state fault aborts the whole Tick. No partial Site, ID allocation, topology revision,
event, Heat, Integrity, progress, position, or terminal status is observable.

Command rejection is not a Run error and remains per-envelope. A malformed direct API input or a
canonical runtime invariant failure is a Tick error. `RunEnded` has absolute precedence over
command/world-input validation on subsequent calls.

## 13. Executable completion gates

### Gate 1 — retained compatibility

- all Stage 0 and S1-M0 through S1-M3 suites/gates pass;
- Semantics v1, Numeric/Physical v1, Balance v2-v4, Scenario v1-v3, Replay v2 envelope, Command v1
  tags 0..7, old generator semantics, Module/Design hashes, and Experiment Run IDs do not migrate;
- only global State headers/checkpoints migrate to V7.

### Gate 2 — strict Balance v5

- v2-v4 forbid and v5 requires both new sections and all retained v5 prerequisites;
- every scalar/nested kind field validates, hashes, round-trips, rejects unknown/duplicate/float,
  and has independent hash sensitivity;
- exact suffix order is proven independently; retained hashes are exact.

### Gate 3 — strict Scenario v4 and generator

- v4 accepts only the new world/feature/profile triad; v1-v3 retain exact rules;
- Enemy order permutation normalizes; complete duplicate, zero/negative radius, quantum, Integrity,
  overflow, and cross-version faults fail with exact precedence;
- identity allocation is Core, sorted Sources, sorted Enemies; generator consumes zero randomness.

### Gate 4 — State V7

- independent Empty, retained MainCorePower, and v4 full-world encoders match fixed goldens;
- every new field/store/tag has single-field sensitivity and precise logical offset tests;
- reports/caches/temperatures/grants remain excluded; retained Replays regenerate to V7 only.

### Gate 5 — Construction arithmetic and reservations

- all four target kinds have exact Work; Wire Work is nondecreasing generally and the frozen
  `1 WU -> 3 < 1 WU + 1 raw unit -> 4` boundary pair is strict; redundant vertices preserve Work;
  substrate area and rational ceil boundaries match an independent `u128` oracle;
- active/Site/Site reservations reject overlaps deterministically without ID consumption;
- malformed geometry and arithmetic boundaries are typed and atomic.

### Gate 6 — Construction Power and phases

- per-Mobile tag-12 loads enter the complete Phase-4 set at the Phase-1 Track offset;
- source-less/partial/full ratios use the common solve and retained `scale_work` exactly;
- multi-builder Work is `(site,builder)` stable, completion is Phase 11, activation next Phase 0,
  active target has fresh ID, and Capacity starts only then;
- legacy direct Commands retain exact behavior and are not feature-gated.

### Gate 7 — Live Wire demand

- only positive HIGH Drive creates tag-5 demand at WireBody;
- exact Strength × length Rational law uses one final ceiling and checked boundaries;
- no-contact Wire still pays; partial/source-less Power gives proportional/zero grant and no Energy
  creation.

### Gate 8 — contact geometry

- swept circle uses actual Wire body radius, inclusive exact integer geometry, and one pair per Tick;
- sensing-only overlap outside the body does not contact; HostileFrame never contacts;
- segment/input/spatial-index permutations cannot change candidates or reports.

### Gate 9 — C-10 conservation

- exact `20,2,1,1 -> 5,5,10 Heat` facts pass;
- odd remainder goes to lower Enemy ID; rows conserve `sum(absorbed)+heat=grant`;
- empty/zero/duplicate/zero-weight/overflow cases fail or return their exact frozen result.

### Gate 10 — Heat and temperature timing

- retained Power Heat and Contact remainder integrate once into the right canonical owner;
- cancelled Gate Heat is consumed/reset exactly once;
- Phase-1 temperature, not new Phase-9 Heat, controls current Thermal damage;
- no thermal edge, ambient cooling, Heat timing/drive/leakage modifier, or thermal-cell fiction exists.

### Gate 11 — damage and simultaneous destruction

- Electrical floor and Thermal nearest-even laws match independent exact oracles;
- all exposures reduce by stable keys and Integrity clamps at zero without saturation elsewhere;
- mutually lethal actors both complete the current Tick and are removed only next Phase 0;
- no Capacity damage type, premature removal, partial mutation, or ID reuse exists.

### Gate 12 — C-09 Wire break

- the retained in-flight arrival timeline matches section 11.2 exactly;
- Wire loses Signal/Power/Sense/Track/Capacity together at Phase 0; the stale arrival is discarded
  only by due-time Path Certificate validation;
- topology/generation/slot/report observations and absence of reconstruction are exact.

### Gate 13 — Main Core run end

- fatal exposure completes the Tick, commits V7 and every current action, then stores Ended;
- all later step APIs return `RunEnded` before input validation without mutation;
- Replay reports a terminal-boundary mismatch precisely; read-only hosts/analyzers still agree.

### Gate 14 — reports, analyzer, and host equivalence

- Construction rows use stable `(site,builder)` order and conservative applied Work; Contact and
  Heat rows conserve exact granted Live Energy; Damage and Destruction rows preserve canonical
  target/key order in the retained and bounded multi-row cases;
- repeated analyzers/rendering/probes do not mutate canonical truth;
- retained S1-M4 Replay produces identical complete reports and per-Tick V7 hashes in Headless and
  Bevy.

### Gate 15 — fuzz/property and negative scope

- bounded independent oracles cover Work, Live demand, contact allocation, Heat, damage, ordering,
  and numeric limits; strict Balance/Scenario/Replay/Command decoders reach retained corpus cases;
- no Relay, cargo/transfer, Module flatten, automatic reconstruction, Radiation, Quartz, extraction,
  four-enemy pressure, physics engine, random movement, full thermal exchange/cooling, or later gate
  result appears.

### Gate 16 — Windows-native clean evidence

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
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\s1-m3-technical-gate.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\s1-m4-technical-gate.ps1
```

The S1-M4 gate uses exact manifest discovery, fails on missing/skipped/renamed/duplicate evidence,
pins all fixture/profile/State/report values, and leaves the verification clone clean.

## 14. Closure boundary

S1-M4 may be marked complete only after all sixteen gates pass on the committed tree and a fresh
Windows-native clone. The closure record must name the implementation commit, exact Balance v5 and
Scenario v4 semantic/file hashes, initial/final State V7 hashes, C-09/C-10 timeline/report values,
Construction Work/activation facts, registered suite/gate counts, both host results, fuzz corpus,
and clean-clone evidence.

That closure advances the tracker only for S1-M4. S1-M5, S1-M6, both complete Stage 1 gates, Stage
2, and MVP remain open.
