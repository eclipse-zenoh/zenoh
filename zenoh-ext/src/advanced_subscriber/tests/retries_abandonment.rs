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

/// Empty, erroneous, partial, timed-out and unavailable replies must release a
/// complete successor. Late replies/publications cannot resurrect its predecessor.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_failed_fragment_recovery_unblocks_successor() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/failure").unwrap();
    for failure in ["empty", "error", "partial", "timeout", "unavailable"] {
        let mut cache =
            Some(ztimeout!(session.declare_queryable("test/ext/frag/failure/@adv/**")).unwrap());
        let sub = ztimeout!(session
            .declare_subscriber(&key)
            .advanced()
            .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(20)))
            .query_timeout(Duration::from_millis(150)))
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
        if failure == "unavailable" {
            drop(cache.take());
        }
        ztimeout!(session
            .put(&key, "ab")
            .source_info(SourceInfo::new(id, 1))
            .frag_info(FragInfo::new(3, 0)))
        .unwrap();
        ztimeout!(session
            .put(&key, "next")
            .source_info(SourceInfo::new(id, 2)))
        .unwrap();
        let mut held_query = None;
        if let Some(cache) = cache.as_ref() {
            let query = ztimeout!(cache.recv_async()).unwrap();
            assert_eq!(query.parameters().get("_fn"), Some("1.."));
            assert!(sub.try_recv().unwrap().is_none());
            match failure {
                "error" => {
                    ztimeout!(query.reply_err("unavailable")).unwrap();
                }
                "partial" => {
                    ztimeout!(query.reply_sample(fragment(&key, id, 1, 1, 3, "cd"))).unwrap();
                }
                _ => {}
            }
            if failure == "timeout" {
                held_query = Some(query);
            } else {
                drop(query);
            }
        }
        let next = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(next.source_info().unwrap().source_sn(), 2, "{failure}");
        assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
        if let Some(query) = held_query {
            let _ = query.reply_sample(fragment(&key, id, 1, 2, 3, "ef")).wait();
        }
        for num in 0..3 {
            ztimeout!(session
                .put(&key, "late")
                .source_info(SourceInfo::new(id, 1))
                .frag_info(FragInfo::new(3, num)))
            .unwrap();
        }
        assert!(sub.try_recv().unwrap().is_none());
        assert!(misses.try_recv().unwrap().is_none());
        if let Some(cache) = cache.as_ref() {
            assert!(
                cache.try_recv().unwrap().is_none(),
                "overlapping/retried query: {failure}"
            );
        }
    }
    ztimeout!(session.close()).unwrap();
}

