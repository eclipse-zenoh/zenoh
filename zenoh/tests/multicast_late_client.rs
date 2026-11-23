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

//! Regression test for entities of UDP multicast peers missing from "current" interest replies.
//!
//! Two peers share a UDP multicast transport. A client attached to peer02 subscribes first. Only
//! afterwards a client connects to peer01 and declares a publisher. The publisher's initial
//! interest is answered from peer01's view of the subscribers it already knows; the subscriber
//! learned over multicast must be part of that reply, otherwise the publisher reports no
//! matching subscriber (and, with write-side filtering, may never send).
//! See eclipse-zenoh/zenoh#2818.
//!
//! Subscribers are only propagated to multicast groups on non-Windows targets.
#![cfg(not(target_os = "windows"))]

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    time::{Duration, Instant},
};

use zenoh::{Config, Session};
use zenoh_core::ztimeout;
use zenoh_test::{get_free_udp_port, get_tcp_locator};

const TIMEOUT: Duration = Duration::from_secs(60);
const JOIN_INTERVAL_MS: u64 = 500;
// Time for the subscriber declaration to reach peer01 over the multicast group.
const PROPAGATION: Duration = Duration::from_secs(1);
// How long the late publisher may take to report the existing subscriber as matching.
const MATCHING: Duration = Duration::from_secs(3);

fn peer_config(group: SocketAddrV4) -> Config {
    let mut config = Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config
        .insert_json5(
            "listen/endpoints",
            &format!(r#"["udp/{group}#iface=127.0.0.1", "tcp/127.0.0.1:0"]"#),
        )
        .unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("scouting/gossip/enabled", "false")
        .unwrap();
    config
        .insert_json5(
            "transport/multicast/join_interval",
            &JOIN_INTERVAL_MS.to_string(),
        )
        .unwrap();
    config
}

async fn open_client(peer: &Session) -> Session {
    let mut config = Config::default();
    config.insert_json5("mode", r#""client""#).unwrap();
    config
        .insert_json5(
            "connect/endpoints",
            &format!(r#"["{}"]"#, get_tcp_locator(peer).await),
        )
        .unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    ztimeout!(zenoh::open(config)).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn late_client_publisher_matches_multicast_subscriber() {
    zenoh::init_log_from_env_or("error");
    let key_expr = "test/multicast/late_client";
    let group = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 1), get_free_udp_port());

    let peer01 = ztimeout!(zenoh::open(peer_config(group))).unwrap();
    let peer02 = ztimeout!(zenoh::open(peer_config(group))).unwrap();
    let (zid01, zid02) = (peer01.zid(), peer02.zid());
    ztimeout!(async {
        while !peer01.info().transports().await.any(|t| t.zid() == &zid02)
            || !peer02.info().transports().await.any(|t| t.zid() == &zid01)
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });

    // The subscriber is declared before the publishing client exists.
    let client02 = open_client(&peer02).await;
    let _sub = ztimeout!(client02.declare_subscriber(key_expr)).unwrap();
    tokio::time::sleep(PROPAGATION).await;

    // Late client: connects to peer01 only now and publishes through a declared publisher.
    let client01 = open_client(&peer01).await;
    let publisher = ztimeout!(client01.declare_publisher(key_expr)).unwrap();
    let start = Instant::now();
    let mut matching = false;
    while !matching && start.elapsed() < MATCHING {
        matching = ztimeout!(publisher.matching_status()).unwrap().matching();
        if !matching {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    assert!(
        matching,
        "The late client's publisher does not see the subscriber known from the multicast \
         peer within {MATCHING:?}"
    );

    ztimeout!(publisher.undeclare()).unwrap();
    for session in [client01, client02, peer01, peer02] {
        ztimeout!(session.close()).unwrap();
    }
}
