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

use serde::{Deserialize, Serialize};
use tokio::sync::RwLockWriteGuard;
use zenoh::{
    bytes::{Encoding, ZBytes},
    key_expr::{format::keformat, keyexpr_tree::IKeyExprTreeMut, OwnedKeyExpr},
    sample::{Sample, SampleFields, SampleKind},
    session::ZenohId,
    Result as ZResult,
};
use zenoh_backend_traits::{StorageInsertionResult, StoredData};

use crate::{
    replication::{
        classification::{EventRemoval, IntervalIdx, SubIntervalIdx},
        core::{aligner_key_expr_formatter, aligner_query::AlignmentQuery, Replication},
        digest::Fingerprint,
        log::{Action, EventMetadata},
        Event, LogLatest,
    },
    storages_mgt::service::Update,
};

/// The `AlignmentReply` enumeration contains the possible information needed by a Replica to align
/// its storage.
///
/// The are sent in the following order:
///
///   Intervals -> SubIntervals -> Events -> Retrieval
///
/// Not all replies are made, it depends on the Era where a misalignment was detected.
///
/// The `Discovery` Reply is used to perform the initial alignment. The Replica sends its Zenoh ID
/// such that the newly joined Replica can retrieve all the content without having to go through
/// an exchange of Digest.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum AlignmentReply {
    Discovery(ZenohId),
    Intervals(HashMap<IntervalIdx, Fingerprint>),
    SubIntervals(HashMap<IntervalIdx, HashMap<SubIntervalIdx, Fingerprint>>),
    EventsMetadata(Vec<EventMetadata>),
    Retrieval(EventMetadata),
}

