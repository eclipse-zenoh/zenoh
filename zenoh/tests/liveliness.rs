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

/// -------------------------------------------------------
/// DOUBLE CLIENT
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47451";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47452";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/middle";

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

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47453";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/after";

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

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47454";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47455";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/middle";

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

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_history_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47456";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/after";

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

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE PEER
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47457";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47458";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/middle";

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

    let peer_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47459";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/after";

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

    let peer_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();

    let sub2 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47460";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47461";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/middle";

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

    let peer_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_history_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47462";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/after";

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

    let peer_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();

    let sub2 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE ROUTER
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47463";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:47464";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_sub = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47465";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:47466";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/middle";

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

    let router_sub = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47467";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:47468";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/after";

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

    let router_sub = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47469";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:47470";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_sub = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47471";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:47472";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/middle";

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

    let router_sub = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_history_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47473";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:47474";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/after";

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

    let router_sub = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE CLIENT VIA PEER
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47475";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:47476";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/before";

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

    let peer_dummy = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (dummy) ZID: {}", s.zid());
        s
    };

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47477";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:47478";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/middle";

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

    let peer_dummy = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (dummy) ZID: {}", s.zid());
        s
    };

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47479";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:47480";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/after";

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

    let peer_dummy = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (dummy) ZID: {}", s.zid());
        s
    };

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47481";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:47482";
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/before";

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

    let peer_dummy = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (dummy) ZID: {}", s.zid());
        s
    };

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47483";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:47484";
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/middle";

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

    let peer_dummy = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (dummy) ZID: {}", s.zid());
        s
    };

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_history_after() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47485";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:47486";
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/after";

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

    let peer_dummy = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (dummy) ZID: {}", s.zid());
        s
    };

    let client_sub = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![PEER_DUMMY_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (sub) ZID: {}", s.zid());
        s
    };

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().is_err());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().is_err());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// SUBGET CLIENT
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47487";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(client_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47488";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/middle";

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

    let client_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(client_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47489";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/history/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(client_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47490";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/history/middle";

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

    let client_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(client_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// SUBGET PEER
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47491";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47492";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/middle";

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

    let peer_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47493";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/history/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47494";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/history/middle";

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

    let peer_subget = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// SUBGET ROUTER
/// -------------------------------------------------------

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47495";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:47496";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_subget = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUBGET_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(router_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47497";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:47498";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/middle";

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

    let router_subget = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUBGET_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(router_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_history_before() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47499";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:47500";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/history/before";

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

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_subget = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUBGET_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(router_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_history_middle() {
    use std::time::Duration;

    use zenoh::{config, sample::SampleKind};
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47501";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:47502";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/history/middle";

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

    let router_subget = {
        let mut c = config::default();
        c.listen
            .endpoints
            .set(vec![ROUTER_SUBGET_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router (subget) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(router_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err());

    let client_tok = {
        let mut c = config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(get.recv_async()).unwrap().into_result().unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(get.try_recv().is_err());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().is_err());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}
