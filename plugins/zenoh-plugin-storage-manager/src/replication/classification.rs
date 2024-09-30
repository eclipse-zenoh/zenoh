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
    collections::{BTreeMap, HashMap, HashSet},
    ops::{Deref, Sub},
};

use serde::{Deserialize, Serialize};
use zenoh::{key_expr::OwnedKeyExpr, time::Timestamp};

use super::{digest::Fingerprint, log::Event};

/// The `EventRemoval` enumeration lists the possible outcomes when searching for an older [Event]
/// and removing it if one was found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EventRemoval {
    /// The Replication Log contains no [Event] with the provided key expression.
    NotFound,
    /// An [Event] with the same key expression and an earlier (or identical) timestamp is already
    /// present in the Replication Log.
    KeptNewer,
    /// An [Event] with the same key expression and an older timestamp was removed from the
    /// Replication Log.
    RemovedOlder(Event),
}

/// An `IntervalIdx` represents the index of an `Interval`.
///
/// It is a thin wrapper around a `u64`.
#[derive(Deserialize, Serialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
#[repr(transparent)]
pub struct IntervalIdx(pub(crate) u64);

impl Deref for IntervalIdx {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u64> for IntervalIdx {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Sub<u64> for IntervalIdx {
    type Output = IntervalIdx;

    fn sub(self, rhs: u64) -> Self::Output {
        (self.0 - rhs).into()
    }
}

/// An `Interval` is a subdivision of a replication Log.
///
/// It contains a set of [SubInterval]s, each of which, in turn, contains a set of [Event]s.
///
/// A [Fingerprint] is associated to an `Interval` and is equal to the "exclusive or" (XOR) of the
/// [Fingerprint] of all the [SubInterval]s it contains.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Interval {
    pub(crate) fingerprint: Fingerprint,
    pub(crate) sub_intervals: BTreeMap<SubIntervalIdx, SubInterval>,
}

impl<const N: usize> From<[(SubIntervalIdx, SubInterval); N]> for Interval {
    fn from(sub_intervals: [(SubIntervalIdx, SubInterval); N]) -> Self {
        Self {
            fingerprint: sub_intervals
                .iter()
                .fold(Fingerprint::default(), |acc, (_, sub_interval)| {
                    acc ^ sub_interval.fingerprint
                }),
            sub_intervals: sub_intervals.into(),
        }
    }
}

impl Interval {
    /// Returns true if the Replication Log only contains a single Event for each key expression.
    ///
    /// To perform that check a HashSet is constructed by visiting each Interval and each
    /// SubInterval, filling the HashSet with the key expression of all the Events contained.
    ///
    /// ⚠️ This method will only be called if Zenoh is compiled in Debug mode.
    #[cfg(debug_assertions)]
    pub(crate) fn assert_only_one_event_per_key_expr(
        &self,
        events: &mut HashSet<Option<OwnedKeyExpr>>,
    ) -> bool {
        for sub_interval in self.sub_intervals.values() {
            if !sub_interval.assert_only_one_event_per_key_expr(events) {
                return false;
            }
        }

        true
    }

    /// Returns the [Fingerprint] of this [Interval].
    ///
    /// The [Fingerprint] of an [Interval] is equal to the XOR (exclusive or) of the fingerprints
    /// of the all the [SubInterval]s it contains.
    pub(crate) fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Lookup the provided key expression and return, if found, its associated [Event].
    pub(crate) fn lookup(&self, stripped_key: &Option<OwnedKeyExpr>) -> Option<&Event> {
        for sub_interval in self.sub_intervals.values() {
            if let Some(event) = sub_interval.events.get(stripped_key) {
                return Some(event);
            }
        }

        None
    }

    /// Returns an [HashMap] of the index and [Fingerprint] of all the [SubInterval]s contained in
    /// this [Interval].
    pub(crate) fn sub_intervals_fingerprints(&self) -> HashMap<SubIntervalIdx, Fingerprint> {
        self.sub_intervals
            .iter()
            .filter(|(_, sub_interval)| sub_interval.fingerprint != Fingerprint::default())
            .map(|(sub_interval_idx, sub_interval)| (*sub_interval_idx, sub_interval.fingerprint))
            .collect()
    }

    /// Inserts the [Event] in the [SubInterval] specified by the provided [SubIntervalIdx],
    /// regardless of its [Timestamp].
    ///
    /// The fingerprint of the [Interval] is also updated.
    ///
    /// # Caveat: "_unchecked"
    ///
    /// As its name indicates, this method DOES NOT CHECK if there is another [Event] associated to
    /// the same key expression (regardless of its [Timestamp]).
    ///
    /// This uniqueness property (i.e. there should only be a single [Event] in the replication Log
    /// for a given key expression) cannot be enforced at the [Interval] level. Hence, this method
    /// assumes the check has already been performed and thus does not do redundant work.
    pub(crate) fn insert_unchecked(&mut self, sub_interval_idx: SubIntervalIdx, event: Event) {
        self.fingerprint ^= event.fingerprint();
        self.sub_intervals
            .entry(sub_interval_idx)
            .or_default()
            .insert_unchecked(event);
    }

