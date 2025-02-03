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
    collections::{HashMap, HashSet},
    ops::{BitXor, BitXorAssign, Deref},
};

use serde::{Deserialize, Serialize};

use super::classification::{IntervalIdx, SubIntervalIdx};

/// A [Fingerprint] is a 64 bits hash of the content it "represents".
///
/// The crate used to obtain this hash is <https://crates.io/crates/xxhash-rust>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Default, Deserialize, Serialize)]
#[repr(transparent)]
pub struct Fingerprint(u64);

impl Deref for Fingerprint {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl BitXor for Fingerprint {
    type Output = Fingerprint;

    fn bitxor(self, rhs: Self) -> Self::Output {
        (self.0 ^ rhs.0).into()
    }
}

impl BitXorAssign for Fingerprint {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0
    }
}

impl From<u64> for Fingerprint {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A `Digest` is a concise view of the data present in a Storage.
///
/// The purpose of this structure is to be sent over the network to other replicated Storage
/// (i.e. remote Replicas) such that they can quickly assess if they are misaligned, i.e. if their
/// data differ.
///
/// To make this assessment quick and lightweight, the `Digest` is composed of a set [Fingerprint]s
/// for each "Era". An "Era" is a way of grouping sets of [Event]s according to their
/// [Timestamp]. There are three eras: "Hot", "Warm" and "Cold". The closer a timestamp is to the
/// current time, the "hotter" the era it belongs to will be.
///
/// Eras are further divided into [Interval]s and [SubInterval]s — which duration and number can be
/// configured.
///
/// [Event]: super::log::Event
/// [Timestamp]: zenoh::time::Timestamp
/// [Interval]: super::classification::Interval
/// [SubInterval]: super::classification::SubInterval
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Digest {
    pub(crate) configuration_fingerprint: Fingerprint,
    pub(crate) cold_era_fingerprint: Fingerprint,
    pub(crate) warm_era_fingerprints: HashMap<IntervalIdx, Fingerprint>,
    pub(crate) hot_era_fingerprints: HashMap<IntervalIdx, HashMap<SubIntervalIdx, Fingerprint>>,
}

/// The `DigestDiff` summarises the differences between two [Digest]s.
///
/// For the Cold Era, a `bool` indicates if the two [Fingerprint]s differ.
///
/// For the Warm Era, the set of [IntervalIdx] that differ is computed.
///
/// For the Hot Era, the set of [SubIntervalIdx], grouped by their [IntervalIdx], that differ is
/// computed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DigestDiff {
    pub(crate) cold_eras_differ: bool,
    pub(crate) warm_eras_differences: HashSet<IntervalIdx>,
    pub(crate) hot_eras_differences: HashMap<IntervalIdx, HashSet<SubIntervalIdx>>,
}

impl Digest {
    /// Returns a [DigestDiff] if the two [Digest] differ, `None` otherwise.
    ///
    /// Two Digests are considered different if any of the following is true:
    /// - The Fingerprints of their Cold Era differ.
    /// - At least one Fingerprint in their Warm Era differ (for the same Interval).
    /// - `other` contains at least one Interval in its Warm Era that `self` does not have.
    /// - At least one Fingerprint in their Hot Era differ (for the same Interval and Sub-Interval).
    /// - `other` contains at least one Sub-Interval in its Hot Era that `self` does not have.
    pub(crate) fn diff(&self, mut other: Digest) -> Option<DigestDiff> {
        if self.configuration_fingerprint != other.configuration_fingerprint {
            return None;
        }

        // Hot era. For all the intervals that are contained in `self`, we remove the sub-intervals
        // in `other` that have the same fingerprints as their counterpart in `self` for the same
        // sub-interval.
        //
        // The ultimate purpose if these loops is to keep in `other` only the intervals /
        // sub-intervals that differ or that are only present in `other`.
        for (interval_idx, sub_intervals_fingerprints) in &self.hot_era_fingerprints {
            if let Some(other_sub_intervals_fingerprints) =
                other.hot_era_fingerprints.get_mut(interval_idx)
            {
                other_sub_intervals_fingerprints.retain(|other_idx, other_fingerprint| {
                    match sub_intervals_fingerprints.get(other_idx) {
                        Some(fingerprint) => other_fingerprint != fingerprint,
                        None => true,
                    }
                });
            }
        }
        other
            .hot_era_fingerprints
            .retain(|_, sub_intervals| !sub_intervals.is_empty());

        // Warm era. Same process as for the hot era, we want to keep only the values that differ or
        // that are present only in `other`.
        other
            .warm_era_fingerprints
            .retain(|other_idx, other_fingerprint| {
                match self.warm_era_fingerprints.get(other_idx) {
                    Some(fingerprint) => other_fingerprint != fingerprint,
                    None => true,
                }
            });

        if !other.hot_era_fingerprints.is_empty() || !other.warm_era_fingerprints.is_empty() {
            return Some(DigestDiff {
                cold_eras_differ: self.cold_era_fingerprint != other.cold_era_fingerprint,
                warm_eras_differences: other.warm_era_fingerprints.into_keys().collect(),
                hot_eras_differences: other
                    .hot_era_fingerprints
                    .into_iter()
                    .map(|(interval_idx, sub_intervals)| {
                        (interval_idx, sub_intervals.into_keys().collect())
                    })
                    .collect(),
            });
        }

        if self.cold_era_fingerprint != other.cold_era_fingerprint {
            return Some(DigestDiff {
                cold_eras_differ: true,
                warm_eras_differences: HashSet::default(),
                hot_eras_differences: HashMap::default(),
            });
        }

        None
    }
}

#[cfg(test)]
#[path = "tests/digest.test.rs"]
mod tests;
