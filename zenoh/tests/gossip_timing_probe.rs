//
// Copyright (c) 2026 Oliver Harley
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Oliver Harley
//

//! Reproducer for an **upstream** session-close hang. Asserts nothing; prints
//! timings. `#[ignore]`d — run it deliberately:
//!
//! ```text
//! cargo test -p zenoh --features unstable --test gossip_timing_probe -- --nocapture --ignored
//! PROBE_OPEN_TIMEOUT=1 cargo test ... --ignored     # the kill-test
//! ```
//!
//! ## What it shows
//!
//! With gossip off, every open and close is sub-20ms. With gossip on, **opens
//! are unchanged** and closes pin at exactly 10.00s and fail. So nothing is
//! *spending* 20 seconds — a task is not cancelling, and `close()` is giving up
//! at its own bound.
//!
//! ## The chain, verified
//!
//! 1. Gossip discovers a peer; peer-mode autoconnect fires
//!    (`orchestrator.rs:866`, `spawn_peer_connector`). A client never gossips
//!    and a router autoconnects to nothing, which is why only peer-mode
//!    topologies with something to discover are affected.
//! 2. It uses `TaskController::spawn` (`orchestrator.rs:878`), which does **not**
//!    wrap the future in a select on the cancellation token — cancellation is
//!    opt-in per task (`zenoh-task/src/lib.rs:95-104`).
//! 3. Inside, the retry *sleep* is cancellable (`orchestrator.rs:907-916`) but
//!    the connect attempt — `open_transport_unicast` at `orchestrator.rs:942` —
//!    is not. It runs to `transport.unicast.open_timeout`, default **10000ms**.
//! 4. `terminate_all_async` (`zenoh-task/src/lib.rs:142-146`) awaits
//!    `tracker.wait()` with no bound of its own, so close blocks on that task.
//! 5. `CloseBuilder`'s own 10s (`close.rs:58`, applied at `:132-137`) expires
//!    first → `"close operation timed out"`.
//!
//! Step 3 is the defect; the rest is why it surfaces where it does.
//!
//! ## Why the failures are *faster* than the passes
//!
//! ~12s failing versus ~20.4s passing looks backwards until you see it is a
//! timeout: a run that gives up at 10s is quicker than one that waits for the
//! task to finish on its own. That inversion is the signature.
//!
//! ## The kill-test
//!
//! `PROBE_OPEN_TIMEOUT=1` sets `open_timeout` to 2000ms with gossip still on.
//! Every close then succeeds and the hang tracks the new value —
//! measured 1.86s / 2.74s / 2.02s against >10s. Direct causal confirmation
//! rather than arithmetic that happens to fit.
//!
//! Loopback only, multicast off, so no firewall prompt.

#![cfg(feature = "unstable")]

use std::time::{Duration, Instant};

use zenoh::Config;

fn cfg(listen: Option<&str>, connect: Option<&str>, gossip: bool) -> Config {
    let mut c = Config::default();
    // Exactly what TestSessions does...
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    // ...plus the one variable under test.
    c.scouting.gossip.set_enabled(Some(gossip)).unwrap();
    // Kill-test: if the stuck task is blocked in `open_transport_unicast`
    // (orchestrator.rs:942, which is NOT wrapped in a select on the
    // cancellation token), then shortening this must shorten the hang.
    if std::env::var("PROBE_OPEN_TIMEOUT").is_ok() {
        c.transport.unicast.set_open_timeout(2000).unwrap();
    }
    if let Some(l) = listen {
        c.listen.endpoints.set(vec![l.parse().unwrap()]).unwrap();
    }
    if let Some(x) = connect {
        c.connect.endpoints.set(vec![x.parse().unwrap()]).unwrap();
    }
    c
}

