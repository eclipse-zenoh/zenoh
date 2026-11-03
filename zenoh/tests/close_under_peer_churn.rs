//
// Copyright (c) 2026 ZettaScale Technology
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
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::time::timeout;
use zenoh_config::Config;
use zenoh_test::get_free_udp_port;

/// Closing a session must not wait forever on a peer connection.
///
/// Two peers that have discovered each other over multicast and then open and
/// close repeatedly used to deadlock in `Session::close`: gossip's autoconnect
/// task was spawned with `TaskController::spawn`, so the cancellation token
/// never reached it, and `terminate_all_async()` -- which has no timeout --
/// waited for a peer connection that would never complete.
///
/// A regression here HANGS rather than fails, so every close is bounded by an
/// explicit timeout and the whole test is bounded by the number of rounds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_does_not_hang_under_peer_churn() {
    zenoh::init_log_from_env_or("error");

    // A multicast group of this test's own, so it neither discovers nor is
    // discovered by anything else on the machine -- including other tests
    // running in parallel.
    let mcast_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(224, 0, 0, 224)),
        get_free_udp_port(),
    );

    // Everything except the multicast group is left at its default, and that
    // matters twice over: the peers must LISTEN for each other to connect at
    // all, and autoconnect must stay enabled because the gossip autoconnect
    // task is the thing under test. Clearing either makes this test pass
    // against the bug.
    let config = move || {
        let mut config = Config::default();
        config
            .scouting
            .multicast
            .set_enabled(Some(true))
            .unwrap();
        config
            .scouting
            .multicast
            .set_address(Some(mcast_addr))
            .unwrap();
        config
            .scouting
            .multicast
            .set_interface(Some("127.0.0.1".to_string()))
            .unwrap();
        config
    };

    // The other peers, churning alongside us. Several of them, and all of them
    // opening AND closing: a long-lived peer plus one side closing does not
    // reproduce this, and neither does a single neighbour -- there has to be a
    // connection in flight when a session goes away.
    let stop = Arc::new(AtomicBool::new(false));
    let neighbours: Vec<_> = (0..4).map(|_| tokio::spawn({
        let stop = stop.clone();
        let config = config.clone();
        async move {
            while !stop.load(Ordering::Relaxed) {
                let session = zenoh::open(config()).await.unwrap();
                // Long enough to be discovered. A session that lives a
                // millisecond is never found, and then this test proves
                // nothing -- see the assertion on the other side.
                tokio::time::sleep(Duration::from_millis(30)).await;
                let _ = session.close().await;
            }
        }
    })).collect();

    const ROUNDS: usize = 30;
    const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

    let mut rounds_with_a_peer = 0usize;
    for round in 0..ROUNDS {
        let session = zenoh::open(config()).await.unwrap();

        // WAIT FOR THE PEER. The deadlock needs a peer connection to be in
        // flight when the session closes, so a round where nobody was found
        // exercises nothing.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while tokio::time::Instant::now() < deadline {
            if session.info().peers_zid().await.next().is_some() {
                found = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if found {
            rounds_with_a_peer += 1;
        }

        // Two ways this can report the regression: `close()` returns its own
        // "close operation timed out" error, or it never returns at all. The
        // outer timeout catches the second, so a failure is a failure rather
        // than a hung CI job.
        timeout(CLOSE_TIMEOUT, session.close())
            .await
            .unwrap_or_else(|_| panic!("session close never returned on round {round}"))
            .unwrap_or_else(|e| panic!("session close failed on round {round}: {e}"));
    }

    // Guards the test against quietly becoming a no-op: if the peers never
    // find each other, nothing above can deadlock and a pass means nothing.
    assert!(
        rounds_with_a_peer > 0,
        "no round ever saw a peer, so this test exercised nothing"
    );
    eprintln!("{rounds_with_a_peer} of {ROUNDS} rounds had a peer connected");

    stop.store(true, Ordering::Relaxed);
    for n in neighbours {
        let _ = timeout(CLOSE_TIMEOUT, n).await;
    }
}
