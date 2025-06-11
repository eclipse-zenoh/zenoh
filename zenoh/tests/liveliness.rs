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
mod common;

use std::{collections::HashSet, time::Duration};

use zenoh::sample::SampleKind;
use zenoh_core::ztimeout;

use crate::common::{open_client, open_peer, open_router};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_liveliness_subscriber_clique() {
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/clique";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer());
    let peer2 = ztimeout!(open_peer().connect_to(&peer1));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/clique";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer());
    let peer2 = ztimeout!(open_peer().connect_to(&peer1));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/brokered";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let client1 = ztimeout!(open_client().connect_to(&router));
    let client2 = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/brokered";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let client1 = ztimeout!(open_client().connect_to(&router));
    let client2 = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/local";

    zenoh_util::init_log_from_env_or("error");

    let peer = ztimeout!(open_peer());

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/query/local";

    zenoh_util::init_log_from_env_or("error");

    let peer = ztimeout!(open_peer());

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/clique";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer());
    let peer2 = ztimeout!(open_peer().connect_to(&peer1));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_sub = ztimeout!(open_client().connect_to(&router));

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_sub = ztimeout!(open_client().connect_to(&router));

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_sub = ztimeout!(open_client().connect_to(&router));

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/client/history/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_sub = ztimeout!(open_client().connect_to(&router));

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

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = ztimeout!(open_peer().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_sub = ztimeout!(open_peer().connect_to(&router));

    let sub1 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_sub = ztimeout!(open_peer().connect_to(&router));

    let sub1 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();

    let sub2 = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = ztimeout!(open_peer().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_sub = ztimeout!(open_peer().connect_to(&router));

    let sub1 = ztimeout!(peer_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/peer/history/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_sub = ztimeout!(open_peer().connect_to(&router));

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

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_sub = ztimeout!(open_router().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let router_sub = ztimeout!(open_router().connect_to(&router));

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let router_sub = ztimeout!(open_router().connect_to(&router));

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();

    let sub2 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_sub = ztimeout!(open_router().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let router_sub = ztimeout!(open_router().connect_to(&router));

    let sub1 = ztimeout!(router_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/router/history/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let router_sub = ztimeout!(open_router().connect_to(&router));

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

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_dummy = ztimeout!(open_peer().connect_to(&router));

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = ztimeout!(open_client().connect_to(&peer_dummy));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_dummy = ztimeout!(open_peer().connect_to(&router));

    let client_sub = ztimeout!(open_client().connect_to(&peer_dummy));

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subscriber/double/clientviapeer/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_dummy = ztimeout!(open_peer().connect_to(&router));

    let client_sub = ztimeout!(open_client().connect_to(&peer_dummy));

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();

    let sub2 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_dummy = ztimeout!(open_peer().connect_to(&router));

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_sub = ztimeout!(open_client().connect_to(&peer_dummy));

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
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_dummy = ztimeout!(open_peer().connect_to(&router));

    let client_sub = ztimeout!(open_client().connect_to(&peer_dummy));

    let sub1 = ztimeout!(client_sub
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str =
        "test/liveliness/subscriber/double/clientviapeer/history/after";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_dummy = ztimeout!(open_peer().connect_to(&router));

    let client_sub = ztimeout!(open_client().connect_to(&peer_dummy));

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

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_subget = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_subget = ztimeout!(open_client().connect_to(&router));

    let sub = ztimeout!(client_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_subget = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/client/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_subget = ztimeout!(open_client().connect_to(&router));

    let sub = ztimeout!(client_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_subget = ztimeout!(open_peer().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_subget = ztimeout!(open_peer().connect_to(&router));

    let sub = ztimeout!(peer_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_subget = ztimeout!(open_peer().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/peer/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_subget = ztimeout!(open_peer().connect_to(&router));

    let sub = ztimeout!(peer_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_subget = ztimeout!(open_router().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let router_subget = ztimeout!(open_router().connect_to(&router));

    let sub = ztimeout!(router_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/history/before";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let client_tok = ztimeout!(open_client().connect_to(&router));

    let token = ztimeout!(client_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let router_subget = ztimeout!(open_router().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/subget/router/history/middle";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let router_subget = ztimeout!(open_router().connect_to(&router));

    let sub = ztimeout!(router_subget
        .liveliness()
        .declare_subscriber(LIVELINESS_KEYEXPR)
        .history(true))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let client_tok = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/1";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_tok = ztimeout!(open_peer().connect_to(&router));

    let token = ztimeout!(peer_tok.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = ztimeout!(open_peer().connect_to([&router, &peer_tok]));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/2";

    zenoh_util::init_log_from_env_or("error");

    let peer_tok1 = ztimeout!(open_peer());

    let token1 = ztimeout!(peer_tok1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = ztimeout!(open_peer().connect_to(&peer_tok1));

    let sub = ztimeout!(peer_sub.liveliness().declare_subscriber(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let peer_tok2 = ztimeout!(open_peer().connect_to([&peer_tok1, &peer_sub]));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/2/history";

    zenoh_util::init_log_from_env_or("error");

    let peer_tok1 = ztimeout!(open_peer());

    let token1 = ztimeout!(peer_tok1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = ztimeout!(open_peer().connect_to([&peer_tok1]));

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

    let peer_tok2 = ztimeout!(open_peer().connect_to([&peer_tok1, &peer_sub]));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/regression/3";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());

    let peer_tok1 = ztimeout!(open_peer().connect_to([&router]));

    let token1 = ztimeout!(peer_tok1.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client_tok2 = ztimeout!(open_client().connect_to([&router]));

    let token2 = ztimeout!(client_tok2.liveliness().declare_token(LIVELINESS_KEYEXPR)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer_sub = ztimeout!(open_peer().connect_to([&router, &peer_tok1]));

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
    const LIVELINESS_KEYEXPR_PREFIX: &str = "test/liveliness/issue/1470/*";
    const LIVELINESS_KEYEXPR_ROUTER0: &str = "test/liveliness/issue/1470/a0";
    const LIVELINESS_KEYEXPR_ROUTER1: &str = "test/liveliness/issue/1470/a1";
    const LIVELINESS_KEYEXPR_PEER: &str = "test/liveliness/issue/1470/b";

    zenoh_util::init_log_from_env_or("error");

    let router0 = ztimeout!(open_router().with("id", "a0"));

    let _token_a0 = ztimeout!(router0
        .liveliness()
        .declare_token(LIVELINESS_KEYEXPR_ROUTER0))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let router1 = ztimeout!(open_router().with("id", "a1").connect_to(&router0));

    let _token_a1 = ztimeout!(router1
        .liveliness()
        .declare_token(LIVELINESS_KEYEXPR_ROUTER1))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let peer = ztimeout!(open_peer().with("id", "b").connect_to([&router0, &router1]));

    let _token_b = ztimeout!(peer.liveliness().declare_token(LIVELINESS_KEYEXPR_PEER)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let client0 = ztimeout!(open_client().with("id", "c0").connect_to(&peer));

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

    let client1 = ztimeout!(open_client().with("id", "c1").connect_to(&peer));

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
    const LIVELINESS_KEYEXPR: &str = "test/liveliness/double/undeclare/clique";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer());

    let peer2 = ztimeout!(open_peer().connect_to(&peer1));

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