/// open + close a two-peer topology, timing each phase separately.
async fn probe(gossip: bool) {
    let t = Instant::now();
    let listener = zenoh::open(cfg(Some("tcp/127.0.0.1:0"), None, gossip))
        .await
        .unwrap();
    let open_listener = t.elapsed();

    let ep = zenoh_test::get_tcp_locator(&listener).await;

    let t = Instant::now();
    let connector = zenoh::open(cfg(None, Some(&ep.to_string()), gossip))
        .await
        .unwrap();
    let open_connector = t.elapsed();

    // A second connector into the now-standing topology. If the cost is paid at
    // topology *formation*, this is cheap; if it is per-participant, it is not.
    let t = Instant::now();
    let connector2 = zenoh::open(cfg(None, Some(&ep.to_string()), gossip))
        .await
        .unwrap();
    let open_connector2 = t.elapsed();

    // Deliberately not unwrapped: a close that times out is the thing being
    // measured, so it must be reported rather than panicked on.
    let t = Instant::now();
    let r1 = connector.close().await;
    let close_connector = t.elapsed();

    let t = Instant::now();
    let r2 = connector2.close().await;
    let close_connector2 = t.elapsed();

    let t = Instant::now();
    let r3 = listener.close().await;
    let close_listener = t.elapsed();

    let ok = |r: &zenoh::Result<()>| if r.is_ok() { "ok" } else { "TIMEOUT" };
    println!(
        "                close outcomes: conn1 {}  conn2 {}  listener {}",
        ok(&r1),
        ok(&r2),
        ok(&r3)
    );

    println!(
        "gossip={gossip:<5}  open[listener {:>7.2?}  conn1 {:>7.2?}  conn2 {:>7.2?}]  \
         close[conn1 {:>7.2?}  conn2 {:>7.2?}  listener {:>7.2?}]",
        open_listener,
        open_connector,
        open_connector2,
        close_connector,
        close_connector2,
        close_listener,
    );
}

/// The same hang with **gossip off entirely** — the minimal form.
///
/// `peers_connector_retry` is not gossip-specific. It is reached from the
/// configured `connect.endpoints` path (`orchestrator.rs:426`) as well as from
/// gossip (`:994`). So gossip is a *trigger*, not the condition: any session
/// closing while a configured connect is in flight to an unresponsive endpoint
/// inherits the same 10s stall.
///
/// The black hole is a plain `TcpListener` that accepts and then says nothing.
/// That matters — a *refused* connection returns immediately and the retry
/// sleep that follows is cancellable. It is the accepted-but-silent case that
/// parks inside `open_transport_unicast` until `open_timeout`.
/// Does `spawn_add_listener` actually hang close, or does it only look like it?
///
/// `spawn_add_listener` (`orchestrator.rs`) spawns `add_listener_retry`, a bare
/// `loop { if add_listener().await.is_ok() { break } sleep(period).await }` with
/// **no cancellation token at all** — so unlike `peers_connector_retry` it has
/// not even a partial guard. If that is a real instance of the bug, a listener
/// that can never bind should block close.
///
/// `tcp/8.8.8.8:8` is unbindable locally: the kernel rejects it with
/// `EADDRNOTAVAIL` without a packet leaving the machine. Deterministic, offline,
/// and no firewall prompt. `exit_on_failure: false` is what selects the
/// spawn-and-retry branch rather than failing the open.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "diagnostic reproducer for an upstream close hang; asserts nothing, run deliberately"]
async fn does_the_listener_retry_site_hang_close() {
    zenoh_util::init_log_from_env_or("error");

    let mut c = Config::default();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    c.scouting.gossip.set_enabled(Some(false)).unwrap();
    c.listen
        .endpoints
        .set(vec!["tcp/8.8.8.8:8".parse().unwrap()])
        .unwrap();
    c.listen
        .set_exit_on_failure(Some(zenoh_config::ModeDependentValue::Unique(false)))
        .unwrap();
    // Load-bearing, and the reason a naive attempt at this shows nothing:
    // `listen.timeout_ms` defaults to **0**, so `add_listener_retry` gives up
    // immediately and there is no loop left to hang on. -1 is infinite retry,
    // which is what an operator sets for a listener expected to come back.
    c.listen
        .set_timeout_ms(Some(zenoh_config::ModeDependentValue::Unique(-1)))
        .unwrap();

    let t = Instant::now();
    let session = match zenoh::open(c).await {
        Ok(s) => s,
        Err(e) => {
            println!("\nopen refused ({e}) - exit_on_failure did not take; test inconclusive\n");
            return;
        }
    };
    let opened = t.elapsed();

    let t = Instant::now();
    let r = session.close().await;
    println!(
        "\nunbindable listener, exit_on_failure=false:  open {:>9.2?}  close {:>9.2?}  {}\n\
         (a close in the seconds means `spawn_add_listener` is a real instance;\n \
          a close in milliseconds means it is not, and the site is pattern-match only)\n",
        opened,
        t.elapsed(),
        if r.is_ok() { "ok" } else { "TIMEOUT" }
    );
}

