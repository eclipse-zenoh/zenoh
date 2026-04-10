# Zenoh Test Framework

When testing the Zenoh protocol, we often need to establish a specific network topology. Typically, this involves creating a listener and multiple connectors to verify various functionalities.

## Why Dynamic Ports?

Historically, Zenoh tests used fixed TCP ports to facilitate connections. However, this approach presented several challenges:

1. **Parallel Execution Collisions**: Running multiple tests simultaneously often led to port conflicts.
2. **System Conflicts**: Hardcoded ports might conflict with other services running on the host system.
3. **Maintenance Overhead**: Managing hardcoded ports across a growing test suite became increasingly difficult as they were scattered throughout the codebase.

### The Dynamic Approach

To resolve these issues, the test framework now utilizes dynamic port assignment. By binding to `port 0` (e.g., `tcp/127.0.0.1:0`), the operating system assigns an available random port. Once the listener is started, the assigned port is retrieved from the Zenoh session and passed to the connectors.

## The `TestSessions` Utility

To avoid repeating the boilerplate of dynamic port discovery and session management, we provide a `TestSessions` utility located in `zenoh/tests/common/mod.rs`. This utility:

* Automatically manages connections between listeners and connectors.
* Tracks all sessions created during a test.
* Provides a simple way to close all sessions at once.

## Usage Examples

Here are the most common patterns for using the test framework:

### 1. Simple Peer-to-Peer

Use `open_pairs()` to quickly create a listener and a connector that is already connected to it.
*Reference test case: `zenoh_session_unicast` in `session.rs`*

```rust
// Initialize the test context
let mut test_context = TestSessions::new();

// Open a pair of sessions: peer01 (listener) and peer02 (connector)
let (peer01, peer02) = test_context.open_pairs().await;

// Perform test actions...

// Clean up all sessions
test_context.close().await;
```

### 2. One Listener, Multiple Connectors

Open a listener followed by multiple connectors. The connectors will automatically connect to the last opened listener by default.
*Reference test case: `test_link_events` in `connectivity.rs`*

```rust
let mut test_context = TestSessions::new();

// Open a listener with default configuration
let session1 = test_context.open_listener().await;

// Open multiple connectors (connected to session1)
let session2 = test_context.open_connector().await;
let session3 = test_context.open_connector().await;

test_context.close().await;
```

### 3. Custom Configurations

You can retrieve default configurations for listeners or connectors and modify them before opening the sessions.
*Reference test case: `zenoh_unicity_brokered` in `unicity.rs`*

```rust
let mut test_context = TestSessions::new();

// Create a listener with a customized configuration
let mut config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
config.set_mode(Some(WhatAmI::Router)).unwrap();
let router = test_context.open_listener_with_cfg(config).await;

// Create connectors with custom modes
let mut config = test_context.get_connector_config();
config.set_mode(Some(WhatAmI::Client)).unwrap();
let s01 = test_context.open_connector_with_cfg(config.clone()).await;
let s02 = test_context.open_connector_with_cfg(config.clone()).await;

test_context.close().await;
```

### 4. Advanced: Manual Topology

For complex topologies where multiple listeners need to be interconnected manually, use the `get_tcp_locator` helper to retrieve assigned ports.
*Reference test case: `test_liveliness_subget_router_middle` in `liveliness.rs`*

```rust
// Create a listener manually on port 0
let router = {
    let mut c = zenoh_config::Config::default();
    c.listen.endpoints.set(vec!["tcp/127.0.0.1:0".parse::<EndPoint>().unwrap()]).unwrap();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    let _ = c.set_mode(Some(WhatAmI::Router));
    ztimeout!(zenoh::open(c)).unwrap()
};

// Key Point: Get the actual assigned endpoint
let router_endpoint = get_tcp_locator(&router).await;

// Use this endpoint to configure another session
let mut c2 = zenoh_config::Config::default();
c2.connect.endpoints.set(vec![router_endpoint]).unwrap();
// ...
```

### 5. (Not Recommended) Pre-allocating Free Ports

In rare cases where you must know the endpoint *before* creating the session, you can use `get_free_port()`.
*Reference test case: `router_linkstate` in `routing.rs`*

**Warning**: This is not recommended because it is susceptible to TOCTOU (Time-of-check to time-of-use) race conditions, where another process could bind to the "free" port before Zenoh does.

```rust
// Get a system free port (potential race condition)
let locator1 = format!("tcp/127.0.0.1:{}", get_free_port());

// Use it for configuration...
let router1_node = Node {
    listen: vec![locator1.clone()],
    // ...
};
```
