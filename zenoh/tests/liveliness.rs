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
#![cfg(feature = "internal_config")]

use zenoh_core::ztimeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_clique() {
    use std::time::Duration;

    use zenoh::{config::WhatAmI, sample::SampleKind};
    use zenoh_config::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:27347";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/clique";

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_query_clique() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:27348";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/clique";

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_brokered() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27350";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/brokered";

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_query_brokered() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27451";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/brokered";

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_local() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/local";

    zenoh_util::init_log_from_env_or("error");

    let peer = {
        let mut c = zenoh::Config::default();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_query_local() {
    use std::time::Duration;

    use zenoh::{config::WhatAmI, sample::SampleKind};
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/local";

    zenoh_util::init_log_from_env_or("error");

    let peer = {
        let mut c = zenoh::Config::default();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_after_close() {
    use std::time::Duration;

    use zenoh::{config::WhatAmI, sample::SampleKind};
    use zenoh_config::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:27452";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/clique";

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

    let sub = ztimeout!(peer1.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let _token = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    peer1.close().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().is_err())
}

/// -------------------------------------------------------
/// DOUBLE CLIENT
/// -------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27453";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27454";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/middle";

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

    let client_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27354";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/after";

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

    let client_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27455";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27456";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/middle";

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

    let client_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_client_history_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27357";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/after";

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

    let client_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE PEER
/// -------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27458";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27459";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/middle";

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

    let peer_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27460";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/after";

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

    let peer_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27461";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27462";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/middle";

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

    let peer_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_peer_history_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27463";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/after";

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

    let peer_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE ROUTER
/// -------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30464";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:30465";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27466";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:27467";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/middle";

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

    let router_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27468";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:27469";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/after";

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

    let router_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27470";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:27471";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27472";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:27473";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/middle";

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

    let router_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_router_history_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27474";
    const ROUTER_SUB_ENDPOINT: &str = "tcp/localhost:27475";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/after";

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

    let router_sub = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_sub.close().await.unwrap();
    router.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE CLIENT VIA PEER
/// -------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27476";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:27477";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/before";

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

    let peer_dummy = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27478";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:27479";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/middle";

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

    let peer_dummy = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27480";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:27481";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/after";

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

    let peer_dummy = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:27482";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:27483";
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/before";

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

    let peer_dummy = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30484";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:30485";
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/middle";

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

    let peer_dummy = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    sub1.undeclare().await.unwrap();
    sub2.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_sub.close().await.unwrap();
    peer_dummy.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_double_clientviapeer_history_after() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30486";
    const PEER_DUMMY_ENDPOINT: &str = "tcp/localhost:30487";
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/after";

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

    let peer_dummy = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub1.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub2.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub2.try_recv().unwrap().is_none());

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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30488";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30489";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/middle";

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

    let client_subget = {
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = {
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30490";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/history/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(client_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    client_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_client_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30491";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/history/middle";

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

    let client_subget = {
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = {
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30492";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30493";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/middle";

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

    let peer_subget = {
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = {
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30494";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/history/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(peer_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    peer_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_peer_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30495";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/history/middle";

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

    let peer_subget = {
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = {
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30496";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:27497";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30498";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:30499";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/middle";

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

    let router_subget = {
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = {
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_history_before() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30500";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:30501";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/history/before";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
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
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subget_router_history_middle() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30502";
    const ROUTER_SUBGET_ENDPOINT: &str = "tcp/localhost:30503";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/history/middle";

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

    let router_subget = {
        let mut c = zenoh::Config::default();
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

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = {
        let mut c = zenoh::Config::default();
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
    assert!(sub.try_recv().unwrap().is_none());

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
    assert!(sub.try_recv().unwrap().is_none());

    let get = ztimeout!(router_subget.liveliness().get(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(get.try_recv().is_err());

    sub.undeclare().await.unwrap();

    client_tok.close().await.unwrap();
    router_subget.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_regression_1() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30504";
    const PEER_TOK_ENDPOINT: &str = "tcp/localhost:30505";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/1";

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

    let peer_tok = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER_TOK_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(peer_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![
                ROUTER_ENDPOINT.parse::<EndPoint>().unwrap(),
                PEER_TOK_ENDPOINT.parse::<EndPoint>().unwrap(),
            ])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().unwrap().is_none());

    peer_tok.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_regression_2() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER_TOK1_ENDPOINT: &str = "tcp/localhost:30506";
    const PEER_SUB_ENDPOINT: &str = "tcp/localhost:30507";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/2";

    zenoh_util::init_log_from_env_or("error");

    let peer_tok1 = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER_TOK1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (token 1) ZID: {}", s.zid());
        s
    };

    let token1 = ztimeout!(peer_tok1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![PEER_TOK1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let peer_tok2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![
                PEER_TOK1_ENDPOINT.parse::<EndPoint>().unwrap(),
                PEER_SUB_ENDPOINT.parse::<EndPoint>().unwrap(),
            ])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (token 2) ZID: {}", s.zid());
        s
    };

    let token2 = ztimeout!(peer_tok2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token2.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().unwrap().is_none());

    peer_tok1.close().await.unwrap();
    peer_tok2.close().await.unwrap();
    peer_sub.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_regression_2_history() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER_TOK1_ENDPOINT: &str = "tcp/localhost:30508";
    const PEER_SUB_ENDPOINT: &str = "tcp/localhost:30509";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/2/history";

    zenoh_util::init_log_from_env_or("error");

    let peer_tok1 = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER_TOK1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (token 1) ZID: {}", s.zid());
        s
    };

    let token1 = ztimeout!(peer_tok1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER_SUB_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![PEER_TOK1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().unwrap().is_none());

    let peer_tok2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![
                PEER_TOK1_ENDPOINT.parse::<EndPoint>().unwrap(),
                PEER_SUB_ENDPOINT.parse::<EndPoint>().unwrap(),
            ])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (token 2) ZID: {}", s.zid());
        s
    };

    let token2 = ztimeout!(peer_tok2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token2.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().unwrap().is_none());

    peer_tok1.close().await.unwrap();
    peer_tok2.close().await.unwrap();
    peer_sub.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_regression_3() {
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30510";
    const PEER_TOK_ENDPOINT: &str = "tcp/localhost:30511";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/3";

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

    let peer_tok1 = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER_TOK_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (token 1) ZID: {}", s.zid());
        s
    };

    let token1 = ztimeout!(peer_tok1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token 2) ZID: {}", s.zid());
        s
    };

    let token2 = ztimeout!(client_tok2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![
                ROUTER_ENDPOINT.parse::<EndPoint>().unwrap(),
                PEER_TOK_ENDPOINT.parse::<EndPoint>().unwrap(),
            ])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (sub) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token1.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token2.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub.try_recv().unwrap().is_none());

    peer_tok1.close().await.unwrap();
    client_tok2.close().await.unwrap();
    peer_sub.close().await.unwrap();
    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_issue_1470() {
    // https://github.com/eclipse-zenoh/zenoh/issues/1470
    use std::{collections::HashSet, str::FromStr, time::Duration};

    use zenoh::sample::SampleKind;
    use zenoh_config::{WhatAmI, ZenohId};
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER0_ENDPOINT: &str = "tcp/localhost:30512";
    const ROUTER1_ENDPOINT: &str = "tcp/localhost:30513";
    const PEER_ENDPOINT: &str = "tcp/localhost:30514";
    const LIVELINESS_KEYEXPR_PREFIX: &str = "test/liveliness/issue/1470/*";
    const LIVELINESS_KEYEXPR_ROUTER0: &str = "test/liveliness/issue/1470/a0";
    const LIVELINESS_KEYEXPR_ROUTER1: &str = "test/liveliness/issue/1470/a1";
    const LIVELINESS_KEYEXPR_PEER: &str = "test/liveliness/issue/1470/b";

    zenoh_util::init_log_from_env_or("error");

    let router0 = {
        let mut c = zenoh::Config::default();
        c.set_id(Some(ZenohId::from_str("a0").unwrap())).unwrap();
        c.listen
            .endpoints
            .set(vec![ROUTER0_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        ztimeout!(zenoh::open(c)).unwrap()
    };

    let _token_a0 = ztimeout!(router0
        .liveliness()
        .declare_token(LIVELINESS_KEYEXPR_ROUTER0))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let router1 = {
        let mut c = zenoh::Config::default();
        c.set_id(Some(ZenohId::from_str("a1").unwrap())).unwrap();
        c.listen
            .endpoints
            .set(vec![ROUTER1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![ROUTER0_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        ztimeout!(zenoh::open(c)).unwrap()
    };

    let _token_a1 = ztimeout!(router1
        .liveliness()
        .declare_token(LIVELINESS_KEYEXPR_ROUTER1))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer = {
        let mut c = zenoh::Config::default();
        c.set_id(Some(ZenohId::from_str("b").unwrap())).unwrap();
        c.listen
            .endpoints
            .set(vec![PEER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.connect
            .endpoints
            .set(vec![
                ROUTER0_ENDPOINT.parse::<EndPoint>().unwrap(),
                ROUTER1_ENDPOINT.parse::<EndPoint>().unwrap(),
            ])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        ztimeout!(zenoh::open(c)).unwrap()
    };

    let _token_b = ztimeout!(peer.liveliness().declare_token(LIVELINESS_KEYEXPR_PEER)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client0 = {
        let mut c = zenoh::Config::default();
        c.set_id(Some(ZenohId::from_str("c0").unwrap())).unwrap();
        c.connect
            .endpoints
            .set(vec![PEER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        ztimeout!(zenoh::open(c)).unwrap()
    };

    let sub0 = ztimeout!(client0
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR_PREFIX)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let mut puts0 = HashSet::new();

    let sample = ztimeout!(sub0.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    puts0.insert(sample.key_expr().to_string());

    let sample = ztimeout!(sub0.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    puts0.insert(sample.key_expr().to_string());

    let sample = ztimeout!(sub0.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    puts0.insert(sample.key_expr().to_string());

    assert!(sub0.try_recv().unwrap().is_none());

    assert_eq!(
        puts0,
        HashSet::from([
            LIVELINESS_KEYEXPR_ROUTER0.to_string(),
            LIVELINESS_KEYEXPR_ROUTER1.to_string(),
            LIVELINESS_KEYEXPR_PEER.to_string(),
        ])
    );

    client0.close().await.unwrap();

    let client1 = {
        let mut c = zenoh::Config::default();
        c.set_id(Some(ZenohId::from_str("c1").unwrap())).unwrap();
        c.connect
            .endpoints
            .set(vec![PEER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        ztimeout!(zenoh::open(c)).unwrap()
    };

    let sub1 = ztimeout!(client1
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR_PREFIX)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let mut puts1 = HashSet::new();

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    puts1.insert(sample.key_expr().to_string());

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    puts1.insert(sample.key_expr().to_string());

    let sample = ztimeout!(sub1.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    puts1.insert(sample.key_expr().to_string());

    assert!(sub1.try_recv().unwrap().is_none());

    assert_eq!(
        puts1,
        HashSet::from([
            LIVELINESS_KEYEXPR_ROUTER0.to_string(),
            LIVELINESS_KEYEXPR_ROUTER1.to_string(),
            LIVELINESS_KEYEXPR_PEER.to_string(),
        ])
    );

    router0.close().await.unwrap();
    router1.close().await.unwrap();
    peer.close().await.unwrap();
    client0.close().await.unwrap();
    client1.close().await.unwrap();
}

/// -------------------------------------------------------
/// DOUBLE UNDECLARE CLIQUE
/// -------------------------------------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_double_undeclare_clique() {
    use std::time::Duration;

    use zenoh::{config::WhatAmI, sample::SampleKind};
    use zenoh_config::EndPoint;
    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:30515";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/double/undeclare/clique";

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

    let sub = ztimeout!(peer1.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let token = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    let token2 = ztimeout!(peer2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    token2.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    sub.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_sub_history_conflict() {
    // https://github.com/eclipse-zenoh/zenoh/issues/2071
    use std::time::Duration;

    use zenoh::sample::SampleKind;
    use zenoh_config::WhatAmI;
    use zenoh_link::EndPoint;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:30516";
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/history/conflict";

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

    let client_tok = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (token) ZID: {}", s.zid());
        s
    };

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_subs = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (subs) ZID: {}", s.zid());
        s
    };

    let sub_no_history = ztimeout!(client_subs
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(false))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub_no_history.try_recv().unwrap().is_none());

    let sub_history = ztimeout!(client_subs
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub_history.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Put);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);
    assert!(sub_history.try_recv().unwrap().is_none());

    assert!(sub_no_history.try_recv().unwrap().is_none());

    token.undeclare().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub_history.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    let sample = ztimeout!(sub_no_history.recv_async()).unwrap();
    assert!(sample.kind() == SampleKind::Delete);
    assert!(sample.key_expr().as_str() == LIVELINESS_KEYEXPR);

    client_tok.close().await.unwrap();
    client_subs.close().await.unwrap();
    router.close().await.unwrap();
}
