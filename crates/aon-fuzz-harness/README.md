# AON deterministic fuzz regression harness

This crate is the dependency-free fallback for the malformed-decoder, geometry, and command fuzz
gates in TRD section 31.8. It is intentionally small enough to run in ordinary workspace CI when
`cargo-fuzz`/libFuzzer is unavailable.

The decoder target uses the low two bits of byte 0 to select scenario, numeric profile, physical
scale profile, or balance profile decoding. At most 16 KiB is interpreted. Profile cases replace
only the selected profile in an otherwise valid reference package, ensuring that the selected
typed decoder is reached.

The geometry target maps at most 4 KiB to no more than 64 points. It derives a positive power-of-two
quantum and signed 32-bit coordinate multiples from the input, then runs every point through
`validate_quantized` and the complete polyline through `polyline_length`. Short inputs cycle their
bytes, so interpretation is deterministic for every input length.

The command targets map at most 4 KiB to no more than 16 envelopes and 8 raw Wire points per
envelope, then exercise both canonical command encoders. The stateless target applies the batch to
a fresh world. The stateful target first builds a deterministic three-Tick Fixed Substrate prefix
through public commands, leaving live Gate/Junction/Wire records and a tombstoned EntityId before
applying the arbitrary batch.

Run the deterministic generated cases and retained regression corpus with:

```sh
cargo test -p aon-fuzz-harness --locked
```

Replay an arbitrary stream from standard input with:

```sh
cargo run -p aon-fuzz-harness --locked -- decoder < input.bin
cargo run -p aon-fuzz-harness --locked -- geometry < input.bin
cargo run -p aon-fuzz-harness --locked -- commands < input.bin
cargo run -p aon-fuzz-harness --locked -- all < input.bin
```

Normal typed decoder or checked numeric-overflow results are accepted outcomes. A panic, reference
package/prefix failure, non-numeric simulation invariant error, or disagreement between the two
command encoders fails CI and command-mode CLI replay. Add every minimized reproducer under
`corpus/decoder`, `corpus/geometry`, or `corpus/command` and register it in
`tests/regression_corpus.rs` so CI replays it permanently.
