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

/// Session-backed fixture for query dispatch and reply-completion tests.
struct QueryFixture {
    states: Arc<Mutex<State>>,
    received: Arc<Mutex<Vec<Sample>>>,
    misses: Arc<Mutex<Vec<u32>>>,
    id: EntityGlobalId,
    key: KeyExpr<'static>,
}

impl QueryFixture {
    fn new(session: &Session, depth: usize) -> Self {
        let key = KeyExpr::try_from("test/ext/pending").unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let misses = Arc::new(Mutex::new(Vec::new()));
        let states = frag_recovery_state(session, &key, received.clone());
        {
            let mut states = zlock!(states);
            states.retransmission = false;
            states.max_pending_samples = depth;
            states.miss_handlers.insert(
                0,
                Callback::from({
                    let misses = misses.clone();
                    move |miss: Miss| misses.lock().unwrap().push(miss.nb())
                }),
            );
        }
        Self {
            states,
            received,
            misses,
            id: EntityGlobalId::new(session.zid(), 7),
            key,
        }
    }

    fn whole(&self, sn: u32, payload: &str) -> Sample {
        whole(&self.key, self.id, sn, payload)
    }

    fn fragment(&self, sn: u32, num: u32, count: u32, payload: &str) -> Sample {
        fragment(&self.key, self.id, sn, num, count, payload)
    }

    fn receive(&self, sample: Sample) -> SampleInsertion {
        handle_sample(&mut zlock!(self.states), sample)
    }

    fn delivered(&self) -> Vec<u32> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.source_info().unwrap().source_sn())
            .collect()
    }

    fn payloads(&self) -> Vec<String> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.payload().try_to_string().unwrap().into_owned())
            .collect()
    }
}

fn finish_sample_query(history: bool, fixture: &QueryFixture) {
    drop(RepliesHandler {
        scope: if history {
            QueryScope::Global
        } else {
            QueryScope::Sequenced(fixture.id)
        },
        statesref: fixture.states.clone(),
    });
}

/// Held periodic queries coalesce; completion authorizes skipping an unavailable
/// SN, and subsequent ticks must still recover later gaps.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_periodic_queries_coalesce_and_flush_unrecoverable_gap() {
    let session = isolated_session().await;
    let cache = ztimeout!(session.declare_queryable("test/ext/pending/@adv/**")).unwrap();
    let fixture = QueryFixture::new(&session, 10);
    zlock!(fixture.states).retransmission = true;
    fixture.receive(fixture.whole(0, "base"));
    fixture.receive(fixture.whole(2, "next"));
    RepliesHandler::periodic(&fixture.states, fixture.id);
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some("1.."));
    for _ in 0..5 {
        RepliesHandler::periodic(&fixture.states, fixture.id);
        let states = zlock!(fixture.states);
        let source = states.sequenced_states.peek(&fixture.id).unwrap();
        assert_eq!(source.pending_queries, 1);
        assert_eq!(source.last_delivered, Some(WrappingSn(0)));
    }
    assert!(cache.try_recv().unwrap().is_none());
    assert_eq!(fixture.received.lock().unwrap().len(), 1);
    drop(query);
    queries_finished(&fixture.states, fixture.id).await;
    assert_eq!(fixture.delivered(), [0, 2]);
    assert_eq!(*fixture.misses.lock().unwrap(), [1]);
    assert!(zlock!(fixture.states)
        .sequenced_states
        .peek(&fixture.id)
        .unwrap()
        .pending_samples
        .is_empty());
    RepliesHandler::periodic(&fixture.states, fixture.id);
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some("3.."));
    assert_eq!(
        zlock!(fixture.states)
            .sequenced_states
            .peek(&fixture.id)
            .unwrap()
            .pending_queries,
        1
    );
    drop(query);
    queries_finished(&fixture.states, fixture.id).await;
    assert_eq!(fixture.received.lock().unwrap().len(), 2);
    assert_eq!(*fixture.misses.lock().unwrap(), [1]);
    ztimeout!(session.close()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_periodic_queries_respect_pending_queries() {
    let session = isolated_session().await;
    let cache = ztimeout!(session.declare_queryable("test/ext/pending/@adv/**")).unwrap();
    let fixture = QueryFixture::new(&session, 10);
    for (global_pending, source_pending) in [(1, 0), (0, 1)] {
        {
            let mut states = zlock!(fixture.states);
            states.global_pending_queries = global_pending;
            states
                .sequenced_states
                .get_or_insert_mut(fixture.id, Default::default)
                .pending_queries = source_pending;
        }
        RepliesHandler::periodic(&fixture.states, fixture.id);
        {
            let states = zlock!(fixture.states);
            assert_eq!(states.global_pending_queries, global_pending);
            assert_eq!(
                states
                    .sequenced_states
                    .peek(&fixture.id)
                    .unwrap()
                    .pending_queries,
                source_pending
            );
        }
        assert!(cache.try_recv().unwrap().is_none());
    }
    zlock!(fixture.states)
        .sequenced_states
        .peek_mut(&fixture.id)
        .unwrap()
        .pending_queries = 0;
    RepliesHandler::periodic(&fixture.states, fixture.id);
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some(".."));
    drop(query);
    ztimeout!(session.close()).unwrap();
}