impl Replication {
    /// Processes the [AlignmentReply] sent by the Replica that has potentially data this Storage is
    /// missing.
    ///
    /// This method is a big "match" statement, processing each variant of the [AlignmentReply] in
    /// the following manner:
    ///
    /// - Intervals: the remote Replica sent a list of [IntervalIdx] and their associated
    ///   [Fingerprint]. This Replica needs to compare these [Fingerprint] with its local state
    ///   and, for each that differ, request the [Fingerprint] of their [SubInterval].
    ///
    ///   This only happens as a response to a misalignment detected in the Cold Era.
    ///
    ///
    /// - SubIntervals: the remote Replica sent a list of [IntervalIdx], their associated
    ///   [SubIntervalIdx] and the [Fingerprint] of these [SubInterval].  This Replica again needs
    ///   to compare these [Fingerprint] with its local state and, for each that differ, request all
    ///   the [EventMetadata] the [SubInterval] contains.
    ///
    ///   This would happen as a response to a misalignment detected in the Warm Era or as a
    ///   follow-up step from a misalignment in the Cold Era.
    ///
    ///
    /// - Events: the remote Replica sent a list of [EventMetadata].
    ///   This Replica needs to check, for each of them, if it has a newer [Event] stored. If not,
    ///   it needs to ask to retrieve the associated data from the Replica.
    ///   If the [EventMetadata] is indeed more recent and its associated action is `Delete` then
    ///   the data will be directly deleted from the Storage without requiring an extra exchange.
    ///
    ///   This would happen as a response to a misalignment detected in the Hot Era or as a
    ///   follow-up step from a misalignment in the Cold / Warm Eras.
    ///
    ///
    /// - Retrieval: the remote Replica sent an [Event] and its associated payload.
    ///   This Replica needs to check if it is still more recent and, if so, add it.
    ///
    ///   ⚠️ Note that only one [Event] is sent per reply but multiple replies are sent to the same
    ///   Query (by setting `Consolidation::None`).
    #[tracing::instrument(skip_all, fields(storage = self.storage_key_expr.as_str(), replica = replica_aligner_ke.as_str(), sample, t))]
    pub(crate) async fn process_alignment_reply(
        &self,
        replica_aligner_ke: OwnedKeyExpr,
        alignment_reply: AlignmentReply,
        sample: Sample,
    ) {
        match alignment_reply {
            AlignmentReply::Discovery(replica_zid) => {
                let parsed_ke = match aligner_key_expr_formatter::parse(&replica_aligner_ke) {
                    Ok(ke) => ke,
                    Err(e) => {
                        tracing::error!(
                            "Failed to parse < {replica_aligner_ke} > as a valid Aligner key \
                             expression: {e:?}"
                        );
                        return;
                    }
                };

                let replica_aligner_ke = match keformat!(
                    aligner_key_expr_formatter::formatter(),
                    hash_configuration = parsed_ke.hash_configuration(),
                    zid = replica_zid,
                ) {
                    Ok(ke) => ke,
                    Err(e) => {
                        tracing::error!("Failed to generate a valid Aligner key expression: {e:?}");
                        return;
                    }
                };

                tracing::debug!("Performing initial alignment with Replica < {replica_zid} >");

                if let Err(e) = self
                    .spawn_query_replica_aligner(replica_aligner_ke, AlignmentQuery::All)
                    .await
                {
                    tracing::error!("Error returned while performing the initial alignment: {e:?}");
                }
            }
            AlignmentReply::Intervals(replica_intervals) => {
                tracing::trace!("Processing `AlignmentReply::Intervals`");
                let intervals_diff = {
                    let replication_log_guard = self.replication_log.read().await;
                    replica_intervals
                        .into_iter()
                        .filter(|(idx, fp)| match replication_log_guard.intervals.get(idx) {
                            Some(interval) => interval.fingerprint() != *fp,
                            None => true,
                        })
                        .map(|(idx, _)| idx)
                        .collect::<HashSet<_>>()
                };

                if !intervals_diff.is_empty() {
                    self.spawn_query_replica_aligner(
                        replica_aligner_ke,
                        AlignmentQuery::Intervals(intervals_diff),
                    );
                }
            }
            AlignmentReply::SubIntervals(replica_sub_intervals) => {
                tracing::trace!("Processing `AlignmentReply::SubIntervals`");
                let sub_intervals_diff = {
                    let mut sub_ivl_diff = HashMap::default();
                    let replication_log_guard = self.replication_log.read().await;
                    for (interval_idx, replica_sub_ivl) in replica_sub_intervals {
                        match replication_log_guard.intervals.get(&interval_idx) {
                            None => {
                                sub_ivl_diff.insert(
                                    interval_idx,
                                    replica_sub_ivl.into_keys().collect::<HashSet<_>>(),
                                );
                            }
                            Some(interval) => {
                                let diff = replica_sub_ivl
                                    .into_iter()
                                    .filter(|(sub_idx, sub_fp)| {
                                        match interval.sub_interval_at(sub_idx) {
                                            None => true,
                                            Some(sub_interval) => {
                                                sub_interval.fingerprint() != *sub_fp
                                            }
                                        }
                                    })
                                    .map(|(sub_idx, _)| sub_idx)
                                    .collect();
                                sub_ivl_diff.insert(interval_idx, diff);
                            }
                        }
                    }

                    sub_ivl_diff
                };

                if !sub_intervals_diff.is_empty() {
                    self.spawn_query_replica_aligner(
                        replica_aligner_ke,
                        AlignmentQuery::SubIntervals(sub_intervals_diff),
                    );
                }
            }
            AlignmentReply::EventsMetadata(replica_events) => {
                tracing::trace!("Processing `AlignmentReply::EventsMetadata`");
                let mut diff_events = Vec::default();

                for replica_event in replica_events {
                    tracing::trace!("Checking if < {replica_event:?} > is missing");
                    if let Some(missing_event_metadata) =
                        self.process_event_metadata(replica_event).await
                    {
                        tracing::trace!(
                            "Requesting: < {:?} >",
                            missing_event_metadata.stripped_key
                        );
                        diff_events.push(missing_event_metadata);
                    }
                }

                if !diff_events.is_empty() {
                    self.spawn_query_replica_aligner(
                        replica_aligner_ke,
                        AlignmentQuery::Events(diff_events),
                    );
                }
            }
            AlignmentReply::Retrieval(replica_event) => {
                self.process_event_retrieval(replica_event, sample).await;
            }
        }
    }