/// Preserve the full failure × recovery-path matrix for a final sample with no
/// successor: live fragments, fragmented sample replies and whole sample replies.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_final_fragmented_sample_recovers_after_failure() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/final_retry").unwrap();
    for failure in ["empty", "error", "timeout"] {
        for recovery in [
            "live",
            "sample-query-fragments",
            "sample-query-unfragmented",
        ] {
            let cache =
                ztimeout!(session.declare_queryable("test/ext/frag/final_retry/@adv/**")).unwrap();
            let sub = ztimeout!(session
                .declare_subscriber(&key)
                .advanced()
                .recovery(
                    RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(60))
                )
                .query_timeout(Duration::from_millis(100)))
            .unwrap();
            let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
            ztimeout!(session
                .put(&key, "base")
                .source_info(SourceInfo::new(id, 0)))
            .unwrap();
            ztimeout!(sub.recv_async()).unwrap();
            ztimeout!(session
                .put(&key, "ef")
                .source_info(SourceInfo::new(id, 1))
                .frag_info(FragInfo::new(3, 2)))
            .unwrap();
            let query = ztimeout!(cache.recv_async()).unwrap();
            assert_eq!(query.parameters().get("_fn"), Some("0..1"));
            if failure == "error" {
                ztimeout!(query.reply_err("temporarily unavailable")).unwrap();
            }
            let held = if failure == "timeout" {
                Some(query)
            } else {
                drop(query);
                None
            };
            recovery_failed(&sub.statesref, id, 1).await;
            drop(held);
            {
                let states = zlock!(sub.statesref);
                let slot =
                    &states.sequenced_states.peek(&id).unwrap().pending_samples[&WrappingSn(1)];
                assert!(slot.is_incomplete());
                assert!(!slot.is_abandoned());
                let PendingSample::Fragmented(assembly) = slot else {
                    panic!("expected retained partial assembly");
                };
                assert_eq!(assembly.frags.iter().flatten().count(), 1);
            }
            assert!(sub.try_recv().unwrap().is_none());
            assert!(misses.try_recv().unwrap().is_none());
            assert!(cache.try_recv().unwrap().is_none());
            if recovery == "live" {
                for (num, payload) in [(0, "ab"), (1, "cd")] {
                    ztimeout!(session
                        .put(&key, payload)
                        .source_info(SourceInfo::new(id, 1))
                        .frag_info(FragInfo::new(3, num)))
                    .unwrap();
                }
            } else {
                RepliesHandler::periodic(&sub.statesref, id);
                let query = ztimeout!(cache.recv_async()).unwrap();
                assert_eq!(query.parameters().get("_sn"), Some("1.."));
                assert!(query.parameters().get("_fn").is_none());
                if recovery == "sample-query-unfragmented" {
                    ztimeout!(query.reply_sample(whole(&key, id, 1, "abcdef"))).unwrap();
                } else {
                    for (num, payload) in [(0, "ab"), (1, "cd")] {
                        ztimeout!(query.reply_sample(fragment(&key, id, 1, num, 3, payload)))
                            .unwrap();
                    }
                }
                drop(query);
            }
            let sample = ztimeout!(sub.recv_async()).unwrap();
            assert_eq!(sample.source_info().unwrap().source_sn(), 1);
            assert_eq!(sample.payload().try_to_string().unwrap(), "abcdef");
            assert!(sub.try_recv().unwrap().is_none());
            assert!(misses.try_recv().unwrap().is_none());
        }
    }
    ztimeout!(session.close()).unwrap();
}

/// Only a complete successor makes a retained failure terminal, including wraparound.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_later_complete_successor_abandons_failed_sample() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = "test/ext/frag/later_successor";
    for baseline in [0u32, u32::MAX - 1] {
        let partial = baseline.wrapping_add(1);
        let successor = baseline.wrapping_add(2);
        let cache =
            ztimeout!(session.declare_queryable("test/ext/frag/later_successor/@adv/**")).unwrap();
        let sub = ztimeout!(session
            .declare_subscriber(key)
            .advanced()
            .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(60))))
        .unwrap();
        let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
        ztimeout!(session
            .put(key, "base")
            .source_info(SourceInfo::new(id, baseline)))
        .unwrap();
        ztimeout!(sub.recv_async()).unwrap();
        ztimeout!(session
            .put(key, "b")
            .source_info(SourceInfo::new(id, partial))
            .frag_info(FragInfo::new(2, 1)))
        .unwrap();
        drop(ztimeout!(cache.recv_async()).unwrap());
        recovery_failed(&sub.statesref, id, partial).await;
        ztimeout!(session
            .put(key, "c")
            .source_info(SourceInfo::new(id, successor))
            .frag_info(FragInfo::new(2, 0)))
        .unwrap();
        {
            let states = zlock!(sub.statesref);
            let source = states.sequenced_states.peek(&id).unwrap();
            assert!(source.pending_samples[&WrappingSn(partial)].can_abandon());
            assert_eq!(source.pending_samples.len(), 2);
            assert_eq!(source.last_delivered, Some(WrappingSn(baseline)));
        }
        assert!(misses.try_recv().unwrap().is_none());
        ztimeout!(session
            .put(key, "d")
            .source_info(SourceInfo::new(id, successor))
            .frag_info(FragInfo::new(2, 1)))
        .unwrap();
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), successor);
        assert_eq!(sample.payload().try_to_string().unwrap(), "cd");
        assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
        ztimeout!(session
            .put(key, "a")
            .source_info(SourceInfo::new(id, partial))
            .frag_info(FragInfo::new(2, 0)))
        .unwrap();
        assert!(sub.try_recv().unwrap().is_none());
        assert!(misses.try_recv().unwrap().is_none());
        assert!(cache.try_recv().unwrap().is_none());
    }
    ztimeout!(session.close()).unwrap();
}

