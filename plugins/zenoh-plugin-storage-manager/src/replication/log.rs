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
    ops::Deref,
};

use bloomfilter::Bloom;
use serde::{Deserialize, Serialize};
use zenoh::{key_expr::OwnedKeyExpr, sample::SampleKind, time::Timestamp, Result as ZResult};
use zenoh_backend_traits::config::ReplicaConfig;

use super::{
    classification::{EventLookup, EventRemoval, Interval, IntervalIdx},
    configuration::Configuration,
    digest::{Digest, Fingerprint},
};

/// The `Action` enumeration facilitates dealing with Wildcard Updates. It is a super-set of
/// [SampleKind].
///
/// The non-stripped key expression of a Wildcard Update is kept as it actually cannot be stripped.
///
/// For instance, if the configured `strip_prefix` is "test/replication", then the Wildcard Update
/// `put test/** 1` will (i) apply to all entries of the Storage yet (ii) does not start with the
/// prefix "test/replication".
///
/// We could, in theory, avoid storing the key expression of a Wildcard Update in this enumeration
/// but doing so simplifies the code of the Replication: if we deal with a Wildcard Update we have
/// the full key expression ready.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Hash)]
pub(crate) enum Action {
    Put,
    Delete,
    WildcardPut(OwnedKeyExpr),
    WildcardDelete(OwnedKeyExpr),
}

impl From<SampleKind> for Action {
    fn from(kind: SampleKind) -> Self {
        match kind {
            SampleKind::Put => Action::Put,
            SampleKind::Delete => Action::Delete,
        }
    }
}

impl From<&Action> for SampleKind {
    fn from(action: &Action) -> Self {
        match action {
            Action::Put | Action::WildcardPut(_) => SampleKind::Put,
            Action::Delete | Action::WildcardDelete(_) => SampleKind::Delete,
        }
    }
}

/// The enumeration `ActionKind` is used to generate the keys of the `BTreeMap` used internally to
/// store the Replication Log.
///
/// This enumeration helps differentiating a Wildcard Update from a "regular" one.
///
/// Indeed, two entries can be present for a Wildcard Update: one for a Wildcard Put, another for a
/// Wildcard Delete. This is so because their order matters: if a Wildcard Delete occurs first, the
/// keys it deletes cannot be overridden by a Wildcard Put. As we are in a distributed system, we
/// have no guarantee on the order in which the updates will be received.
///
/// Hence, if a Replica receives the Wildcard Update first, it might (rightfully) update
/// entries. Once it receives the Wildcard Delete, it should actually deletes these entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionKind {
    PutOrDelete,
    WildcardPut,
    WildcardDelete,
}

impl From<&Action> for ActionKind {
    fn from(action: &Action) -> Self {
        match action {
            Action::Put | Action::Delete => Self::PutOrDelete,
            Action::WildcardPut(_) => Self::WildcardPut,
            Action::WildcardDelete(_) => Self::WildcardDelete,
        }
    }
}

/// The `EventMetadata` structure contains all the information needed by a replica to assess if it
/// is missing an [Event] in its log.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Hash)]
pub struct EventMetadata {
    pub(crate) stripped_key: Option<OwnedKeyExpr>,
    pub(crate) timestamp: Timestamp,
    pub(crate) timestamp_last_non_wildcard_update: Option<Timestamp>,
    pub(crate) action: Action,
}

impl EventMetadata {
    pub fn key_expr(&self) -> &Option<OwnedKeyExpr> {
        &self.stripped_key
    }

    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }

    pub fn action(&self) -> &Action {
        &self.action
    }

    /// Returns the [LogLatestKey] corresponding to this [Event].
    pub fn log_key(&self) -> LogLatestKey {
        LogLatestKey {
            maybe_stripped_key: self.stripped_key.clone(),
            action: (&self.action).into(),
        }
    }
}

impl From<&Event> for EventMetadata {
    fn from(event: &Event) -> Self {
        Self {
            stripped_key: event.stripped_key.clone(),
            timestamp: event.timestamp,
            timestamp_last_non_wildcard_update: event.timestamp_last_non_wildcard_update,
            action: event.action.clone(),
        }
    }
}

/// An `Event` records the fact that a publication occurred on the associated key expression at the
/// associated timestamp.
///
/// When an `Event` is created, its [Fingerprint] is computed, using the `xxhash-rust` crate. This
/// [Fingerprint] is used to construct the [Digest] associated with the replication log.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Event {
    metadata: EventMetadata,
    fingerprint: Fingerprint,
}

