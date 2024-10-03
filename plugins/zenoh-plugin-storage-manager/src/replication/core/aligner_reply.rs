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
use zenoh::{
    key_expr::{format::keformat, OwnedKeyExpr},
    sample::{Sample, SampleKind},
    session::ZenohId,
};
use zenoh_backend_traits::StorageInsertionResult;

use crate::replication::{
    classification::{EventRemoval, IntervalIdx, SubIntervalIdx},
    core::{aligner_key_expr_formatter, aligner_query::AlignmentQuery, Replication},
    digest::Fingerprint,
    log::EventMetadata,
};

/// The `AlignmentReply` enumeration contains the possible information needed by a Replica to align
/// its storage.
///
/// The are sent in the following order:
///
///   Intervals -> SubIntervals -> Events -> Retrieval
///
/// Not all replies are made, it depends on the Era when a misalignment was detected.
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
    /// - Intervals: the Replica sent a list of [IntervalIdx] and their associated [Fingerprint].
    ///   This Storage needs to compare these [Fingerprint] with its local state and, for each that
    ///   differ, request the [Fingerprint] of their [SubInterval].
    ///
    ///   This only happens as a response to a misalignment detected in the Cold Era.
    ///
    ///
    /// - SubIntervals: the Replica sent a list of [IntervalIdx], their associated [SubIntervalIdx]
    ///   and the [Fingerprint] of these [SubInterval].
    ///   This Storage again needs to compare these [Fingerprint] with its local state and, for each
    ///   that differ, request all the [EventMetadata] the [SubInterval] contains.
    ///
    ///   This would happen as a response to a misalignment detected in the Warm Era or as a
    ///   follow-up step from a misalignment in the Cold Era.
    ///
    ///
    /// - Events: the Replica sent a list of [EventMetadata].
    ///   This Storage needs to check, for each of them, if it has a newer [Event] stored. If not,
    ///   it needs to ask to retrieve the associated data from the Replica.
    ///   If the [EventMetadata] is indeed more recent and its associated action is `Delete` then
    ///   the data will be directly deleted from the Storage without requiring an extra exchange.
    ///
    ///   This would happen as a response to a misalignment detected in the Hot Era or as a
    ///   follow-up step from a misalignment in the Cold / Warm Eras.
    ///
    ///
    /// - Retrieval: the Replica sent an [Event] and its associated payload.
    ///   This Storage needs to check if it is still more recent and, if so, add it.
    ///
    ///   Note that only one [Event] is sent per reply but multiple replies are sent to the same
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
                    if let Some(missing_event_metadata) =
                        self.process_event_metadata(replica_event).await
                    {
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

    /// Processes the [EventMetadata] sent by the Replica we are aligning with, determining if we
    /// need to retrieve the associated payload or not.
    ///
    /// If we need to retrieve the payload then this [EventMetadata] is returned. If we don't,
    /// `None` is returned.
    ///
    /// Furthermore, depending on the `action` of the event, different operations are performed:
    ///
    /// - If it is a [Put] then we only need to check if in the Cache or the Replication Log we
    ///   have an event with the same or a more recent timestamp.
    ///   If we do then we are either already up to date with that event or the Replica is
    ///   "lagging". In both cases, we don't need to retrieve the associated payload and return
    ///   `None`.
    ///   If we don't, then we need to retrieve the payload and return the [EventMetadata].
    ///
    /// - If it is a [Delete] then if we don't have a more recent event in our Cache or Replication
    ///   Log then we have all the information needed to perform the delete and, thus, perform it.
    async fn process_event_metadata(&self, replica_event: EventMetadata) -> Option<EventMetadata> {
        if self
            .latest_updates
            .read()
            .await
            .get(&replica_event.stripped_key)
            .is_some_and(|latest_event| latest_event.timestamp >= replica_event.timestamp)
        {
            return None;
        }

        match &replica_event.action {
            SampleKind::Put => {
                let replication_log_guard = self.replication_log.read().await;
                if let Some(latest_event) =
                    replication_log_guard.lookup(&replica_event.stripped_key)
                {
                    if latest_event.timestamp >= replica_event.timestamp {
                        return None;
                    }
                }
                return Some(replica_event);
            }
            SampleKind::Delete => {
                let mut replication_log_guard = self.replication_log.write().await;
                match replication_log_guard
                    .remove_older(&replica_event.stripped_key, &replica_event.timestamp)
                {
                    EventRemoval::NotFound => {}
                    EventRemoval::KeptNewer => return None,
                    EventRemoval::RemovedOlder(older_event) => {
                        if older_event.action == SampleKind::Put {
                            // NOTE: In some of our backend implementation, a deletion on a
                            //       non-existing key will return an error. Given that we cannot
                            //       distinguish an error from a missing key, we will assume
                            //       the latter and move forward.
                            //
                            // FIXME: Once the behaviour described above is fixed, check for
                            //        errors.
                            let _ = self
                                .storage
                                .lock()
                                .await
                                .delete(replica_event.stripped_key.clone(), replica_event.timestamp)
                                .await;
                        }
                    }
                }

                replication_log_guard.insert_event_unchecked(replica_event.clone().into());
            }
        }

        Some(replica_event)
    }

    /// Processes the [EventMetadata] and [Sample] sent by the Replica, adding it to our Storage if
    /// needed.
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
        tracing::trace!("Processing `AlignmentReply::Retrieval`");

        if self
            .latest_updates
            .read()
            .await
            .get(&replica_event.stripped_key)
            .is_some_and(|latest_event| latest_event.timestamp >= replica_event.timestamp)
        {
            return;
        }

        let mut replication_log_guard = self.replication_log.write().await;
        match replication_log_guard
            .remove_older(&replica_event.stripped_key, &replica_event.timestamp)
        {
            EventRemoval::KeptNewer => return,
            EventRemoval::RemovedOlder(_) | EventRemoval::NotFound => {}
        }

        // NOTE: This code can only be called with `action` set to `delete` on an initial
        // alignment, in which case the Storage of the receiving Replica is empty => there
        // is no need to actually call `storage.delete`.
        //
        // Outside of an initial alignment, the `delete` action will be performed at the
        // step above, in `AlignmentReply::EventsMetadata`.
        if replica_event.action == SampleKind::Put
            && matches!(
                self.storage
                    .lock()
                    .await
                    .put(
                        replica_event.stripped_key.clone(),
                        sample.into(),
                        replica_event.timestamp,
                    )
                    .await,
                Ok(StorageInsertionResult::Outdated) | Err(_)
            )
        {
            return;
        }

        replication_log_guard.insert_event_unchecked(replica_event.into());
    }
}
