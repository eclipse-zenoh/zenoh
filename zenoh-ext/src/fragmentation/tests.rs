//
// Copyright (c) 2022 ZettaScale Technology
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
use std::time::{Duration, Instant};

use zenoh::{
    key_expr::KeyExpr,
    sample::{FragInfo, Sample, SampleBuilder},
};

use super::{
    fragment_count, FragInsertError, FragRange, FragmentAssembly, FragmentRecovery, PendingSample,
    RecoveryTrigger::{FragmentArrival, TimerTick},
    MAX_FRAGMENTS_DEFAULT,
};

const ZERO: Duration = Duration::ZERO;

fn make_sample(payload: &str, frag_num: u32, frag_count: u32) -> Sample {
    SampleBuilder::put(KeyExpr::try_from("test/key").unwrap(), payload)
        .frag_info(FragInfo::new(frag_count, frag_num))
        .into()
}

#[test]
fn out_of_order_insert_completes() {
    let s0 = make_sample("A", 0, 3);
    let s1 = make_sample("B", 1, 3);
    let s2 = make_sample("C", 2, 3);

    let mut pending = PendingSample::from_first_fragment(s2, 2, 3, MAX_FRAGMENTS_DEFAULT).unwrap();
    assert!(!pending.is_complete());
    pending.insert(s1, 1, 3).unwrap();
    assert!(!pending.is_complete());
    pending.insert(s0, 0, 3).unwrap();
    assert!(pending.is_complete());

    let sample = pending.into_sample().unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "ABC");
}

#[test]
fn invalid_frag_num_ge_count() {
    let s = make_sample("A", 5, 3);
    assert!(matches!(
        PendingSample::from_first_fragment(s, 5, 3, MAX_FRAGMENTS_DEFAULT),
        Err(FragInsertError::InvalidFragNum {
            frag_count: 3,
            frag_num: 5,
        })
    ));
}

#[test]
fn invalid_frag_count_zero() {
    let s = make_sample("A", 0, 0);
    assert!(matches!(
        PendingSample::from_first_fragment(s, 0, 0, MAX_FRAGMENTS_DEFAULT),
        Err(FragInsertError::InvalidFragCount { frag_count: 0 })
    ));
}

#[test]
fn exceeds_max() {
    let s = make_sample("A", 0, 5000);
    assert!(matches!(
        PendingSample::from_first_fragment(s, 0, 5000, MAX_FRAGMENTS_DEFAULT),
        Err(FragInsertError::ExceedsMax {
            frag_count: 5000,
            max: 4096,
        })
    ));
}

#[test]
fn incomplete_assembly_does_not_deliver() {
    let pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 2), 0, 2, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    assert!(!pending.is_complete());
    assert!(pending.is_incomplete());
    assert!(pending.into_sample().is_none());
}

#[test]
fn single_fragment_is_whole() {
    let s = make_sample("A", 0, 1);
    let mut pending = PendingSample::from_first_fragment(s, 0, 1, MAX_FRAGMENTS_DEFAULT).unwrap();
    assert!(matches!(pending, PendingSample::Whole(_)));
    assert!(pending.is_complete());
    assert!(!pending.is_incomplete());
    assert!(!pending.can_abandon());
    assert!(pending.prepare_recovery(ZERO, TimerTick).is_none());
    assert_eq!(
        pending
            .into_sample()
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap()
            .as_ref(),
        "A"
    );
}

#[test]
fn in_order_insert_completes() {
    let s0 = make_sample("A", 0, 3);
    let s1 = make_sample("B", 1, 3);
    let s2 = make_sample("C", 2, 3);
    let mut pending = PendingSample::from_first_fragment(s0, 0, 3, MAX_FRAGMENTS_DEFAULT).unwrap();
    pending.insert(s1, 1, 3).unwrap();
    pending.insert(s2, 2, 3).unwrap();
    assert!(pending.is_complete());
    assert_eq!(
        pending
            .into_sample()
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap()
            .as_ref(),
        "ABC"
    );
}

#[test]
fn missing_ranges() {
    let s0 = make_sample("A", 0, 5);
    let s2 = make_sample("C", 2, 5);
    let s4 = make_sample("E", 4, 5);
    let mut pending = PendingSample::from_first_fragment(s0, 0, 5, MAX_FRAGMENTS_DEFAULT).unwrap();
    pending.insert(s2, 2, 5).unwrap();
    pending.insert(s4, 4, 5).unwrap();
    let (ranges, _) = pending.clone().prepare_recovery(ZERO, TimerTick).unwrap();
    assert_eq!(
        ranges,
        vec![FragRange::new(1, Some(1)), FragRange::new(3, Some(3))]
    );

    // Without a tail, the live and scan paths select the same holes.
    assert_eq!(
        pending.prepare_recovery(ZERO, FragmentArrival).unwrap().0,
        ranges
    );
}

