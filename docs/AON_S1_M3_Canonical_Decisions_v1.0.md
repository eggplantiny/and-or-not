# A/O/N — S1-M3 Canonical Decisions

**Status:** implementation authority
**Applies to:** `S1-M3 — Capacity Support Load`
**Source baseline:** PRD v1.0 GO Candidate / SSS v1.0 Draft / TRD v1.0 Draft /
S1-M2 Canonical Decisions

This document freezes only the representation, arithmetic, phase ownership, artifact policy, and
evidence needed to implement S1-M3. It is subordinate to the source documents in their respective
areas of authority:

- the PRD determines the product question, product scope, and absolute product invariants;
- the SSS determines observable World behavior, phase timing, numeric laws, and conformance;
- the TRD determines data structures, encoders, APIs, ordering, and test structure;
- the S1-M2 authority freezes the retained sensing, Power, Brownout, Scenario v3, Replay v2, and
  State V6 baseline;
- this document closes only gaps required by S1-M3.

The coefficients below are transparent conformance constants. They are test inputs, not a claim
that product balance has been found. S1-M6 still owns the parameter sweep, and both complete Stage 1
gates remain open after S1-M3.

## 0. Source authority map and resolved gaps

| Subject | Authority |
|---|---|
| Soft Capacity is not a build permission | PRD §§12–14; SSS §§13.6–13.10 |
| Exact excess and support curve | SSS §§13.6–13.7; TRD §§17.7–17.8 |
| Per-Wire distribution and stable remainder | SSS §13.8; TRD §17.9 |
| Granted Support Heat | SSS §13.9; TRD §17.10 |
| Phase 4 accounting and nominal demand | SSS Phase 4; TRD Phase 4 |
| Phase 8 interaction/heat ownership | SSS Phase 8; TRD Phase 8 |
| Analyzer observations | TRD §§17.12 and 28.3–28.4 |
| Exact C-22 behavior | SSS C-22; TRD C-22 |
| S1-M3 milestone boundary | TRD §33.4; implementation tracker Stage 1 |
| Retained Power/report boundary | S1-M2 authority §§8–13 |

Two source-level representation gaps are resolved here:

1. The TRD sketches `calculate_support_demand(CapacityInputs, &BalanceProfile)`. The implemented
   public kernel instead accepts explicit typed values and probe references. Section 5 freezes the
   exact names and signatures. This changes no SSS arithmetic.
2. The SSS says granted support becomes Wire Heat, while S1-M4 owns canonical thermal state and
   integration. S1-M3 therefore emits the exact Phase-8 `PowerHeatReport` contribution but does not
   add or mutate accumulated temperature, Heat state, thermal factors, damage, or destruction.

These are representation and milestone-boundary resolutions, not semantic deviations. No other
known contradiction is left unresolved by this authority.

## 1. Scope and milestone ownership

S1-M3 owns exactly:

- global excess `E = max(0, U - S)` from the already-derived Capacity accounting;
- the exact soft support curve and its checked rational arithmetic;
- proportional per-Wire distribution and ascending-`WireId` integer remainder;
- positive per-Wire `DemandKind::OvercapacitySupport` loads;
- solving those loads with every other nominal Power load in the same complete Phase-4 set;
- Phase-8 report-only Heat derived from actually granted support Energy;
- read-only Capacity and Power analyzer/report observations;
- strict Balance schema v4 and one C-22 Scenario/Replay fixture;
- C-22, monotonicity, numeric-boundary, host-equivalence, and retained-regression evidence.

S1-M3 does not implement:

- a hard Capacity build limit, build rejection, automatic Wire removal, or Capacity damage;
- a direct Capacity multiplier on Signal delay, Gate delay/drive, Sense, Movement, or Work;
- Relay entities, Relay modes, Relay contribution, Relay activation/upkeep, or C-23/C-24;
- Construction Site state, Live Wire demand, Contact allocation, Integrity damage, destruction,
  Main Core run end, C-09, or C-10;
- canonical temperature/Heat accumulation, thermal exchange/cooling, thermal response, or damage;
- a new Scenario schema, Replay format, State Hash encoder, or World generator;
- S1-M5 reference architectures, S1-M6 tuning, or either complete Stage 1 gate.

