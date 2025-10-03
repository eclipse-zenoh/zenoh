<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

# Eclipse Zenoh

Eclipse Zenoh: Zero Overhead Pub/Sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more information and [installation instructions](https://zenoh.io/docs/getting-started/installation/)

See also the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed technical information.

# Zenoh Rust API

The crate `zenoh` provides the main implementation of the Zenoh network protocol in Rust and the [API](https://docs.rs/zenoh/latest/zenoh/) to use it.

The primary features of Zenoh are

* Arbitrary network topology support
* Publish-subscribe and query-reply paradigms
* Support for a great variety of underlying network protocols
* Hierarchical keys with glob support (key expressions)
* Zero-copy data buffers
* Scouting for nodes in the network
* Monitor data availability (liveliness)
* Monitor the interest to published data (matching)
* Shared memory support
* Compact and efficient platform-independent data serialization and deserialization (in [zenoh-ext](../zenoh-ext))
* Components for reliable data publishing with retransmissions (AdvancedSubscriber in [zenoh-ext](../zenoh-ext))

# Usage Examples

## Publish and subscribe

Publishing data:

```rust
#[tokio::main]
async fn main() {
    let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    session.put("key/expression", "value").await.unwrap();
    session.close().await.unwrap();
}
```

Subscribing to data:

```rust
use futures::prelude::*;
#[tokio::main]
async fn main() {
    let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    let subscriber = session.declare_subscriber("key/expression").await.unwrap();
    while let Ok(sample) = subscriber.recv_async().await {
        println!("Received: {:?}", sample);
    };
}
```

## Query and reply

Declare a queryable:

```rust
#[tokio::main]
async fn main() {
    let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    let queryable = session.declare_queryable("key/expression").await.unwrap();
    while let Ok(query) = queryable.recv_async().await {
        query.reply("key/expression", "value").await.unwrap();
    }
}
```

Request data:

```rust
use futures::prelude::*;
#[tokio::main]
async fn main() {
    let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    let replies = session.get("key/expression").await.unwrap();
    while let Ok(reply) = replies.recv_async().await {
        println!(">> Received {:?}", reply.result());
    }
}
```

# Rust 1.75 support

The crate `zenoh` can be compiled with Rust 1.75.0, but some of its dependencies may require higher Rust versions.
To compile `zenoh` with Rust 1.75, add a dependency on the crate [zenoh-pinned-deps-1-75](http://crates.io/crates/zenoh-pinned-deps-1-75) to your `Cargo.toml`:

```toml
zenoh = "1.5.1"
zenoh-pinned-deps-1-75 = "1.5.1"
```

# Documentation and examples

For more information, see its documentation: <https://docs.rs/zenoh>
and some examples of usage in <https://github.com/eclipse-zenoh/zenoh/tree/main/examples>
