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

//! Regression test for subscriber declarations being echoed back into a UDP multicast group.
//!
//! Two peers share a UDP multicast transport. Each peer has a local client attached over TCP,
//! and both clients subscribe to the same key expression. A peer must not re-send into the group
//! a `DeclareSubscriber` it received from that group: otherwise every peer with a matching
//! subscriber echoes it, and the declaration ping-pongs forever (thousands of datagrams per
//! second). See eclipse-zenoh/zenoh#2817.
//!
//! The re-propagation to multicast groups only exists on non-Windows targets.
#![cfg(not(target_os = "windows"))]

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::{Duration, Instant},
};

use socket2::{Domain, Protocol, Socket, Type};
use zenoh::{Config, Session};
use zenoh_core::ztimeout;
use zenoh_test::{get_free_udp_port, get_tcp_locator};

const TIMEOUT: Duration = Duration::from_secs(60);
const JOIN_INTERVAL_MS: u64 = 500;
// Long enough for both peers to receive each other's JOIN before the subscribers are declared.
const DISCOVERY: Duration = Duration::from_millis(4 * JOIN_INTERVAL_MS);
const SETTLE: Duration = Duration::from_secs(3);
const OBSERVE: Duration = Duration::from_secs(6);
// In steady state the group only carries a few JOIN and KEEP_ALIVE messages per peer and
// second. The declaration storm produces thousands of datagrams per second.
const MAX_STEADY_STATE_DATAGRAMS: usize = 200;

/// Opens a socket that receives every datagram sent to `group`, bound the same way
/// zenoh binds its own multicast socket (shared address and port).
fn multicast_observer(group: SocketAddrV4) -> std::net::UdpSocket {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
    socket.set_reuse_address(true).unwrap();
    socket.set_reuse_port(true).unwrap();
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, group.port())).into())
        .unwrap();
    socket
        .join_multicast_v4(group.ip(), &Ipv4Addr::UNSPECIFIED)
        .unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    socket.into()
}

/// Counts the datagrams received on `socket` during `duration`.
fn count_datagrams(socket: &std::net::UdpSocket, duration: Duration) -> usize {
    let mut buf = vec![0u8; u16::MAX as usize];
    let mut count = 0;
    let start = Instant::now();
    while start.elapsed() < duration {
        if socket.recv(&mut buf).is_ok() {
            count += 1;
        }
    }
    count
}

fn peer_config(group: SocketAddrV4) -> Config {
    let mut config = Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config
        .insert_json5(
            "listen/endpoints",
            &format!(r#"["udp/{group}", "tcp/127.0.0.1:0"]"#),
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
async fn multicast_subscriber_declarations_are_not_echoed() {
    zenoh::init_log_from_env_or("error");
    let key_expr = "test/multicast/declare_echo";
    let group = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 1), get_free_udp_port());
    println!("Observing multicast group {group}");
    let observer = multicast_observer(group);

    let peer01 = ztimeout!(zenoh::open(peer_config(group))).unwrap();
    let peer02 = ztimeout!(zenoh::open(peer_config(group))).unwrap();
    let client01 = open_client(&peer01).await;
    let client02 = open_client(&peer02).await;
    // The echo only happens once the peers know each other on the multicast group.
    tokio::time::sleep(DISCOVERY).await;

    let _sub01 = ztimeout!(client01.declare_subscriber(key_expr)).unwrap();
    let _sub02 = ztimeout!(client02.declare_subscriber(key_expr)).unwrap();
    tokio::time::sleep(SETTLE).await;

    let datagrams = tokio::task::spawn_blocking(move || count_datagrams(&observer, OBSERVE))
        .await
        .unwrap();
    println!("Observed {datagrams} datagrams in {OBSERVE:?} on {group}");

    // The observer must see the group traffic at all (JOIN messages are sent periodically),
    // otherwise the upper bound below would pass trivially.
    assert!(
        datagrams > 0,
        "The observer did not receive any datagram on {group}"
    );
    assert!(
        datagrams <= MAX_STEADY_STATE_DATAGRAMS,
        "{datagrams} datagrams in {OBSERVE:?} on {group}: subscriber declarations are echoed \
         back into the multicast group (expected at most {MAX_STEADY_STATE_DATAGRAMS})"
    );

    for session in [client01, client02, peer01, peer02] {
        ztimeout!(session.close()).unwrap();
    }
}