Passing S1-M3 closes only C-22. It retains C-07, C-08, and C-21 and does not fake-pass C-09,
C-10, the Stage 1 technical gate, or the Stage 1 product gate.

## 2. Frozen versions

| Contract or artifact | S1-M3 value |
|---|---|
| Semantics Version | `aon-semantics-v1` retained |
| Numeric Profile schema | `1` retained |
| Physical Scale Profile schema | `1` retained |
| Balance Profile schema | v2/v3 retained; v4 Capacity Support probe added |
| Profile canonical encoder | `1` retained |
| Scenario schema | v1/v2/v3 retained; current S1-M3 fixture uses v3 |
| Replay format | v1 decode retained; v2 current |
| Canonical State Hash | `aon-state-v6` retained globally |
| Command canonical encoder | `1` retained |
| World generator | `aon-main-core-power-v1` retained |

No new canonical World field is required. `E`, total/per-Wire support demand, Power regions, grants,
and Phase-8 Heat Contributions are reconstructible Tick scratch/report facts. Therefore advancing
State V6, Replay v2, Scenario v3, or the generator would create a false migration.

## 3. Strict Balance schema v4

### 3.1 Public profile shape

The exact new public API is:

```rust
pub const BALANCE_SCHEMA_VERSION_V4: u32 = 4;

pub struct CapacitySupportProbeProfile {
    pub support_power_per_ncu: Rational,
}

pub struct BalanceProfile {
    // retained fields
    pub capacity_probe: Option<CapacityProbeProfile>,
    pub power_probe: Option<PowerProbeProfile>,
    pub capacity_support_probe: Option<CapacitySupportProbeProfile>,
}

BalanceProfile::capacity_support_probe_alpha(profile_id)
```

Its strict JSON field is:

```json
"capacitySupportProbe": {
  "supportPowerPerNCU": { "numerator": 1, "denominator": 1 }
}
```

The schema matrix is exact:

| Balance schema | `powerProbe` | `capacitySupportProbe` | Capacity Support active |
|---|---:|---:|---:|
| v2 | forbidden | forbidden | no |
| v3 | required | forbidden | no |
| v4 | required | required | yes |

Balance v4 additionally requires `capacityProbe`. A missing required section is
`ProfileValidationError::FieldRequiredForSchema`; a v2/v3 support section is
`FieldForbiddenForSchema`. Unknown/duplicate fields remain strict decoder errors.

For v4, validation requires:

```text
capacityProbe.overcapLinearK >= 0
capacityProbe.overcapQuadraticK > 0
capacityProbe.capacityDenominatorFloor > 0
0 < capacityProbe.supportHeatFraction <= 1
capacitySupportProbe.supportPowerPerNCU > 0
```

The stricter positive quadratic rule applies only to v4. Previously valid v2/v3 profiles retain
their exact validation, canonical bytes, semantic hashes, and runtime meaning.

### 3.2 Encoder suffix and conformance profile

The Profile encoder remains version 1. It writes the retained Balance fields, the existing v3
Power-probe suffix for v3/v4, and for v4 appends the normalized `supportPowerPerNCU` Rational.
The schema tag in the existing header selects the stream; no v2/v3 byte is reinterpreted.

The S1-M3 conformance profile is
`profiles/balance/s1-m3-capacity-support-alpha.json` with:

```text
mainCoreCapacity              = 100 NCU
overcapLinearK                = 1/1
overcapQuadraticK             = 2/1
capacityDenominatorFloor      = 1 NCU
supportHeatFraction           = 1/4
supportPowerPerNCU            = 1/1 Energy per whole NCU
wireLeakagePerWU              = 1/1
wireSenseDemandPerWU          = 1/1
powerLossK                    = 0/1
```

`mainCoreCapacity=100` and `supportPowerPerNCU=1/1` are C-22 test constants only. They do not
replace the previous generic alpha value or freeze product balance.

Frozen evidence:

```text
Balance v4 semantic hash
a0a8974aebc87e30d602ffa019340e59c908912c0b36e0e0634e51214afc45ef

Balance v4 fixture SHA-256
73A3B3512D0809808469BE614426867789D17F1BE86C5E9513893A4DE686624B
```

## 4. Units and exact soft support curve

`Capacity(pub u64)` is raw Fixed-NCU:

