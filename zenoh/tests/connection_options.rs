//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! Tests for `zenoh::open()` behavior under various combinations of:
//!
//! - `connect/timeout_ms`
//!   - `-1` → retry forever (peer/router default)
//!   - `0`  → one-shot, no retry (client default)
//!   - `N`  → stop retrying after N total milliseconds (returns Err on expiry)
//!
//! - `connect/exit_on_failure`
//!   - `true`  → return `Err` when the connection attempt fails (peer/client default: false/true)
//!   - `false` → continue in the background, return `Ok`
//!   - NOTE: for *peer* mode (`connect_peers_multiply_links`), `exit_on_failure` is checked
//!     even in the one-shot path (`timeout_ms=0`).  For *client* mode
//!     (`connect_peers_single_link`), `exit_on_failure` is only checked when there are
//!     endpoints queued for retry, so with `timeout_ms=0` it is silently ignored.
//!
//! - `open/return_conditions/connect_configured`
//!   - `true` (default) → `open()` waits up to `scouting/delay` (default 500 ms) for the
//!     connection to the configured endpoints to be established before returning.
//!   - `false` → `open()` returns without waiting for any connection.
//!
//! - `open/return_conditions/connect_scouted`
//!   - Analogous to `connect_configured` but for peers discovered via scouting.
//!
//! - `scouting/timeout`
//!   - `-1` → scout in background, `open()` returns after `scouting/delay` (500 ms)
//!   - `N`  → block until a router is found via multicast or N ms have elapsed
//!   - NOTE: only affects *client* mode when no `connect/endpoints` are configured and
//!     multicast scouting is enabled.  With multicast scouting disabled and no configured
//!     endpoints, `open()` fails immediately regardless of this setting.
//!
//! Tests are split into three groups:
//!   A – configured `connect/endpoints`, router IS available
//!   B – configured `connect/endpoints`, router NOT available (PEER mode)
//!   C – no `connect/endpoints`, scouting-based discovery (CLIENT mode, multicast enabled)

use std::time::Duration;

use zenoh_config::{Config, ModeDependentValue};
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;
use zenoh_protocol::core::WhatAmI;

// Ports 17501–17530 are reserved for this file.
// (scouting_delay_regression in routing.rs uses 17490)
const EP_A1: &str = "tcp/127.0.0.1:17501"; // A1 – defaults, router available
const EP_A2: &str = "tcp/127.0.0.1:17502"; // A2 – connect_configured=false, router available
const EP_B: &str = "tcp/127.0.0.1:17510"; // B group – no listener; router NOT available
const EP_C: &str = "tcp/127.0.0.1:17520"; // C4 – scouting timeout, router available

// ─── Timing constants ────────────────────────────────────────────────────────
/// Required by the `ztimeout!` macro from `zenoh_core`.
const TIMEOUT: Duration = Duration::from_secs(10);

/// `scouting/delay` default (500 ms).  After a failed or background connection,
/// `open()` waits at most this long for `start_conditions` before returning Ok.
const SCOUTING_DELAY_MS: u64 = 500;

///  Maximum time for an operation that should fail after scouting delay.
#[cfg(not(target_os = "windows"))]
const SCOUTING_FAILURE_MAX: u64 = SCOUTING_DELAY_MS + 1000;
#[cfg(target_os = "windows")]
const SCOUTING_FAILURE_MAX: u64 = SCOUTING_DELAY_MS + 5000;

/// Maximum time for an operation that should return "immediately" (well before
/// the scouting delay fires).
const IMMEDIATE_MAX: Duration = Duration::from_millis(300);

/// Maximum time for an operation that should fail "immediately" (well before
/// the scouting delay fires).
#[cfg(not(target_os = "windows"))]
const IMMEDIATE_FAIL_MAX: Duration = Duration::from_millis(300);
#[cfg(target_os = "windows")]
const IMMEDIATE_FAIL_MAX: Duration = Duration::from_millis(5000);

