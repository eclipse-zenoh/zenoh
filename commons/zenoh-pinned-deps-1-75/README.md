<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3ACI+branch%3Amain++)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh

Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounced _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/).

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Zenoh Rust 1.75 pinned dependencies

The [zenoh](http://crates.io/crates/zenoh) crate's minimal Rust version is 1.75, but if you just add `zenoh` as a dependency and try to compile your project with

```sh
cargo +1.75 build
```

it will fail because recent versions of crates that `zenoh` depends on require newer Rust versions. `zenoh` doesn't use "=" in its dependencies to lock
the latest 1.75-compatible versions of these crates, as such a lock would lead to conflicts in projects dependent on `zenoh`. Instead, we provide a separate crate with
these locks. Add a dependency on it to your `Cargo.toml` if you need Rust 1.75 compatibility:

```toml
zenoh = "1.5.1"
zenoh-pinned-deps-1-75 = "1.5.1"
```
