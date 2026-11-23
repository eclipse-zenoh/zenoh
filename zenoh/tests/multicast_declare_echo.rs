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
use zenoh_codec::{RCodec, Zenoh080};
use zenoh_core::ztimeout;
use zenoh_protocol::{
    network::{declare::DeclareBody, Declare, NetworkBody},
    transport::{TransportBody, TransportMessage},
};
use zenoh_test::{get_free_udp_port, get_tcp_locator};

const TIMEOUT: Duration = Duration::from_secs(60);
const JOIN_INTERVAL_MS: u64 = 500;
const OBSERVE: Duration = Duration::from_secs(6);

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
        .join_multicast_v4(group.ip(), &Ipv4Addr::LOCALHOST)
        .unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    socket.into()
}

fn observe_declarations(socket: &std::net::UdpSocket, key_expr: &str) -> Vec<SocketAddr> {
    let mut buf = vec![0u8; u16::MAX as usize];
    let mut sources = Vec::new();
    let start = Instant::now();
    while start.elapsed() < OBSERVE && sources.len() <= 2 {
        let Ok((len, source)) = socket.recv_from(&mut buf) else {
            continue;
        };
        let mut datagram = &buf[..len];
        while !datagram.is_empty() {
            let message: TransportMessage = Zenoh080::new().read(&mut datagram).unwrap();
            if let TransportBody::Frame(frame) = message.body {
                sources.extend(frame.payload.iter().filter_map(|message| {
                    matches!(
                        &message.body,
                        NetworkBody::Declare(Declare {
                            body: DeclareBody::DeclareSubscriber(subscriber),
                            ..
                        }) if subscriber.wire_expr.scope == 0 && subscriber.wire_expr.suffix == key_expr
                    )
                    .then_some(source)
                }));
            }
        }
    }
    sources
}

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
    let (zid01, zid02) = (peer01.zid(), peer02.zid());
    ztimeout!(async {
        while !peer01.info().transports().await.any(|t| t.zid() == &zid02)
            || !peer02.info().transports().await.any(|t| t.zid() == &zid01)
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });

    let _sub01 = ztimeout!(client01.declare_subscriber(key_expr)).unwrap();
    let _sub02 = ztimeout!(client02.declare_subscriber(key_expr)).unwrap();

    let sources = tokio::task::spawn_blocking(move || observe_declarations(&observer, key_expr))
        .await
        .unwrap();
    assert!(
        matches!(sources.as_slice(), [first, second] if first != second),
        "Expected one subscriber declaration from each peer on {group}, got {sources:?}"
    );

    for session in [client01, client02, peer01, peer02] {
        ztimeout!(session.close()).unwrap();
    }
}
