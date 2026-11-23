//
// Copyright (c) 2026 ZettaScale Technology
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

use super::*;

/// Only middle fragments are injected. The actual publisher cache must supply
/// both missing ranges; arming for a newer sample must preserve the older one.
async fn check_cached_recovery(payloads: &[&str]) {
    let session = ztimeout!(zenoh::open(peer_config())).unwrap();
    let key = KeyExpr::try_from("test/ext/frag/recovery").unwrap();
    let selectors = Arc::new(Mutex::new(Vec::<String>::new()));
    let _spy = ztimeout!(session
        .declare_queryable("test/ext/frag/recovery/@adv/**")
        .callback({
            let selectors = selectors.clone();
            move |query: Query| {
                selectors.lock().unwrap().push(query.selector().to_string());
                let _ = query.reply_err(ZBytes::new()).wait();
            }
        }))
    .unwrap();
    let publisher = ztimeout!(session
        .declare_publisher(&key)
        .advanced()
        .fragmentation(4)
        .cache(crate::CacheConfig::default().max_samples(10))
        .sample_miss_detection(crate::MissDetectionConfig::default()))
    .unwrap();
    let id = publisher.id();
    for payload in payloads {
        ztimeout!(publisher.put(*payload)).unwrap();
    }
    let received = Arc::new(Mutex::new(Vec::<Sample>::new()));
    let states = frag_recovery_state(&session, &key, received.clone());
    for (sn, payload) in payloads.iter().enumerate() {
        let mut state = zlock!(states);
        let inserted = handle_sample(
            &mut state,
            fragment(&key, id, sn as u32, 1, 3, &payload[4..8]),
        );
        assert_eq!(inserted.new_source, sn == 0);
        assert!(inserted.new_fragment_slot);
        state
            .sequenced_states
            .get_mut(&id)
            .unwrap()
            .arm_fragment_recovery(&states, id, Duration::from_millis(500));
    }
    wait_for(|| received.lock().unwrap().len() >= payloads.len()).await;
    {
        let received = received.lock().unwrap();
        assert_eq!(received.len(), payloads.len());
        let delivered: Vec<_> = received
            .iter()
            .map(|s| s.payload().try_to_string().unwrap().into_owned())
            .collect();
        assert_eq!(
            delivered, payloads,
            "all fragmented samples must be recovered in order"
        );
        let mut states = zlock!(states);
        let source = states.sequenced_states.get(&id).unwrap();
        assert!(source.pending_samples.is_empty());
        assert_eq!(
            source.last_delivered,
            Some(WrappingSn(payloads.len() as u32 - 1))
        );
        let selectors = selectors.lock().unwrap();
        for sn in 0..payloads.len() {
            assert!(
                selectors
                    .iter()
                    .filter(|q| q.contains(&format!("_sn={sn}..{sn}")))
                    .count()
                    >= 2,
                "one query per missing range expected: {selectors:?}"
            );
        }
        assert!(
            selectors.iter().any(|q| q.contains("_fn=0..0;")),
            "{selectors:?}"
        );
        assert!(
            selectors.iter().any(|q| q.contains("_fn=2..;")),
            "{selectors:?}"
        );
    }
    ztimeout!(session.close()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_recovery_of_missing_fragments() {
    check_cached_recovery(&[PAYLOAD]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_recovers_multiple_incomplete_samples() {
    check_cached_recovery(&[PAYLOAD, "abcdefghijkl"]).await;
}

/// Fragment completion must resume recovery of a preceding entirely missing
/// sequence number without needing a new publication, heartbeat, or timer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_resumes_sample_recovery() {
    let session = isolated_session().await;
    let key = "test/ext/frag/sequence_gap";
    let publisher = ztimeout!(session
        .declare_publisher(key)
        .advanced()
        .fragmentation(4)
        .cache(crate::CacheConfig::default().max_samples(3))
        .sample_miss_detection(crate::MissDetectionConfig::default()))
    .unwrap();
    let id = publisher.id();
    for payload in ["base", "lost", PAYLOAD] {
        ztimeout!(publisher.put(payload)).unwrap();
    }
    let sub = ztimeout!(session
        .declare_subscriber(key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(50))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session.put(key, "base").source_info(SourceInfo::new(id, 0))).unwrap();
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .source_info()
            .unwrap()
            .source_sn(),
        0
    );
    ztimeout!(session
        .put(key, "4567")
        .source_info(SourceInfo::new(id, 2))
        .frag_info(FragInfo::new(3, 1)))
    .unwrap();
    for (sn, payload) in [(1, "lost"), (2, PAYLOAD)] {
        let received = tokio::time::timeout(Duration::from_secs(5), sub.recv_async()).await;
        if received.is_err() {
            let diagnostic = {
                let states = zlock!(sub.statesref);
                let source = states.sequenced_states.peek(&id).unwrap();
                format!("SN {sn} not delivered after fragment recovery: last_delivered={:?}, pending_queries={}, pending_samples={:?}",
                    source.last_delivered, source.pending_queries,
                    source.pending_samples.iter().map(|(sn, s)| (*sn, s.is_complete())).collect::<Vec<_>>())
            };
            ztimeout!(session.close()).unwrap();
            panic!("{diagnostic}");
        }
        let sample = received.unwrap().unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
    }
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// Duplicates must not postpone the stalled-tail recovery deadline.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_tail_survives_duplicate_activity() {
    let delay = Duration::from_millis(50);
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/duplicate_tail").unwrap();
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/duplicate_tail/@adv/**")).unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let statesref = frag_recovery_state(&session, &key, received.clone());
    {
        let mut states = zlock!(statesref);
        handle_sample(&mut states, whole(&key, id, 0, "base"));
        let fragment = fragment(&key, id, 1, 0, 2, "ab");
        let inserted = handle_sample(&mut states, fragment.clone());
        let source = states.sequenced_states.peek_mut(&id).unwrap();
        assert!(source
            .on_fragment(
                &statesref,
                id,
                WrappingSn(1),
                inserted.new_fragment_slot,
                delay
            )
            .is_none());
        let PendingSample::Fragmented(assembly) =
            source.pending_samples.get_mut(&WrappingSn(1)).unwrap()
        else {
            panic!("expected partial assembly");
        };
        assembly.last_progress = Instant::now() - Duration::from_secs(1);
        // Age and duplicate under one lock; the scan cannot run between them.
        for _ in 0..5 {
            handle_sample(&mut states, fragment.clone());
        }
        assert_eq!(
            states.sequenced_states.peek(&id).unwrap().pending_samples[&WrappingSn(1)]
                .clone()
                .prepare_recovery(delay, RecoveryTrigger::TimerTick)
                .map(|(ranges, _)| ranges),
            Some(vec![FragRange::new(1, None)])
        );
    }
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some("1..1"));
    assert_eq!(query.parameters().get("_fn"), Some("1.."));
    ztimeout!(query.reply_sample(fragment(&key, id, 1, 1, 2, "cd"))).unwrap();
    drop(query);
    wait_for(|| received.lock().unwrap().len() == 2).await;
    {
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].source_info().unwrap().source_sn(), 0);
        assert_eq!(received[1].source_info().unwrap().source_sn(), 1);
        assert_eq!(received[1].payload().try_to_string().unwrap(), "abcd");
    }
    assert!(cache.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// An out-of-order fragment opens an immediate hole query. Neither later live
/// fragments nor scan ticks may overlap that held attempt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_immediate_hole_query() {
    const DELAY: Duration = Duration::from_millis(300);
    let session = ztimeout!(zenoh::open(peer_config())).unwrap();
    let held = Arc::new(Mutex::new(Vec::<Query>::new()));
    let selectors = Arc::new(Mutex::new(Vec::<String>::new()));
    let _spy = ztimeout!(session
        .declare_queryable("test/ext/frag/hole/@adv/**")
        .callback({
            let held = held.clone();
            let selectors = selectors.clone();
            move |query: Query| {
                selectors.lock().unwrap().push(query.selector().to_string());
                held.lock().unwrap().push(query);
            }
        }))
    .unwrap();
    let publisher = ztimeout!(session
        .declare_publisher("test/ext/frag/hole")
        .advanced()
        .fragmentation(4)
        .cache(crate::CacheConfig::default().max_samples(10))
        .sample_miss_detection(crate::MissDetectionConfig::default()))
    .unwrap();
    let id = publisher.id();
    let key = KeyExpr::try_from("test/ext/frag/hole").unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let states = frag_recovery_state(&session, &key, received.clone());
    feed_fragment(&states, &key, id, "0123", 5, 0, 0, DELAY);
    feed_fragment(&states, &key, id, "4567", 5, 1, 0, DELAY);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        selectors.lock().unwrap().is_empty(),
        "sequential fragments must not trigger recovery"
    );
    feed_fragment(&states, &key, id, "CDEF", 5, 3, 0, DELAY);
    tokio::time::sleep(Duration::from_millis(50)).await;
    {
        let queries = selectors.lock().unwrap();
        assert_eq!(
            queries.iter().filter(|q| q.contains("_fn=2..2")).count(),
            1,
            "{queries:?}"
        );
        assert!(
            queries.iter().all(|q| q.contains("_sn=0..0")),
            "{queries:?}"
        );
    }
    feed_fragment(&states, &key, id, "GHIJ", 5, 4, 0, DELAY);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        selectors
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.contains("_fn=2..2"))
            .count(),
        1,
        "a recorded hole must not be re-queried immediately"
    );
    tokio::time::sleep(DELAY + Duration::from_millis(100)).await;
    assert_eq!(
        selectors
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.contains("_fn=2..2"))
            .count(),
        1,
        "the scan must not overlap the outstanding attempt"
    );
    feed_fragment(&states, &key, id, "89AB", 5, 2, 0, DELAY);
    wait_for(|| !received.lock().unwrap().is_empty()).await;
    assert_eq!(
        received.lock().unwrap()[0]
            .payload()
            .try_to_string()
            .unwrap(),
        "0123456789ABCDEFGHIJ"
    );
    held.lock().unwrap().clear();
    {
        let mut states = zlock!(states);
        let source = states.sequenced_states.get(&id).unwrap();
        assert!(source.pending_samples.is_empty());
        assert!(source.frag_recovery_task.is_none());
    }
    ztimeout!(session.close()).unwrap();
}

/// Tail recovery waits for the sequential stream to stall, including the first
/// scan tick after a newer fragment has refreshed progress.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_tail_queried_when_stalled() {
    const DELAY: Duration = Duration::from_millis(300);
    let session = ztimeout!(zenoh::open(peer_config())).unwrap();
    let held = Arc::new(Mutex::new(Vec::<Query>::new()));
    let selectors = Arc::new(Mutex::new(Vec::<String>::new()));
    let _spy = ztimeout!(session
        .declare_queryable("test/ext/frag/tail/@adv/**")
        .callback({
            let held = held.clone();
            let selectors = selectors.clone();
            move |query: Query| {
                selectors.lock().unwrap().push(query.selector().to_string());
                held.lock().unwrap().push(query);
            }
        }))
    .unwrap();
    let publisher = ztimeout!(session
        .declare_publisher("test/ext/frag/tail")
        .advanced()
        .fragmentation(4)
        .cache(crate::CacheConfig::default().max_samples(10))
        .sample_miss_detection(crate::MissDetectionConfig::default()))
    .unwrap();
    let id = publisher.id();
    let key = KeyExpr::try_from("test/ext/frag/tail").unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let states = frag_recovery_state(&session, &key, received.clone());
    feed_fragment(&states, &key, id, "0123", 4, 0, 0, DELAY);
    tokio::time::sleep(Duration::from_millis(100)).await;
    feed_fragment(&states, &key, id, "4567", 4, 1, 0, DELAY);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        selectors.lock().unwrap().is_empty(),
        "active sequential stream queried"
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        selectors.lock().unwrap().is_empty(),
        "active tail queried on first scan tick"
    );
    let mut tail_queried = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if selectors
            .lock()
            .unwrap()
            .iter()
            .any(|q| q.contains("_fn=2..") && !q.contains("_fn=2..2"))
        {
            tail_queried = true;
            break;
        }
    }
    assert!(
        tail_queried,
        "stalled tail must be queried: {:?}",
        selectors.lock().unwrap()
    );
    feed_fragment(&states, &key, id, "89AB", 4, 2, 0, DELAY);
    feed_fragment(&states, &key, id, "CDEF", 4, 3, 0, DELAY);
    wait_for(|| !received.lock().unwrap().is_empty()).await;
    assert_eq!(
        received.lock().unwrap()[0]
            .payload()
            .try_to_string()
            .unwrap(),
        "0123456789ABCDEF"
    );
    held.lock().unwrap().clear();
    {
        let mut states = zlock!(states);
        let source = states.sequenced_states.get(&id).unwrap();
        assert!(source.pending_samples.is_empty());
        assert!(source.frag_recovery_task.is_none());
    }
    ztimeout!(session.close()).unwrap();
}

/// A successful hole query does not abandon an unrequested active tail.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_successful_hole_recovery_preserves_active_tail() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/hole_tail").unwrap();
    let cache = ztimeout!(session.declare_queryable("test/ext/frag/hole_tail/@adv/**")).unwrap();
    let sub = ztimeout!(session
        .declare_subscriber(&key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(10))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session
        .put(&key, "base")
        .source_info(SourceInfo::new(id, 0)))
    .unwrap();
    ztimeout!(sub.recv_async()).unwrap();
    for (num, payload) in [(0, "a"), (2, "c")] {
        ztimeout!(session
            .put(&key, payload)
            .source_info(SourceInfo::new(id, 1))
            .frag_info(FragInfo::new(4, num)))
        .unwrap();
    }
    ztimeout!(session
        .put(&key, "next")
        .source_info(SourceInfo::new(id, 2)))
    .unwrap();
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_fn"), Some("1..1"));
    ztimeout!(query.reply_sample(fragment(&key, id, 1, 1, 4, "b"))).unwrap();
    drop(query);
    queries_finished(&sub.statesref, id).await;
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(session
        .put(&key, "d")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(4, 3)))
    .unwrap();
    for (sn, payload) in [(1, "abcd"), (2, "next")] {
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
    }
    assert!(misses.try_recv().unwrap().is_none());
    assert!(cache.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// Disjoint ranges share one attempt; an empty first range cannot finalize it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_waits_for_all_requested_ranges() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/ranges").unwrap();
    let cache = ztimeout!(session.declare_queryable("test/ext/frag/ranges/@adv/**")).unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let statesref = frag_recovery_state(&session, &key, received.clone());
    {
        let mut states = zlock!(statesref);
        handle_sample(&mut states, whole(&key, id, 0, "base"));
        for (num, payload) in [(0, "a"), (2, "c"), (4, "e")] {
            handle_sample(&mut states, fragment(&key, id, 1, num, 5, payload));
        }
        handle_sample(&mut states, whole(&key, id, 2, "next"));
        states
            .sequenced_states
            .peek_mut(&id)
            .unwrap()
            .arm_fragment_recovery(&statesref, id, Duration::from_millis(20));
    }
    let first = ztimeout!(cache.recv_async()).unwrap();
    let second = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(first.parameters().get("_fn"), Some("1..1"));
    assert_eq!(second.parameters().get("_fn"), Some("3..3"));
    drop(first);
    tokio::time::sleep(Duration::from_millis(80)).await;
    {
        let states = zlock!(statesref);
        let source = states.sequenced_states.peek(&id).unwrap();
        assert_eq!(source.pending_queries, 1);
        assert!(source.pending_samples[&WrappingSn(1)].is_incomplete());
    }
    assert_eq!(received.lock().unwrap().len(), 1);
    assert!(cache.try_recv().unwrap().is_none());
    ztimeout!(second.reply_sample(fragment(&key, id, 1, 3, 5, "d"))).unwrap();
    drop(second);
    wait_for(|| received.lock().unwrap().len() == 2).await;
    assert_eq!(
        received.lock().unwrap()[1]
            .source_info()
            .unwrap()
            .source_sn(),
        2
    );
    ztimeout!(session.close()).unwrap();
}

/// A newer complete sample waits while its predecessor is still recoverable.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_recovery_preserves_partial_sample_before_complete_successor() {
    let session = isolated_session().await;
    let key = KeyExpr::try_from("test/ext/frag/preserve_partial").unwrap();
    let id = EntityGlobalId::new(session.zid(), 7);
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/preserve_partial/@adv/**")).unwrap();
    let sub = ztimeout!(session
        .declare_subscriber(&key)
        .advanced()
        .max_pending_samples(10)
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(50))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session
        .put(&key, "base")
        .source_info(SourceInfo::new(id, 0)))
    .unwrap();
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .source_info()
            .unwrap()
            .source_sn(),
        0
    );
    ztimeout!(session
        .put(&key, "ab")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(2, 0)))
    .unwrap();
    ztimeout!(session
        .put(&key, "next")
        .source_info(SourceInfo::new(id, 2)))
    .unwrap();
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some("1..1"));
    assert_eq!(query.parameters().get("_fn"), Some("1.."));
    {
        let states = zlock!(sub.statesref);
        let source = states.sequenced_states.peek(&id).unwrap();
        assert_eq!(source.last_delivered, Some(WrappingSn(0)));
        assert_eq!(
            source
                .pending_samples
                .iter()
                .map(|(sn, s)| (*sn, s.is_complete()))
                .collect::<Vec<_>>(),
            [(WrappingSn(1), false), (WrappingSn(2), true)]
        );
    }
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(query.reply_sample(fragment(&key, id, 1, 1, 2, "cd"))).unwrap();
    drop(query);
    for (sn, payload) in [(1, "abcd"), (2, "next")] {
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
    }
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    assert!(zlock!(sub.statesref)
        .sequenced_states
        .peek(&id)
        .unwrap()
        .pending_samples
        .is_empty());
    ztimeout!(session.close()).unwrap();
}