impl Deref for Event {
    type Target = EventMetadata;

    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

impl From<EventMetadata> for Event {
    fn from(metadata: EventMetadata) -> Self {
        let fingerprint = Event::compute_fingerprint(metadata.key_expr(), metadata.timestamp());

        Event {
            metadata,
            fingerprint,
        }
    }
}

impl Event {
    // This helper function determines what [Action] should be associated to an [Event] considering
    // the provided `key_expr` and `action`.
    //
    // The tricky cases are when we deal with a Wildcard Update. When that happens, we need to check
    // the value of the `key_expr`: if it is equal to what is contained in the enumeration, then we
    // are dealing with the Wildcard Update itself and should return the action as is. However, if
    // they differ it means we are dealing with an [Event] that is overridden by the Wildcard Update
    // in which case the resulting action should be either Put or Delete (depending on what the
    // Wildcard is).
    fn determine_action(key_expr: &Option<OwnedKeyExpr>, action: &Action) -> Action {
        match action {
            Action::Put => return Action::Put,
            Action::Delete => return Action::Delete,
            Action::WildcardPut(wildcard_ke) => {
                if let Some(ke) = &key_expr {
                    if ke != wildcard_ke {
                        return Action::Put;
                    }
                }
            }
            Action::WildcardDelete(wildcard_ke) => {
                if let Some(ke) = &key_expr {
                    if ke != wildcard_ke {
                        return Action::Delete;
                    }
                }
            }
        }

        action.clone()
    }

    /// Creates a new [Event] with the provided key expression and timestamp.
    ///
    /// This function computes the [Fingerprint] of both using the `xxhash_rust` crate.
    pub fn new(key_expr: Option<OwnedKeyExpr>, timestamp: Timestamp, action: &Action) -> Self {
        let timestamp_last_non_wildcard_update = match action {
            Action::Put | Action::Delete => Some(timestamp),
            Action::WildcardPut(_) | Action::WildcardDelete(_) => None,
        };

        let actual_action = Event::determine_action(&key_expr, action);

        Self {
            fingerprint: Event::compute_fingerprint(&key_expr, &timestamp),
            metadata: EventMetadata {
                stripped_key: key_expr,
                timestamp,
                timestamp_last_non_wildcard_update,
                action: actual_action,
            },
        }
    }

    /// Computes the [Fingerprint] of the [Event], which is equal to the hash of its fields
    /// `timestamp` and `maybe_stripped_key`.
    ///
    /// We do not hash the other fields as they do not provide any additional properties -- and
    /// hashing more information would take more time, which could be detrimental if we have to
    /// process huge amounts of data.
    fn compute_fingerprint(
        maybe_stripped_key: &Option<OwnedKeyExpr>,
        timestamp: &Timestamp,
    ) -> Fingerprint {
        let mut hasher = xxhash_rust::xxh3::Xxh3::default();
        if let Some(key_expr) = maybe_stripped_key {
            hasher.update(key_expr.as_bytes());
        }
        hasher.update(&timestamp.get_time().0.to_le_bytes());
        hasher.update(&timestamp.get_id().to_le_bytes());

        hasher.digest().into()
    }

    /// Sets the `timestamp` and, according to the `action`, the
    /// `timestamp_last_non_wildcard_update` and updates the [Fingerprint] of the [Event].
    pub fn set_timestamp_and_action(&mut self, timestamp: Timestamp, action: Action) {
        if matches!(action, Action::Put | Action::Delete) {
            self.metadata.timestamp_last_non_wildcard_update = Some(timestamp);
        }

        self.metadata.timestamp = timestamp;
        self.metadata.action = Event::determine_action(self.key_expr(), &action);
        self.fingerprint = Event::compute_fingerprint(self.key_expr(), self.timestamp());
    }

