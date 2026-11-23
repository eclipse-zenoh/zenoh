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

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use zenoh::{
    bytes::ZBytes,
    internal::{zlock, ztimeout},
    key_expr::KeyExpr,
    query::Queryable,
    sample::{FragInfo, Sample, SampleBuilder, SourceInfo},
    session::EntityGlobalId,
    Config, Session, Wait,
};

use super::{FragmentRequest, QueryContext};
use crate::{
    advanced_subscriber::{handle_sample, tests::frag_recovery_state, State},
    utils::WrappingSn,
    AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig,
};

const TIMEOUT: Duration = Duration::from_secs(10);

fn fragment(key: &KeyExpr<'static>, source: EntityGlobalId, sn: u32, num: u32) -> Sample {
    SampleBuilder::put(key.clone(), "x")
        .source_info(SourceInfo::new(source, sn))
        .frag_info(FragInfo::new(5, num))
        .into()
}

async fn check_nonmatching_reply_from_sample_callback(fragment_recovery: bool) {
    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    let session = ztimeout!(zenoh::open(config)).unwrap();
    let source_id = EntityGlobalId::new(session.zid(), 7);
    let live_source_id = EntityGlobalId::new(session.zid(), 8);
    let key = KeyExpr::try_from("test/ext/recovery/nonmatching").unwrap();
    let cache = session
        .declare_queryable("test/ext/recovery/nonmatching/@adv/**")
        .wait()
        .unwrap();
    let retained_query = Arc::new(Mutex::new(None::<zenoh::query::Query>));
    let (delivered_tx, delivered_rx) = flume::unbounded();
    let (checked_tx, checked_rx) = flume::bounded(1);
    let builder = session
        .declare_subscriber(key.clone())
        .advanced()
        .query_timeout(Duration::from_secs(60));
    let builder = if fragment_recovery {
        builder
            .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(60)))
    } else {
        builder.history(HistoryConfig::default().max_samples(1))
    };
    let _sub = builder
        .callback({
            let retained_query = retained_query.clone();
            move |sample| {
                if sample
                    .source_info()
                    .is_some_and(|info| *info.source_id() == live_source_id)
                {
                    let query = retained_query.lock().unwrap().as_ref().unwrap().clone();
                    // Use the requested source and sequence number so only the
                    // key filter can reject this fragment reply before locking.
                    let unrelated: Sample = SampleBuilder::put(
                        KeyExpr::try_from("test/ext/recovery/unrelated").unwrap(),
                        "ignored",
                    )
                    .source_info(SourceInfo::new(source_id, 0))
                    .frag_info(FragInfo::new(2, 0))
                    .into();
                    let (finished, result) = std::sync::mpsc::channel();
                    let worker = std::thread::spawn({
                        let query = query.clone();
                        let unrelated = unrelated.clone();
                        move || {
                            let replied = query.reply_sample(unrelated).wait().is_ok();
                            let _ = finished.send(replied);
                        }
                    });
                    // This callback holds the subscriber lock. First check on
                    // another thread that rejecting the reply needs no lock.
                    // On failure, return and release the lock so the test can
                    // report the regression instead of hanging forever.
                    let rejected_without_lock =
                        result.recv_timeout(Duration::from_secs(2)).unwrap_or(false);
                    let replied_inline =
                        rejected_without_lock && query.reply_sample(unrelated).wait().is_ok();
                    checked_tx.send((replied_inline, worker)).unwrap();
                }
                delivered_tx.send(sample).unwrap();
            }
        })
        .wait()
        .unwrap();

    if fragment_recovery {
        ztimeout!(session
            .put(&key, "B")
            .source_info(SourceInfo::new(source_id, 0))
            .frag_info(FragInfo::new(2, 1)))
        .unwrap();
    }
    let query = ztimeout!(cache.recv_async()).unwrap();
    if fragment_recovery {
        assert_eq!(query.parameters().get("_fn"), Some("0..0"));
    } else {
        assert_eq!(query.parameters().get("_max"), Some("1"));
    }
    *retained_query.lock().unwrap() = Some(query.clone());
    // History depth 1 allows this live sample through while the history
    // query is held. A different source also lets it through while the
    // fragment query is held, using the same subscriber mutex.
    ztimeout!(session
        .put(&key, "live")
        .source_info(SourceInfo::new(live_source_id, 0)))
    .unwrap();
    let (replied_inline, worker) = ztimeout!(checked_rx.recv_async()).unwrap();
    worker.join().unwrap();
    assert!(
        replied_inline,
        "nonmatching reply waited for the subscriber lock"
    );
    assert_eq!(
        ztimeout!(delivered_rx.recv_async())
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap(),
        "live"
    );
    assert!(delivered_rx.try_recv().is_err());

    // Matching replies must still be delivered after ignoring unrelated ones.
    let reply = SampleBuilder::put(key, if fragment_recovery { "A" } else { "recovered" })
        .source_info(SourceInfo::new(source_id, 0));
    let reply: Sample = if fragment_recovery {
        reply.frag_info(FragInfo::new(2, 0)).into()
    } else {
        reply.into()
    };
    ztimeout!(query.reply_sample(reply)).unwrap();
    let sample = ztimeout!(delivered_rx.recv_async()).unwrap();
    assert_eq!(
        sample.payload().try_to_string().unwrap(),
        if fragment_recovery { "AB" } else { "recovered" }
    );
    assert!(delivered_rx.try_recv().is_err());
    retained_query.lock().unwrap().take();
    drop(query);
    ztimeout!(session.close()).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonmatching_history_reply_from_sample_callback() {
    check_nonmatching_reply_from_sample_callback(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonmatching_fragment_reply_from_sample_callback() {
    check_nonmatching_reply_from_sample_callback(true).await;
}

/// A current-thread runtime lets each test control when dispatch is first polled.
struct DispatchFixture {
    _session: Session,
    states: Arc<Mutex<State>>,
    source_id: EntityGlobalId,
    key: KeyExpr<'static>,
    queries: Arc<Mutex<Vec<(String, String)>>>,
    _cache: Queryable<()>,
    runtime: tokio::runtime::Runtime,
}

impl DispatchFixture {
    fn new() -> Self {
        let mut config = Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(vec![]).unwrap();
        let session = zenoh::open(config).wait().unwrap();
        let source_id = EntityGlobalId::new(session.zid(), 7);
        let key = KeyExpr::try_from("test/ext/dispatch/owned").unwrap();
        let states = frag_recovery_state(&session, &key, Arc::new(Mutex::new(Vec::new())));
        let queries = Arc::new(Mutex::new(Vec::new()));
        let cache = session
            .declare_queryable("test/ext/dispatch/owned/@adv/**")
            .callback({
                let queries = queries.clone();
                move |query| {
                    queries.lock().unwrap().push((
                        query.parameters().get("_sn").unwrap().to_owned(),
                        query.parameters().get("_fn").unwrap().to_owned(),
                    ));
                }
            })
            .wait()
            .unwrap();
        Self {
            _session: session,
            states,
            source_id,
            key,
            queries,
            _cache: cache,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(),
        }
    }

    fn prepare(&self, sn: u32) -> (FragmentRequest, Arc<()>, QueryContext) {
        let mut states = zlock!(self.states);
        for num in [0, 2, 4] {
            handle_sample(&mut states, fragment(&self.key, self.source_id, sn, num));
        }
        let source = states.sequenced_states.peek_mut(&self.source_id).unwrap();
        // Reserve an immediate attempt without starting a recurring scan.
        let request = source
            .on_fragment(
                &self.states,
                self.source_id,
                sn.into(),
                false,
                Duration::ZERO,
            )
            .unwrap();
        (
            request,
            source.generation.clone(),
            QueryContext::new(&states),
        )
    }

    fn spawn(&self, sn: u32) -> tokio::task::JoinHandle<()> {
        let (request, generation, context) = self.prepare(sn);
        self.runtime.spawn(context.fragment_query_task(
            &self.states,
            self.source_id,
            &generation,
            request,
        ))
    }

    fn finish(&self, task: tokio::task::JoinHandle<()>) {
        self.runtime.block_on(async { ztimeout!(task).unwrap() });
    }

    fn drive_until(&self, mut ready: impl FnMut() -> bool) {
        self.runtime.block_on(async {
            ztimeout!(async {
                while !ready() {
                    tokio::task::yield_now().await;
                }
            });
        });
    }
}

impl Drop for DispatchFixture {
    fn drop(&mut self) {
        self.states.lock().unwrap().sequenced_states.clear();
    }
}

// Cancel before the first poll: the pre-created attempt must release its count.
#[test]
fn cancelled_fragment_query_releases_pending_count() {
    let fixture = DispatchFixture::new();
    let task = fixture.spawn(0);
    task.abort();
    assert!(fixture
        .runtime
        .block_on(async { ztimeout!(task).unwrap_err() })
        .is_cancelled());
    {
        let states = zlock!(fixture.states);
        let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
        assert_eq!(source.pending_queries, 0);
        assert!(source.pending_samples[&WrappingSn(0)].is_retry());
        assert!(!source.frag_dispatch_cancel.is_cancelled());
    }
    assert!(fixture.queries.lock().unwrap().is_empty());
}

/// One source token cancels all queued attempts and releases their reservations.
#[test]
fn source_token_cancels_all_queued_fragment_dispatches() {
    let fixture = DispatchFixture::new();
    let first = fixture.spawn(0);
    let second = fixture.spawn(1);
    {
        let states = zlock!(fixture.states);
        let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
        assert_eq!(source.pending_queries, 2);
        source.frag_dispatch_cancel.cancel();
    }
    fixture.finish(first);
    fixture.finish(second);
    {
        let states = zlock!(fixture.states);
        let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
        assert_eq!(source.pending_queries, 0);
        for sn in [0, 1] {
            assert!(source.pending_samples[&WrappingSn(sn)].is_retry());
        }
    }
    assert!(fixture.queries.lock().unwrap().is_empty());
}

/// Source removal cancels queued work; a recreated source gets a fresh token.
#[test]
fn removing_source_cancels_queued_fragment_dispatch() {
    for recreate in [false, true] {
        let fixture = DispatchFixture::new();
        let first = fixture.spawn(0);
        let second = fixture.spawn(1);
        let cancellation = zlock!(fixture.states)
            .sequenced_states
            .peek(&fixture.source_id)
            .unwrap()
            .frag_dispatch_cancel
            .clone();
        // Retention GC drops the source through this same ownership path.
        zlock!(fixture.states)
            .sequenced_states
            .pop(&fixture.source_id);
        assert!(cancellation.is_cancelled());
        let replacement = recreate.then(|| {
            let (request, generation, context) = fixture.prepare(0);
            context.fragment_query_task(&fixture.states, fixture.source_id, &generation, request)
        });

        fixture.finish(first);
        fixture.finish(second);
        assert!(fixture.queries.lock().unwrap().is_empty());
        {
            let states = zlock!(fixture.states);
            if recreate {
                let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
                assert_eq!(source.pending_queries, 1);
                assert!(!source.frag_dispatch_cancel.is_cancelled());
            } else {
                assert!(!states.sequenced_states.contains(&fixture.source_id));
            }
        }
        if let Some(replacement) = replacement {
            fixture.runtime.block_on(replacement);
            fixture.drive_until(|| {
                zlock!(fixture.states)
                    .sequenced_states
                    .peek(&fixture.source_id)
                    .unwrap()
                    .pending_queries
                    == 0
            });
            let mut queries = fixture.queries.lock().unwrap().clone();
            queries.sort();
            assert_eq!(
                queries,
                [
                    ("0..0".to_owned(), "1..1".to_owned()),
                    ("0..0".to_owned(), "3..3".to_owned()),
                ]
            );
        }
    }
}

/// A stale reservation cannot capture the replacement source's cancellation token.
#[test]
fn stale_fragment_dispatch_cannot_use_recreated_source_token() {
    let fixture = DispatchFixture::new();
    let (request, generation, context) = fixture.prepare(0);
    let retired = Arc::downgrade(&generation);
    zlock!(fixture.states)
        .sequenced_states
        .pop(&fixture.source_id);
    let (replacement, replacement_generation, replacement_context) = fixture.prepare(0);
    let replacement = replacement_context.fragment_query_task(
        &fixture.states,
        fixture.source_id,
        &replacement_generation,
        replacement,
    );

    let task = fixture.runtime.spawn(context.fragment_query_task(
        &fixture.states,
        fixture.source_id,
        &generation,
        request,
    ));
    drop(generation);
    fixture.finish(task);
    assert!(retired.upgrade().is_none());
    assert!(fixture.queries.lock().unwrap().is_empty());
    {
        let states = zlock!(fixture.states);
        let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
        assert_eq!(source.pending_queries, 1);
        assert!(!source.frag_dispatch_cancel.is_cancelled());
    }
    drop(replacement);
}

/// Source removal also cancels a prepared dispatch that has not been spawned yet.
#[test]
fn prepared_fragment_dispatch_is_cancelled_with_source() {
    let fixture = DispatchFixture::new();
    let (request, generation, context) = fixture.prepare(0);
    let query_task =
        context.fragment_query_task(&fixture.states, fixture.source_id, &generation, request);
    zlock!(fixture.states)
        .sequenced_states
        .pop(&fixture.source_id);
    let (replacement, generation, context) = fixture.prepare(0);
    let replacement =
        context.fragment_query_task(&fixture.states, fixture.source_id, &generation, replacement);

    fixture.runtime.block_on(query_task);
    assert!(fixture.queries.lock().unwrap().is_empty());
    assert_eq!(
        zlock!(fixture.states)
            .sequenced_states
            .peek(&fixture.source_id)
            .unwrap()
            .pending_queries,
        1
    );
    drop(replacement);
}

/// Completing one dispatch does not cancel the shared token or other dispatches.
#[test]
fn fragment_dispatches_share_token_and_complete_independently() {
    let fixture = DispatchFixture::new();
    let first = fixture.spawn(0);
    let second = fixture.spawn(1);
    {
        let states = zlock!(fixture.states);
        let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
        assert_eq!(source.pending_queries, 2);
        assert!(!source.frag_dispatch_cancel.is_cancelled());
    }
    fixture.finish(first);
    fixture.finish(second);

    // Reuse the same source token for another wave of recovery.
    let third = fixture.spawn(2);
    fixture.finish(third);
    fixture.drive_until(|| {
        zlock!(fixture.states)
            .sequenced_states
            .peek(&fixture.source_id)
            .unwrap()
            .pending_queries
            == 0
    });
    let mut queries = fixture.queries.lock().unwrap().clone();
    queries.sort();
    assert_eq!(
        queries,
        [
            ("0..0".to_owned(), "1..1".to_owned()),
            ("0..0".to_owned(), "3..3".to_owned()),
            ("1..1".to_owned(), "1..1".to_owned()),
            ("1..1".to_owned(), "3..3".to_owned()),
            ("2..2".to_owned(), "1..1".to_owned()),
            ("2..2".to_owned(), "3..3".to_owned()),
        ]
    );
}

/// Runtime shutdown may drop the future during spawn, before its first poll.
#[test]
fn stopped_runtime_releases_fragment_dispatch_reservation() {
    let fixture = DispatchFixture::new();
    let (request, generation, context) = fixture.prepare(0);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();
    drop(runtime);

    let task = handle.spawn(context.fragment_query_task(
        &fixture.states,
        fixture.source_id,
        &generation,
        request,
    ));
    assert!(fixture
        .runtime
        .block_on(async { ztimeout!(task).unwrap_err() })
        .is_cancelled());
    let states = zlock!(fixture.states);
    let source = states.sequenced_states.peek(&fixture.source_id).unwrap();
    assert_eq!(source.pending_queries, 0);
    assert!(source.pending_samples[&WrappingSn(0)].is_retry());
    assert!(!source.frag_dispatch_cancel.is_cancelled());
    assert!(fixture.queries.lock().unwrap().is_empty());
}

/// Even if the first query finishes immediately or sending fails, keep the
/// attempt counted as pending until all range queries have been sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fragment_dispatch_releases_exactly_one_reservation() {
    for closed_session in [false, true] {
        let mut config = Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(vec![]).unwrap();
        let session = ztimeout!(zenoh::open(config)).unwrap();
        let source_id = EntityGlobalId::new(session.zid(), 7);
        let key = KeyExpr::try_from("test/ext/dispatch/completion").unwrap();
        let statesref = frag_recovery_state(&session, &key, Arc::new(Mutex::new(Vec::new())));
        let queries = Arc::new(AtomicUsize::new(0));
        let _cache = session
            .declare_queryable("test/ext/dispatch/completion/@adv/**")
            .callback({
                let queries = queries.clone();
                let statesref = statesref.clone();
                move |query| {
                    {
                        let states = zlock!(statesref);
                        let source = states.sequenced_states.peek(&source_id).unwrap();
                        assert_eq!(source.pending_queries, 1);
                        assert!(!source.pending_samples[&WrappingSn(0)].is_retry());
                    }
                    queries.fetch_add(1, Ordering::SeqCst);
                    query.reply_err(ZBytes::new()).wait().unwrap();
                }
            })
            .wait()
            .unwrap();
        let (requests, generation, context) = {
            let mut states = zlock!(statesref);
            for num in [0, 2, 4] {
                handle_sample(&mut states, fragment(&key, source_id, 0, num));
            }
            let source = states.sequenced_states.peek_mut(&source_id).unwrap();
            let requests = source.prepare_fragment_scan(Duration::ZERO);
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].ranges.len(), 2);
            (
                requests,
                source.generation.clone(),
                QueryContext::new(&states),
            )
        };
        if closed_session {
            ztimeout!(session.close()).unwrap();
        }
        context.issue_fragments(&statesref, source_id, &generation, requests);
        ztimeout!(async {
            loop {
                if zlock!(statesref)
                    .sequenced_states
                    .peek(&source_id)
                    .unwrap()
                    .pending_queries
                    == 0
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        });
        {
            let states = zlock!(statesref);
            let source = states.sequenced_states.peek(&source_id).unwrap();
            assert!(source.pending_samples[&WrappingSn(0)].is_retry());
            assert_eq!(
                queries.load(Ordering::SeqCst),
                if closed_session { 0 } else { 2 }
            );
        }
        if !closed_session {
            ztimeout!(session.close()).unwrap();
        }
    }
}

