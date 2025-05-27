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

use zenoh::{internal::ztimeout, key_expr::OwnedNonWildKeyExpr, sample::SampleKind};
use zenoh_ext::{
    AdvancedPublisherBuilderExt, AdvancedSubscriberBuilderExt, CacheConfig, HistoryConfig,
    MissDetectionConfig, RecoveryConfig,
};
use zenoh_macros::nonwild_ke;

use crate::common::{open_client, open_peer, open_router, TcpForward};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const RECONNECT_SLEEP: Duration = Duration::from_secs(5);

async fn test_advanced_history_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    zenoh_util::init_log_from_env_or("error");

    let peer1 = ztimeout!(open_peer()
        .with("timestamping/enabled", true)
        .with("namespace", pub_namespace));

    let publ = ztimeout!(peer1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(3)))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();
    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let peer2 = ztimeout!(open_peer()
        .connect_to(&peer1)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(peer2
        .declare_subscriber(sub_ke)
        .history(HistoryConfig::default()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("5")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "2");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "3");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "4");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "5");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_history() {
    test_advanced_history_inner("test/advanced/history", "test/advanced/history", None, None).await;
    test_advanced_history_inner(
        "test/advanced/history",
        "ns/test/advanced/history",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_history_inner(
        "ns/test/advanced/history",
        "test/advanced/history",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_history_inner(
        "test/advanced/history",
        "test/advanced/history",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}

async fn test_advanced_retransmission_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let mut forward = TcpForward::connect(&router);
    let client1 = ztimeout!(open_client()
        .connect_to(&forward)
        .with("namespace", pub_namespace));
    let client2 = ztimeout!(open_client()
        .connect_to(&router)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(client2
        .declare_subscriber(sub_ke)
        .recovery(RecoveryConfig::default()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(10))
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "1");

    assert!(sub.try_recv().unwrap().is_none());

    forward.disable();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    forward.enable();
    tokio::time::sleep(RECONNECT_SLEEP).await;

    ztimeout!(publ.put("5")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "2");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "3");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "4");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "5");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    client1.close().await.unwrap();
    client2.close().await.unwrap();

    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_retransmission() {
    test_advanced_retransmission_inner(
        "test/advanced/retransmission",
        "test/advanced/retransmission",
        None,
        None,
    )
    .await;
    test_advanced_retransmission_inner(
        "test/advanced/retransmission",
        "ns/test/advanced/retransmission",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_retransmission_inner(
        "ns/test/advanced/retransmission",
        "test/advanced/retransmission",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_retransmission_inner(
        "test/advanced/retransmission",
        "test/advanced/retransmission",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}

async fn test_advanced_retransmission_periodic_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let mut forward = TcpForward::connect(&router);
    let client1 = ztimeout!(open_client()
        .connect_to(&forward)
        .with("namespace", pub_namespace));
    let client2 = ztimeout!(open_client()
        .connect_to(&router)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(client2
        .declare_subscriber(sub_ke)
        .recovery(RecoveryConfig::default().periodic_queries(Duration::from_secs(1))))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(10))
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "1");

    assert!(sub.try_recv().unwrap().is_none());

    forward.disable();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    forward.enable();
    tokio::time::sleep(RECONNECT_SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "2");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "3");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "4");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    client1.close().await.unwrap();
    client2.close().await.unwrap();

    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_retransmission_periodic() {
    test_advanced_retransmission_periodic_inner(
        "test/advanced/retransmission/periodic",
        "test/advanced/retransmission/periodic",
        None,
        None,
    )
    .await;
    test_advanced_retransmission_periodic_inner(
        "test/advanced/retransmission/periodic",
        "ns/test/advanced/retransmission/periodic",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_retransmission_periodic_inner(
        "ns/test/advanced/retransmission/periodic",
        "test/advanced/retransmission/periodic",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_retransmission_periodic_inner(
        "test/advanced/retransmission/periodic",
        "test/advanced/retransmission/periodic",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}

async fn test_advanced_sample_miss_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let mut forward = TcpForward::connect(&router);
    let client1 = ztimeout!(open_client()
        .connect_to(&forward)
        .with("namespace", pub_namespace));
    let client2 = ztimeout!(open_client()
        .connect_to(&router)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(client2.declare_subscriber(sub_ke).advanced()).unwrap();
    let miss_listener = ztimeout!(sub.sample_miss_listener()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(pub_ke)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "1");

    assert!(sub.try_recv().unwrap().is_none());

    forward.disable();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    forward.enable();
    tokio::time::sleep(RECONNECT_SLEEP).await;

    ztimeout!(publ.put("3")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let miss = ztimeout!(miss_listener.recv_async()).unwrap();
    assert_eq!(miss.source(), publ.id());
    assert_eq!(miss.nb(), 1);

    assert!(miss_listener.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "3");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    client1.close().await.unwrap();
    client2.close().await.unwrap();

    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_sample_miss() {
    test_advanced_sample_miss_inner(
        "test/advanced/sample_miss",
        "test/advanced/sample_miss",
        None,
        None,
    )
    .await;
    test_advanced_sample_miss_inner(
        "test/advanced/sample_miss",
        "ns/test/advanced/sample_miss",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_sample_miss_inner(
        "ns/test/advanced/sample_miss",
        "test/advanced/sample_miss",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_sample_miss_inner(
        "test/advanced/sample_miss",
        "test/advanced/sample_miss",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}

async fn test_advanced_retransmission_sample_miss_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let mut forward = TcpForward::connect(&router);
    let client1 = ztimeout!(open_client()
        .connect_to(&forward)
        .with("namespace", pub_namespace));
    let client2 = ztimeout!(open_client()
        .connect_to(&router)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(client2
        .declare_subscriber(sub_ke)
        .recovery(RecoveryConfig::default().periodic_queries(Duration::from_secs(1))))
    .unwrap();
    let miss_listener = ztimeout!(sub.sample_miss_listener()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(1))
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "1");

    assert!(sub.try_recv().unwrap().is_none());

    forward.disable();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    forward.enable();
    tokio::time::sleep(RECONNECT_SLEEP).await;

    ztimeout!(publ.put("5")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let miss = ztimeout!(miss_listener.recv_async()).unwrap();
    assert_eq!(miss.source(), publ.id());
    assert_eq!(miss.nb(), 2);

    assert!(miss_listener.try_recv().unwrap().is_none());

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "4");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "5");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    client1.close().await.unwrap();
    client2.close().await.unwrap();

    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_retransmission_sample_miss() {
    test_advanced_retransmission_sample_miss_inner(
        "test/advanced/retransmission/sample_miss",
        "test/advanced/retransmission/sample_miss",
        None,
        None,
    )
    .await;
    test_advanced_retransmission_sample_miss_inner(
        "test/advanced/retransmission/sample_miss",
        "ns/test/advanced/retransmission/sample_miss",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_retransmission_sample_miss_inner(
        "ns/test/advanced/retransmission/sample_miss",
        "test/advanced/retransmission/sample_miss",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_retransmission_sample_miss_inner(
        "test/advanced/retransmission/sample_miss",
        "test/advanced/retransmission/sample_miss",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}

async fn test_advanced_late_joiner_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let mut forward = TcpForward::connect(&router);
    forward.disable();
    let peer1 = ztimeout!(open_peer()
        .connect_to(&forward)
        .with("timestamping/enabled", true)
        .with("namespace", pub_namespace));
    let peer2 = ztimeout!(open_peer()
        .connect_to(&router)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(peer2
        .declare_subscriber(sub_ke)
        .history(HistoryConfig::default().detect_late_publishers()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(10))
        .publisher_detection())
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();
    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();

    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());
    forward.enable();
    tokio::time::sleep(RECONNECT_SLEEP).await;

    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "1");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "2");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "3");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "4");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    peer1.close().await.unwrap();
    peer2.close().await.unwrap();

    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_late_joiner() {
    test_advanced_late_joiner_inner(
        "test/advanced/late_joiner",
        "test/advanced/late_joiner",
        None,
        None,
    )
    .await;
    test_advanced_late_joiner_inner(
        "test/advanced/late_joiner",
        "ns/test/advanced/late_joiner",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_late_joiner_inner(
        "ns/test/advanced/late_joiner",
        "test/advanced/late_joiner",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_late_joiner_inner(
        "test/advanced/late_joiner",
        "test/advanced/late_joiner",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}

async fn test_advanced_retransmission_heartbeat_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
) {
    const HEARTBEAT_PERIOD: Duration = Duration::from_secs(4);

    zenoh_util::init_log_from_env_or("error");

    let router = ztimeout!(open_router());
    let mut forward = TcpForward::connect(&router);
    let client1 = ztimeout!(open_client()
        .connect_to(&forward)
        .with("namespace", pub_namespace));
    let client2 = ztimeout!(open_client()
        .connect_to(&router)
        .with("namespace", sub_namespace));

    let sub = ztimeout!(client2
        .declare_subscriber(sub_ke)
        .recovery(RecoveryConfig::default().heartbeat()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(10))
        .sample_miss_detection(MissDetectionConfig::default().heartbeat(HEARTBEAT_PERIOD)))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "1");

    assert!(sub.try_recv().unwrap().is_none());

    forward.disable();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    forward.enable();
    tokio::time::sleep(RECONNECT_SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "2");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "3");

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.kind(), SampleKind::Put);
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "4");

    assert!(sub.try_recv().unwrap().is_none());

    publ.undeclare().await.unwrap();
    // sub.undeclare().await.unwrap();

    client1.close().await.unwrap();
    client2.close().await.unwrap();

    router.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_retransmission_heartbeat() {
    test_advanced_retransmission_heartbeat_inner(
        "test/advanced/retransmission/heartbeat",
        "test/advanced/retransmission/heartbeat",
        None,
        None,
    )
    .await;
    test_advanced_retransmission_heartbeat_inner(
        "test/advanced/retransmission/heartbeat",
        "ns/test/advanced/retransmission/heartbeat",
        Some(nonwild_ke!("ns").into()),
        None,
    )
    .await;
    test_advanced_retransmission_heartbeat_inner(
        "ns/test/advanced/retransmission/heartbeat",
        "test/advanced/retransmission/heartbeat",
        None,
        Some(nonwild_ke!("ns").into()),
    )
    .await;
    test_advanced_retransmission_heartbeat_inner(
        "test/advanced/retransmission/heartbeat",
        "test/advanced/retransmission/heartbeat",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
    )
    .await;
}