#[test]
fn missing_holes_and_tail() {
    // Fragments 0, 1 and 3 of 5 received: hole at 2, tail at 4.
    let s0 = make_sample("A", 0, 5);
    let s1 = make_sample("B", 1, 5);
    let s3 = make_sample("D", 3, 5);
    let mut pending = PendingSample::from_first_fragment(s0, 0, 5, MAX_FRAGMENTS_DEFAULT).unwrap();
    pending.insert(s1, 1, 5).unwrap();
    pending.insert(s3, 3, 5).unwrap();
    assert_eq!(
        pending
            .clone()
            .prepare_recovery(ZERO, FragmentArrival)
            .unwrap()
            .0,
        vec![FragRange::new(2, Some(2))]
    );
    assert_eq!(
        pending.clone().prepare_recovery(ZERO, TimerTick).unwrap().0,
        vec![FragRange::new(2, Some(2)), FragRange::new(4, None)]
    );
    // An active tail is excluded even on scan ticks.
    assert_eq!(
        pending
            .prepare_recovery(Duration::from_secs(3600), TimerTick)
            .unwrap()
            .0,
        vec![FragRange::new(2, Some(2))]
    );
}

#[test]
fn duplicate_fragments_do_not_postpone_tail_recovery() {
    let delay = Duration::from_secs(60);
    let stalled = Instant::now() - delay;
    let mut pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 3), 0, 3, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    let PendingSample::Fragmented(FragmentAssembly { last_progress, .. }) = &mut pending else {
        panic!("expected fragmented assembly");
    };
    *last_progress = stalled;

    for _ in 0..5 {
        pending.insert(make_sample("X", 0, 3), 0, 3).unwrap();
        let PendingSample::Fragmented(FragmentAssembly { last_progress, .. }) = &pending else {
            panic!("expected fragmented assembly");
        };
        assert_eq!(*last_progress, stalled);
        assert_eq!(
            pending
                .clone()
                .prepare_recovery(delay, TimerTick)
                .unwrap()
                .0,
            vec![FragRange::new(1, None)]
        );
    }
}

#[test]
fn new_fragment_postpones_tail_recovery() {
    let delay = Duration::from_secs(60);
    let stalled = Instant::now() - delay;
    let mut pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 3), 0, 3, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    let PendingSample::Fragmented(FragmentAssembly { last_progress, .. }) = &mut pending else {
        panic!("expected fragmented assembly");
    };
    *last_progress = stalled;

    pending.insert(make_sample("B", 1, 3), 1, 3).unwrap();
    let PendingSample::Fragmented(FragmentAssembly { last_progress, .. }) = &pending else {
        panic!("expected fragmented assembly");
    };
    assert!(*last_progress > stalled);
    assert!(pending.prepare_recovery(delay, TimerTick).is_none());
    assert_eq!(
        pending.prepare_recovery(ZERO, TimerTick).unwrap().0,
        vec![FragRange::new(2, None)]
    );
}

#[test]
fn recovery_is_exclusive_and_preserves_unrequested_tail() {
    let mut pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 5), 0, 5, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    pending.insert(make_sample("D", 3, 5), 3, 5).unwrap();
    let (ranges, token) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    pending.insert(make_sample("B", 1, 5), 1, 5).unwrap();
    assert!(pending.prepare_recovery(ZERO, TimerTick).is_none());
    pending.insert(make_sample("C", 2, 5), 2, 5).unwrap();
    assert!(!pending.finish_recovery(&token, &ranges));
    assert!(pending.is_incomplete());
    assert!(!pending.is_retry());
    assert_eq!(
        pending.prepare_recovery(ZERO, TimerTick).unwrap().0,
        vec![FragRange::new(4, None)]
    );
}

#[test]
fn failed_recovery_preserves_fragments_and_enforces_cooldown() {
    let mut pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 4), 0, 4, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    // Missing middle fragments are queryable immediately. Only the retry
    // cooldown can prevent another attempt; the tail delay is irrelevant.
    pending.insert(make_sample("D", 3, 4), 3, 4).unwrap();
    let (ranges, token) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    assert_eq!(ranges, vec![FragRange::new(1, Some(2))]);
    pending.insert(make_sample("B", 1, 4), 1, 4).unwrap();
    assert!(pending.finish_recovery(&token, &ranges));
    assert!(!pending.is_abandoned());
    assert!(pending.is_incomplete());
    assert!(pending.is_retry());
    assert!(pending.can_abandon());
    assert!(pending
        .prepare_recovery(Duration::from_secs(60), FragmentArrival)
        .is_none());
    assert!(!pending.recovery_matches(&token));
    // Duplicates do not erase failure or bypass the cooldown.
    pending.insert(make_sample("A", 0, 4), 0, 4).unwrap();
    assert!(pending.can_abandon());
    assert!(pending
        .prepare_recovery(Duration::from_secs(60), FragmentArrival)
        .is_none());
    pending.insert(make_sample("C", 2, 4), 2, 4).unwrap();
    assert!(pending.is_complete());
    assert!(!pending.can_abandon());
    assert_eq!(
        pending
            .into_sample()
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap(),
        "ABCD"
    );
}

