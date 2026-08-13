# AON deterministic fuzz regression harness

This crate is the dependency-free fallback for the malformed-decoder, geometry, and command fuzz
gates in TRD section 31.8. It is intentionally small enough to run in ordinary workspace CI when
`cargo-fuzz`/libFuzzer is unavailable.

The decoder target uses the low two bits of byte 0 to select scenario, numeric profile, physical
scale profile, or balance profile decoding. At most 16 KiB is interpreted. Profile cases replace
only the selected profile in an otherwise valid reference package, ensuring that the selected
typed decoder is reached.

The `replay` target supplies at most 16 KiB directly to the strict Replay JSON decoder. Its retained
corpus includes legacy Empty-world input plus current Replay v2/State V6 S1-M1 and S1-M2 capacity,
sensing, and brownout artifacts, as well as truncated and unknown-field artifacts. CI therefore
preserves both accepted versioned paths and stable rejection paths without treating typed decode
errors as harness failures.

The `experiment` and `module` targets independently supply at most 16 KiB to the strict Experiment
Plan v1 and Module v1 decoders. The Module boundary rejects invalid UTF-8 without replacement.
Their retained corpora preserve valid inputs plus truncated and unknown-field rejection paths, and
the regression tests require identical typed outcomes when an input is replayed.

The geometry target maps at most 4 KiB to no more than 64 points. It derives a positive power-of-two
quantum and signed 32-bit coordinate multiples from the input, then runs every point through
`validate_quantized` and the complete polyline through `polyline_length`. Short inputs cycle their
bytes, so interpretation is deterministic for every input length.

The command targets map at most 4 KiB to no more than 16 envelopes and 8 raw Wire points per
envelope, then exercise both canonical command encoders. The stateless target applies the batch to
a fresh world. The stateful target first builds a deterministic three-Tick Fixed Substrate prefix
through public commands, leaving live Gate/Junction/Wire records and a tombstoned EntityId before
applying the arbitrary batch.

The `signal` target maps at most 32 bytes to 32 post-prefix Ticks. It constructs two identical
two-NOT/one-Wire circuits entirely through the public `Simulation` command API, retains observed
live and removed Driver IDs, and then interprets each byte as one of these operations:

- valid source or target external update, including threshold-adjacent and `u64::MAX` strength;
- removed, predicted-frontier, or live Gate-output Driver update;
- simultaneous updates to two Drivers;
- two updates to one Driver that exercise ordinal-last coalescing;
- an empty Tick that lets inertial and transport events mature.

Multi-command batches are submitted to the second simulation in reverse insertion order. Every
report, public Gate/Driver/Sink/Wire observation, and post-Tick state hash must still agree. Any
command-encoder mismatch, modeled acceptance/rejection mismatch, `NumericOverflow`,
`InvalidCanonicalState`, other run error, or replica disagreement fails the target. The signal
target therefore does not silently classify a fatal engine error as an ordinary arbitrary-input
outcome.

The `topology` target maps at most 16 bytes to 16 complete stateful S0-M4 micro-scenarios. Every
scenario is run against two fresh replicas; multi-command batches are inserted in forward order in
one replica and reverse order in the other. The harness compares the full `StepReport`, public
Gate/Driver/Sink/Slot/Wire observations, and State Hash after every Tick. A retained case reaches
and verifies all of these outcomes rather than merely recording intended operations:

- a physical delay-three Route is Added while a Gate-output transition is due, staging
  TopologySync Revision N and Propagation Revision N+1 in the same Tick;
- the due group reports one stale Revision and applies the Revision N+1 Slot sample;
- removing or rebinding the Wire while its Arrivals are in flight reports invalid-path discards;
- bind-away/bind-back advances the stamped generation, reports Replaced, invalidates old
  Arrivals, and later applies the replacement sync;
- destroy/rebuild of identical geometry uses a new EntityId with the same old/new behavior;
- an unrelated Junction edit reports Retained and leaves the old Certificate deliverable;
- removing an already-applied Route deletes its Slot and resolves the Sink passive Low;
- `u64::MAX` Driver strength, a complete-sample no-op, and a subsequent strength change verify
  boundary sample handling and monotonic Revision observations.

Every `SimulationError` and every command encoder error or mismatch fails the topology target. The
public `Simulation` API intentionally exposes no mutable Tick, Revision, Certificate, or payload
frontier, so a real near-exhaustion whole-Tick rollback cannot be constructed by this external
crate. Those exact rollback boundaries remain covered by bounded seams inside `aon-sim`; this
harness does not claim an intent flag as external rollback coverage.

The `mobility` target maps at most 16 bytes to 16 complete stateful S0-M7 micro-scenarios. Each
scenario runs on two fresh replicas with reverse insertion for multi-command batches, comparing the
full `StepReport`, State Hash, and immutable render snapshot after every Tick. The retained corpus
verifies all four C14 LOW/HIGH turn rows through settled Mobile-local control routes, explicit Track
binding and unbinding, occupied Track rejection, Mobile/Track removal, failed-placement identity
rollback, dead-end bounce, and movement adjacent to both the largest and smallest world-pitch
quantized `i64` coordinates. Every simulation error, encoder mismatch, unexpected typed command
outcome, or replica disagreement fails this target.

Run the deterministic generated cases and retained regression corpus with:

```sh
cargo test -p aon-fuzz-harness --locked
```

Replay an arbitrary stream from standard input with:

```sh
cargo run -p aon-fuzz-harness --locked -- decoder < input.bin
cargo run -p aon-fuzz-harness --locked -- replay < input.bin
cargo run -p aon-fuzz-harness --locked -- experiment < input.bin
cargo run -p aon-fuzz-harness --locked -- module < input.bin
cargo run -p aon-fuzz-harness --locked -- geometry < input.bin
cargo run -p aon-fuzz-harness --locked -- commands < input.bin
cargo run -p aon-fuzz-harness --locked -- signal < input.bin
cargo run -p aon-fuzz-harness --locked -- topology < input.bin
cargo run -p aon-fuzz-harness --locked -- mobility < input.bin
cargo run -p aon-fuzz-harness --locked -- all < input.bin
```

Normal typed decoder errors and checked numeric-overflow outcomes in the geometry and legacy
command targets are accepted. The signal, topology, and mobility targets treat every simulation
run error as a failure. A panic, reference package/prefix failure, unexpected simulation invariant error, or
disagreement between the two command encoders fails CI and CLI replay. Add every minimized
reproducer under `corpus/decoder`, `corpus/replay`, `corpus/geometry`, `corpus/command`,
`corpus/experiment`, `corpus/module`, `corpus/signal-runtime`, `corpus/topology-runtime`, or
`corpus/mobility-runtime` and register it in `tests/regression_corpus.rs` so CI replays it
permanently.
