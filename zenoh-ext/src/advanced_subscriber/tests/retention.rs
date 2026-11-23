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
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zenoh::{
    internal::ztimeout,
    key_expr::KeyExpr,
    sample::{FragInfo, Sample, SampleBuilder, SourceInfo},
    session::EntityGlobalId,
    Config, Session,
};

use super::{garbage_collect, on_data_sample};
use crate::{
    advanced_subscriber::{tests::frag_recovery_state, State},
    fragmentation::PendingSample,
    utils::WrappingSn,
};

const TIMEOUT: Duration = Duration::from_secs(10);
const RETENTION: Duration = Duration::from_secs(60);
const RECOVERY_DELAY: Duration = Duration::from_secs(3600);
const MAX_FRAGMENTS: u32 = 3;
const INVALID_METADATA: &[(u32, u32)] = &[
    (0, 0),
    (3, 3),
    (u32::MAX, 3),
    (0, MAX_FRAGMENTS + 1),
    (u32::MAX - 1, u32::MAX),
];

struct Fixture {
    session: Session,
    states: Arc<Mutex<State>>,
    received: Arc<Mutex<Vec<Sample>>>,
    key: KeyExpr<'static>,
    target: EntityGlobalId,
    other: EntityGlobalId,
}

impl Fixture {
    async fn new(recovery: bool) -> Self {
        let mut config = Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config.listen.endpoints.set(vec![]).unwrap();
        let session = ztimeout!(zenoh::open(config)).unwrap();
        let key = KeyExpr::try_from("test/ext/retention").unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let states = frag_recovery_state(&session, &key, received.clone());
        {
            let mut states = states.lock().unwrap();
            states.retransmission = recovery;
            states.max_fragments = MAX_FRAGMENTS;
            states.frag_recovery_delay = RECOVERY_DELAY;
        }
        Self {
            target: EntityGlobalId::new(session.zid(), 7),
            other: EntityGlobalId::new(session.zid(), 8),
            session,
            states,
            received,
            key,
        }
    }

    fn whole(&self, id: EntityGlobalId, sn: u32, payload: &str) -> Sample {
        SampleBuilder::put(self.key.clone(), payload)
            .source_info(SourceInfo::new(id, sn))
            .into()
    }

    fn fragment(&self, sn: u32, num: u32, count: u32, payload: &str) -> Sample {
        SampleBuilder::put(self.key.clone(), payload)
            .source_info(SourceInfo::new(self.target, sn))
            .frag_info(FragInfo::new(count, num))
            .into()
    }

    fn publish(&self, sample: Sample) {
        // Exercise the real callback, including its post-arrival recovery lookup.
        on_data_sample(&self.states, sample);
    }

    /// Restore a known LRU/time order without waiting for retention to elapse.
    fn age_target_behind_other(&self) -> Instant {
        self.publish(self.whole(self.other, 0, "other"));
        let old = Instant::now() - RETENTION * 2;
        let mut states = self.states.lock().unwrap();
        states
            .sequenced_states
            .peek_mut(&self.target)
            .unwrap()
            .latest_access = old;
        states
            .sequenced_states
            .peek_mut(&self.other)
            .unwrap()
            .latest_access = old + RETENTION;
        old
    }

    fn order(&self) -> Vec<EntityGlobalId> {
        self.states
            .lock()
            .unwrap()
            .sequenced_states
            .iter()
            .map(|(id, _)| *id)
            .collect()
    }

    fn latest_access(&self) -> Instant {
        self.states
            .lock()
            .unwrap()
            .sequenced_states
            .peek(&self.target)
            .unwrap()
            .latest_access
    }