/// Publications can fill the missing fragments and deliver the sample while
/// queries are still running. When those queries finish, they must decrease
/// pending_queries even though the sample is no longer in the buffer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fragment_reservation_outlives_delivered_assembly() {
    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    let session = ztimeout!(zenoh::open(config)).unwrap();
    let source_id = EntityGlobalId::new(session.zid(), 7);
    let key = KeyExpr::try_from("test/ext/dispatch/live").unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let statesref = frag_recovery_state(&session, &key, received.clone());
    let cache = session
        .declare_queryable("test/ext/dispatch/live/@adv/**")
        .wait()
        .unwrap();
    let (requests, generation, context) = {
        let mut states = zlock!(statesref);
        for num in [0, 2, 4] {
            handle_sample(&mut states, fragment(&key, source_id, 0, num));
        }
        let source = states.sequenced_states.peek_mut(&source_id).unwrap();
        let requests = source.prepare_fragment_scan(Duration::ZERO);
        (
            requests,
            source.generation.clone(),
            QueryContext::new(&states),
        )
    };
    context.issue_fragments(&statesref, source_id, &generation, requests);
    let first = ztimeout!(cache.recv_async()).unwrap();
    let second = ztimeout!(cache.recv_async()).unwrap();
    {
        let mut states = zlock!(statesref);
        for num in [1, 3] {
            handle_sample(&mut states, fragment(&key, source_id, 0, num));
        }
        let source = states.sequenced_states.peek(&source_id).unwrap();
        assert!(source.pending_samples.is_empty());
        assert_eq!(source.pending_queries, 1);
        assert_eq!(received.lock().unwrap().len(), 1);
    }
    drop(first);
    drop(second);
    ztimeout!(async {
        loop {
            if zlock!(statesref)
                .sequenced_states
                .peek(&source_id)
                .unwrap()
                .pending_queries
                == 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    });
    assert_eq!(received.lock().unwrap().len(), 1);
    ztimeout!(session.close()).unwrap();
}
