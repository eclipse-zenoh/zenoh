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

use std::collections::{HashMap, HashSet};

use super::{Digest, Fingerprint};
use crate::replication::{
    classification::{IntervalIdx, SubIntervalIdx},
    digest::DigestDiff,
};

#[test]
fn test_diff() {
    // Base Digest. The actual values of the Fingerprints do not matter.
    let digest = Digest {
        configuration_fingerprint: Fingerprint(15),
        cold_era_fingerprint: Fingerprint(10),
        warm_era_fingerprints: HashMap::from([
            (IntervalIdx(1), Fingerprint(1)),
            (IntervalIdx(2), Fingerprint(2)),
            (IntervalIdx(4), Fingerprint(4)),
        ]),
        hot_era_fingerprints: HashMap::from([
            (
                IntervalIdx(1),
                HashMap::from([(SubIntervalIdx(1), Fingerprint(1))]),
            ),
            (
                IntervalIdx(2),
                HashMap::from([
                    (SubIntervalIdx(1), Fingerprint(1)),
                    (SubIntervalIdx(3), Fingerprint(3)),
                ]),
            ),
            (
                IntervalIdx(4),
                HashMap::from([
                    (SubIntervalIdx(1), Fingerprint(1)),
                    (SubIntervalIdx(2), Fingerprint(2)),
                ]),
            ),
        ]),
    };

    // Test: everything matches except the configuration -> should return None.
    let mut other_digest = digest.clone();
    other_digest.configuration_fingerprint = Fingerprint(5); // 5 vs 15

    assert!(digest.diff(other_digest).is_none());

    // Test: everything matches -> should return None.
    assert!(digest.diff(digest.clone()).is_none());

    // Test: only cold era differs -> should return Some with everything else being empty.
    let mut other_digest = digest.clone();
    other_digest.cold_era_fingerprint = Fingerprint(15); // 15 vs 10
    let expected_diff = Some(DigestDiff {
        cold_eras_differ: true,
        warm_eras_differences: HashSet::default(),
        hot_eras_differences: HashMap::default(),
    });
    assert_eq!(expected_diff, digest.diff(other_digest));

    // Test: `digest` has an interval that other does not have -> should not appear in the diff. The
    // diff is only concerned by intervals from `other` that either differ from `digest` or that are
    // missing from `digest`.
    let mut other_digest = digest.clone();
    other_digest.warm_era_fingerprints = HashMap::from([
        (IntervalIdx(1), Fingerprint(1)),
        (IntervalIdx(2), Fingerprint(2)),
        // (IntervalIdx(4), Fingerprint(4)),  // Not missing in `digest`.
    ]);

    assert!(digest.diff(other_digest).is_none());

    // Test: `other` has both (i) an interval also present in `digest` that differs and (ii) an
    // interval that `digest` does not have.
    let mut other_digest = digest.clone();
    other_digest.warm_era_fingerprints = HashMap::from([
        (IntervalIdx(1), Fingerprint(1)),
        (IntervalIdx(2), Fingerprint(20)), // 20 vs 2
        (IntervalIdx(3), Fingerprint(3)),
        // (IntervalIdx(4), Fingerprint(4)),  // Not missing in `digest`.
    ]);

    let expected_diff = Some(DigestDiff {
        cold_eras_differ: false,
        warm_eras_differences: HashSet::from([IntervalIdx(2), IntervalIdx(3)]),
        hot_eras_differences: HashMap::default(),
    });
    assert_eq!(expected_diff, digest.diff(other_digest));

    // Test: `digest` has an interval that `other` does not have + in an interval they both have,
    // `digest` has a sub-interval that `other` does not have -> the diff should still be None.
    let mut other_digest = digest.clone();
    other_digest.hot_era_fingerprints = HashMap::from([
        // Not missing in `digest`
        // (
        //     IntervalIdx(1),
        //     HashMap::from([(SubIntervalIdx(1), Fingerprint(1))]),
        // ),
        (
            IntervalIdx(2),
            HashMap::from([
                (SubIntervalIdx(1), Fingerprint(1)),
                (SubIntervalIdx(3), Fingerprint(3)),
            ]),
        ),
        (
            IntervalIdx(4),
            HashMap::from([
                (SubIntervalIdx(1), Fingerprint(1)),
                // (SubIntervalIdx(2), Fingerprint(2)),  // Not missing in `digest`
            ]),
        ),
    ]);

    assert!(digest.diff(other_digest).is_none());

    // Test: `other` has (i) an interval also present in `digest` that differs, (ii) an interval
    // that `digest` does not have, (iii) a sub-interval in an interval shared by both that `digest`
    // does not have, and (iv) a sub-interval in an interval shared by both that differs from
    // `digest`.
    let mut other_digest = digest.clone();
    other_digest.hot_era_fingerprints = HashMap::from([
        // Not missing in `digest`
        // (
        //     IntervalIdx(1),
        //     HashMap::from([(SubIntervalIdx(1), Fingerprint(1))]),
        // ),
        (
            IntervalIdx(2),
            HashMap::from([
                (SubIntervalIdx(1), Fingerprint(1)),
                (SubIntervalIdx(2), Fingerprint(2)), // Not present in `digest`
                (SubIntervalIdx(3), Fingerprint(30)), // 30 vs 3
            ]),
        ),
        (
            IntervalIdx(3),
            HashMap::from([
                (SubIntervalIdx(1), Fingerprint(1)),
                (SubIntervalIdx(2), Fingerprint(2)),
            ]),
        ),
        (
            IntervalIdx(4),
            HashMap::from([
                (SubIntervalIdx(1), Fingerprint(1)),
                // (SubIntervalIdx(2), Fingerprint(2)),  // Not missing in `digest`
            ]),
        ),
    ]);

    let expected_diff = Some(DigestDiff {
        cold_eras_differ: false,
        hot_eras_differences: HashMap::from([
            (
                IntervalIdx(2),
                HashSet::from([SubIntervalIdx(2), SubIntervalIdx(3)]),
            ),
            (
                IntervalIdx(3),
                HashSet::from([SubIntervalIdx(1), SubIntervalIdx(2)]),
            ),
        ]),
        warm_eras_differences: HashSet::default(),
    });
    assert_eq!(expected_diff, digest.diff(other_digest));
}