/// Maximum time for a successful `open()` when a router is available.
const FAST_OPEN_MAX: Duration = Duration::from_millis(2000);

/// Total timeout for the global-connect-timeout tests (`timeout_ms = SHORT_MS`).
const SHORT_MS: i64 = 500;
/// Minimum elapsed time expected (SHORT_MS minus a 150 ms grace period).
const SHORT_MIN: Duration = Duration::from_millis(350);
/// Maximum elapsed time tolerated (SHORT_MS plus two full scouting delays).
const SHORT_MAX: Duration = Duration::from_millis(SHORT_MS as u64 + 2 * SCOUTING_DELAY_MS + 500);

// ─── Protective outer timeout ─────────────────────────────────────────────────
/// Wraps a future with a 30-second protective outer timeout so that a hung
/// test terminates rather than blocking the whole test suite.
macro_rules! ztimeout_slow {
    ($f:expr) => {{
        tokio::time::timeout(Duration::from_secs(30), $f)
            .await
            .expect("test timed out (>30 s)")
    }};
}

// ─── Config helpers ───────────────────────────────────────────────────────────

fn router_config(endpoint: &str) -> Config {
    let mut c = Config::default();
    c.listen
        .endpoints
        .set(vec![endpoint.parse::<EndPoint>().unwrap()])
        .unwrap();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    let _ = c.set_mode(Some(WhatAmI::Router));
    c
}

/// Peer config with a configured connect endpoint, multicast scouting disabled.
/// Peer defaults: `timeout_ms = -1` (retry forever), `exit_on_failure = false`.
fn peer_with_connect(endpoint: &str) -> Config {
    let mut c = Config::default();
    c.connect
        .endpoints
        .set(vec![endpoint.parse::<EndPoint>().unwrap()])
        .unwrap();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    let _ = c.set_mode(Some(WhatAmI::Peer));
    c
}

/// Client config with NO connect endpoints and multicast scouting ENABLED.
/// Only used for Group C tests that exercise the `scouting/timeout` path.
///
/// NOTE: Group C tests assume no other Zenoh router is reachable via the
/// default multicast group on the test host.  They may give false results
/// in an environment with active Zenoh instances on the same network.
fn client_no_connect_multicast() -> Config {
    let mut c = Config::default();
    // Do NOT set scouting.multicast.enabled = false here.
    let _ = c.set_mode(Some(WhatAmI::Client));
    c
}

// ═════════════════════════════════════════════════════════════════════════════
// Group A: With connect/endpoints, router IS available
// ═════════════════════════════════════════════════════════════════════════════

/// A1 – Peer with default settings and an available router.
///
/// `open()` should succeed well within `FAST_OPEN_MAX`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a1_connect_router_available_default() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let router = ztimeout!(zenoh::open(router_config(EP_A1))).unwrap();

    let start = std::time::Instant::now();
    let peer = ztimeout!(zenoh::open(peer_with_connect(EP_A1))).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < FAST_OPEN_MAX,
        "open() with available router took {elapsed:?}, expected < {FAST_OPEN_MAX:?}",
    );

    peer.close().await.unwrap();
    router.close().await.unwrap();
    Ok(())
}

/// A2 – `connect_configured = false` with an available router.
///
/// `open()` must NOT wait for the connection to complete and should therefore
/// return in under `IMMEDIATE_MAX`, well before `scouting/delay` (500 ms)
/// would normally fire.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a2_connect_router_available_no_wait() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let router = ztimeout!(zenoh::open(router_config(EP_A2))).unwrap();

    let mut c = peer_with_connect(EP_A2);
    c.insert_json5("open/return_conditions/connect_configured", "false")
        .unwrap();

    let start = std::time::Instant::now();
    let peer = ztimeout!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < IMMEDIATE_MAX,
        "open() with connect_configured=false took {elapsed:?}, expected < {IMMEDIATE_MAX:?}",
    );

    peer.close().await.unwrap();
    router.close().await.unwrap();
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Group B: With connect/endpoints, router NOT available (Peer mode)
//
// Peer mode uses `connect_peers_multiply_links` (single_link=false), in which
// `exit_on_failure` is checked even in the one-shot path (timeout_ms=0).
// This is different from Client mode (`connect_peers_single_link`, single_link=true)
// where `exit_on_failure` is only consulted when there are endpoints queued for retry.
// ═════════════════════════════════════════════════════════════════════════════

