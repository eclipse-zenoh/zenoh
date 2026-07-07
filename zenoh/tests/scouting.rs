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
