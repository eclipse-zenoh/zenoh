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

use zenoh::{key_expr::OwnedNonWildKeyExpr, sample::SampleKind};
use zenoh_config::{EndPoint, ModeDependentValue, WhatAmI};
use zenoh_ext::{
    AdvancedPublisherBuilderExt, AdvancedSubscriberBuilderExt, CacheConfig, HistoryConfig,
    MissDetectionConfig, RecoveryConfig,
};
use zenoh_macros::nonwild_ke;

async fn test_advanced_history_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.timestamping
            .set_enabled(Some(ModeDependentValue::Unique(true)))
            .unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let publ = ztimeout!(peer1
        .declare_publisher(pub_ke)
        .cache(CacheConfig::default().max_samples(3)))
    .unwrap();
    ztimeout!(publ.put("1")).unwrap();
    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();

    tokio::time::sleep(SLEEP).await;

    let peer2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

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
    test_advanced_history_inner(
        "test/advanced/history",
        "test/advanced/history",
        None,
        None,
        "tcp/localhost:27050",
    )
    .await;
    test_advanced_history_inner(
        "test/advanced/history",
        "ns/test/advanced/history",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27051",
    )
    .await;
    test_advanced_history_inner(
        "ns/test/advanced/history",
        "test/advanced/history",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27052",
    )
    .await;
    test_advanced_history_inner(
        "test/advanced/history",
        "test/advanced/history",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27053",
    )
    .await;
}

async fn test_advanced_retransmission_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
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
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

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

    router.close().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };
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
        "tcp/localhost:27054",
    )
    .await;
    test_advanced_retransmission_inner(
        "test/advanced/retransmission",
        "ns/test/advanced/retransmission",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27055",
    )
    .await;
    test_advanced_retransmission_inner(
        "ns/test/advanced/retransmission",
        "test/advanced/retransmission",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27056",
    )
    .await;
    test_advanced_retransmission_inner(
        "test/advanced/retransmission",
        "test/advanced/retransmission",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27057",
    )
    .await;
}

async fn test_advanced_retransmission_periodic_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(8);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
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
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

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

    router.close().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };
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
        "tcp/localhost:27058",
    )
    .await;
    test_advanced_retransmission_periodic_inner(
        "test/advanced/retransmission/periodic",
        "ns/test/advanced/retransmission/periodic",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27059",
    )
    .await;
    test_advanced_retransmission_periodic_inner(
        "ns/test/advanced/retransmission/periodic",
        "test/advanced/retransmission/periodic",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27060",
    )
    .await;
    test_advanced_retransmission_periodic_inner(
        "test/advanced/retransmission/periodic",
        "test/advanced/retransmission/periodic",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27061",
    )
    .await;
}

async fn test_advanced_sample_miss_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
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
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

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

    router.close().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };
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
        "tcp/localhost:27062",
    )
    .await;
    test_advanced_sample_miss_inner(
        "test/advanced/sample_miss",
        "ns/test/advanced/sample_miss",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27063",
    )
    .await;
    test_advanced_sample_miss_inner(
        "ns/test/advanced/sample_miss",
        "test/advanced/sample_miss",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27064",
    )
    .await;
    test_advanced_sample_miss_inner(
        "test/advanced/sample_miss",
        "test/advanced/sample_miss",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27065",
    )
    .await;
}

async fn test_advanced_retransmission_sample_miss_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
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
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

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

    router.close().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };
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
        "tcp/localhost:27066",
    )
    .await;
    test_advanced_retransmission_sample_miss_inner(
        "test/advanced/retransmission/sample_miss",
        "ns/test/advanced/retransmission/sample_miss",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27067",
    )
    .await;
    test_advanced_retransmission_sample_miss_inner(
        "ns/test/advanced/retransmission/sample_miss",
        "test/advanced/retransmission/sample_miss",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27068",
    )
    .await;
    test_advanced_retransmission_sample_miss_inner(
        "test/advanced/retransmission/sample_miss",
        "test/advanced/retransmission/sample_miss",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27069",
    )
    .await;
}

async fn test_advanced_late_joiner_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(8);

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.timestamping
            .set_enabled(Some(ModeDependentValue::Unique(true)))
            .unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let peer2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

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
    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };
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
        "tcp/localhost:27070",
    )
    .await;
    test_advanced_late_joiner_inner(
        "test/advanced/late_joiner",
        "ns/test/advanced/late_joiner",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27071",
    )
    .await;
    test_advanced_late_joiner_inner(
        "ns/test/advanced/late_joiner",
        "test/advanced/late_joiner",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27072",
    )
    .await;
    test_advanced_late_joiner_inner(
        "test/advanced/late_joiner",
        "test/advanced/late_joiner",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27073",
    )
    .await;
}

async fn test_advanced_retransmission_heartbeat_inner(
    pub_ke: &str,
    sub_ke: &str,
    pub_namespace: Option<OwnedNonWildKeyExpr>,
    sub_namespace: Option<OwnedNonWildKeyExpr>,
    endpoint: &str,
) {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);
    const HEARTBEAT_PERIOD: Duration = Duration::from_secs(4);

    zenoh_util::init_log_from_env_or("error");

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
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
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = pub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (1) ZID: {}", s.zid());
        s
    };

    let client2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.namespace = sub_namespace;
        let _ = c.set_mode(Some(WhatAmI::Client));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Client (2) ZID: {}", s.zid());
        s
    };

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

    router.close().await.unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(publ.put("2")).unwrap();
    ztimeout!(publ.put("3")).unwrap();
    ztimeout!(publ.put("4")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());

    let router = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![endpoint.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Router));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Router ZID: {}", s.zid());
        s
    };
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
        "tcp/localhost:27074",
    )
    .await;
    test_advanced_retransmission_heartbeat_inner(
        "test/advanced/retransmission/heartbeat",
        "ns/test/advanced/retransmission/heartbeat",
        Some(nonwild_ke!("ns").into()),
        None,
        "tcp/localhost:27075",
    )
    .await;
    test_advanced_retransmission_heartbeat_inner(
        "ns/test/advanced/retransmission/heartbeat",
        "test/advanced/retransmission/heartbeat",
        None,
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27076",
    )
    .await;
    test_advanced_retransmission_heartbeat_inner(
        "test/advanced/retransmission/heartbeat",
        "test/advanced/retransmission/heartbeat",
        Some(nonwild_ke!("ns").into()),
        Some(nonwild_ke!("ns").into()),
        "tcp/localhost:27077",
    )
    .await;
}