#[test]
fn retry_cooldown_expires_and_stale_attempt_cannot_finish_retry() {
    let delay = Duration::from_secs(60);
    let mut pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 3), 0, 3, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    pending.insert(make_sample("C", 2, 3), 2, 3).unwrap();
    let (ranges, first) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    assert_eq!(ranges, vec![FragRange::new(1, Some(1))]);
    assert!(pending.finish_recovery(&first, &ranges));
    assert!(pending.prepare_recovery(delay, FragmentArrival).is_none());
    let PendingSample::Fragmented(FragmentAssembly { recovery, .. }) = &mut pending else {
        panic!("expected fragmented assembly");
    };
    // Expire only the retry cooldown. No change to last_progress is needed
    // because the missing middle fragment does not depend on the tail delay.
    *recovery = FragmentRecovery::FailedAt(Instant::now() - delay);
    let (retry_ranges, retry) = pending.prepare_recovery(delay, FragmentArrival).unwrap();
    assert_eq!(retry_ranges, ranges);
    assert!(!pending.can_abandon());
    assert!(!pending.is_retry());
    assert!(!pending.finish_recovery(&first, &ranges));
    assert!(pending.recovery_matches(&retry));
    assert!(pending.prepare_recovery(ZERO, FragmentArrival).is_none());
}

#[test]
fn progress_after_failure_earns_a_fresh_attempt() {
    let mut pending =
        PendingSample::from_first_fragment(make_sample("C", 2, 4), 2, 4, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    let (ranges, attempt) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    assert_eq!(ranges, vec![FragRange::new(0, Some(1))]);
    assert!(pending.finish_recovery(&attempt, &ranges));
    assert!(pending.can_abandon());
    pending.insert(make_sample("A", 0, 4), 0, 4).unwrap();
    assert!(pending.is_incomplete());
    assert!(!pending.can_abandon());
    // Progress prevents abandonment but still respects the retry cooldown.
    assert!(pending
        .prepare_recovery(Duration::from_secs(60), FragmentArrival)
        .is_none());
    let (ranges, retry) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    assert_eq!(ranges, vec![FragRange::new(1, Some(1))]);
    pending.insert(make_sample("B", 1, 4), 1, 4).unwrap();
    assert!(!pending.finish_recovery(&retry, &ranges));
    assert!(!pending.can_abandon());
    assert_eq!(
        pending.prepare_recovery(ZERO, TimerTick).unwrap().0,
        vec![FragRange::new(3, None)]
    );
}

#[test]
fn missing_tail_only() {
    // Sequential prefix 0, 1, 2 of 5: no hole, tail at 3..
    let s0 = make_sample("A", 0, 5);
    let s1 = make_sample("B", 1, 5);
    let s2 = make_sample("C", 2, 5);
    let mut pending = PendingSample::from_first_fragment(s0, 0, 5, MAX_FRAGMENTS_DEFAULT).unwrap();
    pending.insert(s1, 1, 5).unwrap();
    pending.insert(s2, 2, 5).unwrap();
    assert!(pending.prepare_recovery(ZERO, FragmentArrival).is_none());
    assert_eq!(
        pending.prepare_recovery(ZERO, TimerTick).unwrap().0,
        vec![FragRange::new(3, None)]
    );
}

#[test]
fn whole_sample_count_mismatch() {
    let s = make_sample("A", 0, 1);
    let mut pending = PendingSample::single(s);
    let s3 = make_sample("B", 0, 3);
    assert!(matches!(
        pending.insert(s3, 0, 3),
        Err(FragInsertError::CountMismatch {
            expected: 1,
            got: 3
        })
    ));
}

#[test]
fn insert_fragment_duplicate_overwrites() {
    let s0 = make_sample("A", 0, 3);
    let s0_dup = make_sample("X", 0, 3);
    let s1 = make_sample("B", 1, 3);
    let s2 = make_sample("C", 2, 3);

    let mut pending = PendingSample::from_first_fragment(s0, 0, 3, MAX_FRAGMENTS_DEFAULT).unwrap();
    pending.insert(s0_dup, 0, 3).unwrap(); // duplicate, overwrites
    pending.insert(s1, 1, 3).unwrap();
    pending.insert(s2, 2, 3).unwrap();

    let sample = pending.into_sample().unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap().as_ref(), "XBC");
}

