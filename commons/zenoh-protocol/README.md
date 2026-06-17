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
