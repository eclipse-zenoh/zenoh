<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3ACI+branch%3Amain++)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![codecov](https://codecov.io/github/eclipse-zenoh/zenoh/branch/main/graph/badge.svg?token=F8T4C8WPZD)](https://codecov.io/github/eclipse-zenoh/zenoh)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh

Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Structure of the repository

This repository contains the following elements:

* [zenoh](zenoh) Rust crate

  This crate is the primary and reference implementation of the Zenoh protocol. The Zenoh libraries for other languages
  are bindings to this Rust implementation, except for the pure-C
  [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) (see "Languages support" section below).

* [zenoh-ext](zenoh-ext) Rust crate

  This crate contains extended components of Zenoh:
  * `AdvancedPublisher` / `AdvancedSubscriber` - the API to send/receive data with advanced delivery guarantees.
  * Data serialization support. This serialization is lightweight and universal for all `zenoh` bindings, which simplifies interoperability.

* [zenohd](zenohd) router binary

  The zenoh router - the standalone daemon which is used to support zenoh network infrastructure.

* [plugins](plugins)

  The crates related to plugins support in `zenohd`

* [examples](examples)

  Zenoh usage examples. These examples have a double purpose: they not only demonstrate writing Zenoh applications in Rust but also serve as a set of tools for experimenting with and testing Zenoh functionality.

# Documentation

* [Docs.rs for Zenoh](https://docs.rs/zenoh/latest/zenoh/)

* [Docs.rs for Zenoh-ext](https://docs.rs/zenoh/latest/zenoh-ext/)

# Build and run

Install [Cargo and Rust](https://doc.rust-lang.org/cargo/getting-started/installation.html).
If you already have the Rust toolchain installed, make sure it is up-to-date with:

```bash
rustup update
```

Zenoh can be successfully compiled with Rust stable (>= 1.75.0), but some of its dependencies may require
higher Rust versions. The `zenoh` crate itself doesn't lock its dependencies with "=" to avoid conflicts.
Instead we provide the crate [zenoh-pinned-deps-1-75](commons/zenoh-pinned-deps-1-75)
with `zenoh` dependencies locked to Rust 1.75-compatible versions. To compile `zenoh` with Rust 1.75
add dependency on it to your Cargo.toml

```toml
zenoh = "1.5.1"
zenoh-pinned-deps-1-75 = "1.5.1"
```

To build Zenoh, just type the following command after having followed the previous instructions:

```bash
cargo build --release --all-targets
```

The router can be run with the command `cargo run` or from `target/release/zenohd`. When running with cargo use `--` to pass command line arguments to `zenohd`:

```bash
cargo run --release -- --config DEFAULT_CONFIG.json5
```

Examples can also be executed with cargo or directly from `target/release/examples`

Publish/subscribe

```bash
cargo run --example z_sub
```

```bash
cargo run --example z_pub
```

Query/reply

```bash
cargo run --example z_queryable
```

```bash
cargo run --example z_get
```

# Languages support

* Rust - this repository
* C - there are two implementations with the same API:
  * [zenoh-c](https://github.com/eclipse-zenoh/zenoh-c) - Rust library binding
  * [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) - pure C implementation
* C++ [zenoh-cpp](https://github.com/eclipse-zenoh/zenoh-cpp) - C++ wrapper over C libraries
* Python - [zenoh-python](https://github.com/eclipse-zenoh/zenoh-python)
* Kotlin - [zenoh-kotlin](https://github.com/eclipse-zenoh/zenoh-kotlin)
* Java - [zenoh-java](https://github.com/eclipse-zenoh/zenoh-java)
* TypeScript - [zenoh-ts](https://github.com/eclipse-zenoh/zenoh-ts) - the WebSocket client to the plugin in [zenohd](zenohd)

# Troubleshooting

In case of troubles, please first check on [this page](https://zenoh.io/docs/getting-started/troubleshooting/) if the trouble and cause are already known.
Otherwise, you can ask a question on the [Zenoh Discord server](https://discord.gg/vSDSpqnbkm), or [create an issue](https://github.com/eclipse-zenoh/zenoh/issues).