/// Keep history/sample-query × wraparound × missing-SN × fragment-outcome
/// coverage. Completion authorizes a bounded skip without discarding recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_query_flush_preserves_recoverable_partials() {
    let session = isolated_session().await;
    let cache = ztimeout!(session.declare_queryable("test/ext/pending/@adv/**")).unwrap();
    for history in [false, true] {
        for baseline in [0u32, u32::MAX - 2] {
            for gap in [false, true] {
                for outcome in ["live", "reply", "empty", "timeout"] {
                    let fixture = QueryFixture::new(&session, 10);
                    let partial = baseline.wrapping_add(1);
                    let successor = baseline.wrapping_add(if gap { 3 } else { 2 });
                    let frag =
                        |num| fixture.fragment(partial, num, 2, if num == 0 { "ab" } else { "cd" });
                    {
                        let mut states = zlock!(fixture.states);
                        states.retransmission = true;
                        states.frag_recovery_delay = Duration::from_millis(10);
                        states.query_timeout = Duration::from_millis(100);
                        handle_sample(&mut states, fixture.whole(baseline, "base"));
                        states.global_pending_queries = u64::from(history);
                        handle_sample(&mut states, frag(0));
                        handle_sample(&mut states, fixture.whole(successor, "next"));
                        states
                            .sequenced_states
                            .peek_mut(&fixture.id)
                            .unwrap()
                            .pending_queries = u64::from(!history);
                    }
                    finish_sample_query(history, &fixture);
                    {
                        let mut states = zlock!(fixture.states);
                        let source = states.sequenced_states.peek(&fixture.id).unwrap();
                        assert_eq!(source.last_delivered, Some(WrappingSn(baseline)));
                        assert_eq!(source.pending_samples.len(), 2);
                        assert_eq!(source.gap_skip_through, Some(WrappingSn(successor)));
                        if outcome == "live" {
                            handle_sample(&mut states, frag(1));
                        }
                    }
                    if outcome != "live" {
                        let query = ztimeout!(cache.recv_async()).unwrap();
                        let range = format!("{partial}..{partial}");
                        assert_eq!(query.parameters().get("_sn"), Some(range.as_str()));
                        assert_eq!(query.parameters().get("_fn"), Some("1.."));
                        if outcome == "reply" {
                            ztimeout!(query.reply_sample(frag(1))).unwrap();
                        }
                        if outcome == "timeout" {
                            wait_for(|| fixture.received.lock().unwrap().len() == 2).await;
                        }
                        drop(query);
                    }
                    wait_for(|| {
                        zlock!(fixture.states)
                            .sequenced_states
                            .peek(&fixture.id)
                            .unwrap()
                            .last_delivered
                            == Some(WrappingSn(successor))
                    })
                    .await;
                    let recovered = outcome == "live" || outcome == "reply";
                    assert_eq!(
                        fixture.delivered(),
                        if recovered {
                            vec![baseline, partial, successor]
                        } else {
                            vec![baseline, successor]
                        }
                    );
                    if recovered {
                        assert_eq!(fixture.payloads()[1], "abcd");
                    }
                    let missed = u32::from(gap) + u32::from(!recovered);
                    assert_eq!(
                        *fixture.misses.lock().unwrap(),
                        if missed == 0 { vec![] } else { vec![missed] }
                    );
                    let states = zlock!(fixture.states);
                    let source = states.sequenced_states.peek(&fixture.id).unwrap();
                    assert!(source.pending_samples.is_empty());
                    assert!(source.gap_skip_through.is_none());
                }
            }
        }
    }
    ztimeout!(session.close()).unwrap();
}

