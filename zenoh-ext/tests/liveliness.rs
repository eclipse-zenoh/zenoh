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

mod common;

use std::time::Duration;

use zenoh::{internal::ztimeout, sample::SampleKind, Wait};
#[allow(deprecated)]
use zenoh_ext::SubscriberBuilderExt;

use crate::common::{open_client, open_peer, open_router};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(deprecated)]
async fn test_liveliness_querying_subscriber_clique() {
    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer().with("listen/endpoints", ["udp/127.0.0.1:0"]));
    let peer2 = ztimeout!(open_peer().connect_to(&peer1));

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
    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let client1 = ztimeout!(open_client().connect_to(&router));
    let client2 = ztimeout!(open_client().connect_to(&router));
    let client3 = ztimeout!(open_client().connect_to(&router));

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
    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer().with("listen/endpoints", ["udp/127.0.0.1:0"]));
    let peer2 = ztimeout!(open_peer().connect_to(&peer1));

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
    const LIVELINESS_KEYEXPR_1: &str = "test/liveliness/querying-subscriber/brokered/1";
    const LIVELINESS_KEYEXPR_2: &str = "test/liveliness/querying-subscriber/brokered/2";
    const LIVELINESS_KEYEXPR_ALL: &str = "test/liveliness/querying-subscriber/brokered/*";

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let client1 = ztimeout!(open_client().connect_to(&router));
    let client2 = ztimeout!(open_client().connect_to(&router));
    let client3 = ztimeout!(open_client().connect_to(&router));

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