    fn delivered(&self) -> Vec<u32> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .filter_map(|sample| {
                let info = sample.source_info()?;
                (*info.source_id() == self.target).then_some(info.source_sn())
            })
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Stop source timers holding references to the fixture's state.
        self.states.lock().unwrap().sequenced_states.clear();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_first_fragment_does_not_create_source_or_start_recovery() {
    for history_pending in [0, 1] {
        let fixture = Fixture::new(true).await;
        fixture.publish(fixture.whole(fixture.other, 0, "other"));
        let other_access = {
            let mut states = fixture.states.lock().unwrap();
            states.global_pending_queries = history_pending;
            // A new source would start both periodic and fragment timers.
            states.period = Some(RECOVERY_DELAY);
            states
                .sequenced_states
                .peek(&fixture.other)
                .unwrap()
                .latest_access
        };
        for &(num, count) in INVALID_METADATA {
            fixture.publish(fixture.fragment(0, num, count, "invalid"));
            let states = fixture.states.lock().unwrap();
            assert!(
                !states.sequenced_states.contains(&fixture.target),
                "num={num}, count={count}"
            );
            assert_eq!(states.sequenced_states.len(), 1);
            assert!(states.timestamped_states.is_empty());
            assert_eq!(states.global_pending_queries, history_pending);
            let other = states.sequenced_states.peek(&fixture.other).unwrap();
            assert_eq!(other.latest_access, other_access);
            assert!(other.periodic_task.is_none());
            assert!(other.frag_recovery_task.is_none());
            assert_eq!(other.pending_queries, 0);
        }
        assert_eq!(fixture.order(), [fixture.other]);
        assert!(fixture.delivered().is_empty());

        // A subsequent valid first fragment must still create and arm the source.
        let before = Instant::now();
        fixture.publish(fixture.fragment(0, 0, MAX_FRAGMENTS, "valid"));
        let states = fixture.states.lock().unwrap();
        let source = states.sequenced_states.peek(&fixture.target).unwrap();
        assert!(source.latest_access >= before);
        assert!(source.periodic_task.is_some());
        assert!(source.frag_recovery_task.is_some());
        assert_eq!(source.pending_queries, 0);
        assert_eq!(source.pending_samples.len(), 1);
    }
}

/// Rejection must be observable independently of the branch's delivery work.
/// Cover history, no baseline, consecutive and ahead arrivals with/without recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_existing_fragments_preserve_activity_and_assembly() {
    for recovery in [false, true] {
        for (history_pending, baseline) in [(1, None), (0, None), (0, Some(4)), (0, Some(2))] {
            let fixture = Fixture::new(recovery).await;
            fixture.states.lock().unwrap().global_pending_queries = history_pending;
            if let Some(baseline) = baseline {
                fixture.publish(fixture.whole(fixture.target, baseline, "base"));
                // Model a held sample query so this test isolates arrival activity.
                fixture
                    .states
                    .lock()
                    .unwrap()
                    .sequenced_states
                    .peek_mut(&fixture.target)
                    .unwrap()
                    .pending_queries = 1;
            }
            fixture.publish(fixture.fragment(5, 0, MAX_FRAGMENTS, "prefix"));
            let old = fixture.age_target_behind_other();
            let (generation, timer_armed) = {
                let mut states = fixture.states.lock().unwrap();
                let source = states.sequenced_states.peek_mut(&fixture.target).unwrap();
                source.pending_queries = 1;
                let PendingSample::Fragmented(assembly) =
                    source.pending_samples.get_mut(&WrappingSn(5)).unwrap()
                else {
                    panic!("expected pending assembly");
                };
                assembly.last_progress = old;
                (
                    source.generation.clone(),
                    source.frag_recovery_task.is_some(),
                )
            };
            let delivered = fixture.delivered();
            // Counts 1 and 2 are individually valid, but conflict with the
            // existing three-fragment assembly. Other cases fail metadata checks.
            for (num, count) in [(0, 1), (0, 2)]
                .into_iter()
                .chain(INVALID_METADATA.iter().copied())
            {
                fixture.publish(fixture.fragment(5, num, count, "rejected"));
                assert_eq!(fixture.latest_access(), old, "history={history_pending}, baseline={baseline:?}, recovery={recovery}, num={num}, count={count}");
                assert_eq!(fixture.order(), [fixture.other, fixture.target]);
                assert_eq!(fixture.delivered(), delivered);
                let states = fixture.states.lock().unwrap();
                let source = states.sequenced_states.peek(&fixture.target).unwrap();
                assert!(Arc::ptr_eq(&source.generation, &generation));
                assert_eq!(source.pending_queries, 1);
                assert_eq!(source.frag_recovery_task.is_some(), timer_armed);
                assert_eq!(source.pending_samples.len(), 1);
                let PendingSample::Fragmented(assembly) = &source.pending_samples[&WrappingSn(5)]
                else {
                    panic!("rejection changed the assembly");
                };
                assert_eq!(assembly.last_progress, old);
                assert_eq!(assembly.frags.len(), MAX_FRAGMENTS as usize);
                assert_eq!(assembly.frags.iter().flatten().count(), 1);
                assert_eq!(
                    assembly.frags[0]
                        .as_ref()
                        .unwrap()
                        .payload()
                        .try_to_string()
                        .unwrap(),
                    "prefix"
                );
            }
        }
    }
}

