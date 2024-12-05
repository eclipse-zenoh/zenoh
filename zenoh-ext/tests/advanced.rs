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

use zenoh::sample::SampleKind;
use zenoh_config::{EndPoint, ModeDependentValue, WhatAmI};
use zenoh_ext::{
    AdvancedPublisherBuilderExt, AdvancedSubscriberBuilderExt, CacheConfig, HistoryConfig,
    RecoveryConfig,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_advanced_history() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const PEER1_ENDPOINT: &str = "tcp/localhost:47450";

    const ADVANCED_HISTORY_KEYEXPR: &str = "test/advanced/history";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = zenoh::Config::default();
        c.listen
            .endpoints
            .set(vec![PEER1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.timestamping
            .set_enabled(Some(ModeDependentValue::Unique(true)))
            .unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let publ = ztimeout!(peer1
        .declare_publisher(ADVANCED_HISTORY_KEYEXPR)
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
            .set(vec![PEER1_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer2
        .declare_subscriber(ADVANCED_HISTORY_KEYEXPR)
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
async fn test_advanced_retransmission() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47451";

    const ADVANCED_RETRANSMISSION_KEYEXPR: &str = "test/advanced/retransmission";

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

    let sub = ztimeout!(client2
        .declare_subscriber(ADVANCED_RETRANSMISSION_KEYEXPR)
        .recovery(RecoveryConfig::default()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(ADVANCED_RETRANSMISSION_KEYEXPR)
        .cache(CacheConfig::default().max_samples(10))
        .sample_miss_detection())
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
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
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
async fn test_advanced_retransmission_periodic() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(8);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47452";

    const ADVANCED_RETRANSMISSION_PERIODIC_KEYEXPR: &str = "test/advanced/retransmission/periodic";

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

    let sub = ztimeout!(client2
        .declare_subscriber(ADVANCED_RETRANSMISSION_PERIODIC_KEYEXPR)
        .recovery(RecoveryConfig::default().periodic_queries(Some(Duration::from_secs(1)))))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(ADVANCED_RETRANSMISSION_PERIODIC_KEYEXPR)
        .cache(CacheConfig::default().max_samples(10))
        .sample_miss_detection())
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
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
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
async fn test_advanced_sample_miss() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47453";

    const ADVANCED_SAMPLE_MISS_KEYEXPR: &str = "test/advanced/sample_miss";

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

    let sub = ztimeout!(client2
        .declare_subscriber(ADVANCED_SAMPLE_MISS_KEYEXPR)
        .advanced())
    .unwrap();
    let miss_listener = ztimeout!(sub.sample_miss_listener()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(ADVANCED_SAMPLE_MISS_KEYEXPR)
        .sample_miss_detection())
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
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
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
async fn test_advanced_retransmission_sample_miss() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(5);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47454";

    const ADVANCED_RETRANSMISSION_SAMPLE_MISS_KEYEXPR: &str =
        "test/advanced/retransmission/sample_miss";

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

    let sub = ztimeout!(client2
        .declare_subscriber(ADVANCED_RETRANSMISSION_SAMPLE_MISS_KEYEXPR)
        .recovery(RecoveryConfig::default().periodic_queries(Some(Duration::from_secs(1)))))
    .unwrap();
    let miss_listener = ztimeout!(sub.sample_miss_listener()).unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(client1
        .declare_publisher(ADVANCED_RETRANSMISSION_SAMPLE_MISS_KEYEXPR)
        .cache(CacheConfig::default().max_samples(1))
        .sample_miss_detection())
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
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
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
async fn test_advanced_late_joiner() {
    use std::time::Duration;

    use zenoh::internal::ztimeout;

    const TIMEOUT: Duration = Duration::from_secs(60);
    const SLEEP: Duration = Duration::from_secs(1);
    const RECONNECT_SLEEP: Duration = Duration::from_secs(8);
    const ROUTER_ENDPOINT: &str = "tcp/localhost:47455";

    const ADVANCED_LATE_JOINER_KEYEXPR: &str = "test/advanced/late_joiner";

    zenoh_util::init_log_from_env_or("error");

    let peer1 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        c.timestamping
            .set_enabled(Some(ModeDependentValue::Unique(true)))
            .unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let peer2 = {
        let mut c = zenoh::Config::default();
        c.connect
            .endpoints
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

    let sub = ztimeout!(peer2
        .declare_subscriber(ADVANCED_LATE_JOINER_KEYEXPR)
        .history(HistoryConfig::default().detect_late_publishers()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher(ADVANCED_LATE_JOINER_KEYEXPR)
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
            .set(vec![ROUTER_ENDPOINT.parse::<EndPoint>().unwrap()])
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