/// B1 – `timeout_ms = 0` (one-shot), `exit_on_failure = true`, no router.
///
/// One-shot TCP attempt fails immediately (connection refused).  Because
/// `exit_on_failure = true` is checked in the per-endpoint one-shot path for
/// peer mode, `open()` returns `Err` nearly instantly.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b1_connect_no_router_oneshot_exit() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = peer_with_connect(EP_B);
    c.connect.timeout_ms = Some(ModeDependentValue::Unique(0i64));
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(true));

    let start = std::time::Instant::now();
    let result = zenoh::open(c).await;
    let elapsed = start.elapsed();

    assert!(
        result.is_err(),
        "open() should return Err with timeout_ms=0/exit_on_failure=true and no router",
    );
    assert!(
        elapsed < IMMEDIATE_FAIL_MAX,
        "open() should fail fast, but took {elapsed:?}",
    );
    Ok(())
}

/// B2 – `timeout_ms = 0` (one-shot), `exit_on_failure = false`, no router.
///
/// One-shot attempt fails, but `exit_on_failure = false` means the failure is
/// silently swallowed and `open()` returns `Ok`.  With `connect_configured = true`
/// (default), `open()` then waits up to `scouting/delay` (500 ms) for the
/// connection to be established — which it never is — before returning.
///
/// Expected: `Ok` after approximately `scouting/delay` ≈ 500 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b2_connect_no_router_oneshot_no_exit() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = peer_with_connect(EP_B);
    c.connect.timeout_ms = Some(ModeDependentValue::Unique(0i64));
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(false));

    let start = std::time::Instant::now();
    let session = ztimeout_slow!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    // Should NOT return before the scouting delay has elapsed.
    assert!(
        elapsed >= Duration::from_millis(SCOUTING_DELAY_MS - 150),
        "open() returned too fast ({elapsed:?}); expected scouting delay ≥ {SCOUTING_DELAY_MS} ms",
    );
    assert!(
        elapsed < Duration::from_millis(SCOUTING_FAILURE_MAX),
        "open() took too long: {elapsed:?}",
    );

    session.close().await.unwrap();
    Ok(())
}

/// B3 – `timeout_ms = 0`, `exit_on_failure = false`, `connect_configured = false`.
///
/// Same as B2 but `connect_configured = false` tells `open()` NOT to wait for
/// the connection to be established.  `open()` should return `Ok` immediately,
/// well before `scouting/delay`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b3_connect_no_router_oneshot_no_exit_no_wait() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = peer_with_connect(EP_B);
    c.connect.timeout_ms = Some(ModeDependentValue::Unique(0i64));
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(false));
    c.insert_json5("open/return_conditions/connect_configured", "false")
        .unwrap();

    let start = std::time::Instant::now();
    let session = ztimeout!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < IMMEDIATE_FAIL_MAX,
        "open() with connect_configured=false took {elapsed:?}, expected < {IMMEDIATE_FAIL_MAX:?}",
    );

    session.close().await.unwrap();
    Ok(())
}