```text
Capacity(FIXED_ONE) = 1 NCU
FIXED_ONE            = 65_536
```

`U`, `S`, and `E` use that raw unit. `capacityDenominatorFloor` remains a whole-NCU JSON value and
is converted with checked multiplication by `FIXED_ONE`. `supportPowerPerNCU` is Energy per whole
NCU, not per raw Fixed quantum.

The exact law is:

```text
E = max(0, U - S)

D_support = ceil(
    supportPowerPerNCU
    × (
        overcapLinearK × E
        + overcapQuadraticK × E² / max(S, capacityDenominatorFloorRaw)
      )
    / FIXED_ONE
)
```

All coefficients are exact normalized Rationals. The implementation must cancel common factors
where useful, use checked `u128` arithmetic, and apply exactly one final nonnegative ceiling.
It must not round the linear term, quadratic term, denominator conversion, or coefficient product
independently. Floating point, saturation, wrapping, and platform-width arithmetic are forbidden.

`E=0` short-circuits to `Energy(0)`. That result does not require a positive-demand distribution,
but active v4 reports still expose `Some(0)` so zero is distinguished from pre-v4 absence.

The reference arithmetic is exact:

```text
U = 120 NCU
S = 100 NCU
E = 20 NCU

curve = 1 × 20 + 2 × 20² / 100
      = 20 + 8
      = 28

supportPowerPerNCU = 1
D_support          = 28 Energy
```

## 5. Frozen pure-kernel API and errors

The exact public API is:

```rust
pub const fn capacity_excess(
    used: Capacity,
    supported: Capacity,
) -> Capacity;

pub fn calculate_capacity_support_demand(
    used: Capacity,
    supported: Capacity,
    capacity: &CapacityProbeProfile,
    support: &CapacitySupportProbeProfile,
) -> Result<Energy, CapacitySupportError>;

pub fn distribute_capacity_support_demand(
    used: Capacity,
    total_demand: Energy,
    wire_lengths: &[(WireId, Capacity)],
) -> Result<Vec<WireCapacitySupportShare>, CapacitySupportError>;

pub struct WireCapacitySupportShare { /* private fields */ }

WireCapacitySupportShare::wire() -> WireId
WireCapacitySupportShare::length() -> Capacity
WireCapacitySupportShare::demand() -> Energy
```

`CapacitySupportError` has exactly these variants:

```rust
NegativeLinearCoefficient
NonPositiveQuadraticCoefficient
NonPositiveSupportPowerPerNcu
ZeroCapacityDenominatorFloor
CapacityDenominatorFloorOverflow
ArithmeticOverflow
DemandOutOfRange
EmptyWireSet
ZeroWireLength { wire: WireId }
DuplicateWire { wire: WireId }
UsedCapacityMismatch { declared: Capacity, actual: Capacity }
```

Coefficient errors are fail-closed even when callers bypass Profile validation. Floor conversion,
exact-rational arithmetic, final `Energy` conversion, length summation, floor shares, and remainder
increments are all checked. No error may partially mutate the World or publish a partial report.

## 6. Length-proportional distribution

For positive `U`, each alive Wire receives:

```text
floorShare_e = floor(D_support × length_e / U)
```

The input is normalized by ascending `WireId`. Duplicate IDs, zero lengths, arithmetic overflow,
and a length sum different from declared `U` are typed errors. After all floor shares are computed:

```text
remainder = D_support - sum(floorShare_e)
```

One Energy unit is added to each of the first `remainder` rows in ascending `WireId` order.
Because positive Wire lengths sum exactly to `U`, `remainder < wire_count`. The final shares are
sorted by `WireId` and sum exactly to `D_support`.

The exact C-22 distribution is:

```text
lower WireId length 70 NCU: floor(28 × 70 / 120) = 16, plus 1 = 17
higher WireId length 50 NCU: floor(28 × 50 / 120) = 11         = 11
sum                                                     28
```

Wire input permutation cannot alter the result. Segment layout, Signal/Power/Sense role count, and
routing domain cannot cause a second Capacity or support charge. The same per-Wire length rows
measured for Capacity accounting are reused for support distribution.

The empty canonical case is exact: empty rows with `U=0,D=0` return an empty vector. An empty set
with positive accounting/demand is `EmptyWireSet`.

## 7. Phase ownership and Power integration

