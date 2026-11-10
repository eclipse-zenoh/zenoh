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
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use zenoh::{
    internal::ztimeout,
    key_expr::KeyExpr,
    sample::{FragInfo, SampleBuilder, SourceInfo},
    session::EntityGlobalId,
    timestamp_stack::{InterceptionPoint, TimestampInstrumentationBuilder},
    Config, Wait,
};
use zenoh_config::{EndPoint, EndPoints, WhatAmI};
use zenoh_ext::{
    AdvancedPublisherBuilderExt, AdvancedSubscriberBuilderExt, CacheConfig, MissDetectionConfig,
    RecoveryConfig,
};

const SLEEP: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(60);

// The synchronous local delivery path must create recovery timers on Zenoh's
// runtime, even when the caller has no Tokio runtime.
#[test]
fn test_fragmentation_periodic_recovery_without_tokio_runtime() {
    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    let session = zenoh::open(config).wait().unwrap();
    let sub = session
        .declare_subscriber("test/fragmentation/sync")
        .advanced()
        .recovery(RecoveryConfig::default().periodic_queries(Duration::from_millis(50)))
        .wait()
        .unwrap();
    let publ = session
        .declare_publisher("test/fragmentation/sync")
        .advanced()
        .fragmentation(4)
        .cache(CacheConfig::default())
        .sample_miss_detection(MissDetectionConfig::default())
        .wait()
        .unwrap();
    publ.put("fragmented synchronous publication")
        .wait()
        .unwrap();
    let sample = sub.recv_timeout(TIMEOUT).unwrap().unwrap();
    assert_eq!(
        sample.payload().try_to_string().unwrap(),
        "fragmented synchronous publication"
    );
    session.close().wait().unwrap();
}

// A queryable may publish fragments while answering another query. Recovery
// must not call that queryable again on the same stack: callback_mut holds a
// mutex until the first call returns.
#[test]
fn test_fragment_recovery_does_not_reenter_queryable() {
    thread_local! {
        static IN_QUERY_CALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    let session = zenoh::open(config).wait().unwrap();
    let source_id = EntityGlobalId::new(session.zid(), 7);
    let key = "test/fragmentation/reentrant_query";
    let sub = session
        .declare_subscriber(key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(60)))
        .wait()
        .unwrap();
    let reentered = Arc::new(AtomicBool::new(false));
    let serialized_callback = zenoh::handlers::locked({
        let session = session.downgrade();
        move |query: zenoh::query::Query| {
            IN_QUERY_CALLBACK.with(|active| active.set(true));
            if query.parameters().get("start") == Some("1") {
                // Fragment 1 of the first sample triggers fragment recovery,
                // without a sequence-number gap triggering sample recovery.
                session
                    .put(key, "B")
                    .source_info(SourceInfo::new(source_id, 0))
                    .frag_info(FragInfo::new(2, 1))
                    .wait()
                    .unwrap();
            } else {
                assert_eq!(query.parameters().get("_sn"), Some("0..0"));
                assert_eq!(query.parameters().get("_fn"), Some("0..0"));
                query
                    .reply_sample(
                        SampleBuilder::put(KeyExpr::try_from(key).unwrap(), "A")
                            .source_info(SourceInfo::new(source_id, 0))
                            .frag_info(FragInfo::new(2, 0))
                            .into(),
                    )
                    .wait()
                    .unwrap();
            }
            IN_QUERY_CALLBACK.with(|active| active.set(false));
        }
    });
    let _cache = session
        .declare_queryable("test/fragmentation/reentrant_query/@adv/**")
        .callback({
            let reentered = reentered.clone();
            move |query| {
                // Use the same lock as callback_mut, but detect same-thread
                // re-entry before locking so a regression fails instead of hanging.
                if IN_QUERY_CALLBACK.with(|active| active.get()) {
                    reentered.store(true, Ordering::SeqCst);
                    return;
                }
                serialized_callback(query);
            }
        })
        .wait()
        .unwrap();

    let _replies = session
        .get("test/fragmentation/reentrant_query/@adv/**?start=1")
        .wait()
        .unwrap();
    assert!(
        !reentered.load(Ordering::SeqCst),
        "fragment recovery re-entered the queryable callback"
    );
    let sample = sub.recv_timeout(TIMEOUT).unwrap().unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap(), "AB");
    assert!(sub.try_recv().unwrap().is_none());
    session.close().wait().unwrap();
}