/// B4 – `timeout_ms = 500` (global timeout), `exit_on_failure = true`, no router.
///
/// Because `timeout_ms > 0`, `connect_peers` wraps the connection loop in a
/// `tokio::time::timeout(500 ms, ...)`.  With `exit_on_failure = true` the inner
/// loop retries synchronously; the 500 ms outer timeout then fires and `open()`
/// propagates the resulting `Err`.
///
/// Expected: `Err` after approximately 500 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b4_connect_no_router_timeout_exit() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = peer_with_connect(EP_B);
    c.connect.timeout_ms = Some(ModeDependentValue::Unique(SHORT_MS));
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(true));

    let start = std::time::Instant::now();
    let result = ztimeout_slow!(zenoh::open(c));
    let elapsed = start.elapsed();

    assert!(
        result.is_err(),
        "open() should return Err when timeout_ms={SHORT_MS}/exit=true and no router",
    );
    assert!(
        elapsed >= SHORT_MIN,
        "open() with timeout_ms={SHORT_MS} returned too fast ({elapsed:?}), expected ≥ {SHORT_MIN:?}",
    );
    assert!(
        elapsed < SHORT_MAX,
        "open() with timeout_ms={SHORT_MS} took too long: {elapsed:?}",
    );
    Ok(())
}

/// B5 – `timeout_ms = 500`, `exit_on_failure = false`, no router.
///
/// With `exit_on_failure = false` the inner loop spawns a background connector
/// and returns immediately — the 500 ms global timeout never bites the spawn.
/// `open()` then waits up to `scouting/delay` (500 ms) for the background
/// connector to signal success before returning `Ok`.
///
/// Expected: `Ok` after approximately `scouting/delay` ≈ 500 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b5_connect_no_router_timeout_no_exit() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = peer_with_connect(EP_B);
    c.connect.timeout_ms = Some(ModeDependentValue::Unique(SHORT_MS));
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(false));

    let start = std::time::Instant::now();
    let session = ztimeout_slow!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(SCOUTING_DELAY_MS - 150),
        "open() returned too fast ({elapsed:?}); expected scouting delay ≥ {SCOUTING_DELAY_MS} ms",
    );
    assert!(
        elapsed < Duration::from_millis(SCOUTING_FAILURE_MAX),
        "open() took too long: {elapsed:?}",
    );

    session.close().await.unwrap();
    Ok(())
}

/// B6 – `timeout_ms = -1` (retry forever), `exit_on_failure = false`,
///       `connect_configured = false`.
///
/// The background connector keeps retrying indefinitely, but
/// `connect_configured = false` means `open()` does not wait for it and returns
/// immediately.
///
/// Expected: `Ok` immediately (well below `scouting/delay`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b6_connect_no_router_forever_no_wait() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = peer_with_connect(EP_B);
    // Peer default is already timeout_ms=-1, exit_on_failure=false; be explicit.
    c.connect.timeout_ms = Some(ModeDependentValue::Unique(-1i64));
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(false));
    c.insert_json5("open/return_conditions/connect_configured", "false")
        .unwrap();

    let start = std::time::Instant::now();
    let session = ztimeout!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < IMMEDIATE_FAIL_MAX,
        "open() with connect_configured=false took {elapsed:?}, expected < {IMMEDIATE_FAIL_MAX:?}",
    );

    session.close().await.unwrap();
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Group C: No connect/endpoints – scouting-based discovery (Client mode,
//          multicast scouting ENABLED)
//
// IMPORTANT: These tests assume no Zenoh router is reachable via the default
// multicast group (224.0.0.224:7446) on the test host.  They may give false
// results in an environment with active Zenoh instances on the same network.
// ═════════════════════════════════════════════════════════════════════════════

/// C1 – `scouting/timeout = 500`, no router reachable via multicast.
///
/// `open()` launches multicast scouting wrapped in a 500 ms deadline.  No
/// router responds, so `connect_first` fires `bail!("timeout")` and `open()`
/// returns `Err` after approximately 500 ms.
///
/// Fragile: will pass `Ok` and fail the `is_err` assertion if a Zenoh router
/// happens to be reachable via multicast on the test machine.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c1_scout_no_router_short_timeout() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = client_no_connect_multicast();
    c.insert_json5("scouting/timeout", &SHORT_MS.to_string())
        .unwrap();

    let start = std::time::Instant::now();
    let result = ztimeout_slow!(zenoh::open(c));
    let elapsed = start.elapsed();

    assert!(
        result.is_err(),
        "open() should return Err when no router found within scouting/timeout={SHORT_MS} ms \
         (if this fails a Zenoh router may be running on the local network)",
    );
    assert!(
        elapsed >= SHORT_MIN,
        "open() with scouting/timeout={SHORT_MS} returned too fast ({elapsed:?}), expected ≥ {SHORT_MIN:?}",
    );
    assert!(
        elapsed < SHORT_MAX,
        "open() with scouting/timeout={SHORT_MS} took too long: {elapsed:?}",
    );
    Ok(())
}

