# Zenoh Test Utilities

`zenoh-test` is a workspace crate that provides shared test utilities for the
Zenoh project.  It centralises session lifecycle management so that every
integration test in the workspace can reuse the same patterns without
duplicating boilerplate.

## Why this crate exists

Historically, Zenoh tests used fixed TCP ports and each test file carried its
own copy of helpers.  This caused:

1. **Port collisions** when running tests in parallel.
2. **System Conflicts**: Hardcoded ports might conflict with other running services.
3. **Maintenance overhead** from scattered, duplicated utility code.

`zenoh-test` can help by providing:

* **Dynamic port allocation** — bind to `tcp/127.0.0.1:0` and let the OS
  assign an available port.
* **Automatic locator resolution** — after a listener starts, the actual
  assigned endpoint is retrieved and passed to connectors.
* **Easier teardown** — provides a simple way to close all sessions at once.

## Public API

### Free functions

| Function | Description |
|---|---|
| `get_free_port()` | Binds to a random TCP port and returns the port number.  ⚠️ Susceptible to TOCTOU races; prefer port `0` listeners when possible. |
| `get_tcp_locator(&session)` | Returns the first `tcp/` endpoint from a session's locators. |
| `get_locators_from_session(&session)` | Returns all locators from a session (async). |
| `get_locators_from_session_sync(&session)` | Returns all locators from a session (blocking). |
| `close_session(s1, s2)` | Closes two sessions in order.  Prefer `TestSessions::close()` for new code. |
| `TIMEOUT` | Default 60-second timeout used by `ztimeout!`. |

### `TestSessions`

A struct that tracks listener and connector sessions and provides convenience
methods for common test topologies.

## Usage Examples

### 1. Simple Peer-to-Peer

Use `open_pairs()` to create a listener and a connector in one call.
*Reference: `zenoh_session_unicast` in `session.rs`*

```rust
let mut test_sessions = TestSessions::new();
let (peer01, peer02) = test_sessions.open_pairs().await;

// … run assertions …

test_sessions.close().await;
```

### 2. One Listener, Multiple Connectors

Open a listener, then attach multiple connectors.  Connectors automatically
connect to the most recently opened listener.
*Reference: `test_link_events` in `connectivity.rs`*

```rust
let mut test_sessions = TestSessions::new();

let session1 = test_sessions.open_listener().await;
let session2 = test_sessions.open_connector().await;
let session3 = test_sessions.open_connector().await;

test_sessions.close().await;
```

### 3. Custom Configurations

Retrieve a default config, customise it, then open the session.
*Reference: `zenoh_unicity_brokered` in `unicity.rs`*

```rust
let mut test_sessions = TestSessions::new();

let mut config = test_sessions.get_listener_config("tcp/127.0.0.1:0", 1);
config.set_mode(Some(WhatAmI::Router)).unwrap();
let router = test_sessions.open_listener_with_cfg(config).await;

let mut config = test_sessions.get_connector_config();
config.set_mode(Some(WhatAmI::Client)).unwrap();
let client = test_sessions.open_connector_with_cfg(config).await;

test_sessions.close().await;
```

### 4. Advanced: Manual Topology

For complex topologies where multiple listeners need to be interconnected
manually, use the free functions to discover assigned ports.
*Reference: `test_liveliness_subget_router_middle` in `liveliness.rs`*

```rust
use zenoh_test::{get_tcp_locator, get_locators_from_session};

let router = {
    let mut c = zenoh_config::Config::default();
    c.listen.endpoints.set(vec!["tcp/127.0.0.1:0".parse().unwrap()]).unwrap();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    let _ = c.set_mode(Some(WhatAmI::Router));
    ztimeout!(zenoh::open(c)).unwrap()
};

// Get the actual assigned endpoint
let router_endpoint = get_tcp_locator(&router).await;

// Use it to configure another session
let mut c2 = zenoh_config::Config::default();
c2.connect.endpoints.set(vec![router_endpoint]).unwrap();
// …
```

### 5. (Not Recommended) Pre-allocating Free Ports

In rare cases where you must know the endpoint *before* creating the session,
use `get_free_port()`.
*Reference: `router_linkstate` in `routing.rs`*

> **Warning**: This is susceptible to TOCTOU race conditions.  Another process
> could bind to the "free" port before Zenoh does.

```rust
let locator = format!("tcp/127.0.0.1:{}", get_free_port());

let node = Node {
    listen: vec![locator.clone()],
    // …
};
```