/// Query-only partials must arm fragment recovery even with a whole successor.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_query_only_partial_before_complete_successor_is_recovered() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/query_barrier").unwrap();
    for history in [false, true] {
        let cache =
            ztimeout!(session.declare_queryable("test/ext/frag/query_barrier/@adv/**")).unwrap();
        let builder = session.declare_subscriber(&key).advanced().recovery(
            RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(50)),
        );
        let sub = ztimeout!(if history {
            builder.history(HistoryConfig::default())
        } else {
            builder
        })
        .unwrap();
        let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
        if !history {
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
                .put(&key, "next")
                .source_info(SourceInfo::new(id, 2)))
            .unwrap();
        }
        let query = ztimeout!(cache.recv_async()).unwrap();
        ztimeout!(query.reply_sample(fragment(&key, id, 1, 0, 2, "ab"))).unwrap();
        if history {
            ztimeout!(query.reply_sample(whole(&key, id, 2, "next"))).unwrap();
        }
        drop(query);
        let recovery = ztimeout!(cache.recv_async()).unwrap();
        assert_eq!(recovery.parameters().get("_sn"), Some("1..1"));
        assert_eq!(recovery.parameters().get("_fn"), Some("1.."));
        assert!(sub.try_recv().unwrap().is_none());
        assert!(misses.try_recv().unwrap().is_none());
        ztimeout!(recovery.reply_sample(fragment(&key, id, 1, 1, 2, "cd"))).unwrap();
        drop(recovery);
        for (sn, payload) in [(1, "abcd"), (2, "next")] {
            let sample = ztimeout!(sub.recv_async()).unwrap();
            assert_eq!(sample.source_info().unwrap().source_sn(), sn);
            assert_eq!(sample.payload().try_to_string().unwrap(), payload);
        }
        assert!(sub.try_recv().unwrap().is_none());
        assert!(misses.try_recv().unwrap().is_none());
    }
    ztimeout!(session.close()).unwrap();
}

/// The skip boundary covers buffered SN 3, but cannot cover a future SN 5.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_deferred_flush_does_not_skip_future_gap() {
    let session = isolated_session().await;
    let fixture = QueryFixture::new(&session, 10);
    {
        let mut states = zlock!(fixture.states);
        states.retransmission = true;
        states.frag_recovery_delay = Duration::from_secs(60);
        handle_sample(&mut states, fixture.whole(0, "single"));
        handle_sample(&mut states, fixture.fragment(1, 0, 2, "ab"));
        handle_sample(&mut states, fixture.whole(3, "single"));
        states.on_history_or_sample_recovery_finished(&fixture.states, &fixture.id);
        handle_sample(&mut states, fixture.whole(5, "single"));
        handle_sample(&mut states, fixture.fragment(1, 1, 2, "ab"));
        let source = states.sequenced_states.peek(&fixture.id).unwrap();
        assert_eq!(source.last_delivered, Some(WrappingSn(3)));
        assert!(source.pending_samples.contains_key(&WrappingSn(5)));
        assert!(source.gap_skip_through.is_none());
    }
    assert_eq!(*fixture.misses.lock().unwrap(), [1]);
    assert_eq!(fixture.received.lock().unwrap().len(), 3);
    let cache = ztimeout!(session.declare_queryable("test/ext/pending/@adv/**")).unwrap();
    RepliesHandler::recover_gap(&fixture.states, fixture.id);
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some("4.."));
    drop(query);
    wait_for(|| fixture.received.lock().unwrap().len() == 4).await;
    assert_eq!(*fixture.misses.lock().unwrap(), [1, 1]);
    ztimeout!(session.close()).unwrap();
}

