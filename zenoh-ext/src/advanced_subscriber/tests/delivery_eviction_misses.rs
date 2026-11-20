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

/// A retired watermark must not reject a new sample at its old half-range boundary.
#[test]
fn test_delivery_retires_eviction_watermark() {
    for baseline in [0u32, u32::MAX - 1] {
        let mut fixture = BufferedSource::new(1);
        let evicted = baseline.wrapping_add(1);
        let successor = baseline.wrapping_add(2);
        let half_range = evicted.wrapping_add(u32::MAX / 2);
        fixture.receive(fixture.whole(baseline, "base"));
        for sn in [evicted, successor] {
            fixture.receive(fixture.fragment(sn, 0, 2, "a"));
        }
        fixture.receive(fixture.fragment(successor, 1, 2, "b"));
        let watermark = fixture.source.last_evicted;
        for sn in [half_range, half_range.wrapping_add(1)] {
            fixture.receive(fixture.whole(sn, "new"));
        }
        assert_eq!(
            fixture.source.last_delivered,
            Some(WrappingSn(half_range.wrapping_add(1)))
        );
        assert_eq!(watermark, None, "delivery must retire last_evicted");
        assert_eq!(
            fixture.delivered(),
            [baseline, successor, half_range, half_range.wrapping_add(1)]
        );
    }
}

/// Late data must not recreate an evicted abandonment tombstone before delivery.
#[test]
fn test_evicted_abandoned_sample_stays_obsolete() {
    let mut fixture = BufferedSource::new(2);
    fixture.receive(fixture.whole(0, "base"));
    fixture
        .source
        .pending_samples
        .insert(WrappingSn(1), PendingSample::Abandoned);
    for sn in [3, 4, 1] {
        fixture.receive(fixture.fragment(sn, 0, 2, "a"));
    }
    fixture.receive(fixture.whole(1, "late whole sample"));
    {
        let source = &fixture.source;
        assert_eq!(source.last_evicted, Some(WrappingSn(1)));
        assert_eq!(
            source.pending_samples.keys().copied().collect::<Vec<_>>(),
            [WrappingSn(3), WrappingSn(4)]
        );
    }
    assert_eq!(fixture.received.lock().unwrap().len(), 1);
    assert!(fixture.misses.lock().unwrap().is_empty());
}

/// Preserve wraparound × put/delete × arrival-order coverage. With recovery off,
/// a complete successor skips obsolete partials without waiting for eviction.
#[test]
fn test_delivery_discards_obsolete_partial_samples() {
    for baseline in [0u32, u32::MAX - 2] {
        for delete in [false, true] {
            for successor_first in [false, true] {
                let mut fixture = BufferedSource::new(10);
                let partial = baseline.wrapping_add(1);
                let jump = baseline.wrapping_add(2);
                let successor = baseline.wrapping_add(3);
                fixture.receive(fixture.whole(baseline, "base"));
                fixture.receive(fixture.fragment(partial, 0, 2, "lost"));
                let complete_successor = |fixture: &mut BufferedSource| {
                    fixture.receive(fixture.fragment(successor, 0, 2, "ab"));
                    fixture.receive(fixture.fragment(successor, 1, 2, "cd"));
                };
                if successor_first {
                    complete_successor(&mut fixture);
                }
                fixture.receive(if delete {
                    SampleBuilder::delete(fixture.key.clone())
                        .source_info(SourceInfo::new(fixture.id, jump))
                        .into()
                } else {
                    fixture.whole(jump, "jump")
                });
                if !successor_first {
                    complete_successor(&mut fixture);
                }
                fixture.receive(fixture.fragment(partial, 1, 2, "late"));
                fixture.receive(fixture.fragment(partial, 0, 2, "lost"));
                assert_eq!(
                    fixture.delivered(),
                    if successor_first {
                        vec![baseline, successor]
                    } else {
                        vec![baseline, jump, successor]
                    },
                    "baseline={baseline}, delete={delete}, successor_first={successor_first}"
                );
                assert_eq!(fixture.payloads().last().unwrap(), "abcd");
                assert_eq!(
                    *fixture.misses.lock().unwrap(),
                    [if successor_first { 2 } else { 1 }]
                );
                assert!(fixture.source.pending_samples.is_empty());
            }
        }
    }
}

/// The depth-one history fast path establishes a newer baseline and retires partials.
#[test]
fn test_history_delivery_discards_obsolete_partial_samples() {
    let mut fixture = BufferedSource::new(1);
    fixture.history_pending = true;
    fixture.receive(fixture.fragment(1, 0, 2, "partial"));
    fixture.receive(fixture.whole(2, "new"));
    assert_eq!(fixture.delivered(), [2]);
    assert!(fixture.misses.lock().unwrap().is_empty());
    assert!(fixture.source.pending_samples.is_empty());
}

