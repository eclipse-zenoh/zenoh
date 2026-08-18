# Zenoh Transport Rotation: Investigation & Design Proposal

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current Architecture Overview](#2-current-architecture-overview)
3. [Session–Transport Decoupling Analysis](#3-sessiontransport-decoupling-analysis)
4. [Connection Lifecycle Today](#4-connection-lifecycle-today)
5. [The Problem: Why Rotation Is Needed](#5-the-problem-why-rotation-is-needed)
6. [Proposed Design: Transport Rotation](#6-proposed-design-transport-rotation)
7. [Configuration Schema](#7-configuration-schema)
8. [Implementation Plan](#8-implementation-plan)
9. [Testing Strategy](#9-testing-strategy)
10. [Edge Cases & Risks](#10-edge-cases--risks)
11. [Relationship to Existing Zenoh Features](#11-relationship-to-existing-zenoh-features)
12. [Open Questions](#12-open-questions)

---

## 1. Executive Summary

Zenoh's transport layer currently maintains persistent connections to configured locators — once a transport link is established, it stays open until failure or explicit closure. While the session lease mechanism (`KEEP_ALIVE` / lease expiry) already decouples the *logical session* from *underlying transport links*, there is no mechanism to proactively rotate connections while they are still healthy.

This document investigates the feasibility of **transport rotation**: a middleware-level capability to automatically close and re-establish transport links on a configurable schedule or policy, enabling cloud-friendly connection cycling that works with load balancers, DNS-based service discovery, and ephemeral endpoints.

**Conclusion:** Transport rotation is feasible and can be implemented as a relatively self-contained feature within the orchestrator layer, leveraging the existing session–transport decoupling. The main work involves (a) a new configuration schema, (b) a rotation timer/policy engine in the orchestrator, and (c) graceful link teardown + re-establishment using existing `del_link` / `open_transport_unicast` primitives.

---

## 2. Current Architecture Overview

### 2.1 Layered Model

```
┌─────────────────────────────────────────┐
│           Application / Session          │  zenoh::Session (API layer)
├─────────────────────────────────────────┤
│         Network / Protocol Layer         │  Routing, declarations, queries
├─────────────────────────────────────────┤
│          Transport Manager               │  TransportManager (unicast + multicast)
│  ┌─────────────┐  ┌──────────────────┐  │
│  │ Transport    │  │ Transport         │  │
│  │ Unicast      │  │ Multicast         │  │
│  │ (Universal/  │  │                   │  │
│  │  LowLatency) │  │                   │  │
│  └──────┬───────┘  └──────────────────┘  │
│         │                                │
│    ┌────┴────┐                            │
│    │ Links   │  TransportLinkUnicast      │
│    │ (TCP,   │  per-link TX/RX loops      │
│    │  TLS,   │                            │
│    │  QUIC,  │                            │
│    │  UDP,   │                            │
│    │  WS...)│                            │
│    └─────────┘                            │
├─────────────────────────────────────────┤
│           Link Layer (zenoh-link)        │  Protocol-specific I/O
└─────────────────────────────────────────┘
```

### 2.2 Key Components

| Component | Location | Role |
|-----------|----------|------|
| `TransportManager` | `io/zenoh-transport/src/manager.rs` | Central manager; holds config, state, link managers, and the transport registry |
| `TransportManagerStateUnicast` | `io/zenoh-transport/src/unicast/manager.rs` | Holds `transports: HashMap<ZenohId, Arc<dyn TransportUnicastTrait>>` — the active transport pool |
| `TransportUnicastUniversal` | `io/zenoh-transport/src/unicast/universal/transport.rs` | The main unicast transport implementation; manages `TransportLinks` (a collection of `TransportLinkUnicastUniversal`) |
| `TransportLinkUnicastUniversal` | `io/zenoh-transport/src/unicast/universal/link.rs` | Per-link TX/RX task management; contains the transmission pipeline |
| `Orchestrator` | `zenoh/src/net/runtime/orchestrator.rs` | Session-level orchestrator; manages connect/listen endpoints, retry logic, scouting, and reconnection |
| `RuntimeSession` | `zenoh/src/net/runtime/mod.rs` | Implements `TransportPeerEventHandler`; receives `new_link`, `del_link`, `closed` callbacks |
| `ConnectionRetryConf` | `commons/zenoh-config/src/connection_retry.rs` | Retry configuration: `period_init_ms`, `period_max_ms`, `period_increase_factor` |

### 2.3 Session vs. Transport vs. Link

From the Zenoh specification:

> A Zenoh *session* is a logical association between two nodes established over one or more *transport links*. All data exchange occurs within the context of an established session.

> The *transport layer* sits between the underlying network (UDP/TCP/QUIC/…) and the network message layer.

> A *transport link* is a point-to-point or multicast channel between two Zenoh nodes.

The hierarchy is:
```
Session (logical, identified by peer ZID)
  └── Transport (TransportUnicastUniversal, keyed by peer ZID in TransportManager)
       └── Links (1..N TransportLinkUnicastUniversal, each over TCP/TLS/QUIC/etc.)
            └── TX task (keep_alive + pipeline drain)
            └── RX task (lease tracking + message dispatch)
```

**Key insight:** A single transport (per peer ZID) can have multiple links (multilink). The transport survives individual link failures as long as at least one link remains. When the last link is removed, the transport is deleted.

---

## 3. Session–Transport Decoupling Analysis

### 3.1 How Decoupling Works Today

The decoupling already exists at multiple levels:

1. **Lease/Keep-Alive:** Each link has a lease duration (default 10s) and sends `KEEP_ALIVE` messages at `lease / keep_alive_count` intervals (default 4, so every 2.5s). If no message is received within the lease period, the RX loop fails with `"expired after N milliseconds"`, triggering `del_link`.

2. **Transport survives link loss:** In `TransportUnicastUniversal::del_link()`, the link is removed from `TransportLinks`. Only when `is_last` is true (no remaining links) does `self.delete()` get called, which removes the transport from the manager and notifies the callback via `closed()`.

3. **Session survives transport loss:** When `closed()` fires on `RuntimeSession`, the orchestrator's `closed_session()` method kicks in. For non-client modes, it collects all configured endpoints, filters out peers that are still connected, and spawns `peers_connector_retry()` to re-establish connections.

4. **Multilink:** When `transport_multilink` feature is enabled, a single transport can have multiple simultaneous links (up to `max_links`). The `MultiLink` extension negotiates this during INIT. This provides redundancy but not rotation.

### 3.2 What's Missing for Rotation

The current architecture handles **reactive** reconnection (after failure) but not **proactive** rotation. Specifically:

- **No rotation timer:** There is no mechanism to close a healthy link and open a new one on a schedule.
- **No rotation policy:** There is no configuration to express "rotate every N seconds" or "rotate after N bytes" or "rotate on DNS change."
- **`OneOf` strategy is unimplemented:** The `LocatorsStrategy::OneOf` enum variant exists but the orchestrator warns: `"connect.endpoints locator groups with strategy=oneOf are not implemented yet; falling back to current allOf behavior"`. This would be the natural place for rotation semantics.
- **No graceful link cycling:** The `del_link` path is designed for failure cleanup, not for intentional teardown-while-healthy. While it would work, there's no orchestration layer that triggers it proactively.

---

## 4. Connection Lifecycle Today

### 4.1 Connection Establishment Flow

```
Config: connect.endpoints = ["tcp/router.example.com:7447"]
                          ↓
Orchestrator::start() / start_client() / start_peer() / start_router()
                          ↓
connect_peers() → connect_peers_impl()
                          ↓
For each EndPoint group:
  ┌─ single_link mode (client): connect_peers_single_link()
  │   Try each endpoint sequentially; first success wins.
  │   Failed endpoints go to peers_connector_retry() with exponential backoff.
  │
  └─ multi_link mode (peer/router): connect_peers_multiply_links()
      For each endpoint:
        - If no retry timeout: try once, exit_on_failure controls behavior
        - If exit_on_failure: peer_connector_retry() with backoff
        - Else: spawn_peer_connector() in background (keeps retrying forever)
                          ↓
peer_connector() → manager.open_transport_unicast(endpoint)
                          ↓
TransportManager::open_transport_unicast_inner()
  1. Create/get LinkManagerUnicast for protocol
  2. Merge endpoint config with global link config
  3. Open link: manager.new_link(endpoint) → LinkUnicast
  4. Establishment: open::open_link() → INIT SYN → INIT ACK → OPEN SYN → OPEN ACK
  5. init_transport_unicast():
     - If transport for this ZID exists: add_link() to existing transport
     - If new: create TransportUnicastUniversal, add_link(), notify handler
  6. Start TX loop (keep_alive + pipeline) and RX loop (lease tracking)
```

### 4.2 Connection Failure & Reconnection Flow

```
Link failure (lease expiry, TCP RST, etc.)
                          ↓
RX task fails → spawns: transport.del_link(link)
                          ↓
TransportUnicastUniversal::del_link()
  1. Remove link from TransportLinks
  2. Notify callback: callback.del_link(link)
  3. Close the underlying socket
  4. If is_last link: transport.delete()
     - Mark transport as Closed
     - Close callback: callback.closed()
     - Remove from TransportManager registry
                          ↓
RuntimeSession::del_link() → Runtime::closed_link(session, endpoint)
  - If not client mode and endpoint is in config:
    - Spawn: peer_connector_retry(endpoint) with exponential backoff
                          ↓
RuntimeSession::closed() → Runtime::closed_session(session)
  - Collect all configured endpoints
  - Remove those still connected
  - Spawn: peers_connector_retry(remaining, client_mode)
```

### 4.3 Retry Configuration

```json5
{
  connect: {
    endpoints: ["tcp/router.example.com:7447"],
    exit_on_failure: { router: false, peer: false, client: true },
    retry: {
      period_init_ms: 1000,      // Initial backoff
      period_max_ms: 4000,      // Maximum backoff
      period_increase_factor: 2 // Exponential factor
    }
  }
}
```

Per-endpoint overrides via query parameters:
```
tcp/192.168.0.1:7447#retry_period_init_ms=20000;retry_period_max_ms=10000
```

---

## 5. The Problem: Why Rotation Is Needed

### 5.1 Cloud-Native Scenarios

| Scenario | Problem with Current Behavior |
|----------|-------------------------------|
| **Load balancer redistribution** | Cloud LBs (AWS NLB, GCP LB, Azure LB) distribute connections. Once a Zenoh session connects, the LB pinning keeps it on the same backend. Rotation allows redistribution across backends. |
| **DNS-based service discovery** | Kubernetes services, Consul, etc. resolve DNS to different IPs over time. A long-lived TCP connection pins to one IP. Rotation allows picking up new endpoints. |
| **Ephemeral endpoints** | Cloud auto-scaling groups rotate instances. Old IPs become stale. Rotation forces re-resolution and connection to fresh endpoints. |
| **Connection draining** | During deployments, cloud providers drain connections. Proactive rotation allows graceful migration before forced disconnect. |
| **Security/compliance** | Some environments require periodic re-keying or re-establishment of TLS sessions. Rotation naturally cycles TLS contexts. |
| **Cost optimization** | Some cloud providers charge per-connection-hour. Rotation can spread load and avoid long-lived connection surcharges. |

### 5.2 Current Workarounds and Their Limitations

1. **External health checks + restart:** Restarting the entire Zenoh session is heavy-handed and causes data loss.
2. **Multilink with multiple endpoints:** Adding multiple endpoints provides redundancy but doesn't rotate — all links stay open simultaneously.
3. **Short lease times:** Reducing the lease duration causes faster failure detection but doesn't cause rotation of healthy connections.
4. **Application-level reconnection:** The application could close and re-open the session, but this is coarse and loses session state (declarations, subscriptions).

---

## 6. Proposed Design: Transport Rotation

### 6.1 Design Principles

1. **Non-disruptive:** Rotation should be graceful — close the old link only after the new link is established (make-before-break), when possible.
2. **Configurable:** Rotation policy should be configurable per-endpoint and globally.
3. **Layered:** Rotation logic belongs in the orchestrator layer, not the transport layer. The transport layer's `del_link` / `add_link` primitives are the building blocks.
4. **Optional:** Rotation is opt-in; existing behavior is unchanged when not configured.
5. **Composable with multilink:** Rotation and multilink should work together. For multilink, rotation can cycle individual links within a transport. For single-link, rotation replaces the entire connection.

### 6.2 Rotation Policies

**Initial implementation:** Only the `Interval` policy will be implemented.

```rust
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum RotationPolicy {
    #[serde(rename = "interval")]
    Interval {
        interval_ms: u64,
        jitter_ms: Option<u64>,
    },
    // Additional variants (ByteThreshold, DnsChange, Schedule, etc.)
    // will be added here when implemented.
}
```

The initial configuration will simply be:

```json5
rotation: {
  enabled: true,
  policy: {
    type: "interval",
    interval_ms: 300000,  // 5 minutes
    jitter_ms: 30000,     // ±30s random jitter
  },
}
```

This keeps the initial implementation simple — a single timer per endpoint — while the `RotationPolicy` enum leaves room to add byte-threshold, DNS-watch, and cron-scheduled policies later without breaking the API.

### 6.3 High-Level Architecture

```
┌──────────────────────────────────────────────┐
│                Orchestrator                    │
│                                                │
│  ┌──────────────┐  ┌───────────────────────┐  │
│  │ Connect       │  │ Rotation Engine        │  │
│  │ Manager       │  │ (new)                  │  │
│  │ (existing)    │  │                        │  │
│  │               │  │ - Per-endpoint timers  │  │
│  │ - endpoints   │  │ - DNS watch            │  │
│  │ - retry logic │  │ - Byte counters        │  │
│  │               │  │                        │  │
│  │               │  │ On trigger:            │  │
│  │               │  │  1. Open new link      │  │
│  │               │  │  2. Wait for establish │  │
│  │               │  │  3. Close old link      │  │
│  │               │  │  4. Update endpoint    │  │
│  │               │  │     tracking           │  │
│  └──────┬────────┘  └───────────┬───────────┘  │
│         │                       │               │
│         └───────────┬───────────┘               │
│                     ↓                           │
│         TransportManager                        │
│         - open_transport_unicast()              │
│         - del_link() / close()                  │
└──────────────────────────────────────────────┘
```

### 6.4 Rotation Flow (Make-Before-Break)

```
Rotation timer fires for endpoint E1 (tcp/router.example.com:7447)
                          ↓
┌─ Step 1: Open new link ──────────────────────────────┐
│  rotation_engine.trigger_rotation(E1)                 │
│    → manager.open_transport_unicast(E1)                │
│    → INIT/OPEN handshake                                │
│    → New link L2 established to same (or new) peer     │
│    → If same peer ZID: link added to existing transport│
│    → If new peer ZID: new transport created            │
│                                                         │
│  If open fails: keep old link, schedule next rotation   │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─ Step 2: Close old link ─────────────────────────────┐
│  Find the old link L1 associated with E1                │
│    → transport.del_link(L1)                             │
│    → L1's TX/RX tasks terminate                        │
│    → Underlying socket closes                           │
│    → callback.del_link(L1) fires                        │
│    → If L1 was last link: transport.delete()            │
│       (but L2 is already active, so transport survives) │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─ Step 3: Update tracking ─────────────────────────────┐
│  session.endpoints: remove old L1 endpoint              │
│  session.endpoints: insert new L2 endpoint              │
│  Reset rotation timer for E1                            │
└─────────────────────────────────────────────────────────┘
```

### 6.5 Rotation Flow (Break-Before-Make for Single-Link Client Mode)

For client mode (single link), make-before-break may briefly result in two links to the same peer. If the peer rejects the second link (e.g., `max_links=1` without multilink), we need a fallback. However, **break-before-make should only be used as a fallback when make-before-break fails** — it should never be the primary strategy.

The reason is that break-before-make causes a full transport teardown, which triggers `closed_session()`, which in turn causes the routing layer to re-declare all interests, subscriptions, and queries on the new connection. If this happens on every rotation cycle across many clients, it creates a **storm of redeclarations** on the target peer — defeating the purpose of graceful rotation.

The rotation strategy is therefore:

1. **Primary: Make-before-break** — always attempt to open the new link first while keeping the old one alive.
2. **Fallback: Break-before-make** — only if the make step fails (e.g., peer rejects second link, connection refused, handshake timeout), and only after a configurable number of retries.

```
Rotation timer fires for endpoint E1
                          ↓
┌─ Step 1: Attempt make-before-break ──────────────────────┐
│  manager.open_transport_unicast(E1)                      │
│    → INIT/OPEN handshake                                  │
│    → If success: proceed to Step 2 (close old link)       │
│    → If failure: retry up to N times with backoff         │
│      → If all retries fail: proceed to fallback (Step 1b) │
└───────────────────────────────────────────────────────────┘
                          ↓ (make succeeded)
┌─ Step 2: Close old link (make-before-break) ────────────┐
│  Find the old link L1 associated with E1                  │
│    → transport.del_link(L1)                               │
│    → L1's TX/RX tasks terminate                          │
│    → callback.del_link(L1) fires                          │
│    → Transport survives (L2 is already active)            │
│    → No closed_session() → no redeclaration storm         │
└───────────────────────────────────────────────────────────┘

--- Fallback (only if make failed after all retries) ---
                          ↓
┌─ Step 1b: Break-before-make (last resort) ───────────────┐
│  transport.del_link(L1)                                  │
│    → If last link: transport.delete()                    │
│    → callback.closed() fires                             │
│    → closed_session() triggers redeclaration             │
│    → peers_connector_retry() re-establishes connection   │
│                                                           │
│  ⚠ This causes a redeclaration burst — use sparingly     │
└───────────────────────────────────────────────────────────┘
```

**Key design point:** The `del_link` path in make-before-break does *not* trigger `closed_session()` because the transport still has the new link. This means no redeclarations are emitted. Only the fallback break-before-make path causes redeclarations, and it should be rare — it only fires when the new connection genuinely cannot be established (e.g., the target endpoint is down).

**Configuration for fallback behavior:**

```json5
rotation: {
  enabled: true,
  policy: {
    type: "interval",
    interval_ms: 300000,
  },
  mode: "make_before_break",  // Primary strategy
  fallback: {
    enabled: true,             // Enable break-before-make fallback
    max_retries: 3,            // Retries before falling back
    retry_backoff_ms: 1000,    // Initial backoff between retries
  }
}
```

If `fallback.enabled` is false and make-before-break fails, the rotation is simply skipped and rescheduled — the old connection remains untouched.

### 6.6 Integration with `OneOf` Strategy

The existing `LocatorsStrategy::OneOf` enum variant is the natural configuration home for rotation. When `strategy = "oneOf"`, the orchestrator would:

1. Connect to one locator from the group
2. Rotate to a different locator from the group on each rotation interval
3. Cycle through all locators in the group

```json5
{
  connect: {
    endpoints: [
      {
        strategy: "oneOf",
        locators: [
          "tcp/router1.example.com:7447",
          "tcp/router2.example.com:7447",
          "tcp/router3.example.com:7447"
        ]
      }
    ],
    rotation: {
      enabled: true,
      interval_ms: 300000,  // 5 minutes
      mode: "make_before_break"
    }
  }
}
```

---

## 7. Configuration Schema

### 7.1 Global Rotation Configuration

**Initial scope:** Only `interval` policy with `make_before_break` mode and optional fallback. Advanced policies (byte threshold, DNS change, cron schedule) are reserved in the schema but not yet implemented.

```json5
{
  connect: {
    // Existing fields...
    endpoints: ["tcp/router.example.com:7447"],
    
    // New: rotation configuration
    rotation: {
      // Enable/disable rotation globally
      enabled: false,
      
      // Rotation policy (only interval is implemented initially)
      policy: {
        type: "interval",
        interval_ms: 300000,  // Rotate every 5 minutes
        jitter_ms: 30000,     // ±30s random jitter
      },
      
      // Rotation mode (make-before-break is the only mode initially;
      // break-before-make is a fallback, not a primary mode)
      mode: "make_before_break",
      
      // Fallback configuration for when make-before-break fails
      fallback: {
        enabled: true,
        max_retries: 3,           // Retries before falling back
        retry_backoff_ms: 1000,   // Initial backoff between retries
      },
      
      // Whether to re-resolve DNS before reconnecting
      // (reserved — not yet implemented)
      reresolve_dns: false,
      
      // Whether to rotate to a different locator in the same group
      // (only meaningful with OneOf strategy)
      rotate_across_locators: true,
    },
  }
}
```

### 7.2 Per-Endpoint Rotation Configuration

```
tcp/router.example.com:7447#rotation_enabled=true;rotation_policy_type=interval;rotation_policy_interval_ms=600000;rotation_mode=make_before_break
```

### 7.3 Rust Configuration Types

```rust
// In commons/zenoh-config/src/rotation.rs (new file)

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RotationConf {
    pub enabled: bool,
    /// Rotation policy (only Interval is implemented initially)
    pub policy: RotationPolicy,
    /// Rotation mode (make_before_break is the primary and only mode initially)
    pub mode: RotationMode,
    /// Fallback configuration for when make-before-break fails
    pub fallback: RotationFallbackConf,
    /// Whether to re-resolve DNS before reconnecting (reserved — not yet implemented)
    pub reresolve_dns: bool,
    /// Whether to rotate to a different locator in the same group (OneOf strategy)
    pub rotate_across_locators: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RotationFallbackConf {
    /// Enable break-before-make as a fallback when make-before-break fails
    pub enabled: bool,
    /// Number of retries before falling back to break-before-make
    pub max_retries: u32,
    /// Initial backoff between retries in milliseconds
    pub retry_backoff_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum RotationMode {
    #[serde(rename = "make_before_break")]
    MakeBeforeBreak,
    // Note: BreakBeforeMake is NOT a user-selectable mode.
    // It is only used internally as a fallback when MakeBeforeBreak fails.
    // This prevents users from accidentally configuring a mode that
    // causes redeclaration storms on every rotation cycle.
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum RotationPolicy {
    #[serde(rename = "interval")]
    Interval {
        interval_ms: u64,
        jitter_ms: Option<u64>,
    },
    // Additional variants (ByteThreshold, DnsChange, Schedule, etc.)
    // will be added here when implemented.
}
```

---

## 8. Implementation Plan

### 8.1 Phase 1: Core Rotation Engine (Minimal Viable)

**Goal:** Time-based (interval) rotation for single endpoints with make-before-break, and break-before-make as a fallback only.

**Files to modify/create:**

| File | Change |
|------|--------|
| `commons/zenoh-config/src/rotation.rs` (new) | `RotationConf`, `RotationFallbackConf`, `RotationMode` types |
| `commons/zenoh-config/src/lib.rs` | Wire rotation config into `Config` |
| `zenoh/src/net/runtime/orchestrator.rs` | Add `RotationEngine` struct; spawn rotation timers in `spawn_peer_connector()`; handle rotation trigger |
| `zenoh/src/net/runtime/mod.rs` | Expose rotation engine on `Runtime` |
| `DEFAULT_CONFIG.json5` | Document rotation configuration |

**Key implementation in orchestrator:**

```rust
// New struct in orchestrator.rs
struct RotationEngine {
    runtime: Runtime,
    endpoints: Vec<EndPoint>,
    config: RotationConf,
    cancellation_token: CancellationToken,
}

impl RotationEngine {
    async fn run(&self) {
        let mut interval = tokio::time::interval(
            match &self.config.policy {
                RotationPolicy::Interval { interval_ms, .. } => {
                    Duration::from_millis(*interval_ms)
                }
            }
        );
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.rotate().await;
                }
                _ = self.cancellation_token.cancelled() => {
                    break;
                }
            }
        }
    }
    
    async fn rotate(&self) {
        for endpoint in &self.endpoints {
            // Step 1: Attempt make-before-break (open new link)
            let make_result = self.try_make_before_break(endpoint).await;
            
            match make_result {
                Ok((old_endpoint, new_transport)) => {
                    // Make succeeded: close old link gracefully.
                    // This does NOT trigger closed_session() because the
                    // transport still has the new link — no redeclaration storm.
                    if let Some(old) = old_endpoint {
                        self.close_old_link(old).await;
                    }
                    self.update_endpoint_tracking(endpoint).await;
                }
                Err(e) => {
                    // Make failed. Try fallback retries first.
                    tracing::warn!(
                        "Rotation make-before-break failed for {}: {}. Retrying...",
                        endpoint, e
                    );
                    
                    if self.config.fallback.enabled {
                        if let Err(e) = self.try_make_with_retries(endpoint).await {
                            tracing::warn!(
                                "Rotation failed after {} retries for {}: {}. \
                                 Falling back to break-before-make.",
                                self.config.fallback.max_retries, endpoint, e
                            );
                            // Last resort: break-before-make.
                            // This WILL trigger closed_session() and redeclarations.
                            self.fallback_break_before_make(endpoint).await;
                        }
                    } else {
                        tracing::warn!(
                            "Rotation failed for {}: {}. Keeping old connection. \
                             Fallback disabled.",
                            endpoint, e
                        );
                    }
                }
            }
        }
    }
}
```

**Integration point in `spawn_peer_connector()`:**

```rust
async fn spawn_peer_connector(&self, peer: EndPoint) -> ZResult<()> {
    // ... existing code ...
    self.spawn(async move {
        if let Ok(zid) = this.peer_connector_retry(peer).await {
            // ... existing start_conditions logic ...
            
            // NEW: Start rotation engine if configured
            let rotation_conf = this.get_rotation_config(&peer);
            if rotation_conf.enabled {
                this.spawn_rotation_engine(peer, rotation_conf);
            }
        }
    });
    Ok(())
}
```

### 8.2 Phase 2: OneOf Strategy + Multi-Locator Rotation

**Goal:** Implement `LocatorsStrategy::OneOf` with rotation across locators in a group.

**Files to modify:**

| File | Change |
|------|--------|
| `zenoh/src/net/runtime/orchestrator.rs` | Implement `connect_peers_single_link_oneof()` — connect to one locator, rotate to next on interval |
| `commons/zenoh-protocol/src/core/endpoint.rs` | Remove "not implemented" warning for `OneOf` |

**Logic:**

```rust
async fn connect_peers_oneof(&self, locators: &[EndPoint], rotation_conf: &RotationConf) {
    let mut current_idx = 0;
    
    // Initial connection
    self.peer_connector(locators[current_idx].clone()).await.ok();
    
    if rotation_conf.enabled {
        let mut interval = tokio::time::interval(
            match &rotation_conf.policy {
                RotationPolicy::Interval { interval_ms, .. } => {
                    Duration::from_millis(*interval_ms)
                }
            }
        );
        loop {
            interval.tick().await;
            
            // Rotate to next locator
            current_idx = (current_idx + 1) % locators.len();
            let new_endpoint = locators[current_idx].clone();
            
            // Make-before-break
            match self.manager().open_transport_unicast(new_endpoint.clone()).await {
                Ok(_transport) => {
                    // Close old connection
                    if let Some(old_endpoint) = self.get_current_endpoint_for_group() {
                        self.close_endpoint(old_endpoint).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Rotation to {} failed: {}", new_endpoint, e);
                    current_idx = (current_idx - 1) % locators.len(); // Revert
                }
            }
        }
    }
}
```

### 8.3 Phase 3: Advanced Policies (Future — Not in Initial Implementation)

**Goal:** DNS-change detection, byte-threshold rotation, and jitter. These are reserved in the API but not implemented in the initial version. The initial implementation only supports the `Interval` policy.

**DNS change detection:**

```rust
async fn dns_watcher(endpoint: EndPoint, trigger: tokio::sync::Notify) {
    let hostname = extract_hostname(&endpoint);
    let mut last_ips = resolve_dns(hostname);
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let current_ips = resolve_dns(hostname);
        if current_ips != last_ips {
            tracing::info!("DNS change detected for {}: {:?} -> {:?}", 
                hostname, last_ips, current_ips);
            trigger.notify_one();
            last_ips = current_ips;
        }
    }
}
```

**Byte threshold:**

This requires hooking into the transport's statistics. The `zenoh-stats` feature already tracks per-link TX/RX byte counts. The rotation engine can poll these counters:

```rust
async fn check_byte_threshold(&self, transport: &TransportUnicast) -> bool {
    if let Some(stats) = transport.stats() {
        let tx = stats.get_tx_bytes();
        let rx = stats.get_rx_bytes();
        return tx > self.config.tx_threshold || rx > self.config.rx_threshold;
    }
    false
}
```

### 8.4 Phase 4: Graceful Drain Integration

**Goal:** Before closing a link during rotation, drain in-flight messages.

The existing `wait_before_close` configuration (default 100ms) already provides a drain period. The rotation engine should:

1. Stop scheduling new messages on the old link's pipeline
2. Wait for the pipeline to drain (or timeout)
3. Close the link

```rust
async fn graceful_close_link(&self, link: &TransportLinkUnicastUniversal) {
    // The pipeline.disable() call in close() already handles draining
    // But we can add an explicit drain wait:
    link.pipeline.disable();  // Stops accepting new messages
    tokio::time::timeout(
        self.drain_timeout,
        link.pipeline.drain(),
    ).await.ok();
    link.close(None).await;
}
```

---

## 9. Testing Strategy

Tests for transport rotation span two layers: the transport layer (unit tests in `io/zenoh-transport/tests/`) and the session/orchestrator layer (integration tests in `zenoh/tests/`). The existing test infrastructure — `SHRouter`/`SHClient` handler stubs, `open_transport_unicast()` / `close_transport()` helpers, and `TestSessions` — provides the building blocks.

### 9.1 Transport Layer Tests (`io/zenoh-transport/tests/`)

These tests operate directly on `TransportManager` and verify link-level rotation behavior without the full session/orchestrator stack.

#### 9.1.1 `test_rotation_make_before_break`

**Goal:** Verify that opening a second link to the same peer (same ZID) adds it to the existing transport, and closing the old link does not trigger `closed()`.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_make_before_break() {
    let server_port = get_free_tcp_port();
    let server_endpoint = EndPoint::new("tcp", format!("127.0.0.1:{server_port}"), "", "").unwrap();
    
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport_unicast(
            &[server_endpoint.clone()],
            &[server_endpoint.clone()],
            false,
            #[cfg(feature = "transport_multilink")]
            1, // max_links = 1 for router
            #[cfg(feature = "transport_multilink")]
            1, // max_links = 1 for client
        ).await;
    
    // Verify initial state: 1 link, transport alive
    assert_eq!(client_transport.get_links().len(), 1);
    assert!(client_transport.get_callback().is_some());
    
    // Simulate rotation: open a second link to the same endpoint
    // (make-before-break)
    let second_transport = ztimeout!(
        client_manager.open_transport_unicast(server_endpoint.clone())
    ).unwrap();
    
    // With multilink disabled (max_links=1), this should either:
    // - Add the link to the existing transport (if multilink is negotiated), or
    // - Fail (if max_links=1 and no multilink)
    // The test should verify the appropriate behavior based on feature flags.
    
    // Close the old link — transport should survive
    // (This is the core of make-before-break: no closed() callback fires)
    
    close_transport(router_manager, client_manager, second_transport, &[server_endpoint]).await;
}
```

#### 9.1.2 `test_rotation_del_link_no_closed_callback`

**Goal:** Verify that when a transport has 2+ links and one is removed via `del_link`, the `closed()` callback is NOT invoked — only `del_link()` fires.

```rust
#[cfg(feature = "transport_multilink")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_del_link_no_closed_callback() {
    let port1 = get_free_tcp_port();
    let port2 = get_free_tcp_port();
    let ep1 = EndPoint::new("tcp", format!("127.0.0.1:{port1}"), "", "").unwrap();
    let ep2 = EndPoint::new("tcp", format!("127.0.0.1:{port2}"), "", "").unwrap();
    
    let closed_count = Arc::new(AtomicUsize::new(0));
    let del_link_count = Arc::new(AtomicUsize::new(0));
    
    // Custom handler that counts closed() and del_link() calls
    let handler = Arc::new(CountingHandler::new(closed_count.clone(), del_link_count.clone()));
    
    // Set up router with 2 listeners, client with 2 links (multilink)
    let router_manager = /* ... build with handler, max_links >= 2 ... */;
    let client_manager = /* ... build with max_links >= 2 ... */;
    
    // Open 2 links to the same router
    let transport = ztimeout!(client_manager.open_transport_unicast(ep1.clone())).unwrap();
    let _ = ztimeout!(client_manager.open_transport_unicast(ep2.clone())).unwrap();
    
    assert_eq!(transport.get_links().len(), 2);
    
    // Close one link — should NOT trigger closed()
    let link_to_close = transport.get_links()[0].clone();
    transport.del_link(link_to_close).await.unwrap();
    
    // Give callbacks time to fire
    tokio::time::sleep(SLEEP).await;
    
    assert_eq!(del_link_count.load(Ordering::SeqCst), 1, "del_link should have fired once");
    assert_eq!(closed_count.load(Ordering::SeqCst), 0, "closed() should NOT have fired");
    assert_eq!(transport.get_links().len(), 1, "one link should remain");
    
    // Close the last link — NOW closed() should fire
    let last_link = transport.get_links()[0].clone();
    transport.del_link(last_link).await.unwrap();
    tokio::time::sleep(SLEEP).await;
    
    assert_eq!(closed_count.load(Ordering::SeqCst), 1, "closed() should have fired after last link removed");
}
```

#### 9.1.3 `test_rotation_data_continuity`

**Goal:** Verify that data continues to flow without loss during a make-before-break rotation cycle.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_data_continuity() {
    // Set up router + client with multilink (max_links >= 2)
    let (router_manager, router_handler, client_manager, client_transport) =
        open_transport_unicast(/* ... */).await;
    
    let msg_count = 1000;
    
    // Start sending messages in a background task
    let sender_transport = client_transport.clone();
    let send_task = tokio::spawn(async move {
        for i in 0..msg_count {
            let msg = make_test_message(i);
            sender_transport.schedule(msg).unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    
    // While sending, perform a rotation: open new link, close old link
    let new_endpoint = /* second listener endpoint */;
    let _new_transport = client_manager.open_transport_unicast(new_endpoint).await.unwrap();
    
    // Close old link
    let old_link = client_transport.get_links()[0].clone();
    client_transport.del_link(old_link).await.unwrap();
    
    // Wait for all messages to be received
    send_task.await.unwrap();
    ztimeout!(async {
        while router_handler.get_count() < msg_count {
            tokio::time::sleep(SLEEP_COUNT).await;
        }
    });
    
    // All messages should have been received — no data loss during rotation
    assert_eq!(router_handler.get_count(), msg_count);
}
```

### 9.2 Session/Orchestrator Layer Tests (`zenoh/tests/`)

These tests use the full `zenoh::Session` API and verify end-to-end rotation behavior including redeclarations, subscription persistence, and orchestrator-level reconnection.

#### 9.2.1 `test_rotation_session_continuity`

**Goal:** Verify that a `Session` with active subscribers/declarations survives a rotation cycle and continues to receive publications without re-subscribing.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_session_continuity() {
    zenoh::init_log_from_env_or("error");
    
    // Set up two router endpoints
    let port1 = get_free_tcp_port();
    let port2 = get_free_tcp_port();
    
    // Router listening on both ports
    let router_config = Config::default();
    router_config.listen().endpoints().set(
        vec![format!("tcp/127.0.0.1:{port1}"), format!("tcp/127.0.0.1:{port2}")].into()
    ).unwrap();
    let router_session = zenoh::open(router_config).await.unwrap();
    
    // Client connecting to port1 with rotation enabled
    let client_config = Config::default();
    client_config.connect().endpoints().set(
        vec![format!("tcp/127.0.0.1:{port1}")].into()
    ).unwrap();
    // Enable rotation with a short interval for testing
    client_config.connect().rotation().enabled(true).unwrap();
    client_config.connect().rotation().policy().set(
        RotationPolicy::Interval { interval_ms: 500, jitter_ms: None }
    ).unwrap();
    
    let client_session = zenoh::open(client_config).await.unwrap();
    
    // Declare subscriber on client
    let received = Arc::new(AtomicUsize::new(0));
    let received_clone = received.clone();
    let subscriber = client_session.declare_subscriber("test/key")
        .callback(move |_| {
            received_clone.fetch_add(1, Ordering::SeqCst);
        })
        .await.unwrap();
    
    // Publish from router
    router_session.put("test/key", "hello").await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(received.load(Ordering::SeqCst), 1, "should receive before rotation");
    
    // Wait for rotation to occur (interval is 500ms)
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Publish again after rotation
    router_session.put("test/key", "world").await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Should still receive — subscriber should survive rotation
    // (In make-before-break mode, no closed_session() fires, so no redeclaration needed)
    assert_eq!(received.load(Ordering::SeqCst), 2, "should receive after rotation");
    
    drop(subscriber);
    drop(client_session);
    drop(router_session);
}
```

#### 9.2.2 `test_rotation_fallback_break_before_make`

**Goal:** Verify that when make-before-break fails (e.g., second link rejected), the fallback to break-before-make works and redeclarations are emitted.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_fallback_break_before_make() {
    zenoh::init_log_from_env_or("error");
    
    // Router with single listener, no multilink (max_links=1)
    let port = get_free_tcp_port();
    let router_config = Config::default();
    router_config.listen().endpoints().set(
        vec![format!("tcp/127.0.0.1:{port}")].into()
    ).unwrap();
    // Disable multilink on router side so second link is rejected
    router_config.transport().unicast().max_links().set(1).unwrap();
    let router_session = zenoh::open(router_config).await.unwrap();
    
    // Client with rotation, fallback enabled
    let client_config = Config::default();
    client_config.connect().endpoints().set(
        vec![format!("tcp/127.0.0.1:{port}")].into()
    ).unwrap();
    client_config.connect().rotation().enabled(true).unwrap();
    client_config.connect().rotation().policy().set(
        RotationPolicy::Interval { interval_ms: 500, jitter_ms: None }
    ).unwrap();
    client_config.connect().rotation().fallback().enabled(true).unwrap();
    client_config.connect().rotation().fallback().max_retries(1).unwrap();
    
    let client_session = zenoh::open(client_config).await.unwrap();
    
    // Declare subscriber
    let received = Arc::new(AtomicUsize::new(0));
    let received_clone = received.clone();
    let _subscriber = client_session.declare_subscriber("test/fallback")
        .callback(move |_| {
            received_clone.fetch_add(1, Ordering::SeqCst);
        })
        .await.unwrap();
    
    // Publish before rotation
    router_session.put("test/fallback", "before").await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(received.load(Ordering::SeqCst), 1);
    
    // Wait for rotation + fallback to complete
    // (make fails → retry once → fallback: break old, reconnect)
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Publish after fallback rotation
    router_session.put("test/fallback", "after").await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Should receive — redeclaration should have happened during fallback
    assert_eq!(received.load(Ordering::SeqCst), 2, "should receive after fallback rotation");
    
    drop(client_session);
    drop(router_session);
}
```

#### 9.2.3 `test_rotation_oneof_strategy`

**Goal:** Verify that `OneOf` strategy with rotation cycles through multiple locators.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_oneof_strategy() {
    zenoh::init_log_from_env_or("error");
    
    // Three router instances
    let port1 = get_free_tcp_port();
    let port2 = get_free_tcp_port();
    let port3 = get_free_tcp_port();
    
    let router1 = zenoh::open(Config::default()).await.unwrap();
    let router2 = zenoh::open(Config::default()).await.unwrap();
    let router3 = zenoh::open(Config::default()).await.unwrap();
    // Each router listens on its own port (simplified — in practice they'd
    // be on different hosts)
    
    // Client with OneOf strategy + rotation
    let client_config = Config::default();
    client_config.connect().endpoints().set(
        vec![EndPoints::Locators(Locators {
            strategy: LocatorsStrategy::OneOf,
            locators: vec![
                format!("tcp/127.0.0.1:{port1}").parse().unwrap(),
                format!("tcp/127.0.0.1:{port2}").parse().unwrap(),
                format!("tcp/127.0.0.1:{port3}").parse().unwrap(),
            ],
        })].into()
    ).unwrap();
    client_config.connect().rotation().enabled(true).unwrap();
    client_config.connect().rotation().policy().set(
        RotationPolicy::Interval { interval_ms: 1000, jitter_ms: None }
    ).unwrap();
    
    let client = zenoh::open(client_config).await.unwrap();
    
    // Wait through several rotation cycles
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify the client connected to different routers over time.
    // This can be checked via admin space or by having each router
    // publish to a unique key and checking which keys the client received.
    
    // Alternative: check connectivity events via admin space
    // @/<client_zid>/session/transport/unicast/<router_zid>/link/<link_id>
    
    drop(client);
    drop(router1); drop(router2); drop(router3);
}
```

#### 9.2.4 `test_rotation_disabled_no_rotation`

**Goal:** Verify that with rotation disabled (default), connections persist indefinitely — no rotation occurs.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rotation_disabled_no_rotation() {
    zenoh::init_log_from_env_or("error");
    
    let port = get_free_tcp_port();
    let router_config = Config::default();
    router_config.listen().endpoints().set(
        vec![format!("tcp/127.0.0.1:{port}")].into()
    ).unwrap();
    let router = zenoh::open(router_config).await.unwrap();
    
    let client_config = Config::default();
    client_config.connect().endpoints().set(
        vec![format!("tcp/127.0.0.1:{port}")].into()
    ).unwrap();
    // Rotation NOT enabled (default)
    
    let client = zenoh::open(client_config).await.unwrap();
    
    // Wait a while
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Verify only 1 transport, 1 link — no rotation happened
    let transports = client.runtime().manager().get_transports_unicast().await;
    assert_eq!(transports.len(), 1, "should have exactly 1 transport");
    assert_eq!(transports[0].get_links().len(), 1, "should have exactly 1 link");
    
    drop(client);
    drop(router);
}
```

### 9.3 Test Infrastructure Requirements

| Requirement | Description |
|-------------|-------------|
| `CountingHandler` | New test helper that wraps `TransportPeerEventHandler` and counts `closed()` and `del_link()` invocations. Needed for transport-layer rotation tests. |
| `RotationPolicy` config wiring | The `Config` builder must expose `connect().rotation()` accessors for test configuration. |
| Short rotation intervals | Tests use 500ms–1000ms intervals. The rotation engine must respect sub-second intervals for testing. |
| Admin space assertions | For OneOf tests, the connectivity events RFC keys (`@/<zid>/session/transport/unicast/...`) can be queried to verify which peers were connected over time. |
| `#[cfg(feature = "transport_multilink")]` | Make-before-break tests that expect 2 simultaneous links require the multilink feature. Tests should be gated accordingly. |

### 9.4 Test File Organization

| File | Tests |
|------|-------|
| `io/zenoh-transport/tests/unicast_transport.rs` | `test_rotation_make_before_break`, `test_rotation_del_link_no_closed_callback`, `test_rotation_data_continuity` (append to existing file) |
| `zenoh/tests/session.rs` | `test_rotation_session_continuity`, `test_rotation_fallback_break_before_make`, `test_rotation_oneof_strategy`, `test_rotation_disabled_no_rotation` (append to existing file) |
| `io/zenoh-transport/tests/helpers.rs` (or inline) | `CountingHandler` struct |

---

## 10. Edge Cases & Risks

### 10.1 Identity Preservation During Rotation

**Risk:** When rotating, the new connection might establish a transport to a *different* peer ZID (e.g., if DNS now points to a different Zenoh router). This means the session state (declarations, subscriptions) must be re-synchronized.

**Mitigation:** The existing session establishment already handles this — `new_unicast()` on the handler triggers declaration re-sending. The orchestrator's `RuntimeSession` tracks endpoints and the `closed_session` / `closed_link` callbacks handle re-registration.

**Consideration:** For make-before-break with different ZIDs, there will be a brief period where the session has transports to two different peers. This is already possible with multilink and is handled by the routing layer.

### 10.2 Race Conditions

**Risk:** Rotation timer fires while a retry is in progress (e.g., after a previous failure).

**Mitigation:** The rotation engine should check if a connection attempt is already in progress for the endpoint. Use a per-endpoint mutex or atomic flag:

```rust
struct RotationState {
    in_progress: AtomicBool,
}

async fn rotate(&self) {
    if self.state.in_progress.swap(true, Ordering::SeqCst) {
        return; // Already rotating
    }
    // ... rotation logic ...
    self.state.in_progress.store(false, Ordering::SeqCst);
}
```

### 10.3 Load Balancer Behavior

**Risk:** Some load balancers may not appreciate rapid connection cycling. AWS NLB has connection idle timeouts (350s default); Azure LB has 4-minute idle timeout.

**Mitigation:** The rotation interval should be configurable and default to a reasonable value (e.g., 5-30 minutes). The jitter parameter helps spread rotation across clients.

### 10.4 Connection Storm

**Risk:** If many clients rotate simultaneously (e.g., synchronized by a common event), the target may experience a connection storm.

**Mitigation:** Jitter is essential. Additionally, the existing `accept_pending` limit (default 10) in `TransportManagerConfigUnicast` protects the server side.

### 10.5 Interaction with Multilink

**Risk:** With multilink enabled, rotation should cycle individual links, not the entire transport. If the transport has 3 links and rotation fires, only one link should be rotated.

**Mitigation:** The rotation engine should be aware of the multilink configuration. For multilink, rotation replaces one link at a time:

```
Transport (peer ZID X)
  ├── Link L1 (tcp/old.example.com:7447)  ← rotate this
  ├── Link L2 (tcp/current.example.com:7447)
  └── Link L3 (tcp/backup.example.com:7447)

After rotation:
Transport (peer ZID X)
  ├── Link L1' (tcp/new.example.com:7447)  ← new link
  ├── Link L2 (tcp/current.example.com:7447)
  └── Link L3 (tcp/backup.example.com:7447)
```

### 10.6 Session State Loss

**Risk:** When rotation results in connecting to a different peer (different ZID), all session state (declarations, interests, queries) must be re-established on the new peer.

**Mitigation:** This is already handled by the Zenoh protocol — when a new transport is established, the `new_unicast()` callback on the handler triggers the routing layer to re-declare all interests and subscriptions. The `RuntimeSession::new_peer()` method in the router already handles this.

---

## 11. Relationship to Existing Zenoh Features

### 11.1 Connection Retry (`ConnectionRetryConf`)

Rotation and retry are complementary:
- **Retry** is reactive: it fires after a connection fails, with exponential backoff.
- **Rotation** is proactive: it fires on a schedule, even if the connection is healthy.

They share the same `open_transport_unicast()` primitive but have different triggers and policies.

### 11.2 Multilink (`transport_multilink`)

Multilink provides redundancy; rotation provides freshness. They compose:
- With multilink: rotation cycles individual links within the transport.
- Without multilink: rotation replaces the single link (with brief gap in break-before-make mode).

### 11.3 Scouting & Autoconnect

Scouting discovers peers via multicast; autoconnect establishes connections to discovered peers. Rotation is orthogonal — it applies to already-established connections, not discovery.

However, rotation with `reresolve_dns: true` is conceptually similar to re-scouting: it re-discovers the endpoint's current address.

### 11.4 `OneOf` Strategy

The `LocatorsStrategy::OneOf` variant is currently a no-op (falls back to `AllOf`). Implementing rotation would be the primary use case for `OneOf`:
- Connect to one locator from the group
- Rotate to the next locator on each interval
- Cycle through all locators

### 11.5 Connectivity Status & Events (RFC)

The [Connectivity Status and Events RFC](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Connectivity%20Status%20and%20Events.md) defines admin-space keys for transport/link events:
- `@/<zid>/session/transport/unicast/<peer_zid>/link/<link_id>` — Put on new link, Delete on closed link

Rotation would naturally emit these events: a `Delete` for the old link and a `Put` for the new link. This allows applications to observe rotation events.

### 11.6 Recent Reconnect Work (PR #2173)

PR #2173 ("Fix multilink reconnect") fixed the issue where only one link reconnects after disconnection (#2130). This work improved the `closed_link` callback to properly retry individual links. The rotation feature would build on this foundation — the reconnect logic for individual links is now robust, which is essential for rotation (which intentionally closes links).

---

## 12. Open Questions

### Q1: Should rotation be transport-layer or orchestrator-layer?

**Recommendation:** Orchestrator-layer. The transport layer's `del_link`/`add_link` primitives are sufficient building blocks. Putting rotation logic in the transport layer would couple it to transport internals and make it harder to configure per-endpoint.

### Q2: Should make-before-break be the default (and only) mode?

**Recommendation:** Yes. Make-before-break is the **only** user-selectable mode. Break-before-make is not a user option — it is purely an internal fallback that activates only when make-before-break fails after all retries. This prevents users from accidentally configuring a mode that causes redeclaration storms on every rotation cycle. The mode auto-selected based on `whatami` and `max_links` configuration only in the sense that the fallback behavior may differ (e.g., client mode may have different retry counts).

### Q3: How to handle DNS re-resolution?

DNS re-resolution requires async DNS resolution. The standard library's `tokio::net::lookup_host` can be used, but it doesn't cache or watch. For production use, consider integrating with a DNS caching library that supports TTL-based expiry.

### Q4: Should rotation emit telemetry?

Yes. Rotation events should be logged at `INFO` level and optionally emitted as admin-space events (per the Connectivity Status RFC). Metrics (rotation count, rotation duration, rotation failures) should be exposed via the stats framework.

### Q5: Interaction with QoS?

When rotating, in-flight messages with reliability guarantees should be drained or retransmitted. The existing `wait_before_close` mechanism handles this at the pipeline level, but the rotation engine should ensure the new link is fully operational before closing the old one.

### Q6: Should rotation be per-endpoint or per-transport?

**Recommendation:** Per-endpoint. Each configured endpoint has its own rotation timer. This allows different rotation intervals for different peers (e.g., fast rotation for cloud-facing endpoints, no rotation for local peers).

### Q7: What about the `exit_on_failure` interaction?

If rotation fails and `exit_on_failure` is true (client mode), should the session exit? **No.** Rotation failure should not cause exit — the old connection is still alive (in make-before-break mode). Only if both the old connection fails AND rotation fails should exit behavior apply.

---

## Appendix A: Key Source Files Reference

| File | Path | Relevance |
|------|------|-----------|
| Transport Manager | `io/zenoh-transport/src/manager.rs` | Central config/state; `open_transport_unicast()` entry point |
| Unicast Manager | `io/zenoh-transport/src/unicast/manager.rs` | Transport registry; `init_transport_unicast()`; `del_transport_unicast()` |
| Universal Transport | `io/zenoh-transport/src/unicast/universal/transport.rs` | `TransportUnicastUniversal`; `del_link()`; `delete()`; link management |
| Universal Link | `io/zenoh-transport/src/unicast/universal/link.rs` | TX/RX task lifecycle; keep_alive/lease tracking; `start_tx()`; `start_rx()` |
| Transport Trait | `io/zenoh-transport/src/unicast/transport_unicast_inner.rs` | `TransportUnicastTrait`; `add_link()`; `close()`; `schedule()` |
| Establishment (Open) | `io/zenoh-transport/src/unicast/establishment/open.rs` | INIT/OPEN handshake FSM for initiator |
| Establishment (Accept) | `io/zenoh-transport/src/unicast/establishment/accept.rs` | INIT/OPEN handshake FSM for responder |
| Multilink Extension | `io/zenoh-transport/src/unicast/establishment/ext/multilink.rs` | MultiLink negotiation (RSA-based auth) |
| Orchestrator | `zenoh/src/net/runtime/orchestrator.rs` | Connect/listen management; retry; scouting; `closed_link()`; `closed_session()` |
| Runtime Session | `zenoh/src/net/runtime/mod.rs` | `TransportPeerEventHandler` impl; `endpoints` tracking |
| Connection Retry Config | `commons/zenoh-config/src/connection_retry.rs` | `ConnectionRetryConf`; `ConnectionRetryPeriod` |
| Endpoint Types | `commons/zenoh-protocol/src/core/endpoint.rs` | `EndPoints`; `Locators`; `LocatorsStrategy` (AllOf/OneOf) |
| Default Config | `DEFAULT_CONFIG.json5` | Configuration documentation and defaults |
| Transport Spec | `https://spec.zenoh.io/spec/1.0.0/transport/index.html` | Transport layer specification |
| Session Spec | `https://spec.zenoh.io/spec/1.0.0/session/index.html` | Session specification |
| Session Establishment Spec | `https://spec.zenoh.io/spec/1.0.0/session/open-accept.html` | INIT/OPEN handshake specification |
| Links Spec | `https://spec.zenoh.io/spec/1.0.0/transport/links.html` | Link failure detection specification |

## Appendix B: Existing Reconnection Code Path

```
Link RX task fails (lease expiry or I/O error)
  ↓
link.rs:244  →  transport.del_link(Link::new_unicast(...))
  ↓
transport.rs:185  TransportUnicastUniversal::del_link()
  ↓
  ├── callback.del_link(link)  →  RuntimeSession::del_link()
  │     ↓
  │     mod.rs:1208  →  Runtime::closed_link(session, endpoint)
  │     ↓
  │     orchestrator.rs:1429  →  spawn: peer_connector_retry(endpoint)
  │
  └── if is_last:  transport.delete()
        ↓
        callback.closed()  →  RuntimeSession::closed()
        ↓
        mod.rs:1217  →  Runtime::closed_session(session)
        ↓
        orchestrator.rs:1404  →  spawn: peers_connector_retry(remaining, client_mode)
```

This existing reconnection path is **reactive** — it only fires on failure. The proposed rotation engine adds a **proactive** trigger that initiates the same `del_link` → reconnect sequence on a schedule, but with the addition of make-before-break semantics.