    /// Processes the [EventMetadata] sent by the remote Replica we are aligning with, determining
    /// if we need to retrieve the associated payload or not.
    ///
    /// If we need to retrieve the payload then this [EventMetadata] is returned. If we don't,
    /// `None` is returned.
    ///
    /// Depending on the `action` of the event, different operations are performed. For instance, if
    /// it is a `Put` we need to check if a Wildcard Update overwrites it in which case we don't
    /// need to retrieve the associated payload.
    ///
    /// See the [needs_further_processing] function for more information on the specific cases we
    /// need to be aware of.
    async fn process_event_metadata(&self, replica_event: EventMetadata) -> Option<EventMetadata> {
        if self
            .latest_updates
            .read()
            .await
            .get(&replica_event.log_key())
            .is_some_and(|latest_event| latest_event.timestamp >= replica_event.timestamp)
        {
            return None;
        }

        let mut replication_log_guard = self.replication_log.write().await;
        if !self
            .needs_further_processing(&mut replication_log_guard, &replica_event)
            .await
        {
            return None;
        }

        match &replica_event.action {
            // To apply a Put or a WildcardPut we need the payload so we return here to indicate
            // that we need to retrieve it from the remote Replica.
            Action::Put | Action::WildcardPut(_) => return Some(replica_event),

            // A Delete can be applied right away, we have all the information we need.
            Action::Delete => {
                match replication_log_guard.remove_older(&replica_event) {
                    EventRemoval::NotFound => {}
                    EventRemoval::KeptNewer => return None,
                    EventRemoval::RemovedOlder(older_event) => {
                        if older_event.action == Action::Put {
                            // NOTE: In some of our backend implementation, a deletion on a
                            //       non-existing key will return an error. Given that we cannot
                            //       distinguish an error from a missing key, we will assume
                            //       the latter and move forward.
                            //
                            // FIXME: Once the behaviour described above is fixed, check for
                            //        errors.
                            let _ = self
                                .storage_service
                                .storage
                                .lock()
                                .await
                                .delete(replica_event.stripped_key.clone(), replica_event.timestamp)
                                .await;
                        }
                    }
                }
            }

            Action::WildcardDelete(_) => {
                self.apply_wildcard_update(
                    &mut replication_log_guard,
                    &replica_event,
                    ZBytes::default(),
                    Encoding::default(),
                )
                .await;
            }
        }

        replication_log_guard.insert_event_unchecked(replica_event.clone().into());
        None
    }

