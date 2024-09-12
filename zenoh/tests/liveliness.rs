//
// Copyright (c) 2023 ZettaScale Technology
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
#[cfg(feature = "unstable")]
use zenoh_core::ztimeout;

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_clique() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:47447";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/clique";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER1_ENDPOINT.parse::<config::EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let peer2 = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer1.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let token = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    sub.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_query_clique() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:47448";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/clique";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let peer2 = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(peer1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let get = ztimeout!(peer2.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    token.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_brokered() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47449";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/brokered";

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };

    let client1 = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(client1.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let token = ztimeout!(client2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    sub.undeclare().await.unwrap();

    router.close().await.unwrap();
    client1.close().await.unwrap();
    client2.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_query_brokered() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47450";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/brokered";

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };

    let client1 = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let get = ztimeout!(client2.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    token.undeclare().await.unwrap();

    router.close().await.unwrap();
    client1.close().await.unwrap();
    client2.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_local() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/local";

    zenoh_util::init_log_from_env_or("error");

    let peer = {
        let mut c = config::default();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let token = ztimeout!(peer.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    sub.undeclare().await.unwrap();
    peer.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_query_local() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/local";

    zenoh_util::init_log_from_env_or("error");

    let peer = {
        let mut c = config::default();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(peer.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let get = ztimeout!(peer.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    token.undeclare().await.unwrap();
    peer.close().await.unwrap();
}
