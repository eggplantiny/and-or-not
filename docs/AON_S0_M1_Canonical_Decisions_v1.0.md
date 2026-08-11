# A/O/N — S0-M1 Canonical Decisions

**Status:** implementation baseline
**Applies to:** `S0-M1 — Contract / Numeric / Identity`

This document closes representation details that are intentionally left open by the PRD, SSS,
and TRD drafts. Changing an observable rule below requires a semantics/profile/schema version
review and new golden fixtures.

## Version and artifact policy

- Supported semantics version: `aon-semantics-v1`.
- Scenario and profile schema version: `1`.
- Hash algorithm: BLAKE3, algorithm identifier `blake3-v1`.
- Artifact syntax is strict RFC 8259 JSON. Comments, duplicate struct fields, unknown fields,
  floating-point coefficients, and trailing data are rejected.
- Every profile carries `schemaVersion`, `profileId`, and `kind` as artifact metadata.
  `profileId`, file path, JSON key order, and whitespace are excluded from the semantic hash.
- Rationals are signed numerator plus positive denominator, reduced by GCD with zero encoded as
  `0/1`. Semantically equal fractions therefore hash identically.

## Canonical profile encoding

- Domain separator: `AON\0PROFILE\0V1\0`.
- Encoder version: unsigned 16-bit integer `1`.
- Integers use fixed-width little-endian encoding.
- Strings use unsigned 32-bit byte length followed by UTF-8 bytes.
- Enums use explicit stable `u8` tags; Rust discriminants and memory layout are never encoded.
- Struct fields use a fixed documented order. Map-like tables are encoded by their stable key
  order, never by input or container iteration order.

## Numeric boundary

- `FIXED_ONE = 65_536`.
- `ceil_isqrt` returns `Result<u64, NumericError>` because `ceil(sqrt(u128::MAX))` is `2^64`
  and cannot fit in the TRD draft's infallible `u64` signature.
- Coordinate deltas, square products, and their sum use checked `i128`/`u128` intermediates.
- Overflow is a deterministic error; wrapping and implicit saturation are forbidden.
- Polyline length collapses consecutive same-direction collinear segments into maximal runs for
  length calculation before applying `ceil_isqrt`. Stored vertices remain unchanged and remain
  part of the state hash. This prevents per-segment ceiling from charging a redundant vertex.

## Stage 0 physical alpha values

- Geometry quantum: `1_024` Fixed (`1/64 wu`).
- Circuit routing pitch: `16_384` Fixed (`1/4 wu`).
- World routing pitch: `65_536` Fixed (`1 wu`).
- Wire body radius: `2_048` Fixed (`1/32 wu`).
- Substrate clearance: `2_048` Fixed; this alpha choice equals one wire-body radius.
- AND/OR/NOT footprint: `32_768 × 32_768` Fixed (`1/2 × 1/2 wu`).
- AND/OR anchors: inputs `(-16_384,-8_192)` and `(-16_384,8_192)`, output
  `(16_384,0)`, power `(0,-16_384)`.
- NOT anchors: input `(-16_384,0)`, no second input, output `(16_384,0)`, power
  `(0,-16_384)`.

These are versioned probe values, not final product-balance conclusions.

## Balance extension alpha values

- Capacity Probe uses Main Core Capacity `1,000`, Relay Capacity `500`, over-cap linear
  coefficient `1/1`, over-cap quadratic coefficient `2/1`, denominator floor `1`, Relay
  offline grace `1` Tick, and support heat fraction `1/4`.
- Radiation Reference uses distance weights `[16, 8, 4, 2, 1]`, delays `[1, 1, 2, 3, 4]`,
  and orientation weights broadside/diagonal/endfire `[4, 2, 1]`.
- The initial integer orientation boundaries use absolute-cross and absolute-dot multipliers of
  `2`. These are versioned probe choices because the TRD requires integer-vector boundaries but
  does not prescribe the two multiplier values.
- Optional balance sections are encoded with explicit presence tags. Their absence is therefore
  semantically different from a populated section and changes the Balance Profile hash.

## Identity boundary

- `EntityId(0)` is reserved as invalid; the first allocated ID is `EntityId(1)`.
- Allocation is monotonically increasing. Removed IDs remain tombstones and are never reused.
- Exhausting `u64` returns a deterministic identity overflow error without mutating the registry.
- `ConnectionGeneration` starts at `0` and advances with checked addition.
- The entity allocation frontier and tombstone/location sequence are canonical state because they
  affect future IDs and therefore future simulation results.
- Dense indices are storage locations only; they are not replay IDs, tie-break keys, or hash
  ordering keys.

## Stage feature boundary

Scenario schema v1 declares a `StageFeatureSet`. `Simulation::new` rejects any requested feature
that the current engine build has not implemented. S0-M1 supports the empty feature set; later
milestones enable features only when their complete semantics and conformance tests exist.
