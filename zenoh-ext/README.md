<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

[![CI](https://github.com/eclipse-zenoh/zenoh/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3ACI+branch%3Amain++)
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh

Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/)

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Zenoh Extension crate

The `zenoh_ext` crate provides some useful extensions on top of the [`zenoh`](https://crates.io/crates/zenoh) crate.

The primary components of this crate are:

## Serialization support

The library implements encoding and decoding data in simple, compact, and platform-independent binary format
described in [Zenoh serialization format](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Serialization.md).

The serialization functions are supported not only in the Rust library, but also in all other language bindings (C, C++,
Java, Kotlin, Python, TypeScript) as well as zenoh-pico, which significantly simplifies interoperability.

### Serialization example

```rust
use zenoh_ext::*;
let zbytes = z_serialize(&(42i32, vec![1u8, 2, 3]));
assert_eq!(z_deserialize::<(i32, Vec<u8>)>(&zbytes).unwrap(), (42i32, vec![1u8, 2, 3]));
```

## Advanced publisher and subscriber

`AdvancedPublisher` and `AdvancedSubscriber` provide:

- **History:** retrieve cached publications at startup and optionally on publisher discovery.
- **Recovery:** request missing publications and fragments while they remain cached and reachable.
- **Fragmentation:** split large payloads and automatically reassemble them in advanced subscribers.
- **Discovery:** advertise publisher/subscriber presence through liveliness.

Enable the **`unstable`** feature with `cargo add zenoh-ext --features unstable`.
These APIs may change between releases. Serialization does not require this feature.

### Example

```rust
use std::time::Duration;
use zenoh_ext::{
    AdvancedPublisherBuilderExt, AdvancedSubscriberBuilderExt,
    CacheConfig, HistoryConfig, MissDetectionConfig, RecoveryConfig,
};

let session = zenoh::open(zenoh::Config::default()).await?;
let publisher = session.declare_publisher("example/data")
    .cache(CacheConfig::default().max_samples(10))
    .sample_miss_detection(MissDetectionConfig::default().heartbeat(Duration::from_secs(1)))
    .fragmentation(1024)
    .publisher_detection()
    .await?;
publisher.put(vec![0u8; 8192]).attachment("metadata").await?;

let subscriber = session.declare_subscriber("example/data")
    .history(HistoryConfig::default().detect_late_publishers())
    .recovery(RecoveryConfig::default().heartbeat())
    .await?;
let sample = subscriber.recv_async().await?;
assert_eq!(sample.payload().len(), 8192);
```

History and recovery are opt-in. Cache/history limits count publications, not fragments.
Ordinary subscribers and direct cache queries receive raw fragments.
Use `sample_miss_listener()` to observe gaps in delivered publications; see the API documentation
for configuration limits and prerequisites.

# Documentation and examples

For more information, see its documentation: <https://docs.rs/zenoh-ext>
and some examples of usage in <https://github.com/eclipse-zenoh/zenoh/tree/main/zenoh-ext/examples/examples>