/// C2 – `scouting/timeout = -1` (background scouting), `connect_scouted = true`
///       (default), no router.
///
/// When `scouting/timeout = -1`, `connect_first` is spawned in the background.
/// With `connect_scouted = true` (default), `open()` then waits up to
/// `scouting/delay` (500 ms) for a scouted connection before returning `Ok`.
///
/// Expected: `Ok` after approximately `scouting/delay` ≈ 500 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c2_scout_no_router_background_with_wait() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = client_no_connect_multicast();
    c.insert_json5("scouting/timeout", "-1").unwrap();
    // connect_scouted defaults to true — open() waits for scouting/delay.

    let start = std::time::Instant::now();
    let session = ztimeout_slow!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    // Should have waited roughly the scouting delay.
    assert!(
        elapsed >= Duration::from_millis(SCOUTING_DELAY_MS - 150),
        "open() returned too fast ({elapsed:?}); expected scouting delay ≥ {SCOUTING_DELAY_MS} ms",
    );
    assert!(
        elapsed < Duration::from_millis(SCOUTING_FAILURE_MAX),
        "open() took too long: {elapsed:?}",
    );

    session.close().await.unwrap();
    Ok(())
}

/// C3 – `scouting/timeout = -1`, `connect_scouted = false`.
///
/// With `connect_scouted = false`, `open()` does not wait for the background
/// scouting to find anything and returns `Ok` immediately, regardless of whether
/// a router is reachable.
///
/// Expected: `Ok` immediately (well below `scouting/delay`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c3_scout_background_connect_scouted_false() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let mut c = client_no_connect_multicast();
    c.insert_json5("scouting/timeout", "-1").unwrap();
    c.insert_json5("open/return_conditions/connect_scouted", "false")
        .unwrap();

    let start = std::time::Instant::now();
    let session = ztimeout!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < IMMEDIATE_MAX,
        "open() with connect_scouted=false took {elapsed:?}, expected < {IMMEDIATE_MAX:?}",
    );

    session.close().await.unwrap();
    Ok(())
}

/// C4 – `scouting/timeout = 5000` ms, router available via configured endpoint.
///
/// Although the scouting timeout is generous, an explicit `connect/endpoints`
/// entry also points to a running router, so the TCP connection is established
/// and `open()` succeeds well before the scouting timeout would expire.
///
/// Expected: `Ok` quickly (well under `FAST_OPEN_MAX`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c4_scout_router_found_via_endpoint_before_timeout() -> zenoh::Result<()> {
    zenoh::init_log_from_env_or("error");

    let router = ztimeout!(zenoh::open(router_config(EP_C))).unwrap();

    let mut c = Config::default();
    c.connect
        .endpoints
        .set(vec![EP_C.parse::<EndPoint>().unwrap()])
        .unwrap();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    let _ = c.set_mode(Some(WhatAmI::Client));
    c.insert_json5("scouting/timeout", "5000").unwrap();
    c.connect.exit_on_failure = Some(ModeDependentValue::Unique(true));

    let start = std::time::Instant::now();
    let session = ztimeout!(zenoh::open(c)).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < FAST_OPEN_MAX,
        "open() with available router took {elapsed:?}, expected < {FAST_OPEN_MAX:?}",
    );

    session.close().await.unwrap();
    router.close().await.unwrap();
    Ok(())
}