    /// Returns the [Fingerprint] associated with this [Event].
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

/// The `EventInsertion` enumeration lists the possible outcomes when attempting to insert an
/// [Event] in the replication [Log].
///
/// The outcomes are:
/// - `New(Event)`: there was no [Event] in the log with the same key expression.
///
/// - `Replaced(Event)`: there was an [Event] in the log with the same key expression but an older
///   [Timestamp].
///
/// - `NotInsertedAsOlder`: there was an [Event] in the log with the same key expression but a more
///   recent [Timestamp].
///
/// - `NotInsertedAsOutOfBound`: the provided [Timestamp] is too far away in the future (compared to
///   the clock of this Zenoh node) and cannot be inserted in the log.
///
/// [Log]: LogLatest
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventInsertion {
    New(Event),
    ReplacedOlder(Event),
    NotInsertedAsOlder,
}

/// The `LogLatest` keeps track of the last publication that happened on a key expression.
///
/// By definition, the `LogLatest` is only compatible with storage that have the capability
/// `History::Latest` (the default value). For instance, that means that it will *not* work for
/// time-series storage that keep track of all the publications that happen for a given key
/// expression.
///
/// Internally, the `LogLatest` groups publications (i.e. [Event]s) according to their [Timestamp]
/// in [Interval]s and [SubInterval]s. The purpose of this grouping is to facilitate the alignment
/// and diminish the amount of data sent over the network. See the [Digest] structure for further
/// explanations.
///
/// As it only keeps track of the latest publication, whenever a new publication is received we need
/// to make sure that there is no [Event] with a newer [Timestamp] already present. The Bloom Filter
/// helps speed up this process by telling quickly if an event for that key expression is already
/// tracked or not.
///
/// [Interval]: super::classification::Interval
/// [SubInterval]: super::classification::SubInterval
pub struct LogLatest {
    pub(crate) configuration: Configuration,
    pub(crate) intervals: BTreeMap<IntervalIdx, Interval>,
    pub(crate) bloom_filter_event: Bloom<LogLatestKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LogLatestKey {
    maybe_stripped_key: Option<OwnedKeyExpr>,
    action: ActionKind,
}

impl LogLatest {
    /// Returns true if the Replication Log only contains a single Event for each key expression.
    ///
    /// To perform that check a HashSet is constructed by visiting each Interval and each
    /// SubInterval, filling the HashSet with the key expression of all the Events contained.
    ///
    /// ⚠️ This method will only be called if Zenoh is compiled in Debug mode.
    #[cfg(debug_assertions)]
    pub(crate) fn assert_only_one_event_per_key_expr(&self) -> bool {
        let mut hash_set = HashSet::new();
        for interval in self.intervals.values() {
            if !interval.assert_only_one_event_per_key_expr(&mut hash_set) {
                return false;
            }
        }

        true
    }

    /// Creates a new [LogLatest] configured with the provided [ReplicaConfig].
    pub fn new(
        storage_key_expr: OwnedKeyExpr,
        prefix: Option<OwnedKeyExpr>,
        replica_config: ReplicaConfig,
    ) -> Self {
        Self {
            configuration: Configuration::new(storage_key_expr, prefix, replica_config),
            intervals: BTreeMap::default(),
            // TODO Should these be configurable?
            //
            //      With their current values, the bloom filter structure will consume ~5MB. Note
            //      that this applies for each Storage that has replication enabled (hence, if a
            //      node has two Storage that have replication enabled, ~10MB of memory will be
            //      consumed on that node).
            //
            // 2 << 22 = 4_194_304 items.
            bloom_filter_event: Bloom::new_for_fp_rate(2 << 22, 0.01),
        }
    }

    /// Returns the [Configuration] associated with the [LogLatest].
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    /// Returns the [Event] from the Replication Log with a newer Timestamp and the same key
    /// expression, or None if no such Event exists in the Replication Log.
    ///
    /// To speed up this lookup, only the intervals that contain Events with a more recent Timestamp
    /// are visited.
    ///
    /// # ⚠️ Caveat
    ///
    /// It is not because this method returns `None` that the Replication Log does not contain any
    /// Event with the same key expression. It could return None and *still contain* an Event with
    /// the same key expression.
    pub fn lookup_newer(&self, event_to_lookup: &EventMetadata) -> Option<&Event> {
        if !self.bloom_filter_event.check(&event_to_lookup.log_key()) {
            return None;
        }

        let Ok((event_interval_idx, event_sub_interval_idx)) = self
            .configuration
            .get_time_classification(&event_to_lookup.timestamp)
        else {
            tracing::error!(
                "Fatal error: failed to compute the time classification of < {event_to_lookup:?} >"
            );
            return None;
        };

        for (interval_idx, interval) in self
            .intervals
            .iter()
            .filter(|(&idx, _)| idx >= event_interval_idx)
        {
            for (_, sub_interval) in interval.sub_intervals().filter(|(&sub_idx, _)| {
                // All sub-intervals must be visited except in one Interval: the Interval in which
                // the `event_to_lookup` belongs. In this specific Interval, only the SubIntervals
                // with a greater index should be visited (lower index means lower timestamp).
                if *interval_idx == event_interval_idx {
                    return sub_idx >= event_sub_interval_idx;
                }

                true
            }) {
                match sub_interval.lookup(event_to_lookup) {
                    EventLookup::NotFound => continue,
                    EventLookup::NewerOrIdentical(event) => return Some(event),
                    EventLookup::Older => return None,
                }
            }
        }

        None
    }

