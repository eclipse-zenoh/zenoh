# ⚠️ WARNING ⚠️

This crate is intended for Zenoh's internal use.
It is not guaranteed that the API will remain unchanged in any version, including patch updates.
It is highly recommended to depend solely on the zenoh and zenoh-ext crates and to utilize their public APIs.

- [Click here for Zenoh's main repository](https://github.com/eclipse-zenoh/zenoh)
- [Click here for Zenoh's documentation](https://zenoh.io)

## Fuzzing

The `zenoh-protocol` crate includes a `cargo-fuzz` target for:

- `endpoint_from_str`

From the `commons/zenoh-protocol/fuzz` directory, run:

```sh
# Generate the deterministic seed corpus
cargo run --bin gen_endpoint_corpus

# Optional: verify the generated corpus matches the current parser behavior
cargo run --bin verify_endpoint_corpus

# Run the fuzz target
cargo +nightly fuzz run endpoint_from_str

# Only rerun a certain input
cargo +nightly fuzz run endpoint_from_str artifacts/endpoint_from_str/crash-xxxx

# Analyze one input without running the fuzz loop
cargo run --bin analyze_endpoint_from_str -- "tcp/127.0.0.1:7447?b=2;a=1#B=2;A=1"
```

### Fuzzing Coverage

To inspect corpus coverage for the fuzz target locally, run:

```sh
# Run the fuzz target first to grow the corpus
cargo +nightly fuzz run endpoint_from_str

# Replay the saved corpus and collect coverage data
cargo +nightly fuzz coverage -s none endpoint_from_str corpus/endpoint_from_str

# Resolve the LLVM tools shipped with the nightly toolchain
LLVM_BIN="$(dirname "$(rustc +nightly --print target-libdir)")/bin"

# Resolve the host target triple used by cargo-fuzz coverage builds
HOST_TRIPLE="$(rustc +nightly -vV | sed -n 's/^host: //p')"

# Hide Rust stdlib and cargo-registry dependencies from the report.
# Print a text summary
"$LLVM_BIN/llvm-cov" report \
  "target/$HOST_TRIPLE/coverage/$HOST_TRIPLE/release/endpoint_from_str" \
  -instr-profile=coverage/endpoint_from_str/coverage.profdata \
  --ignore-filename-regex='^/rustc/|^.*/.cargo/registry' \
  ../src/core/endpoint.rs \
  ../src/core/parameters.rs

# Hide Rust stdlib and cargo-registry dependencies from the report.
# Only render HTML for the endpoint-related sources we want to inspect.
"$LLVM_BIN/llvm-cov" show \
  "target/$HOST_TRIPLE/coverage/$HOST_TRIPLE/release/endpoint_from_str" \
  -instr-profile=coverage/endpoint_from_str/coverage.profdata \
  --format=html \
  --output-dir=coverage/endpoint_from_str/html \
  --ignore-filename-regex='^/rustc/|^.*/.cargo/registry' \
  ../src/core/endpoint.rs \
  ../src/core/parameters.rs
```