### 7.1 Phase 4: complete accounting and nominal set

After Phase 0 structural commit, Phase 4 performs this order:

1. measure alive Wire lengths once in ascending `WireId` order;
2. calculate `U`, Main-Core-only `S`, and v4 `E`;
3. calculate total support demand and exact per-Wire shares;
4. collect ordinary Gate/Wire/Movement nominal loads;
5. append every positive support share as one intrinsic load;
6. freeze the complete `NominalPowerDemandSet` before any region is solved.

Each positive share has:

```text
DemandId::new(wire.entity_id(), DemandKind::OvercapacitySupport)
PowerNodeKey::WireBody(wire)
nominal = that Wire's integer support share
```

`DemandKind::OvercapacitySupport` retains its already-frozen tag `6`. It is intrinsic,
non-switchable, and cannot be disabled by a Signal port. A zero share remains a Capacity analyzer
observation but does not create a zero nominal Power load.

The public integration entry point is:

```rust
collect_nominal_power_demands_with_capacity_support(
    probe,
    gates,
    wires,
    movements,
    capacity_support,
)
```

Every support load participates in the same region generation, route, loss, common-ratio solve,
grant, and Brownout competition as every other load. It receives no priority or reserved pool.
An abandoned/source-less Wire can therefore contribute global excess while its own regional
support load receives zero grant.

### 7.2 Phase 5 private heat scratch

`solve_power_step_with_capacity_support_heat` solves the already-complete nominal set. A support
load without the Capacity probe is `PowerRuntimeError::MissingCapacityProbeForSupport`; a support
load not attached to its owner `WireBody` is `InvalidCapacitySupportAttachment`.

The solved `PowerLoadReport` is the grant source of truth. Support Heat is derived from the actual
grant, never from nominal or unmet demand:

```text
supportHeat_e = round_div_nearest_even(
    grantedSupportEnergy_e × supportHeatFraction.numerator,
    supportHeatFraction.denominator
)
```

This uses checked exact arithmetic and nearest-ties-even, per Wire grant. Zero rounded Heat is
omitted. Unmet support demand and source-less nominal demand create no Heat.

### 7.3 Phase 8 publication and thermal boundary

Append-only report kind tags are:

```rust
pub enum PowerHeatKind {
    LeakageDissipation = 0,
    TransmissionLoss = 1,
    OvercapacitySupport = 2,
}
```

Positive support Heat is published only when Phase 8 moves private heat scratch into
`PowerStepReport.heat_contributions`, sorted by `(owner, kind, demand)`. It is report-only and is
excluded from State V6. Phase 9 still does not integrate it into canonical thermal state.

This milestone boundary is deliberate: S1-M3 proves Power-to-Heat production, while S1-M4 must
separately implement accumulated Heat, thermal response, damage, and destruction. A test that
mutates a canonical Wire Heat field would exceed S1-M3 and does not satisfy this gate.

For the exact C-22 full-grant fixture:

```text
Wire share 17 × 1/4 -> nearest-even 4 Heat
Wire share 11 × 1/4 -> nearest-even 3 Heat
total Support Heat                         7 Heat
```

## 8. Reports and analyzers

The existing Capacity API is extended without placeholder observations:

```rust
NetworkAccounting::used() -> Capacity
NetworkAccounting::supported() -> Capacity
NetworkAccounting::excess() -> Option<Capacity>
NetworkAccounting::total_support_demand() -> Option<Energy>

WireCapacityUsage::wire() -> WireId
WireCapacityUsage::length() -> Capacity
WireCapacityUsage::support_demand() -> Option<Energy>
```

Version meaning is exact:

- Balance v2/v3: `excess`, total support demand, and every per-Wire support demand are `None`;
- Balance v4 under capacity: `Some(Capacity(0))`, `Some(Energy(0))`, and per-Wire `Some(0)`;
- Balance v4 over capacity: exact positive `Some` values.

This `Option` boundary preserves S1-M1's rule against exposing not-yet-implemented `E` as a fake
zero. It also distinguishes active v4 zero from pre-v4 absence.

`StepReport.network_accounting` carries the Phase-4 accounting. Positive support grants appear in
the existing `PowerStepReport.loads` as `DemandKind::OvercapacitySupport`; no duplicate grant report
is introduced. Phase-8 support Heat appears in `heat_contributions` with kind tag 2.