#[test]
fn insert_whole_duplicate_overwrites() {
    let s = make_sample("A", 0, 1);
    let mut pending = PendingSample::single(s);
    pending.insert(make_sample("X", 0, 1), 0, 1).unwrap();
    assert_eq!(
        pending
            .into_sample()
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap()
            .as_ref(),
        "X"
    );
}

#[test]
fn complete_assembly_retains_fragments_and_recovery_until_delivery() {
    let mut pending =
        PendingSample::from_first_fragment(make_sample("A", 0, 3), 0, 3, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    pending.insert(make_sample("C", 2, 3), 2, 3).unwrap();
    let (ranges, token) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    pending.insert(make_sample("B", 1, 3), 1, 3).unwrap();
    assert!(pending.is_complete());
    assert!(!pending.is_incomplete());
    assert!(pending.recovery_matches(&token));
    let PendingSample::Fragmented(assembly) = &pending else {
        panic!("expected complete fragmented assembly");
    };
    let completed_at = assembly.last_progress;

    // Ordered delivery may wait for older publications after this completes.
    pending.insert(make_sample("X", 1, 3), 1, 3).unwrap();
    let PendingSample::Fragmented(assembly) = &pending else {
        panic!("expected complete fragmented assembly");
    };
    assert_eq!(assembly.last_progress, completed_at);
    assert!(pending.recovery_matches(&token));
    assert!(pending.prepare_recovery(ZERO, TimerTick).is_none());
    assert!(!pending.finish_recovery(&token, &ranges));
    assert!(!pending.recovery_matches(&token));
    assert!(!pending.can_abandon());
    assert!(pending.prepare_recovery(ZERO, TimerTick).is_none());
    assert_eq!(
        pending
            .into_sample()
            .unwrap()
            .payload()
            .try_to_string()
            .unwrap(),
        "AXC"
    );
}

#[test]
fn recovery_ranges_preserve_inclusive_bounds_and_open_tail() {
    let mut pending =
        PendingSample::from_first_fragment(make_sample("C", 2, 4), 2, 4, MAX_FRAGMENTS_DEFAULT)
            .unwrap();
    let (ranges, token) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    assert_eq!(ranges, vec![FragRange::new(0, Some(1))]);
    // The prefix starts at fragment zero and includes its end.
    pending.insert(make_sample("A", 0, 4), 0, 4).unwrap();
    assert!(pending.finish_recovery(&token, &ranges));

    let (_, retry) = pending.prepare_recovery(ZERO, FragmentArrival).unwrap();
    pending.insert(make_sample("B", 1, 4), 1, 4).unwrap();
    assert!(!pending.finish_recovery(&retry, &ranges));
    assert!(pending.is_incomplete());
    assert!(!pending.is_retry());

    let (tail, token) = pending.prepare_recovery(ZERO, TimerTick).unwrap();
    assert_eq!(tail, vec![FragRange::new(3, None)]);
    assert!(pending.finish_recovery(&token, &tail));
    let (_, retry) = pending.prepare_recovery(ZERO, TimerTick).unwrap();
    pending.insert(make_sample("D", 3, 4), 3, 4).unwrap();
    assert!(!pending.finish_recovery(&retry, &tail));
    assert!(pending.is_complete());
}

#[test]
fn fragment_count_exact_division() {
    assert_eq!(fragment_count(12, 4).unwrap(), 3);
}

#[test]
fn fragment_count_leaves_remainder() {
    assert_eq!(fragment_count(13, 4).unwrap(), 4);
}

#[test]
fn fragment_count_fits_even_beyond_default_cap() {
    // The subscriber-side cap is not enforced here: 4097 fragments exceed
    // MAX_FRAGMENTS_DEFAULT but are representable on the wire.
    assert_eq!(
        fragment_count(MAX_FRAGMENTS_DEFAULT as usize + 1, 1).unwrap(),
        MAX_FRAGMENTS_DEFAULT + 1
    );
}

#[test]
fn fragment_count_max_u32() {
    assert_eq!(fragment_count(u32::MAX as usize, 1).unwrap(), u32::MAX);
}

#[test]
fn fragment_count_overflow() {
    assert!(fragment_count(u32::MAX as usize + 1, 1).is_err());
    assert!(fragment_count(usize::MAX / 2, 1).is_err());
}