// Upstream timestamp instrumentation must survive both unfragmented advanced
// publications and fragmentation/reassembly over the network.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragmentation_timestamp_instrumentation() {
    let (peer1, peer2) = create_peer_pair().await;
    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/instrumentation")
        .advanced())
    .unwrap();
    let publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/instrumentation")
        .advanced()
        .fragmentation(4)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    for payload in ["tiny", "fragmented instrumented publication"] {
        let instrumentation = TimestampInstrumentationBuilder::new()
            .set_send(true)
            .set_receive(true)
            .build()
            .unwrap();
        ztimeout!(publ.put(payload).timestamp_instrumentation(instrumentation)).unwrap();
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
        let stack = sample.timestamp_stack().expect("missing timestamp stack");
        let points: Vec<_> = stack
            .records()
            .iter()
            .map(|record| record.point())
            .collect();
        assert_eq!(
            points,
            [InterceptionPoint::Send, InterceptionPoint::Receive]
        );
    }
}

async fn create_peer_pair() -> (zenoh::Session, zenoh::Session) {
    let locator = format!("tcp/127.0.0.1:{}", zenoh_test::get_free_tcp_port());
    let peer1 = {
        let mut c = Config::default();
        c.listen
            .endpoints
            .set(vec![locator.parse::<EndPoint>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (1) ZID: {}", s.zid());
        s
    };

    let peer2 = {
        let mut c = Config::default();
        c.connect
            .endpoints
            .set(vec![locator.parse::<EndPoints>().unwrap()])
            .unwrap();
        c.scouting.multicast.set_enabled(Some(false)).unwrap();
        let _ = c.set_mode(Some(WhatAmI::Peer));
        let s = ztimeout!(zenoh::open(c)).unwrap();
        tracing::info!("Peer (2) ZID: {}", s.zid());
        s
    };

    (peer1, peer2)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragmentation_reassembly() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, peer2) = create_peer_pair().await;

    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/reassembly")
        .advanced())
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/reassembly")
        .advanced()
        .fragmentation(4)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();

    let payload = "HelloWorldFromFragmentationTest";
    ztimeout!(publ.put(payload)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), payload);
    assert!(sub.try_recv().unwrap().is_none());
}

// A complete successor must be delivered when the publisher's peer has left
// and the missing fragments can no longer be recovered from its cache.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_progress_after_publisher_disconnect() {
    let (publisher_session, subscriber_session) = create_peer_pair().await;
    let key = "test/fragmentation/disconnect";
    let publisher = ztimeout!(publisher_session
        .declare_publisher(key)
        .advanced()
        .fragmentation(4)
        .sample_miss_detection(MissDetectionConfig::default())
        .cache(CacheConfig::default().max_samples(3)))
    .unwrap();
    let source_id = publisher.id();
    for payload in ["base", "abcdefgh", "next"] {
        ztimeout!(publisher.put(payload)).unwrap();
    }
    let sub = ztimeout!(subscriber_session
        .declare_subscriber(key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(20)),)
        .query_timeout(Duration::from_millis(500)))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(publisher_session.close()).unwrap();

    // Replay the received subset locally after closing the publishing peer.
    // This models queued live data and deterministically prevents recovery
    // from racing with peer shutdown and fetching the missing fragment first.
    ztimeout!(subscriber_session
        .put(key, "base")
        .source_info(SourceInfo::new(source_id, 0)))
    .unwrap();
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .source_info()
            .unwrap()
            .source_sn(),
        0
    );
    ztimeout!(subscriber_session
        .put(key, "abcd")
        .source_info(SourceInfo::new(source_id, 1))
        .frag_info(FragInfo::new(2, 0)))
    .unwrap();
    ztimeout!(subscriber_session
        .put(key, "next")
        .source_info(SourceInfo::new(source_id, 2)))
    .unwrap();
    let next = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(next.source_info().unwrap().source_sn(), 2);
    assert_eq!(next.payload().try_to_string().unwrap(), "next");
    assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(subscriber_session.close()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_max_fragments_enforced() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, peer2) = create_peer_pair().await;

    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/max")
        .advanced()
        .max_fragments(4))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/max")
        .advanced()
        .fragmentation(1)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();

    // 10 bytes -> 10 fragments, but subscriber only accepts up to 4.
    ztimeout!(publ.put("HelloWorld")).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragmentation_builder_order_independent() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, _peer2) = create_peer_pair().await;

    // `.fragmentation()` before `.sample_miss_detection()` must resolve
    // successfully now that validation is deferred.
    let _publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/order")
        .advanced()
        .fragmentation(4)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();
}