/// Overflow must release all complete successors immediately after discarding
/// the incomplete head, with no new publication or timer to drive delivery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_pending_eviction_retries_ready_delivery_with_recovery() {
    let session = isolated_session().await;
    let id = EntityGlobalId::new(session.zid(), 7);
    let key = "test/ext/frag/eviction_progress";
    for depth in [1, 3] {
        let cache = ztimeout!(session.declare_queryable("test/ext/frag/eviction_progress/@adv/**"))
            .unwrap();
        let sub = ztimeout!(session
            .declare_subscriber(key)
            .advanced()
            .max_pending_samples(depth)
            .recovery(RecoveryConfig::default().fragments_recovery_delay(Duration::from_secs(10))))
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
            .put(key, "partial")
            .source_info(SourceInfo::new(id, 1))
            .frag_info(FragInfo::new(2, 0)))
        .unwrap();
        for sn in 2..=depth as u32 + 1 {
            assert!(sub.try_recv().unwrap().is_none());
            ztimeout!(session
                .put(key, "complete")
                .source_info(SourceInfo::new(id, sn)))
            .unwrap();
        }
        for sn in 2..=depth as u32 + 1 {
            let sample = tokio::time::timeout(Duration::from_secs(2), sub.recv_async())
                .await
                .expect("eviction left a complete successor buffered")
                .unwrap();
            assert_eq!(sample.source_info().unwrap().source_sn(), sn);
            assert_eq!(sample.payload().try_to_string().unwrap(), "complete");
        }
        assert_eq!(ztimeout!(misses.recv_async()).unwrap().nb(), 1);
        assert!(misses.try_recv().unwrap().is_none());
        assert!(sub.try_recv().unwrap().is_none());
        assert!(cache.try_recv().unwrap().is_none());
        let states = zlock!(sub.statesref);
        let source = states.sequenced_states.peek(&id).unwrap();
        assert!(source.pending_samples.is_empty());
        assert_eq!(source.last_evicted, None);
        assert!(source.frag_recovery_task.is_none());
    }
    ztimeout!(session.close()).unwrap();
}

/// Delivery after eviction must still stop at a surviving recoverable partial.
#[test]
fn test_pending_eviction_preserves_recoverable_head() {
    let mut fixture = BufferedSource::new(3);
    fixture.retransmission = true;
    fixture.receive(fixture.whole(0, "base"));
    for sn in [1, 2] {
        fixture.receive(fixture.fragment(sn, 0, 2, "a"));
    }
    for sn in [3, 4] {
        fixture.receive(fixture.whole(sn, "complete"));
    }
    {
        let source = &fixture.source;
        assert_eq!(source.last_delivered, Some(WrappingSn(0)));
        assert_eq!(
            source
                .pending_samples
                .iter()
                .map(|(sn, s)| (*sn, s.is_complete()))
                .collect::<Vec<_>>(),
            [
                (WrappingSn(2), false),
                (WrappingSn(3), true),
                (WrappingSn(4), true)
            ]
        );
    }
    assert_eq!(fixture.received.lock().unwrap().len(), 1);
    assert!(fixture.misses.lock().unwrap().is_empty());
    fixture.receive(fixture.fragment(2, 1, 2, "b"));
    assert_eq!(fixture.delivered(), [0, 2, 3, 4]);
    assert_eq!(*fixture.misses.lock().unwrap(), [1]);
}

/// Before the first baseline, eviction unblocks delivery without reporting a miss.
#[test]
fn test_pending_bound_unblocks_incomplete_head() {
    let mut fixture = BufferedSource::new(4);
    fixture.retransmission = true;
    fixture.receive(fixture.fragment(1, 0, 3, "ab"));
    fixture.receive(fixture.fragment(1, 2, 3, "ef"));
    for (sn, p0, p1, p2) in [
        (2, "ab", "cd", "ef"),
        (3, "gh", "ij", "kl"),
        (4, "mn", "op", "qr"),
    ] {
        for (num, payload) in [(0, p0), (1, p1), (2, p2)] {
            fixture.receive(fixture.fragment(sn, num, 3, payload));
        }
    }
    assert!(fixture.received.lock().unwrap().is_empty());
    assert_eq!(fixture.misses.lock().unwrap().len(), 0);
    for (num, payload) in [(0, "st"), (1, "uv"), (2, "wx")] {
        fixture.receive(fixture.fragment(5, num, 3, payload));
    }
    assert_eq!(fixture.payloads(), ["abcdef", "ghijkl", "mnopqr", "stuvwx"]);
    assert!(fixture.misses.lock().unwrap().is_empty());
    assert!(fixture.source.pending_samples.is_empty());
    assert_eq!(fixture.source.last_delivered, Some(WrappingSn(5)));
}

/// Incomplete eviction is silent until a later delivery crosses the gap.
#[test]
fn test_pending_bound_enforced() {
    let mut fixture = BufferedSource::new(4);
    for sn in 0..10 {
        fixture.receive(fixture.fragment(sn, 0, 3, "x"));
    }
    assert!(fixture.misses.lock().unwrap().is_empty());
    assert!(fixture.received.lock().unwrap().is_empty());
    assert_eq!(fixture.source.pending_samples.len(), 4);
}

#[test]
fn test_miss_reported_once_on_crossing_delivery() {
    let mut fixture = BufferedSource::new(4);
    fixture.receive(fixture.whole(0, "base"));
    // Every fragment crosses the gap; only assembled delivery reports it.
    for (num, payload) in [(0, "ab"), (1, "cd"), (2, "ef")] {
        fixture.receive(fixture.fragment(2, num, 3, payload));
    }
    assert_eq!(fixture.payloads(), ["base", "abcdef"]);
    assert_eq!(*fixture.misses.lock().unwrap(), [1]);
}

#[test]
fn test_miss_jump_count_on_delivery() {
    let mut fixture = BufferedSource::new(4);
    fixture.receive(fixture.whole(0, "base"));
    fixture.receive(fixture.whole(3, "jump"));
    assert_eq!(fixture.payloads(), ["base", "jump"]);
    assert_eq!(*fixture.misses.lock().unwrap(), [2]);
}