    /// Processes the [EventMetadata] and [Sample] sent by the remote Replica, adding it to our
    /// Storage if needed.
    ///
    /// # Special case: initial alignment
    ///
    /// Outside of the initial alignment, an [EventMetadata] with an action set to `Delete` will be
    /// processed during the previous step, i.e. in the `AlignmentReply::EventsMetadata` as, as
    /// explained there, we already have at that stage all the required information to perform the
    /// deletion.
    ///
    /// That fact is true except for the initial alignment: the initial alignment bypasses all these
    /// steps and the Replica goes straight to sending all its Replication Log and data in its
    /// Storage. Including for the deleted events.
    async fn process_event_retrieval(&self, replica_event: EventMetadata, sample: Sample) {
        tracing::trace!("Processing `AlignmentReply::Retrieval` for < {replica_event:?} >");

        if self
            .latest_updates
            .read()
            .await
            .get(&replica_event.log_key())
            .is_some_and(|latest_event| latest_event.timestamp() >= replica_event.timestamp())
        {
            return;
        }

        let mut replication_log_guard = self.replication_log.write().await;

        if !self
            .needs_further_processing(&mut replication_log_guard, &replica_event)
            .await
        {
            return;
        }

        // The Event is newer than what we have and is not overridden by a Wildcard Update, we
        // need to process it.
        replication_log_guard.remove_older(&replica_event);

        match &replica_event.action {
            // NOTE: This code can only be called with `action` set to `Delete` or `WildcardDelete`
            // on an initial alignment, in which case the Storage of the receiving Replica is empty
            // => there is no need to actually call `storage.delete`.
            //
            // Outside of an initial alignment, the `Delete` or `WildcardDelete` actions will be
            // performed at the step above, in `AlignmentReply::EventsMetadata`.
            Action::Delete => {}
            Action::WildcardDelete(wildcard_delete_ke) => {
                self.storage_service
                    .register_wildcard_update(
                        wildcard_delete_ke.clone(),
                        SampleKind::Delete,
                        replica_event.timestamp,
                        ZBytes::default(),
                        Encoding::default(),
                    )
                    .await;
            }
            Action::Put => {
                let SampleFields {
                    payload, encoding, ..
                } = sample.into();
                if matches!(
                    self.storage_service
                        .storage
                        .lock()
                        .await
                        .put(
                            replica_event.stripped_key.clone(),
                            payload,
                            encoding,
                            replica_event.timestamp,
                        )
                        .await,
                    Ok(StorageInsertionResult::Outdated) | Err(_)
                ) {
                    // NOTE: This is not necessarily an error: as we are not locking the
                    // `latest_updates` more events on the same key expression can be received
                    // before we have time to store this one.
                    //
                    // In that scenario the Storage should either return an error or `Outdated`.
                    return;
                }
            }
            Action::WildcardPut(_) => {
                let SampleFields {
                    payload, encoding, ..
                } = sample.into();
                self.apply_wildcard_update(
                    &mut replication_log_guard,
                    &replica_event,
                    payload,
                    encoding,
                )
                .await;
            }
        }

        // NOTE: We can only safely call `insert_event_unchecked` because we called earlier
        // `replication_log_guard.remove_older`.
        replication_log_guard.insert_event_unchecked(replica_event.into());
    }

