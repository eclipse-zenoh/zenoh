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
//! A router that restarts with the same zid must get the other routers' declarations again.
//!
//! R1 and R2 are linked, E is linked to both, and a client of E subscribes. R1 is closed and opened again at once with
//! the same zid on the same port (like a container restart), so E's old link to R1 goes and the new one comes before
//! E computes its trees again, and the trees look unchanged. Then a client of R1 publishes.
#![cfg(feature = "unstable")]

use std::time::Duration;

use zenoh::{handlers::FifoChannelHandler, pubsub::Subscriber, sample::Sample, Session};
use zenoh_config::{Config, WhatAmI};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const KEY: &str = "demo/cmd/d1";
const SUB_KEY: &str = "demo/cmd/**";

/// Each round loses the declarations about half of the time without the fix.
const ROUNDS: usize = 8;

fn port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn config(mode: WhatAmI, id: Option<&str>, listen: Option<u16>, connect: &[u16]) -> Config {
    let mut c = Config::default();
    c.set_mode(Some(mode)).unwrap();
    if let Some(id) = id {
        c.insert_json5("id", &format!("\"{id}\"")).unwrap();
    }
    let listen: Vec<String> = listen
        .map(|p| format!("tcp/127.0.0.1:{p}"))
        .into_iter()
        .collect();
    c.insert_json5("listen/endpoints", &serde_json::to_string(&listen).unwrap())
        .unwrap();
    let connect: Vec<String> = connect
        .iter()
        .map(|p| format!("tcp/127.0.0.1:{p}"))
        .collect();
    c.insert_json5(
        "connect/endpoints",
        &serde_json::to_string(&connect).unwrap(),
    )
    .unwrap();
    // Reconnect at once, so that the old link goes and the new one comes within one tree computation delay.
    c.insert_json5(
        "connect/retry",
        r#"{ period_init_ms: 1, period_max_ms: 50, period_increase_factor: 1.0 }"#,
    )
    .unwrap();
    c.scouting.multicast.set_enabled(Some(false)).unwrap();
    c.scouting.gossip.set_enabled(Some(false)).unwrap();
    c
}

fn drain(sub: &Subscriber<FifoChannelHandler<Sample>>) -> usize {
    let mut n = 0;
    while let Ok(Some(_)) = sub.try_recv() {
        n += 1;
    }
    n
}

async fn publish_from(port: u16) -> Session {
    let c = ztimeout!(zenoh::open(config(WhatAmI::Client, None, None, &[port]))).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    for i in 0..10 {
        ztimeout!(c.put(KEY, format!("{i}"))).unwrap();
    }
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_router_restarted_with_the_same_zid_gets_our_subscriptions_again() {
    zenoh::init_log_from_env_or("error");
    let (p1, p2) = (port(), port());
    let r1cfg = || config(WhatAmI::Router, Some("a1"), Some(p1), &[]);
    let mut r1 = ztimeout!(zenoh::open(r1cfg())).unwrap();
    let r2 = ztimeout!(zenoh::open(config(
        WhatAmI::Router,
        Some("a2"),
        Some(p2),
        &[p1]
    )))
    .unwrap();
    // E: connected to R1 and R2, with a random zid; a client of E subscribes.
    let pe = port();
    let e = ztimeout!(zenoh::open(config(
        WhatAmI::Router,
        None,
        Some(pe),
        &[p1, p2]
    )))
    .unwrap();
    let d = ztimeout!(zenoh::open(config(WhatAmI::Client, None, None, &[pe]))).unwrap();
    let sub = ztimeout!(d.declare_subscriber(SUB_KEY)).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    let c = publish_from(p1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(drain(&sub), 10, "before any restart");
    ztimeout!(c.close()).unwrap();

    let mut lost = Vec::new();
    for round in 1..=ROUNDS {
        // The same zid on the same port, back at once.
        ztimeout!(r1.close()).unwrap();
        r1 = ztimeout!(zenoh::open(r1cfg())).unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        drain(&sub);
        let c = publish_from(p1).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let received = drain(&sub);
        ztimeout!(c.close()).unwrap();
        if received < 10 {
            lost.push((round, received));
        }
    }
    for s in [d, e, r2, r1] {
        ztimeout!(s.close()).unwrap();
    }
    assert!(
        lost.is_empty(),
        "rounds where R1's clients did not reach E's subscriber (round, received of 10): {lost:?}"
    );
}