/// Automatic retries remain exclusive; a successor waits for an in-flight retry.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_automatic_fragment_retry_can_win_against_successor() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/automatic_retry").unwrap();
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/automatic_retry/@adv/**")).unwrap();
    let sub = ztimeout!(session
        .declare_subscriber(&key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(50))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session
        .put(&key, "base")
        .source_info(SourceInfo::new(id, 0)))
    .unwrap();
    ztimeout!(sub.recv_async()).unwrap();
    ztimeout!(session
        .put(&key, "ef")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(3, 2)))
    .unwrap();
    drop(ztimeout!(cache.recv_async()).unwrap());
    let retry = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(retry.parameters().get("_sn"), Some("1..1"));
    assert_eq!(retry.parameters().get("_fn"), Some("0..1"));
    ztimeout!(session
        .put(&key, "next")
        .source_info(SourceInfo::new(id, 2)))
    .unwrap();
    for _ in 0..5 {
        ztimeout!(session
            .put(&key, "ef")
            .source_info(SourceInfo::new(id, 1))
            .frag_info(FragInfo::new(3, 2)))
        .unwrap();
    }
    assert!(sub.try_recv().unwrap().is_none());
    assert!(cache.try_recv().unwrap().is_none());
    assert_eq!(
        zlock!(sub.statesref)
            .sequenced_states
            .peek(&id)
            .unwrap()
            .pending_queries,
        1
    );
    for (num, payload) in [(0, "ab"), (1, "cd")] {
        ztimeout!(retry.reply_sample(fragment(&key, id, 1, num, 3, payload))).unwrap();
    }
    drop(retry);
    for (sn, payload) in [(1, "abcdef"), (2, "next")] {
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
    }
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// Both scan ticks and duplicates must wait for a running sample-recovery query.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_retry_waits_for_sample_recovery_query() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/retry_overlap").unwrap();
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/retry_overlap/@adv/**")).unwrap();
    let delay = Duration::from_millis(200);
    let sub = ztimeout!(session
        .declare_subscriber(&key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(delay)))
    .unwrap();
    ztimeout!(session
        .put(&key, "base")
        .source_info(SourceInfo::new(id, 0)))
    .unwrap();
    ztimeout!(sub.recv_async()).unwrap();
    ztimeout!(session
        .put(&key, "ef")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(3, 2)))
    .unwrap();
    drop(ztimeout!(cache.recv_async()).unwrap());
    recovery_failed(&sub.statesref, id, 1).await;
    RepliesHandler::periodic(&sub.statesref, id);
    let sample_query = ztimeout!(cache.recv_async()).unwrap();
    assert!(sample_query.parameters().get("_fn").is_none());
    assert!(tokio::time::timeout(delay * 3, cache.recv_async())
        .await
        .is_err());
    for _ in 0..5 {
        ztimeout!(session
            .put(&key, "ef")
            .source_info(SourceInfo::new(id, 1))
            .frag_info(FragInfo::new(3, 2)))
        .unwrap();
    }
    assert!(cache.try_recv().unwrap().is_none());
    assert_eq!(
        zlock!(sub.statesref)
            .sequenced_states
            .peek(&id)
            .unwrap()
            .pending_queries,
        1
    );
    drop(sample_query);
    let retry = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(retry.parameters().get("_fn"), Some("0..1"));
    for (num, payload) in [(0, "ab"), (1, "cd")] {
        ztimeout!(retry.reply_sample(fragment(&key, id, 1, num, 3, payload))).unwrap();
    }
    drop(retry);
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap(),
        "abcdef"
    );
    ztimeout!(session.close()).unwrap();
}

/// Abandonment stays terminal while a different partial blocks the watermark.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_abandoned_sample_cannot_be_resurrected() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = "test/ext/frag/abandoned";
    let cache = ztimeout!(session.declare_queryable("test/ext/frag/abandoned/@adv/**")).unwrap();
    let sub = ztimeout!(session
        .declare_subscriber(key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(10))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session.put(key, "base").source_info(SourceInfo::new(id, 0))).unwrap();
    ztimeout!(sub.recv_async()).unwrap();
    ztimeout!(session
        .put(key, "b")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    ztimeout!(session
        .put(key, "c")
        .source_info(SourceInfo::new(id, 2))
        .frag_info(FragInfo::new(2, 0)))
    .unwrap();
    ztimeout!(session.put(key, "next").source_info(SourceInfo::new(id, 3))).unwrap();
    drop(ztimeout!(cache.recv_async()).unwrap());
    wait_for(|| {
        zlock!(sub.statesref)
            .sequenced_states
            .peek(&id)
            .unwrap()
            .pending_samples
            .get(&WrappingSn(1))
            .is_some_and(PendingSample::is_abandoned)
    })
    .await;
    for num in 0..2 {
        ztimeout!(session
            .put(key, "late")
            .source_info(SourceInfo::new(id, 1))
            .frag_info(FragInfo::new(2, num)))
        .unwrap();
    }
    ztimeout!(session
        .put(key, "whole late sample")
        .source_info(SourceInfo::new(id, 1)))
    .unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(cache.try_recv().unwrap().is_none());
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    drop(cache);
    ztimeout!(session
        .put(key, "d")
        .source_info(SourceInfo::new(id, 2))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .source_info()
            .unwrap()
            .source_sn(),
        2
    );
    assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .source_info()
            .unwrap()
            .source_sn(),
        3
    );
    ztimeout!(session.close()).unwrap();
}

/// An abandoned first sample must not block the first delivery baseline.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_abandoned_first_sample_allows_fragmented_successor() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = "test/ext/frag/first_abandoned";
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/first_abandoned/@adv/**")).unwrap();
    let sub = ztimeout!(session
        .declare_subscriber(key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(10))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session
        .put(key, "b")
        .source_info(SourceInfo::new(id, 0))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    drop(ztimeout!(cache.recv_async()).unwrap());
    recovery_failed(&sub.statesref, id, 0).await;
    ztimeout!(session
        .put(key, "c")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(2, 0)))
    .unwrap();
    // Whole successors also wait before any baseline is established.
    ztimeout!(session.put(key, "next").source_info(SourceInfo::new(id, 2))).unwrap();
    assert!(sub.try_recv().unwrap().is_none());
    ztimeout!(session
        .put(key, "d")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    let sample = tokio::time::timeout(Duration::from_secs(2), sub.recv_async())
        .await
        .expect("abandoned first sample blocked a complete successor")
        .unwrap();
    assert_eq!(sample.source_info().unwrap().source_sn(), 1);
    assert_eq!(sample.payload().try_to_string().unwrap(), "cd");
    assert_eq!(
        ztimeout!(sub.recv_async())
            .unwrap()
            .source_info()
            .unwrap()
            .source_sn(),
        2
    );
    assert!(misses.try_recv().unwrap().is_none());
    assert!(cache.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// Failing SN 1 cannot discard recoverable SN 2 merely because SN 3 is complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_failure_preserves_other_partial_sample() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = "test/ext/frag/failure_with_active_tail";
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/failure_with_active_tail/@adv/**"))
            .unwrap();
    let sub = ztimeout!(session
        .declare_subscriber(key)
        .advanced()
        .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(10))))
    .unwrap();
    let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
    ztimeout!(session.put(key, "base").source_info(SourceInfo::new(id, 0))).unwrap();
    ztimeout!(sub.recv_async()).unwrap();
    ztimeout!(session
        .put(key, "b")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    let failed_query = ztimeout!(cache.recv_async()).unwrap();
    ztimeout!(session
        .put(key, "c")
        .source_info(SourceInfo::new(id, 2))
        .frag_info(FragInfo::new(2, 0)))
    .unwrap();
    ztimeout!(session.put(key, "next").source_info(SourceInfo::new(id, 3))).unwrap();
    drop(failed_query);
    queries_finished(&sub.statesref, id).await;
    {
        let states = zlock!(sub.statesref);
        let source = states.sequenced_states.peek(&id).unwrap();
        assert_eq!(source.last_delivered, Some(WrappingSn(0)));
        assert_eq!(
            source
                .pending_samples
                .iter()
                .map(|(sn, s)| (*sn, s.is_abandoned(), s.is_complete()))
                .collect::<Vec<_>>(),
            [
                (WrappingSn(1), true, false),
                (WrappingSn(2), false, false),
                (WrappingSn(3), false, true)
            ]
        );
    }
    // Abandoned SN 1 must not make pending SN 2 look entirely missing.
    RepliesHandler::recover_gap(&sub.statesref, id);
    assert!(cache.try_recv().unwrap().is_none());
    assert!(sub.try_recv().unwrap().is_none());
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(session
        .put(key, "d")
        .source_info(SourceInfo::new(id, 2))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    for (sn, payload) in [(2, "cd"), (3, "next")] {
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
    }
    assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
    assert!(misses.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// A known abandonment must not hide an entirely missing sequence number.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_failure_recovers_unknown_gap_after_abandoned_prefix() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/abandoned_then_gap").unwrap();
    let cache =
        ztimeout!(session.declare_queryable("test/ext/frag/abandoned_then_gap/@adv/**")).unwrap();
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
    ztimeout!(session
        .put(&key, "b")
        .source_info(SourceInfo::new(id, 1))
        .frag_info(FragInfo::new(2, 1)))
    .unwrap();
    let failed_query = ztimeout!(cache.recv_async()).unwrap();
    ztimeout!(session
        .put(&key, "three")
        .source_info(SourceInfo::new(id, 3)))
    .unwrap();
    drop(failed_query);
    let gap_query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(gap_query.parameters().get("_sn"), Some("2.."));
    assert_eq!(gap_query.parameters().get("_fn"), None);
    assert!(sub.try_recv().unwrap().is_none());
    ztimeout!(gap_query.reply_sample(whole(&key, id, 2, "two"))).unwrap();
    drop(gap_query);
    for (sn, payload) in [(2, "two"), (3, "three")] {
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), payload);
    }
    assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
    assert!(misses.try_recv().unwrap().is_none());
    assert!(cache.try_recv().unwrap().is_none());
    ztimeout!(session.close()).unwrap();
}

/// Old attempt completion after GC must not change a recreated source's count
/// or abandon its assembly at the same SN. Generation identity is Arc identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_recovery_ignores_recreated_source() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/pending").unwrap();
    let statesref = frag_recovery_state(&session, &key, Arc::new(Mutex::new(Vec::new())));
    let handler = {
        let mut states = zlock!(statesref);
        handle_sample(&mut states, fragment(&key, id, 1, 0, 2, "a"));
        let source = states.sequenced_states.peek_mut(&id).unwrap();
        let attempt = source
            .pending_samples
            .get_mut(&WrappingSn(1))
            .unwrap()
            .prepare_recovery(Duration::ZERO, RecoveryTrigger::TimerTick)
            .unwrap()
            .1;
        source.pending_queries = 1;
        FragRecoveryAttempt {
            source_id: id,
            statesref: statesref.clone(),
            generation: source.generation.clone(),
            sn: WrappingSn(1),
            ranges: vec![FragRange::new(1, None)],
            attempt,
        }
    };
    {
        let mut states = zlock!(statesref);
        states.sequenced_states.pop(&id);
        handle_sample(&mut states, fragment(&key, id, 1, 0, 2, "new"));
        states
            .sequenced_states
            .peek_mut(&id)
            .unwrap()
            .pending_queries = 5;
    }
    drop(handler);
    {
        let states = zlock!(statesref);
        let source = states.sequenced_states.peek(&id).unwrap();
        assert_eq!(source.pending_queries, 5);
        assert!(source.pending_samples[&WrappingSn(1)].is_incomplete());
    }
    ztimeout!(session.close()).unwrap();
}