/// Resolving one barrier preserves another; eviction can resume a deferred skip.
#[test]
fn test_deferred_flush_multiple_barriers_and_eviction() {
    for evict in [false, true] {
        let mut fixture = BufferedSource::new(if evict { 2 } else { 10 });
        fixture.retransmission = true;
        fixture.receive(fixture.whole(0, "single"));
        fixture.receive(fixture.fragment(1, 0, 2, "ab"));
        if evict {
            fixture.receive(fixture.whole(3, "single"));
        } else {
            fixture.receive(fixture.fragment(3, 0, 2, "ab"));
            fixture.receive(fixture.whole(5, "single"));
        }
        fixture.source.authorize_gap_skip();
        fixture.source.drain_authorized_samples(
            &fixture.callback,
            &fixture.miss_handlers,
            fixture.id,
            fixture.retransmission,
        );
        if evict {
            fixture.receive(fixture.fragment(4, 0, 2, "ab"));
            let source = &fixture.source;
            assert_eq!(source.last_delivered, Some(WrappingSn(3)));
            assert_eq!(source.pending_samples.len(), 1);
            assert!(source.pending_samples.contains_key(&WrappingSn(4)));
            assert!(source.gap_skip_through.is_none());
        } else {
            fixture.receive(fixture.fragment(1, 1, 2, "ab"));
            let source = &fixture.source;
            assert_eq!(source.last_delivered, Some(WrappingSn(1)));
            assert!(source.pending_samples[&WrappingSn(3)].is_incomplete());
            assert_eq!(source.gap_skip_through, Some(WrappingSn(5)));
            fixture.receive(fixture.fragment(3, 1, 2, "ab"));
            let source = &fixture.source;
            assert_eq!(source.last_delivered, Some(WrappingSn(5)));
            assert!(source.pending_samples.is_empty());
            assert!(source.gap_skip_through.is_none());
        }
        assert_eq!(
            *fixture.misses.lock().unwrap(),
            if evict { vec![2] } else { vec![1, 1] }
        );
        assert_eq!(
            fixture.received.lock().unwrap().len(),
            if evict { 2 } else { 4 }
        );
    }
}

