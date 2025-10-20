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

Zenoh (pronounced _/zeno/_) unifies data in motion, data at rest, and computations. It carefully blends traditional pub/sub with geo-distributed storage, queries, and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Structure of the Repository

This repository contains the following elements:

* [zenoh](zenoh) Rust crate

  This crate is the primary and reference implementation of the Zenoh protocol. The Zenoh libraries for other languages
  are bindings to this Rust implementation, except for the pure-C
  [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) (see the "Language Support" section below).

* [zenoh-ext](zenoh-ext) Rust crate

  This crate contains extended components of Zenoh:
  * `AdvancedPublisher` / `AdvancedSubscriber` - APIs for sending/receiving data with advanced delivery guarantees.
  * Data serialization support. This serialization is lightweight and universal for all `zenoh` bindings, which simplifies interoperability.

* [zenohd](zenohd) router binary

  The Zenoh router is a standalone daemon used to support Zenoh network infrastructure.

* [plugins](plugins)

  The crates related to plugin support in `zenohd`.

* [commons](commons)

  The internal crates used by `zenoh`. These crates are not intended to be imported directly, and their public APIs can be changed at any time.
  Stable APIs are provided by `zenoh` and `zenoh-ext` only.

* [examples](examples)

  Zenoh usage examples. These examples have a double purpose: they not only demonstrate writing Zenoh applications in Rust but also serve as a set of tools for experimenting with and testing Zenoh functionality.

# Documentation

* [Docs.rs for Zenoh](https://docs.rs/zenoh/latest/zenoh/)

* [Docs.rs for Zenoh-ext](https://docs.rs/zenoh/latest/zenoh-ext/)

# Build and run

Install [Cargo and Rust](https://doc.rust-lang.org/cargo/getting-started/installation.html).
If you already have the Rust toolchain installed, make sure it is up to date with:

```bash
rustup update
```

Zenoh can be successfully compiled with Rust stable (>= 1.75.0), but some of its dependencies may require
newer Rust versions. The `zenoh` crate itself doesn't lock its dependencies with "=" to avoid conflicts.
Instead, we provide the [zenoh-pinned-deps-1-75](commons/zenoh-pinned-deps-1-75) crate
with `zenoh` dependencies locked to Rust 1.75-compatible versions.

To build Zenoh, simply type the command below after having followed the previous instructions:

```bash
cargo build --release --all-targets
```

There are multiple features in `zenoh`; see the full list and descriptions on [docs.rs](https://docs.rs/zenoh/latest/zenoh/). For example, to
use shared memory, it must be explicitly enabled:

```toml
zenoh = {version = "1.5.1", features = ["shared-memory"]}
```

## Examples

[Examples](examples) can be executed with Cargo, or directly from `target/release/examples`. When running with Cargo, use `--` to pass command line arguments to the examples:

### Publish/Subscribe

```bash
cargo run --example z_sub
```

```bash
cargo run --example z_pub
```

### Query/Reply

```bash
cargo run --example z_queryable
```

```bash
cargo run --example z_get
```

## Zenohd Router and Plugins

The [zenohd](zenohd) router can be run with the command `cargo run` or from `target/release/zenohd`. When running with Cargo, use `--` to pass command line arguments to `zenohd`:

```bash
cargo run -- --config DEFAULT_CONFIG.json5
```

The router's purpose is to support Zenoh network infrastructure and provide additional services using [plugins](plugins).
See more details and a directory of available plugins in the [zenohd](zenohd) readme.

# Language Support

* **Rust** - this repository
* **C** - there are two implementations with the same API:
  * [zenoh-c](https://github.com/eclipse-zenoh/zenoh-c) - Rust library binding
  * [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) - pure C implementation
* **C++** - [zenoh-cpp](https://github.com/eclipse-zenoh/zenoh-cpp) - C++ wrapper over C libraries
* **Python** - [zenoh-python](https://github.com/eclipse-zenoh/zenoh-python)
* **Kotlin** - [zenoh-kotlin](https://github.com/eclipse-zenoh/zenoh-kotlin)
* **Java** - [zenoh-java](https://github.com/eclipse-zenoh/zenoh-java)
* **TypeScript** - [zenoh-ts](https://github.com/eclipse-zenoh/zenoh-ts) - WebSocket client for the plugin in [zenohd](zenohd)

# Troubleshooting

In case of trouble, please first check [this page](https://zenoh.io/docs/getting-started/troubleshooting/) to see if the issue and its cause are already known.
Otherwise, you can ask a question on the [Zenoh Discord server](https://discord.gg/vSDSpqnbkm), or [create an issue](https://github.com/eclipse-zenoh/zenoh/issues).