    /// Removes, if one exists, the [Event] associated with the provided key expression if its
    /// [Timestamp] is older than that of the provided one.
    ///
    /// This method will go through all of the [SubInterval]s included in this [Interval] and stop
    /// at the first that indicates having an [Event] for the provided key expression.
    ///
    /// The [Fingerprint] of this Interval will be updated accordingly.
    ///
    /// This method returns, through the [EventRemoval] enumeration, the action that was performed.
    pub(crate) fn remove_older(
        &mut self,
        key_expr: &Option<OwnedKeyExpr>,
        timestamp: &Timestamp,
    ) -> EventRemoval {
        let mut sub_interval_idx_to_remove = None;
        let mut result = EventRemoval::NotFound;

        for (sub_interval_idx, sub_interval) in self.sub_intervals.iter_mut() {
            result = sub_interval.remove_older(key_expr, timestamp);
            if let EventRemoval::RemovedOlder(ref old_event) = result {
                self.fingerprint ^= old_event.fingerprint();
                if sub_interval.events.is_empty() {
                    sub_interval_idx_to_remove = Some(*sub_interval_idx);
                }
            }

            // If the SubInterval returned anything other than `NotFound`, we can exit the search.
            if !matches!(result, EventRemoval::NotFound) {
                break;
            }
        }

        if let Some(sub_interval_idx) = sub_interval_idx_to_remove {
            self.sub_intervals.remove(&sub_interval_idx);
        }

        result
    }
}

/// A `SubIntervalIdx` represents the index of a [SubInterval].
///
/// It is a thin wrapper around a `u64`.
#[derive(Deserialize, Serialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
#[repr(transparent)]
pub struct SubIntervalIdx(pub(crate) u64);

impl Deref for SubIntervalIdx {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u64> for SubIntervalIdx {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A `SubInterval` is a subdivision of an [Interval] and groups together a set of [Event]s.
///
/// A [Fingerprint] is associated to a `SubInterval` and is equal to the "exclusive or" of all the
/// [Event]s it contains.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SubInterval {
    pub(crate) fingerprint: Fingerprint,
    pub(crate) events: HashMap<Option<OwnedKeyExpr>, Event>,
}

impl<const N: usize> From<[Event; N]> for SubInterval {
    fn from(events: [Event; N]) -> Self {
        let fingerprint = events.iter().fold(Fingerprint::default(), |acc, event| {
            acc ^ event.fingerprint()
        });

        Self {
            fingerprint,
            events: events
                .into_iter()
                .map(|event| (event.key_expr().clone(), event))
                .collect(),
        }
    }
}

impl SubInterval {
    /// Returns true if the Replication Log only contains a single Event for each key expression.
    ///
    /// To perform that check a HashSet is constructed by visiting each Interval and each
    /// SubInterval, filling the HashSet with the key expression of all the Events contained.
    ///
    /// ⚠️ This method will only be called if Zenoh is compiled in Debug mode.
    #[cfg(debug_assertions)]
    fn assert_only_one_event_per_key_expr(
        &self,
        events: &mut HashSet<Option<OwnedKeyExpr>>,
    ) -> bool {
        for event_ke in self.events.keys() {
            if !events.insert(event_ke.clone()) {
                tracing::error!(
                    "FATAL ERROR, REPLICATION LOG INVARIANT VIOLATED, KEY APPEARS MULTIPLE TIMES: \
                     < {event_ke:?} >"
                );
                return false;
            }
        }

        true
    }

    /// Inserts the [Event], regardless of its [Timestamp].
    ///
    /// This method also updates the fingerprint of the [SubInterval].
    ///
    /// # Caveat: "_unchecked"
    ///
    /// As its name indicates, this method DOES NOT CHECK if there is another [Event] associated to
    /// the same key expression (regardless of its [Timestamp]).
    ///
    /// This uniqueness property (i.e. there should only be a single [Event] in the replication Log
    /// for a given key expression) cannot be enforced at the [SubInterval] level. Hence, this
    /// method assumes the check has already been performed and thus does not do redundant work.
    ///
    /// In the unlikely scenario that this has happened, the [Fingerprint] of the [SubInterval] will
    /// be updated to keep it correct and a warning message will be emitted.
    fn insert_unchecked(&mut self, event: Event) {
        self.fingerprint ^= event.fingerprint();
        if let Some(replaced_event) = self.events.insert(event.key_expr().clone(), event) {
            tracing::warn!(
                "Call to `insert_unchecked` replaced an Event in the replication Log, this should \
                 NOT have happened: {replaced_event:?}"
            );
            self.fingerprint ^= replaced_event.fingerprint();
        }
    }

    /// Removes, if one exists, the [Event] associated with the provided key expression if its
    /// [Timestamp] is older than that of the provided one.
    ///
    /// This method returns, through the [EventRemoval] enumeration, returns the action that was
    /// performed.
    ///
    /// The [Fingerprint] of this SubInterval will be updated accordingly.
    fn remove_older(
        &mut self,
        key_expr: &Option<OwnedKeyExpr>,
        timestamp: &Timestamp,
    ) -> EventRemoval {
        if let Some((key_expr, event)) = self.events.remove_entry(key_expr) {
            if event.timestamp() < timestamp {
                self.fingerprint ^= event.fingerprint();
                return EventRemoval::RemovedOlder(event);
            } else {
                self.events.insert(key_expr, event);
                return EventRemoval::KeptNewer;
            }
        }

        EventRemoval::NotFound
    }
}

#[cfg(test)]
#[path = "./tests/classification.test.rs"]
mod tests;
