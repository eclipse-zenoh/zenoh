# ⚠️ WARNING ⚠️

This crate is intended for Zenoh's internal use.
It is not guaranteed that the API will remain unchanged in any version, including patch updates.
It is highly recommended to depend solely on the zenoh and zenoh-ext crates and to utilize their public APIs.

- [Click here for Zenoh's main repository](https://github.com/eclipse-zenoh/zenoh)
- [Click here for Zenoh's documentation](https://zenoh.io)

## Fuzzing

The `zenoh-codec` crate includes a `cargo-fuzz` target for `TransportMessage` decoding.

From the `commons/zenoh-codec/fuzz` directory, run:

```sh
# Generate the deterministic seed corpus
cargo run --bin gen_transport_message_corpus

# Optional: verify the generated corpus matches the current encoder
cargo run --bin verify_transport_message_corpus

# Run the fuzz target
cargo +nightly fuzz run transport_message

# Only rerun a certain input
cargo +nightly fuzz run transport_message fuzz/artifacts/transport_message/crash-xxxx
```