/// Fragment completion cannot extend skip authority. Check both completion
/// orders, with and without a later sample query extending the boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_deferred_flush_overlapping_queries_and_boundary_extension() {
    let session = isolated_session().await;
    let cache = ztimeout!(session.declare_queryable("test/ext/pending/@adv/**")).unwrap();
    for (sample_query_first, extend) in [(false, false), (true, false), (true, true)] {
        let fixture = QueryFixture::new(&session, 10);
        let (generation, attempt) = {
            let mut states = zlock!(fixture.states);
            states.retransmission = true;
            states.frag_recovery_delay = Duration::from_secs(60);
            handle_sample(&mut states, fixture.whole(0, "single"));
            handle_sample(&mut states, fixture.fragment(1, 0, 2, "ab"));
            handle_sample(&mut states, fixture.whole(3, "single"));
            let source = states.sequenced_states.peek_mut(&fixture.id).unwrap();
            let attempt = source
                .pending_samples
                .get_mut(&WrappingSn(1))
                .unwrap()
                .prepare_recovery(Duration::ZERO, RecoveryTrigger::TimerTick)
                .unwrap()
                .1;
            source.pending_queries = 2;
            (source.generation.clone(), attempt)
        };
        let sample_query = RepliesHandler {
            scope: QueryScope::Sequenced(fixture.id),
            statesref: fixture.states.clone(),
        };
        let fragment = FragRecoveryAttempt {
            source_id: fixture.id,
            statesref: fixture.states.clone(),
            generation,
            sn: WrappingSn(1),
            ranges: vec![FragRange::new(1, None)],
            attempt,
        };
        if sample_query_first {
            drop(sample_query);
            fixture.receive(fixture.whole(5, "single"));
            if extend {
                zlock!(fixture.states)
                    .sequenced_states
                    .peek_mut(&fixture.id)
                    .unwrap()
                    .pending_queries += 1;
                finish_sample_query(false, &fixture);
                assert_eq!(
                    zlock!(fixture.states)
                        .sequenced_states
                        .peek(&fixture.id)
                        .unwrap()
                        .gap_skip_through,
                    Some(WrappingSn(5))
                );
            }
            drop(fragment);
            let states = zlock!(fixture.states);
            let source = states.sequenced_states.peek(&fixture.id).unwrap();
            assert_eq!(
                source.last_delivered,
                Some(WrappingSn(if extend { 5 } else { 3 }))
            );
            assert_eq!(source.pending_samples.contains_key(&WrappingSn(5)), !extend);
        } else {
            drop(fragment);
            drop(sample_query);
            fixture.receive(fixture.whole(5, "single"));
        }
        // Hold the actual gap query so a scheduled recheck cannot flush early.
        if !extend {
            RepliesHandler::recover_gap(&fixture.states, fixture.id);
            let query = ztimeout!(cache.recv_async()).unwrap();
            assert_eq!(query.parameters().get("_sn"), Some("4.."));
            drop(query);
        }
        wait_for(|| {
            zlock!(fixture.states)
                .sequenced_states
                .peek(&fixture.id)
                .unwrap()
                .last_delivered
                == Some(WrappingSn(5))
        })
        .await;
        {
            let states = zlock!(fixture.states);
            let source = states.sequenced_states.peek(&fixture.id).unwrap();
            assert_eq!(source.last_delivered, Some(WrappingSn(5)));
            assert!(source.gap_skip_through.is_none());
        }
        assert_eq!(*fixture.misses.lock().unwrap(), [2, 1]);
        assert_eq!(fixture.received.lock().unwrap().len(), 3);
    }
    ztimeout!(session.close()).unwrap();
}

/// Non-recovery query completion keeps trailing partials for later live fragments.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_query_completion_preserves_trailing_partial_samples() {
    let session = isolated_session().await;
    for history in [false, true] {
        for advance_past_gap in [false, true] {
            let fixture = QueryFixture::new(&session, 10);
            let tail_sn = if advance_past_gap { 3 } else { 1 };
            {
                let mut states = zlock!(fixture.states);
                handle_sample(&mut states, fixture.whole(0, "base"));
                states.global_pending_queries = u64::from(history);
                let source = states.sequenced_states.peek_mut(&fixture.id).unwrap();
                source.pending_queries = u64::from(!history);
                // Model query-completion state independently of reply/live origin.
                let mut pending = vec![fixture.fragment(1, 0, 2, "ab")];
                if advance_past_gap {
                    pending.push(fixture.whole(2, "next"));
                    pending.push(fixture.fragment(3, 0, 2, "ab"));
                }
                for sample in pending {
                    let info = sample.source_info().unwrap().clone();
                    source
                        .insert_sample(sample, &info, MAX_FRAGMENTS_DEFAULT)
                        .unwrap();
                }
            }
            finish_sample_query(history, &fixture);
            {
                let states = zlock!(fixture.states);
                let source = states.sequenced_states.peek(&fixture.id).unwrap();
                assert_eq!(source.last_delivered, Some(WrappingSn(tail_sn - 1)));
                assert_eq!(
                    source.pending_samples.keys().copied().collect::<Vec<_>>(),
                    [WrappingSn(tail_sn)],
                    "history={history}, advance_past_gap={advance_past_gap}"
                );
            }
            fixture.receive(fixture.fragment(tail_sn, 1, 2, "cd"));
            if advance_past_gap {
                fixture.receive(fixture.fragment(1, 1, 2, "cd"));
            }
            assert_eq!(
                fixture.delivered(),
                if advance_past_gap {
                    vec![0, 2, 3]
                } else {
                    vec![0, 1]
                }
            );
            assert_eq!(
                *fixture.misses.lock().unwrap(),
                if advance_past_gap { vec![1] } else { vec![] }
            );
            assert_eq!(fixture.payloads().last().unwrap(), "abcd");
        }
    }
    ztimeout!(session.close()).unwrap();
}