    /// Remove the [Event] with the provided `key_expr` and `timestamp` from the Replication Log.
    ///
    /// If no [Event] was found, `None` is returned.
    pub(crate) fn remove_event(&mut self, event_to_remove: &EventMetadata) -> Option<Event> {
        let Ok((interval_idx, sub_interval_idx)) = self
            .configuration
            .get_time_classification(&event_to_remove.timestamp)
        else {
            return None;
        };

        self.intervals
            .get_mut(&interval_idx)
            .and_then(|interval| interval.remove_event(&sub_interval_idx, event_to_remove))
    }

    /// Attempts to insert the provided [Event] in the replication log and return the [Insertion]
    /// outcome.
    ///
    /// This method will first go through the Replication Log to determine if the provided [Event]
    /// is indeed newer. If not then this method will do nothing.
    ///
    /// # Caveat: out of bound
    ///
    /// This method will record an error in the Zenoh log if the timestamp associated with the
    /// [Event] is so far in the future that the index of its interval is higher than
    /// [u64::MAX]. This should not happen unless a specially crafted [Event] is sent to this node
    /// or if the internal clock of the host that produced it is (very) far in the future.
    pub(crate) fn insert_event(&mut self, event: Event) -> EventInsertion {
        let event_insertion = match self.remove_older(&(&event).into()) {
            EventRemoval::RemovedOlder(old_event) => EventInsertion::ReplacedOlder(old_event),
            EventRemoval::KeptNewer => return EventInsertion::NotInsertedAsOlder,
            EventRemoval::NotFound => EventInsertion::New(event.clone()),
        };

        self.insert_event_unchecked(event);

        event_insertion
    }

    /// Inserts the provided [Event] in the replication log *without checking if there is another
    /// [Event] with the same key expression*.
    ///
    /// ⚠️ This method is meant to be used *after having called [remove_older] and processed its
    ///    result*, ensuring that the provided event is indeed more recent and the only one for
    ///    that key expression.
    ///
    /// This method will first go through the Replication Log to determine if the provided [Event]
    /// is indeed newer. If not then this method will do nothing.
    ///
    /// # Caveat: out of bound
    ///
    /// This method will record an error in the Zenoh log if the timestamp associated with the
    /// [Event] is so far in the future that the index of its interval is higher than
    /// [u64::MAX]. This should not happen unless a specially crafted [Event] is sent to this node
    /// or if the internal clock of the host that produced it is (very) far in the future.
    pub(crate) fn insert_event_unchecked(&mut self, event: Event) {
        let Ok((interval_idx, sub_interval_idx)) = self
            .configuration
            .get_time_classification(event.timestamp())
        else {
            tracing::error!(
                "Fatal error: timestamp of Event < {:?} > is out of bounds: {}",
                event.stripped_key,
                event.timestamp
            );
            return;
        };

        tracing::trace!("Inserting < {:?} > in Replication Log", event);

        self.bloom_filter_event.set(&event.log_key());

        self.intervals
            .entry(interval_idx)
            .or_default()
            .insert_unchecked(sub_interval_idx, event);

        #[cfg(debug_assertions)]
        assert!(self.assert_only_one_event_per_key_expr());
    }

    /// Removes, if there is one, the previous event from the Replication Log for the provided key
    /// expression *if its associated [Timestamp] is older* than the provided `timestamp`.
    ///
    /// In addition, if an event is indeed found, the index of the Interval and SubInterval in which
    /// it was found are returned. This allows for quick reinsertion if needed.
    pub fn remove_older(&mut self, event_to_remove: &EventMetadata) -> EventRemoval {
        // A Bloom filter never returns false negative. Hence if the call to `check_and_set` we
        // can be sure (provided that we update correctly the Bloom filter) that there is no
        // Event with that key expression.
        if self.bloom_filter_event.check(&event_to_remove.log_key()) {
            // The Bloom filter indicates that there is an Event with the same key expression,
            // we need to check if it is older or not than the one we are processing.
            //
            // By construction of the LogLatest, there can only be a single [Event] with the
            // same key expression, hence the moment we find it we can skip the search.
            //
            // NOTE: `rev()`
            //       We are making here the following assumption: it is more likely that a recent
            //       key will be updated. Iterating over a `BTreeMap` will yield its elements in
            //       increasing order --- in our particular case that means from oldest to
            //       newest. Using `rev()` yields them from newest to oldest.
            for interval in self.intervals.values_mut().rev() {
                let removal = interval.remove_older(event_to_remove);
                if !matches!(removal, EventRemoval::NotFound) {
                    return removal;
                }
            }
        }

        EventRemoval::NotFound
    }

