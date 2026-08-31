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
    time::Duration,
};

use tokio::time::timeout;
use zenoh::config::WhatAmI;
#[cfg(all(feature = "unstable", feature = "transport_tcp"))]
use zenoh::sample::SampleKind;
use zenoh_config::{Config, ModeDependentValue, WhatAmIMatcher};
use zenoh_link::EndPoint;
#[cfg(all(feature = "unstable", feature = "transport_tcp"))]
use zenoh_test::get_free_tcp_port;
use zenoh_test::get_free_udp_port;

#[tokio::test(flavor = "multi_thread")]
async fn multicast_scouting_works_on_loopback() {
    zenoh::init_log_from_env_or("error");

    let mcast_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(224, 0, 0, 224)),
        get_free_udp_port(),
    );

    let mut responder_config = Config::default();
    responder_config.set_mode(Some(WhatAmI::Router)).unwrap();
    responder_config
        .listen
        .endpoints
        .set(Vec::<EndPoint>::new())
        .unwrap();
    responder_config
        .scouting
        .gossip
        .set_enabled(Some(false))
        .unwrap();
    responder_config
        .scouting
        .multicast
        .set_enabled(Some(true))
        .unwrap();
    responder_config
        .scouting
        .multicast
        .set_address(Some(mcast_addr))
        .unwrap();
    responder_config
        .scouting
        .multicast
        .set_interface(Some("127.0.0.1".to_string()))
        .unwrap();
    responder_config
        .scouting
        .multicast
        .set_autoconnect(Some(ModeDependentValue::Unique(WhatAmIMatcher::empty())))
        .unwrap();

    let responder = zenoh::open(responder_config).await.unwrap();
    let responder_zid = responder.zid();

    let mut scout_config = Config::default();
    scout_config
        .scouting
        .gossip
        .set_enabled(Some(false))
        .unwrap();
    scout_config
        .scouting
        .multicast
        .set_address(Some(mcast_addr))
        .unwrap();
    scout_config
        .scouting
        .multicast
        .set_interface(Some("127.0.0.1".to_string()))
        .unwrap();

    let scout = zenoh::scout(WhatAmI::Router, scout_config).await.unwrap();
    let hello = timeout(Duration::from_secs(5), scout.recv_async())
        .await
        .expect("timed out waiting for multicast scout Hello")
        .unwrap();

    assert_eq!(hello.whatami(), WhatAmI::Router);
    assert_eq!(hello.zid(), responder_zid);

    scout.stop();
    responder.close().await.unwrap();
}

#[cfg(all(feature = "unstable", feature = "transport_tcp"))]
#[tokio::test(flavor = "multi_thread")]
async fn gossip_autoconnect_works_on_loopback() {
    zenoh::init_log_from_env_or("error");

    let port_a = get_free_tcp_port();
    let port_b = get_free_tcp_port();
    let port_c = get_free_tcp_port();
    let endpoint_a = format!("tcp/127.0.0.1:{port_a}");
    let endpoint_b = format!("tcp/127.0.0.1:{port_b}");
    let endpoint_c = format!("tcp/127.0.0.1:{port_c}");

    let peer_config = |listen: &str, connect: Option<&str>| {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Peer)).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.scouting.gossip.set_enabled(Some(true)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![listen.parse::<EndPoint>().unwrap()])
            .unwrap();
        if let Some(connect) = connect {
            config
                .connect
                .endpoints
                .set(vec![connect.parse::<zenoh_config::EndPoints>().unwrap()])
                .unwrap();
        }
        config
    };

    let peer_a = zenoh::open(peer_config(&endpoint_a, None)).await.unwrap();
    let peer_a_events = peer_a
        .info()
        .transport_events_listener()
        .with(flume::bounded(32))
        .await
        .unwrap();

    let peer_b = zenoh::open(peer_config(&endpoint_b, Some(&endpoint_a)))
        .await
        .unwrap();
    let peer_b_zid = peer_b.zid();

    timeout(Duration::from_secs(5), async {
        loop {
            let event = peer_a_events.recv_async().await.unwrap();
            if event.kind() == SampleKind::Put && event.transport().zid() == &peer_b_zid {
                break;
            }
        }
    })
    .await
    .expect("timed out waiting for the initial gossip connection");

    // C only connects to A. It must discover B through gossip, including B's
    // loopback listener address.
    let peer_c = zenoh::open(peer_config(&endpoint_c, Some(&endpoint_a)))
        .await
        .unwrap();
    let peer_c_events = peer_c
        .info()
        .transport_events_listener()
        .history(true)
        .await
        .unwrap();

    timeout(Duration::from_secs(5), async {
        loop {
            let event = peer_c_events.recv_async().await.unwrap();
            if event.kind() == SampleKind::Put && event.transport().zid() == &peer_b_zid {
                break;
            }
        }
    })
    .await
    .expect("timed out waiting for gossip discovery on loopback");

    peer_c.close().await.unwrap();
    peer_b.close().await.unwrap();
    peer_a.close().await.unwrap();
}

#[cfg(all(feature = "unstable", feature = "transport_tcp"))]
#[tokio::test(flavor = "multi_thread")]
async fn multicast_autoconnect_works_on_loopback() {
    zenoh::init_log_from_env_or("error");

    let mcast_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(224, 0, 0, 224)),
        get_free_udp_port(),
    );
    let peer_config = || {
        let mut config = Config::default();
        config.set_mode(Some(WhatAmI::Peer)).unwrap();
        config.scouting.gossip.set_enabled(Some(false)).unwrap();
        config.scouting.multicast.set_enabled(Some(true)).unwrap();
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

    let peer1 = zenoh::open(peer_config()).await.unwrap();
    let peer1_events = peer1
        .info()
        .transport_events_listener()
        .with(flume::bounded(32))
        .await
        .unwrap();
    let peer2 = zenoh::open(peer_config()).await.unwrap();
    let peer2_zid = peer2.zid();

    timeout(Duration::from_secs(5), async {
        loop {
            let event = peer1_events.recv_async().await.unwrap();
            if event.kind() == SampleKind::Put && event.transport().zid() == &peer2_zid {
                break;
            }
        }
    })
    .await
    .expect("timed out waiting for loopback scout connection");

    peer2.close().await.unwrap();
    peer1.close().await.unwrap();
}