`Simulation::network_analyzer_snapshot` recomputes the exact Capacity values and sorted Wire rows
without mutation. `Simulation::power_sense_analyzer_snapshot` recomputes positive support nominal
loads, regions, routes, ratios, and grants without mutation. It does not publish Tick-local Phase-8
Heat Contributions. Repeated reads do not advance allocators, events, revisions, ticks, or hashes.

## 9. Exact C-22 artifact

The authoritative files are:

```text
fixtures/scenarios/s1-m3-c22-capacity-support-v1.json
fixtures/replays/s1-m3-c22-capacity-support-v1.json
profiles/balance/s1-m3-capacity-support-alpha.json
```

The Scenario uses schema v3 and `main-core-power-v1`; the Replay uses format v2 and State V6. No
new world kind or generator is introduced. Two alive, powered Wires have exact whole-NCU lengths
70 and 50, with the 70-NCU Wire owning the lower `WireId`.

The exact full-grant Tick facts are:

```text
U                              120 NCU
S                              100 NCU
E                               20 NCU
total Support Demand            28 Energy
lower-WireId support share      17 Energy
higher-WireId support share     11 Energy
ordinary leakage+sensing       240 Energy
total region nominal demand    268 Energy
Source generation              268 Energy
common ratio                     1
Support Heat                   4 + 3 = 7 Heat
Wire deletion                    none
build rejection                  none
direct Capacity delay/damage     none
```

Frozen artifact evidence:

```text
Scenario semantic hash
bdebfe491a2f3a31dfdcd7c2470cf447415137459de5e4d65095d3d38f0e01a5

initial State V6
47cddc7a4a1a1371d6600953bb7c0acc7c7e5e465741869375026e7efcab9369

final State V6 at nextTick 3
7f687d752df7146141be826dbb74668866494c1a024ec6f157bb3eb264c3445c

Scenario file SHA-256
609CE583577F0E65084A75A27644C1D8D58FD054B966D99E98947081D0BCE992

Replay file SHA-256
8BB79ED60AE5CAFBC46F7A077549773BD9C117738D99982E5354EFA8DA777C9C
```

The retained host tests are named:

```text
retained_c22_is_canonical_headless_and_exact_across_support_power_and_heat
retained_c22_v6_trace_and_reports_match_headless_and_bevy
```

## 10. Error precedence and atomicity

Artifact/package precedence remains fail-closed:

1. JSON syntax/category;
2. outer schema/format version;
3. version-specific strict shape, unknown/duplicate fields, and numeric representation;
4. Numeric, Physical, then Balance Profile validity;
5. declared Profile hashes and Simulation contract;
6. feature/InitialWorld/generator coherence;
7. initial canonical-state validation.

Within Balance validation, outer schema selection precedes version-specific required/forbidden
support fields. A compound malformed v4 artifact must not leak a later arithmetic error before its
earlier schema/profile fault.

Within a Tick, all Wire rows, exact support arithmetic, distributions, nominal loads, topology,
and solve inputs are validated before the candidate World is committed. Any Capacity-support,
Power-runtime, numeric, report-reduction, or invariant error aborts the entire candidate Tick. The
prior canonical World, hash, tick, event calendars, allocator frontiers, and observable analyzers
remain unchanged.

## 11. Executable completion gates

### Gate 1 — retained versions and regressions

- Every Stage 0, S1-M0, S1-M1, and S1-M2 suite/gate passes.
- Semantics v1, Scenario v1/v2/v3, Replay v2, State V6, generator, and retained Profile hashes do
  not migrate.
- Balance v2/v3 runtime reports retain `None` for all S1-M3-only observations.

### Gate 2 — strict Balance v4

- v2/v3 forbid and v4 requires `capacitySupportProbe`.
- v4 requires existing Capacity and Power probes and a positive quadratic coefficient.
- every new field validates, hashes, round-trips, rejects unknowns, and has single-field hash
  sensitivity.
- the frozen semantic and byte hashes in section 3 match exactly.

### Gate 3 — exact excess and curve

- under/equal capacity produce exact zero; overcapacity produces exact `U-S`.
- C-22 produces `E=20,D=28` with one final ceiling.
- rational, fractional-NCU, denominator-floor, cancellation, and ceiling-boundary cases match a
  bounded independent exact `u128` oracle, with overflow boundaries exercised separately.