    /// Updates the Replication Log with the provided set of [Event]s.
    pub fn update(&mut self, events: impl Iterator<Item = Event>) {
        events.for_each(|event| {
            self.insert_event(event);
        });
    }

    /// Retrieves the latest [Digest], assuming that the hot era starts at the last elapsed
    /// interval.
    ///
    /// # Errors
    ///
    /// This method will return an error if the index of the last elapsed interval is superior to
    /// [u64::MAX]. In theory, this should not happen but if it does, **it is an error that cannot
    /// be recovered from (⚠️)**.
    pub fn digest(&self) -> ZResult<Digest> {
        let last_elapsed_interval = self.configuration.last_elapsed_interval()?;

        Ok(self.digest_from(last_elapsed_interval))
    }

    /// Considering the upper bound of the hot era, generates a [Digest] of the [LogLatest].
    ///
    /// Passing the upper bound of the hot era allows generating a [Digest] that can be compared
    /// with a possibly older [Digest].
    //
    // NOTE: One of the advantages of having that method take an upper bound is to facilitate unit
    //       testing.
    fn digest_from(&self, hot_era_upper_bound: IntervalIdx) -> Digest {
        let hot_era_lower_bound = self.configuration.hot_era_lower_bound(hot_era_upper_bound);
        let warm_era_lower_bound = self.configuration.warm_era_lower_bound(hot_era_upper_bound);

        let mut warm_era_fingerprints = HashMap::default();
        let mut hot_era_fingerprints = HashMap::default();
        let mut cold_era_fingerprint = Fingerprint::default();
        for (interval_idx, interval) in self
            .intervals
            .iter()
            .filter(|(&idx, _)| idx <= hot_era_upper_bound)
        {
            // NOTE: As the intervals are traversed in increasing order (because of the use of a
            //       [BTreeMap]) and as the cold era contains the most and older intervals
            //       (i.e. with a lower interval index), the order of the comparisons should
            //       minimise their number to generate the Digest.
            if *interval_idx < warm_era_lower_bound {
                cold_era_fingerprint ^= interval.fingerprint();
            } else if *interval_idx < hot_era_lower_bound {
                if interval.fingerprint() != Fingerprint::default() {
                    warm_era_fingerprints.insert(*interval_idx, interval.fingerprint());
                }
            } else {
                hot_era_fingerprints.insert(*interval_idx, interval.sub_intervals_fingerprints());
            }
        }

        Digest {
            configuration_fingerprint: self.configuration.fingerprint(),
            cold_era_fingerprint,
            warm_era_fingerprints,
            hot_era_fingerprints,
        }
    }

    /// Removes and returns the [Event]s overridden by the provided Wildcard Update from the
    /// Replication Log.
    ///
    /// The affected `Interval` and `SubInterval` will have their [Fingerprint] updated accordingly.
    ///
    /// # Error
    ///
    /// This method will return an error if the call to obtain the time classification of the
    /// Wildcard Update failed. This should only happen if the Timestamp is far in the future or if
    /// the internal clock of the host system is misconfigured.
    pub(crate) fn remove_events_overridden_by_wildcard_update(
        &mut self,
        wildcard_key_expr: &OwnedKeyExpr,
        wildcard_timestamp: &Timestamp,
        wildcard_kind: SampleKind,
    ) -> ZResult<HashSet<Event>> {
        let mut overridden_events = HashSet::new();

        for interval in self.intervals.values_mut() {
            overridden_events.extend(interval.remove_events_overridden_by_wildcard_update(
                self.configuration.prefix(),
                wildcard_key_expr,
                wildcard_timestamp,
                wildcard_kind,
            ));
        }

        Ok(overridden_events)
    }
}

#[cfg(test)]
#[path = "tests/log.test.rs"]
mod tests;