    /// Returns `true` if the provided `replica_event` requires more processing.
    ///
    /// This method will:
    /// - check if there is a Wildcard Update that overrides it and / or a more recent Event in the
    ///   Replication Log,
    /// - if there are both, we need to perform additional checks to assess which needs to be
    ///   applied,
    /// - if there is *only* a Wildcard Update that overrides it, apply the Wildcard Update and
    ///   return `false` as a Wildcard Update is self-contained,
    /// - if there is *only* a more recent Event, return `false` as this Replica is up-to-date,
    /// - if there is neither, return `true` as depending on where this method was called, different
    ///   operation(s) need to be performed.
    async fn needs_further_processing(
        &self,
        replication_log_guard: &mut RwLockWriteGuard<'_, LogLatest>,
        replica_event: &EventMetadata,
    ) -> bool {
        // We received an EventMetadata, we need to check if we don't have:
        // 1. a Wildcard Update that overrides it,
        // 2. a more recent Event on that same key expression already in the Replication Log.
        let Ok(maybe_wildcard_update) = self.is_overridden_by_wildcard_update(replica_event).await
        else {
            tracing::error!(
                "Internal error attempting to prefix < {:?} > with < {:?} >",
                replica_event.stripped_key,
                self.storage_service.configuration.strip_prefix
            );
            return false;
        };

        let maybe_newer_event = replication_log_guard.lookup_newer(replica_event);

        match (maybe_wildcard_update, maybe_newer_event) {
            // No Event in the Replication Log or Wildcard Update, we return true as we need to
            // process it.
            (None, None) => {}
            // The timestamp of the Event in the Replication Log for the same key expression has
            // a greater Timestamp: we discard this EventMetadata.
            (None, Some(_)) => return false,
            // We have a Wildcard Update that overrides the Event -> we override it and we're done.
            (Some((wildcard_ke, wildcard_update)), None) => {
                self.store_event_overridden_by_wildcard_update(
                    replication_log_guard,
                    replica_event.clone(),
                    wildcard_ke,
                    wildcard_update,
                )
                .await;
                return false;
            }
            // The Wildcard Update and the Event in the Replication Log have the same Timestamp:
            // that means that what we have in the Replication Log is the application of the
            // Wildcard Update.
            //
            // We thus need to check one very specific case: if the `replica_event` we received is a
            // Delete that came *before* a Wildcard Update Put and *after* the last non-wildcard
            // update.
            //
            // With an example:
            //
            // We have:
            // - Put "a = 1" @t0
            // - Wildcard Update "** = 42" @t2 (hence we have in the Log "a = 42" @t2)
            //
            // We receive:
            // - Delete "a" @t1 (@t0 < @t1 < @t2).
            //
            // Despite having @t2 > @t1, this earlier Event should actually lead to the deletion of
            // "a" because a Wildcard Update should not resuscitate a deleted Event.
            //
            // NOTE: Wildcard Delete needs separate processing, done in the `apply_wildcard_update`
            // method.
            (Some((_, wildcard_update)), Some(log_event))
                if wildcard_update.timestamp() == log_event.timestamp()
                    && replica_event.action == Action::Delete
                    && log_event.timestamp_last_non_wildcard_update.is_some_and(
                        |timestamp_non_wildcard| replica_event.timestamp > timestamp_non_wildcard,
                    ) =>
            {
                if log_event.action == Action::Put {
                    let _ = self
                        .storage_service
                        .storage
                        .lock()
                        .await
                        .delete(replica_event.stripped_key.clone(), *log_event.timestamp())
                        .await;
                }

                let log_event_metadata = log_event.into();
                // NOTE: Despite the `if let Some`, this call can only return `Some`. We entered
                //       this code path because we found that particular Event in the Replication
                //       Log.
                if let Some(mut removed_event) =
                    replication_log_guard.remove_event(&log_event_metadata)
                {
                    removed_event.set_timestamp_and_action(
                        replica_event.timestamp,
                        replica_event.action.clone(),
                    );

                    replication_log_guard.insert_event_unchecked(removed_event);
                    return false;
                }
            }
            // The timestamp of the Wildcard Update is greater than that of the Event: either the
            // Event is a deletion or we have a bug with a Wildcard Update that did not override
            // all the events it should have had.
            //
            // The rationale is the following: a Wildcard Update (be it `Action::WildcardPut` or
            // `Action::WildcardDelete`) should override all `Action::Put`. When so doing, it will
            // also update the timestamp of the overridden event, putting its own. We would thus
            // have `wildcard_update.timestamp() == log_event.timestamp()`.
            //
            // Only an event with an `Action::Delete` will not be overridden. It is thus the only
            // possibility to have both a newer event and a newer Wildcard Update. In that scenario,
            // there is nothing to do: the remote Replica is the one that is not up to date.
            (Some((_, wildcard_update)), Some(log_event))
                if wildcard_update.timestamp() > log_event.timestamp() =>
            {
                debug_assert!(matches!(log_event.action, Action::Delete));
                return false;
            }
            // The timestamp of the Event in the Replication Log is greater or equal to the Wildcard
            // Update: we have nothing to do.
            (Some((_, wildcard_update)), Some(log_event))
                if log_event.timestamp() >= wildcard_update.timestamp() =>
            {
                return false;
            }
            (_, _) => unreachable!(),
        }

        true
    }