/// Measures the **bind-before-accept window** at listener startup.
///
/// `zenoh-link-tcp/src/unicast.rs`: `new_listener` binds *and listens* (`:314`),
/// but the `accept_task` is only constructed (`:326-332`) and registered
/// (`:334-337`) afterwards. Between those, the kernel completes TCP handshakes
/// into the backlog and nothing answers them — structurally the same condition
/// as the black hole, arising from ordinary startup rather than fault injection.
///
/// The question is its **width**. If sub-millisecond it is irrelevant beside a
/// 10 s `open_timeout`; if wide, the close hang is reachable during any rolling
/// restart.
///
/// Method: race a real client against listener startup and time the client's
/// `open()`. A client that lands in the window would park in
/// `open_transport_unicast` and show ~`open_timeout`; one that misses it shows
/// milliseconds. Repeated, because it is a race.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "diagnostic reproducer for an upstream close hang; asserts nothing, run deliberately"]
async fn bind_before_accept_window() {
    zenoh_util::init_log_from_env_or("error");

    println!("\n--- client racing listener startup (10 rounds) ---");
    let mut worst = Duration::ZERO;

    for round in 0..10 {
        // A port nothing holds. The TOCTOU window here is the point: we want the
        // client attempting while the listener is still coming up.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let addr = format!("tcp/127.0.0.1:{port}");

        let mut lc = Config::default();
        lc.scouting.multicast.set_enabled(Some(false)).unwrap();
        lc.scouting.gossip.set_enabled(Some(false)).unwrap();
        lc.listen
            .endpoints
            .set(vec![addr.parse().unwrap()])
            .unwrap();

        let mut cc = Config::default();
        cc.scouting.multicast.set_enabled(Some(false)).unwrap();
        cc.scouting.gossip.set_enabled(Some(false)).unwrap();
        cc.connect
            .endpoints
            .set(vec![addr.parse().unwrap()])
            .unwrap();

        // Client first, so it is already retrying when the listener binds.
        let client_task = tokio::spawn(async move {
            let t = Instant::now();
            let s = zenoh::open(cc).await;
            (t.elapsed(), s.is_ok())
        });

        let listener = zenoh::open(lc).await.unwrap();
        let (client_elapsed, ok) = client_task.await.unwrap();
        worst = worst.max(client_elapsed);
        println!(
            "round {round}: client open {:>9.2?}  {}",
            client_elapsed,
            if ok { "ok" } else { "FAILED" }
        );

        let _ = listener.close().await;
    }

    println!("worst client open: {worst:.2?}");
    println!(
        "(>= ~10s would mean the window is wide enough to park a peer for open_timeout;\n \
         milliseconds means the window is real but too narrow to matter)\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "diagnostic reproducer for an upstream close hang; asserts nothing, run deliberately"]
async fn the_hang_without_gossip() {
    zenoh_util::init_log_from_env_or("error");

    let sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = sink.local_addr().unwrap();
    std::thread::spawn(move || {
        // Hold every accepted connection open and never speak zenoh.
        let mut held = Vec::new();
        for s in sink.incoming() {
            match s {
                Ok(s) => held.push(s),
                Err(_) => break,
            }
        }
    });

    println!("\n--- gossip OFF, configured unresponsive endpoint(s) ---");
    // `n` copies of the same black hole: does the stall stack across endpoints,
    // or do the connector tasks overlap? Severity depends on the answer — a
    // real node often has several configured peers.
    for (open_timeout, n) in [(10_000u64, 1usize), (2_000, 1), (2_000, 3)] {
        let mut c = Config::default();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.scouting.gossip.set_enabled(Some(false)).unwrap();
        c.connect
            .endpoints
            .set(
                (0..n)
                    .map(|_| format!("tcp/{addr}").parse().unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        c.transport.unicast.set_open_timeout(open_timeout).unwrap();

        let t = Instant::now();
        let opened = tokio::time::timeout(Duration::from_secs(30), zenoh::open(c)).await;
        let open_elapsed = t.elapsed();
        let Ok(Ok(session)) = opened else {
            println!("open_timeout={open_timeout:>6}ms n={n}  open did not return in 30s ({open_elapsed:.2?}) - different shape, see header");
            continue;
        };

        let t = Instant::now();
        let r = session.close().await;
        println!(
            "open_timeout={open_timeout:>6}ms  endpoints={n}  open {:>9.2?}  close {:>9.2?}  {}",
            open_elapsed,
            t.elapsed(),
            if r.is_ok() { "ok" } else { "TIMEOUT" }
        );
    }
    println!();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "diagnostic reproducer for an upstream close hang; asserts nothing, run deliberately"]
async fn where_does_the_time_go() {
    zenoh_util::init_log_from_env_or("error");

    println!("\n--- gossip OFF (control) ---");
    for _ in 0..3 {
        probe(false).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    println!("\n--- gossip ON ---");
    for _ in 0..3 {
        probe(true).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!();
}
