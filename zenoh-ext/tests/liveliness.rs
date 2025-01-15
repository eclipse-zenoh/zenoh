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
#![cfg(feature = "unstable")]
use zenoh::{
    config::{EndPoint, WhatAmI},
    sample::SampleKind,
    Wait,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(deprecated)]
async fn test_liveliness_querying_subscriber_clique() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;
    use zenoh_ext::SubscriberBuilderExt;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "udp/localhost:47447";

    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    let token1 = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR_1)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sub = ztimeout!(peer1
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR_ALL)
        .querying())
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let token2 = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR_2)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_2);

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Delete);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    token2.undeclare().await.unwrap();
    sub.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(deprecated)]
async fn test_liveliness_querying_subscriber_brokered() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;
    use zenoh_ext::SubscriberBuilderExt;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27449";

    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    let client3 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (3) ZID: {}", s.zid());
        s
    };

    let token1 = ztimeout!(client2.liveliness().declare_token(LIVELINESS_KEYEXPR_1)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sub = ztimeout!(client1
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR_ALL)
        .querying())
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let token2 = ztimeout!(client3.liveliness().declare_token(LIVELINESS_KEYEXPR_2)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_2);

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Delete);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    token2.undeclare().await.unwrap();
    sub.undeclare().await.unwrap();

    router.close().await.unwrap();
    client1.close().await.unwrap();
    client2.close().await.unwrap();
    client3.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(deprecated)]
async fn test_liveliness_fetching_subscriber_clique() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;
    use zenoh_ext::SubscriberBuilderExt;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "udp/localhost:47449";

    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    let token1 = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR_1)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sub = ztimeout!(peer1
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR_ALL)
        .fetching(|cb| peer1
            .liveliness()
            .get(LIVELINESS_KEYEXPR_ALL)
            .callback(cb)
            .wait()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let token2 = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR_2)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_2);

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Delete);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    token2.undeclare().await.unwrap();
    sub.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(deprecated)]
async fn test_liveliness_fetching_subscriber_brokered() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;
    use zenoh_ext::SubscriberBuilderExt;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47450";

    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    let client3 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (3) ZID: {}", s.zid());
        s
    };

    let token1 = ztimeout!(client2.liveliness().declare_token(LIVELINESS_KEYEXPR_1)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sub = ztimeout!(client1
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR_ALL)
        .fetching(|cb| client1
            .liveliness()
            .get(LIVELINESS_KEYEXPR_ALL)
            .callback(cb)
            .wait()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let token2 = ztimeout!(client3.liveliness().declare_token(LIVELINESS_KEYEXPR_2)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_2);

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Delete);
    assert_eq!(sample.key_expr().as_str(), LIVELINESS_KEYEXPR_1);

    token2.undeclare().await.unwrap();
    sub.undeclare().await.unwrap();

    router.close().await.unwrap();
    client1.close().await.unwrap();
    client2.close().await.unwrap();
    client3.close().await.unwrap();
}