/// A trailing partial in history/sample replies must trigger fragment recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_query_completion_recovers_trailing_partial_sample() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/frag/query_tail").unwrap();
    for history in [false, true] {
        let cache =
            ztimeout!(session.declare_queryable("test/ext/frag/query_tail/@adv/**")).unwrap();
        let builder = session.declare_subscriber(&key).advanced().recovery(
            RecoveryConfig::default().fragments_recovery_delay(Duration::from_millis(50)),
        );
        let sub = ztimeout!(if history {
            builder.history(HistoryConfig::default())
        } else {
            builder
        })
        .unwrap();
        let misses = ztimeout!(sub.sample_miss_listener()).unwrap();
        if !history {
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
                .put(&key, "next")
                .source_info(SourceInfo::new(id, 2)))
            .unwrap();
        }
        let query = ztimeout!(cache.recv_async()).unwrap();
        let tail_sn = if history { 1 } else { 3 };
        ztimeout!(query.reply_sample(fragment(&key, id, tail_sn, 0, 2, "ab"))).unwrap();
        drop(query);
        if !history {
            assert_eq!(
                ztimeout!(sub.recv_async())
                    .unwrap()
                    .source_info()
                    .unwrap()
                    .source_sn(),
                2
            );
            assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
        }
        let recovery = ztimeout!(cache.recv_async()).unwrap();
        assert_eq!(recovery.parameters().get("_fn"), Some("1.."));
        assert_eq!(
            recovery.parameters().get("_sn"),
            Some(if history { "1..1" } else { "3..3" })
        );
        ztimeout!(recovery.reply_sample(fragment(&key, id, tail_sn, 1, 2, "cd"))).unwrap();
        drop(recovery);
        let sample = ztimeout!(sub.recv_async()).unwrap();
        assert_eq!(sample.source_info().unwrap().source_sn(), tail_sn);
        assert_eq!(sample.payload().try_to_string().unwrap(), "abcd");
        assert!(sub.try_recv().unwrap().is_none());
        assert!(misses.try_recv().unwrap().is_none());
    }
    ztimeout!(session.close()).unwrap();
}

/// Whichever query finishes last releases completed historical samples.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_query_completion_flushes_after_overlapping_fragment_query() {
    let session = isolated_session().await;
    for history_first in [false, true] {
        let fixture = QueryFixture::new(&session, 10);
        {
            let mut states = zlock!(fixture.states);
            states.global_pending_queries = 1;
            states.retransmission = true;
            for (num, payload) in [(0, "ab"), (1, "cd")] {
                handle_sample(&mut states, fixture.fragment(0, num, 2, payload));
            }
            states
                .sequenced_states
                .peek_mut(&fixture.id)
                .unwrap()
                .pending_queries = 1;
        }
        let history = RepliesHandler {
            scope: QueryScope::Global,
            statesref: fixture.states.clone(),
        };
        let fragment = FragRecoveryAttempt {
            source_id: fixture.id,
            statesref: fixture.states.clone(),
            generation: zlock!(fixture.states)
                .sequenced_states
                .peek(&fixture.id)
                .unwrap()
                .generation
                .clone(),
            sn: WrappingSn(0),
            ranges: Vec::new(),
            attempt: Arc::new(()),
        };
        if history_first {
            drop(history);
            assert!(fixture.received.lock().unwrap().is_empty());
            drop(fragment);
        } else {
            drop(fragment);
            assert!(fixture.received.lock().unwrap().is_empty());
            drop(history);
        }
        assert_eq!(
            fixture.payloads(),
            ["abcd"],
            "history_first={history_first}"
        );
        assert!(fixture.misses.lock().unwrap().is_empty());
    }
    ztimeout!(session.close()).unwrap();
}
