# ⚠️ WARNING ⚠️

This crate is intended for Zenoh's internal use.
It is not guaranteed that the API will remain unchanged in any version, including patch updates.
It is highly recommended to depend solely on the zenoh and zenoh-ext crates and to utilize their public APIs.

- [Click here for Zenoh's main repository](https://github.com/eclipse-zenoh/zenoh)
- [Click here for Zenoh's documentation](https://zenoh.io)

## Fuzzing

The `zenoh-codec` crate includes `cargo-fuzz` targets for:

- `transport_message`
- `network_message` (structured `arbitrary` model)
- `scouting_message`

From the `commons/zenoh-codec/fuzz` directory, run:

```sh
# Generate the deterministic seed corpus
cargo run --bin gen_all_corpora

# Optional: verify the generated corpus matches the current encoder
cargo run --bin verify_all_corpora

# Run the fuzz targets
cargo +nightly fuzz run transport_message
cargo +nightly fuzz run network_message
cargo +nightly fuzz run scouting_message

# Only rerun a certain input
cargo +nightly fuzz run transport_message artifacts/transport_message/crash-xxxx
cargo +nightly fuzz run network_message artifacts/network_message/crash-xxxx
cargo +nightly fuzz run scouting_message artifacts/scouting_message/crash-xxxx

# Analyze one input without running the fuzz loop
cargo run --bin analyze_transport_message -- "[2, 220, 11, 13, 0]"
cargo run --bin analyze_network_message -- "[29, 0, 1, 2]"
cargo run --bin analyze_scouting_message -- "[1, 1, 10]"
```

`network_message` intentionally differs from the outer parser targets: it uses a
structured `arbitrary` model to generate valid inner-message states more
efficiently, while raw wire parsing is still covered by `transport_message`.

To inspect corpus coverage for the fuzz target, run:

```sh
# Collect coverage data from the corpus
# Use `-s none` to disable sanitizers during coverage collection.
cargo +nightly fuzz coverage -s none transport_message corpus/transport_message

# Resolve the LLVM tools shipped with the nightly toolchain
LLVM_BIN="$(dirname "$(rustc +nightly --print target-libdir)")/bin"

# Hide Rust stdlib and cargo-registry dependencies from the report.
# Print a text summary
"$LLVM_BIN/llvm-cov" report \
  target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/transport_message \
  -instr-profile=coverage/transport_message/coverage.profdata \
  --ignore-filename-regex='^/rustc/|^.*/.cargo/registry'

# Hide Rust stdlib and cargo-registry dependencies from the report.
# Only render HTML for the Zenoh source trees we want to inspect.
# Generate an HTML report focused on Zenoh sources
"$LLVM_BIN/llvm-cov" show \
  target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/transport_message \
  -instr-profile=coverage/transport_message/coverage.profdata \
  --format=html \
  --output-dir=coverage/transport_message/html \
  --ignore-filename-regex='^/rustc/|^.*/.cargo/registry' \
  ../src \
  ../../zenoh-protocol/src \
  ../../zenoh-buffers/src
```