    /// Applies the Wildcard Update provided in the `replica_event`, unless there is a more recent
    /// one already recorded in the Replication Log.
    ///
    /// ⚠️ This method does NOT insert the Wildcard Update in the Replication Log. It still needs
    ///    to be added afterwards. This is why we only take a mutable reference on the Write lock.
    ///    (We do not perform this operation to avoid code duplication in methods calling this one.)
    ///
    /// Applying a Wildcard Update involves several steps:
    ///
    /// - We need to override any Event present in the latest cache and in the Replication Log. That
    ///   means we need to:
    ///   (i)   remove them,
    ///   (ii)  if needed, override or delete the data associated in the Storage,
    ///   (iii) update their metadata (putting the timestamp and action of the Wildcard Update),
    ///   (iv)  re-insert them.
    ///
    /// - We need to register the Wildcard Update such that the Storage is aware of it and can
    ///   apply it on late-comers.
    async fn apply_wildcard_update(
        &self,
        replication_log_guard: &mut RwLockWriteGuard<'_, LogLatest>,
        replica_event: &EventMetadata,
        payload: ZBytes,
        encoding: Encoding,
    ) {
        let (wildcard_ke, wildcard_kind) = match &replica_event.action {
            Action::Put | Action::Delete => unreachable!(),
            Action::WildcardPut(wildcard_ke) => (wildcard_ke, SampleKind::Put),
            Action::WildcardDelete(wildcard_ke) => (wildcard_ke, SampleKind::Delete),
        };

        if matches!(
            replication_log_guard.remove_older(replica_event),
            EventRemoval::KeptNewer
        ) {
            return;
        }

        // We lock the Cache until we are done processing this Wildcard Update to not let pass Event
        // that should be overridden: the Wildcard Update will only be applied once on older Events
        // and that time is now.
        let mut latest_updates_guard = self.latest_updates.write().await;
        let mut overridden_events =
            crate::replication::core::remove_events_overridden_by_wildcard_update(
                &mut latest_updates_guard,
                self.storage_service.configuration.strip_prefix.as_ref(),
                wildcard_ke,
                replica_event.timestamp(),
                wildcard_kind,
            );

        match replication_log_guard.remove_events_overridden_by_wildcard_update(
            wildcard_ke,
            replica_event.timestamp(),
            wildcard_kind,
        ) {
            Ok(events) => overridden_events.extend(events),
            Err(e) => {
                tracing::error!("Skipping Wildcard Update < {wildcard_ke} >: {e:?}");
                return;
            }
        };

        for mut overridden_event in overridden_events {
            let overridden_event_action = overridden_event.action();
            tracing::trace!("Applying < {wildcard_ke:?} > on: < {overridden_event:?} >");
            match (overridden_event_action, wildcard_kind) {
                // The method `remove_events_overridden_by_wildcard_update` will not remove deleted
                // Events.
                (Action::Delete, _) => {
                    #[cfg(debug_assertions)]
                    unreachable!()
                }

                (Action::Put, SampleKind::Put) => {
                    if matches!(
                        self.storage_service
                            .storage
                            .lock()
                            .await
                            .put(
                                overridden_event.key_expr().clone(),
                                payload.clone(),
                                encoding.clone(),
                                replica_event.timestamp,
                            )
                            .await,
                        Ok(StorageInsertionResult::Outdated) | Err(_)
                    ) {
                        continue;
                    }
                }
                (Action::Put, SampleKind::Delete) => {
                    if matches!(
                        self.storage_service
                            .storage
                            .lock()
                            .await
                            .delete(
                                overridden_event.key_expr().clone(),
                                *overridden_event.timestamp(),
                            )
                            .await,
                        Ok(StorageInsertionResult::Outdated)
                    ) {
                        continue;
                    }
                }

                // We are overriding a Wildcard Update with another Wildcard Update, there is no
                // need to touch the Storage or to reinsert the overridden Wildcard Update, the new
                // one is enough.
                (
                    Action::WildcardPut(overridden_wildcard_ke)
                    | Action::WildcardDelete(overridden_wildcard_ke),
                    _,
                ) => {
                    // NOTE: If a Wildcard Update overrides another Wildcard Update, there is no
                    // need to reinsert the previous Wildcard Update, that would lead to extra data
                    // stored for no valid reason, since the newer Wildcard Update will always
                    // "win".
                    //
                    // Example:
                    // 1. z_put -k "test/replication/**" -v "1" @t_0
                    // 2. z_put -k "test/**" -v 42 @t_1  (t_1 > t_0)
                    //
                    // 2. overrides 1., so we remove 1. from the `wildcard_updates` structure.
                    let mut wildcard_puts_guard = self.storage_service.wildcard_puts.write().await;
                    wildcard_puts_guard.remove(overridden_wildcard_ke);
                    wildcard_puts_guard.prune();
                    continue;
                }
            }

            overridden_event
                .set_timestamp_and_action(replica_event.timestamp, replica_event.action.clone());
            // NOTE: We can safely call `insert_event_unchecked` because the call to
            // `remove_events_overridden_by_wildcard_update` already removed the overridden Events.
            replication_log_guard.insert_event_unchecked(overridden_event);
        }

        self.storage_service
            .register_wildcard_update(
                wildcard_ke.clone(),
                (&replica_event.action).into(),
                replica_event.timestamp,
                payload,
                encoding,
            )
            .await;
    }