/// Old fragments previously bypassed validation. Check old/equal SNs as well as
/// new consecutive/ahead SNs, without allowing a rejected arrival to create a slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_fragment_metadata_is_not_activity_at_any_sequence() {
    for recovery in [false, true] {
        let fixture = Fixture::new(recovery).await;
        fixture.publish(fixture.whole(fixture.target, 5, "base"));
        let old = fixture.age_target_behind_other();
        for sn in [4, 5, 6, 8] {
            for &(num, count) in INVALID_METADATA {
                fixture.publish(fixture.fragment(sn, num, count, "invalid"));
                assert_eq!(
                    fixture.latest_access(),
                    old,
                    "sn={sn}, num={num}, count={count}"
                );
                assert_eq!(fixture.order(), [fixture.other, fixture.target]);
                assert_eq!(fixture.delivered(), [5]);
                let states = fixture.states.lock().unwrap();
                let source = states.sequenced_states.peek(&fixture.target).unwrap();
                assert_eq!(source.last_delivered, Some(WrappingSn(5)));
                assert!(source.pending_samples.is_empty());
                assert!(source.frag_recovery_task.is_none());
                assert_eq!(source.pending_queries, 0);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn valid_whole_samples_and_delivered_duplicates_refresh_activity() {
    let fixture = Fixture::new(true).await;
    fixture.publish(fixture.whole(fixture.target, 5, "base"));
    for sample in [
        fixture.whole(fixture.target, 5, "duplicate"),
        fixture.whole(fixture.target, 4, "old"),
        fixture.fragment(5, 1, 3, "delivered fragment"),
        fixture.fragment(4, 0, 1, "older fragment"),
        fixture.whole(fixture.target, 6, "next"),
    ] {
        let old = fixture.age_target_behind_other();
        let before = Instant::now();
        fixture.publish(sample);
        assert!(fixture.latest_access() >= before);
        assert!(fixture.latest_access() > old);
        assert_eq!(fixture.order(), [fixture.target, fixture.other]);
        let states = fixture.states.lock().unwrap();
        let source = states.sequenced_states.peek(&fixture.target).unwrap();
        assert!(source.pending_samples.is_empty());
        assert!(source.frag_recovery_task.is_none());
        assert_eq!(source.pending_queries, 0);
    }
    assert_eq!(
        fixture.delivered(),
        [5, 6],
        "duplicates must not be delivered again"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_fragments_refresh_retention_without_fragment_progress() {
    let fixture = Fixture::new(true).await;
    fixture.publish(fixture.fragment(0, 0, 3, "A"));
    let old = fixture.age_target_behind_other();
    {
        let mut states = fixture.states.lock().unwrap();
        let source = states.sequenced_states.peek_mut(&fixture.target).unwrap();
        let PendingSample::Fragmented(assembly) =
            source.pending_samples.get_mut(&WrappingSn(0)).unwrap()
        else {
            panic!("expected incomplete assembly");
        };
        assembly.last_progress = old;
    }
    let before = Instant::now();
    fixture.publish(fixture.fragment(0, 0, 3, "replacement"));
    assert!(fixture.latest_access() >= before);
    assert_eq!(fixture.order(), [fixture.target, fixture.other]);
    {
        let states = fixture.states.lock().unwrap();
        let source = states.sequenced_states.peek(&fixture.target).unwrap();
        let PendingSample::Fragmented(assembly) = &source.pending_samples[&WrappingSn(0)] else {
            panic!("expected incomplete assembly");
        };
        assert_eq!(
            assembly.last_progress, old,
            "duplicates must not postpone tail recovery"
        );
        assert_eq!(assembly.frags.iter().flatten().count(), 1);
        assert_eq!(
            assembly.frags[0]
                .as_ref()
                .unwrap()
                .payload()
                .try_to_string()
                .unwrap(),
            "replacement"
        );
        assert_eq!(source.pending_queries, 0);
    }
    // Genuine fragment progress refreshes both retention and the assembly clock.
    fixture.age_target_behind_other();
    let before = Instant::now();
    fixture.publish(fixture.fragment(0, 1, 3, "B"));
    assert!(fixture.latest_access() >= before);
    assert_eq!(fixture.order(), [fixture.target, fixture.other]);
    {
        let states = fixture.states.lock().unwrap();
        let source = states.sequenced_states.peek(&fixture.target).unwrap();
        let PendingSample::Fragmented(assembly) = &source.pending_samples[&WrappingSn(0)] else {
            panic!("expected incomplete assembly");
        };
        assert!(assembly.last_progress >= before);
        assert_eq!(assembly.frags.iter().flatten().count(), 2);
    }
    fixture.publish(fixture.fragment(0, 2, 3, "C"));
    assert_eq!(fixture.delivered(), [0]);
    let received = fixture.received.lock().unwrap();
    let sample = received
        .iter()
        .find(|s| s.source_info().unwrap().source_id() == &fixture.target)
        .unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap(), "replacementBC");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evicted_and_abandoned_input_neither_refreshes_nor_promotes() {
    for evicted in [false, true] {
        let fixture = Fixture::new(true).await;
        fixture.publish(fixture.whole(fixture.target, 0, "base"));
        let old = fixture.age_target_behind_other();
        {
            let mut states = fixture.states.lock().unwrap();
            let source = states.sequenced_states.peek_mut(&fixture.target).unwrap();
            if evicted {
                source.last_evicted = Some(WrappingSn(1));
            } else {
                source
                    .pending_samples
                    .insert(WrappingSn(1), PendingSample::Abandoned);
            }
        }
        for sample in [
            fixture.whole(fixture.target, 1, "late whole"),
            fixture.fragment(1, 0, 2, "late first"),
            fixture.fragment(1, 1, 2, "late second"),
        ] {
            fixture.publish(sample);
            assert_eq!(fixture.latest_access(), old);
            assert_eq!(fixture.order(), [fixture.other, fixture.target]);
            assert_eq!(fixture.delivered(), [0]);
            let states = fixture.states.lock().unwrap();
            let source = states.sequenced_states.peek(&fixture.target).unwrap();
            assert_eq!(source.last_delivered, Some(WrappingSn(0)));
            assert!(source.frag_recovery_task.is_none());
            assert_eq!(source.pending_queries, 0);
            if evicted {
                assert_eq!(source.last_evicted, Some(WrappingSn(1)));
                assert!(source.pending_samples.is_empty());
            } else {
                assert_eq!(source.pending_samples.len(), 1);
                assert!(source.pending_samples[&WrappingSn(1)].is_abandoned());
            }
        }
    }
}

/// A bad input must not move an idle source ahead of an active source: GC stops
/// at the first non-expired LRU entry and would otherwise strand the idle one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gc_removes_rejected_only_source_while_retaining_valid_activity() {
    let fixture = Fixture::new(true).await;
    fixture.publish(fixture.fragment(0, 0, 3, "prefix"));
    let old = fixture.age_target_behind_other();
    fixture.publish(fixture.whole(fixture.other, 0, "active duplicate"));
    for _ in 0..3 {
        fixture.publish(fixture.fragment(0, 0, 2, "count mismatch"));
        fixture.publish(fixture.fragment(1, 3, 3, "invalid index"));
    }
    assert_eq!(fixture.latest_access(), old);
    assert_eq!(fixture.order(), [fixture.other, fixture.target]);
    let mut states = fixture.states.lock().unwrap();
    let active_access = states
        .sequenced_states
        .peek(&fixture.other)
        .unwrap()
        .latest_access;
    let oldest = garbage_collect(
        &mut states.sequenced_states,
        RETENTION,
        Instant::now(),
        |s| (&mut s.latest_access, s.alive),
    );
    assert_eq!(oldest, active_access);
    assert!(!states.sequenced_states.contains(&fixture.target));
    assert_eq!(states.sequenced_states.len(), 1);
    assert_eq!(
        states
            .sequenced_states
            .peek(&fixture.other)
            .unwrap()
            .latest_access,
        active_access
    );
}

/// Synchronize on the actual scan's outgoing query, not a retention sleep. Both
/// access and fragment-progress timestamps are explicitly aged before arming it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fragment_scan_bookkeeping_does_not_promote_an_idle_source() {
    let fixture = Fixture::new(true).await;
    let cache = ztimeout!(fixture
        .session
        .declare_queryable("test/ext/retention/@adv/**"))
    .unwrap();
    fixture.publish(fixture.fragment(0, 0, 3, "prefix"));
    let old = fixture.age_target_behind_other();
    fixture.publish(fixture.whole(fixture.other, 0, "active duplicate"));
    {
        let mut states = fixture.states.lock().unwrap();
        let source = states.sequenced_states.peek_mut(&fixture.target).unwrap();
        let PendingSample::Fragmented(assembly) =
            source.pending_samples.get_mut(&WrappingSn(0)).unwrap()
        else {
            panic!("expected incomplete assembly");
        };
        assembly.last_progress = old;
        // Replace the long test interval only after the state is ready. The
        // scan cannot observe it until this lock is released.
        source.frag_recovery_task = None;
        source.arm_fragment_recovery(&fixture.states, fixture.target, Duration::from_millis(1));
    }
    let query = ztimeout!(cache.recv_async()).unwrap();
    assert_eq!(query.parameters().get("_sn"), Some("0..0"));
    assert_eq!(query.parameters().get("_fn"), Some("1.."));
    assert_eq!(fixture.latest_access(), old);
    assert_eq!(fixture.order(), [fixture.other, fixture.target]);
    {
        let mut states = fixture.states.lock().unwrap();
        assert_eq!(
            states
                .sequenced_states
                .peek(&fixture.target)
                .unwrap()
                .pending_queries,
            1
        );
        garbage_collect(
            &mut states.sequenced_states,
            RETENTION,
            Instant::now(),
            |s| (&mut s.latest_access, s.alive),
        );
        assert!(!states.sequenced_states.contains(&fixture.target));
        assert!(states.sequenced_states.contains(&fixture.other));
        assert_eq!(
            states
                .sequenced_states
                .peek(&fixture.other)
                .unwrap()
                .pending_queries,
            0
        );
    }
    drop(query);
}