// A multi-slice `ZBytes` payload must be fragmented and reassembled
// correctly; a multi-slice payload at or below the threshold must be
// delivered as a single unfragmented sample.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragmentation_multislice_payload() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, peer2) = create_peer_pair().await;

    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/multislice")
        .advanced())
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/multislice")
        .advanced()
        .fragmentation(4)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();

    // 24 bytes across two slices, fragmentation(4) -> 6 fragments.
    let mut writer = zenoh::bytes::ZBytes::writer();
    writer.append(zenoh::bytes::ZBytes::from("abcdefghijklmnop"));
    writer.append(zenoh::bytes::ZBytes::from("qrstuvwx"));
    let payload = writer.finish();
    ztimeout!(publ.put(payload.clone())).unwrap();

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.payload(), &payload);
    assert!(sub.try_recv().unwrap().is_none());

    // 4 bytes across two slices, exactly at the threshold: single sample.
    let mut writer = zenoh::bytes::ZBytes::writer();
    writer.append(zenoh::bytes::ZBytes::from("ab"));
    writer.append(zenoh::bytes::ZBytes::from("cd"));
    let payload = writer.finish();
    ztimeout!(publ.put(payload.clone())).unwrap();

    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.payload(), &payload);
    assert!(sample.frag_info().is_none());
    assert!(sub.try_recv().unwrap().is_none());
}

// In-order fragmented publications under no-loss conditions must not trigger
// recovery queries. Delivery cancels the fragment recovery timer once no
// incomplete samples remain.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_no_recovery_query_on_in_order_fragmentation() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, peer2) = create_peer_pair().await;

    // Spy queryable counting any query reaching the @adv space. Sample-recovery
    // and fragment-recovery queries are the only queries this
    // setup can emit: no history is configured on the subscriber.
    let query_count = Arc::new(AtomicUsize::new(0));
    let _spy = ztimeout!(peer2
        .declare_queryable("test/fragmentation/noquery/@adv/**")
        .callback({
            let query_count = query_count.clone();
            move |_| {
                query_count.fetch_add(1, Ordering::Relaxed);
            }
        }))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/noquery")
        .advanced()
        .recovery(RecoveryConfig::default()))
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/noquery")
        .advanced()
        .fragmentation(4)
        .cache(CacheConfig::default().max_samples(10))
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();

    for i in 0..5 {
        let payload = format!("HelloWorldFromFragmentationTest{i}");
        ztimeout!(publ.put(payload.clone())).unwrap();
    }

    // Wait beyond the fragment recovery delay (1s by default) to make any
    // unintended recovery queries observable.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // All samples must have been delivered in order via the live path.
    for i in 0..5 {
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(
            sample.payload().try_to_string().unwrap().as_ref(),
            format!("HelloWorldFromFragmentationTest{i}")
        );
    }
    assert!(sub.try_recv().unwrap().is_none());

    // No recovery query must be emitted for in-order fragmented
    // publications without loss.
    assert_eq!(
        query_count.load(Ordering::Relaxed),
        0,
        "recovery queries were emitted for in-order fragmented publications"
    );
}

// Fragmented samples manually published without `source_info` cannot be
// reassembled by an AdvancedSubscriber and must be dropped: each fragment is
// discarded on arrival instead of being delivered raw.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_orphan_fragment_dropped() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, peer2) = create_peer_pair().await;

    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/orphan")
        .advanced())
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    // Regular publisher: no sample_miss_detection, hence no `source_info`.
    let publ = ztimeout!(peer1.declare_publisher("test/fragmentation/orphan")).unwrap();

    ztimeout!(publ.put("frag0").frag_info(FragInfo::new(2, 0))).unwrap();
    ztimeout!(publ.put("frag1").frag_info(FragInfo::new(2, 1))).unwrap();
    tokio::time::sleep(SLEEP).await;

    assert!(sub.try_recv().unwrap().is_none());
}

// Fragments beyond a subscriber's cap (`max_fragments`, default 4096) are
// enforced on the subscriber side and are rejected on arrival: the
// publication succeeds on the publisher side, and no sample is delivered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_subscriber_drops_fragments_beyond_cap() {
    zenoh_util::init_log_from_env_or("error");
    let (peer1, peer2) = create_peer_pair().await;

    let sub = ztimeout!(peer2
        .declare_subscriber("test/fragmentation/refuse")
        .advanced())
    .unwrap();
    tokio::time::sleep(SLEEP).await;

    let publ = ztimeout!(peer1
        .declare_publisher("test/fragmentation/refuse")
        .advanced()
        .fragmentation(1)
        .sample_miss_detection(MissDetectionConfig::default()))
    .unwrap();

    // 4097 bytes with fragmentation(1) -> 4097 fragments > default cap.
    ztimeout!(publ.put("A".repeat(4097))).unwrap();
    tokio::time::sleep(SLEEP).await;
    assert!(
        sub.try_recv().unwrap().is_none(),
        "fragments beyond the subscriber's cap must not be delivered"
    );

    // The publisher stays usable: an in-bounds publication is delivered.
    ztimeout!(publ.put("ok")).unwrap();
    tokio::time::sleep(SLEEP).await;
    let sample = ztimeout!(sub.recv_async()).unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "ok");
    assert!(sub.try_recv().unwrap().is_none());
}