    /// Checks if the provided `replica_event` is overridden by a Wildcard Update and, if so,
    /// returns the associated [Update].
    ///
    /// If there is no Wildcard Update that overrides it, `None` is returned.
    ///
    /// # Errors
    ///
    /// This method will return an error if the stripped key of the `replica_event` is set to `None`
    /// and this Storage was configured without any `strip_prefix`.
    ///
    /// This is a "fatal" error that cannot be recovered from.
    async fn is_overridden_by_wildcard_update(
        &self,
        replica_event: &EventMetadata,
    ) -> ZResult<Option<(OwnedKeyExpr, Update)>> {
        let full_event_key_expr = match &replica_event.action {
            Action::Put | Action::Delete => crate::prefix(
                self.storage_service.configuration.strip_prefix.as_ref(),
                replica_event.stripped_key.as_ref(),
            )?,
            Action::WildcardPut(wildcard_ke) | Action::WildcardDelete(wildcard_ke) => {
                wildcard_ke.clone()
            }
        };

        Ok(self
            .storage_service
            .overriding_wild_update(
                &full_event_key_expr,
                &replica_event.timestamp,
                &replica_event.timestamp_last_non_wildcard_update,
                &replica_event.action,
            )
            .await)
    }

    /// Stores in the Storage and/or the Replication Log the provided `replica_event` *that is being
    /// overridden by the `wildcard_update`*.
    ///
    /// The `replica_event` was sent by a Replica during the alignment process. It is not associated
    /// to any data in the Storage.
    ///
    /// A payload will be pushed to the Storage if the `wildcard_update` is a put.
    //
    // NOTE: There is no need to attempt to delete an event in the Storage if the `wildcard_update`
    //       is a delete. Indeed, if the wildcard update is a delete then it is impossible to have
    //       a previous event associated to the same key expression as it would have been deleted.
    async fn store_event_overridden_by_wildcard_update(
        &self,
        replication_log_guard: &mut RwLockWriteGuard<'_, LogLatest>,
        replica_event: EventMetadata,
        wildcard_ke: OwnedKeyExpr,
        wildcard_update: Update,
    ) {
        let kind = wildcard_update.kind();
        let StoredData {
            payload,
            encoding,
            timestamp,
        } = wildcard_update.into();

        // A Wildcard Update overrides another Wildcard Update, we have nothing to do.
        if matches!(
            replica_event.action,
            Action::WildcardPut(_) | Action::WildcardDelete(_)
        ) {
            return;
        }

        tracing::trace!(
            "Overriding < {replica_event:?} > with < {wildcard_ke} {} >",
            timestamp
        );

        // Generate the action that will be used to override the metadata of the Event. We do it
        // now to avoid having to clone the `wildcard_update` because we move it below.
        let wildcard_action = match kind {
            SampleKind::Put => Action::WildcardPut(wildcard_ke),
            SampleKind::Delete => Action::WildcardDelete(wildcard_ke),
        };

        if kind == SampleKind::Put
            && matches!(
                self.storage_service
                    .storage
                    .lock()
                    .await
                    .put(
                        replica_event.stripped_key.clone(),
                        payload,
                        encoding,
                        timestamp
                    )
                    .await,
                Ok(StorageInsertionResult::Outdated) | Err(_)
            )
        {
            tracing::error!(
                "Failed to insert Wildcard Put Update applied to < {:?} >",
                replica_event.stripped_key
            );
            return;
        }

        // First create an Event with the metadata sent by the Replica: we want to keep the
        // `timestamp_last_non_wildcard_update`.
        let mut event: Event = replica_event.into();
        // Then update the Event with the values of the Wildcard Update.
        event.set_timestamp_and_action(timestamp, wildcard_action);

        replication_log_guard.insert_event(event);
    }
}