### Gate 4 — monotonicity and numeric boundaries

- demand is nondecreasing in `U` for every valid frozen/profile-boundary input.
- coefficient/floor zero and sign faults return their exact typed variants.
- every checked `u128`, floor conversion, and final `u64` boundary fails without saturation,
  wrapping, panic, or partial output.

### Gate 5 — distribution and remainder

- 70/50 distributes 28 as 17/11 in ascending `WireId` order.
- row permutation is irrelevant; shares sum exactly to total demand.
- empty-zero, duplicate, zero length, sum mismatch, overflow, and one-unit remainder boundaries
  return the exact frozen result/error.

### Gate 6 — Phase-4 intrinsic Power integration

- per-Wire lengths are measured once and reused by Capacity and Power collection.
- only positive shares create tag-6 loads at their owner `WireBody`.
- support is intrinsic/non-switchable and enters the complete nominal set before solve.
- ordinary demand 240 plus support 28 yields exact regional nominal 268.

### Gate 7 — common-ratio solve and no priority

- generation 268 grants all support and ordinary loads at exact ratio one.
- constrained/source-less variants prove support shares receive the same common regional ratio and
  no reserved allocation or first-load monopoly.
- route, loss, load, source, and adjacency permutation cannot change reports.

### Gate 8 — Phase-8 Support Heat boundary

- Heat uses actual grant and nearest-ties-even, producing exact 4 and 3 rows.
- unmet support and zero rounded Heat create no row.
- tag 2 is append-only and rows sort by `(owner,kind,demand)`.
- no canonical thermal state, Phase-9 integration, response, damage, or destruction appears.

### Gate 9 — report and analyzer truth

- v2/v3 are `None`; active v4 zero is `Some(0)`; overcapacity is exact `Some(E/total/share)`.
- `PowerStepReport.loads` is the sole support-grant truth and no duplicate report disagrees.
- analyzers are repeatable, read-only, hash-neutral, and omit Tick-local Phase-8 Heat.

### Gate 10 — exact C-22

- every value in section 9 is asserted, including no deletion/rejection/direct penalty.
- increasing `U` in a paired valid fixture strictly increases demand where the curve requires it.
- the fixture proves real Scenario Source-to-Wire Power routing, not an injected ratio.

### Gate 11 — artifacts and retained identity

- the v3 Scenario, v2 Replay, v4 Profile, and V6 checkpoints reproduce all frozen hashes.
- regeneration is byte-for-byte deterministic and a second generation is clean.
- retained Scenario/Replay/Profile artifacts preserve their declared contracts.

### Gate 12 — Headless/Bevy parity

- both named tests in section 9 pass.
- Headless and Bevy match every completed Tick hash, Capacity report, Power load/grant/ratio, and
  Phase-8 Heat row through nextTick 3.

### Gate 13 — fuzz and order independence

- profile decoder, support arithmetic, distribution, Power collection, solver, replay restart, and
  numeric-boundary corpora run without panic.
- Wire/input/adjacency/source ordering and cache clear/rebuild produce identical reports and hashes.

### Gate 14 — negative scope

- no hard build reject, Wire deletion, Capacity-specific damage, or direct timing modifier exists.
- no Relay, Construction, Contact, canonical thermal, destruction, or run-end behavior appears.
- C-09/C-10/C-23/C-24 and both complete Stage 1 gates remain open.

### Gate 15 — Windows-native fail-closed evidence

From a fresh Windows-native `git clone --no-local`, without WSL:

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
```

The S1-M3 gate must enumerate every gate above, freeze exact registered test identities/counts,
fail on missing/skipped evidence, and leave `git status --short` empty in the verification clone.

## 12. Closure boundary

S1-M3 may be marked complete only after all fifteen gates pass on the committed tree and a fresh
Windows-native clone. The closure record must name the implementation commit, Balance v4 semantic
and byte hashes, Scenario/Replay hashes, exact C-22 arithmetic/report values, suite/gate counts,
Headless/Bevy equivalence, monotonic/oracle evidence, and clean-clone result.

That closure advances the tracker only for S1-M3. S1-M4 through S1-M6, C-09/C-10, and both complete
Stage 1 gates remain open.
