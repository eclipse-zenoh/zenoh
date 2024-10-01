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
use tokio::{sync::RwLockWriteGuard, task::JoinHandle};
use zenoh::{
    bytes::ZBytes,
    internal::Value,
    key_expr::{format::keformat, keyexpr_tree::IKeyExprTree, OwnedKeyExpr},
    query::{ConsolidationMode, Query, Selector},
    sample::{Sample, SampleKind},
    session::ZenohId,
};
use zenoh_backend_traits::StorageInsertionResult;

use super::{
    classification::{IntervalIdx, SubIntervalIdx},
    core::{aligner_key_expr_formatter, Replication},
    digest::{DigestDiff, Fingerprint},
    log::{Action, EventMetadata},
    LogLatest,
};
use crate::storages_mgt::service::Update;

/// The `AlignmentQuery` enumeration represents the information requested by a Replica to align
/// its storage.
///
/// Requests are made in the following order:
///
///   DigestDiff  ->  Intervals  ->  SubIntervals  ->  Events
///
/// Not all requests are made, it depends on the Era where a misalignment was detected.
///
/// For instance, if a divergence is detected in the Cold era then the `AlignmentReply` will provide
/// the Replica with the [Fingerprint] of all the "cold" [Interval]s. In turn, the Replica will
/// requests more details on the [Interval]s that differ (the `Intervals` variant).
///
/// A divergence in the Hot era, will directly let the Replica assess which [SubInterval]s it needs,
/// hence directly skipping to the `SubIntervals` variant.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum AlignmentQuery {
    Discovery,
    All,
    Diff(DigestDiff),
    Intervals(HashSet<IntervalIdx>),
    SubIntervals(HashMap<IntervalIdx, HashSet<SubIntervalIdx>>),
    Events(Vec<EventMetadata>),
}

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
    Events(Vec<EventMetadata>),
    Retrieval(EventMetadata),
}

impl Replication {
    /// Replies with the information requested by the Replica.
    ///
    /// This method will:
    /// 1. Parse the attachment of the received [Query] into an [AlignmentQuery].
    /// 2. Depending on the variant of the [AlignmentQuery], reply with the requested information.
    pub(crate) async fn aligner(&self, query: Query) {
        let attachment = match query.attachment() {
            Some(attachment) => attachment,
            None => {
                tracing::debug!("Skipping query with empty attachment");
                return;
            }
        };

        let alignment_query = match bincode::deserialize::<AlignmentQuery>(&attachment.to_bytes()) {
            Ok(alignment) => alignment,
            Err(e) => {
                tracing::error!(
                    "Failed to deserialize `attachment` of received Query into AlignmentQuery: \
                     {e:?}"
                );
                return;
            }
        };

        match alignment_query {
            AlignmentQuery::Discovery => {
                tracing::trace!("Processing `AlignmentQuery::Discovery`");
                reply_to_query(
                    &query,
                    AlignmentReply::Discovery(self.zenoh_session.zid()),
                    None,
                )
                .await;
            }
            AlignmentQuery::All => {
                tracing::trace!("Processing `AlignmentQuery::All`");

                let idx_intervals = self
                    .replication_log
                    .read()
                    .await
                    .intervals
                    .keys()
                    .copied()
                    .collect::<Vec<_>>();

                for interval_idx in idx_intervals {
                    let mut events_to_send = Vec::default();
                    if let Some(interval) = self
                        .replication_log
                        .read()
                        .await
                        .intervals
                        .get(&interval_idx)
                    {
                        interval.sub_intervals().for_each(|(_, sub_interval)| {
                            events_to_send.extend(
                                sub_interval
                                    .events()
                                    .filter(|(_, event)| {
                                        // NOTE: We process the wildcard actions separately: the
                                        // payload of a WildcardPut is not stored in the Storage but
                                        // separately.
                                        !matches!(
                                            event.action,
                                            Action::WildcardDelete(_) | Action::WildcardPut(_)
                                        )
                                    })
                                    .map(|(_, event)| event.into()),
                            );
                        });
                    }

                    // NOTE: As we took the lock in the `if let` block, it is released here,
                    // diminishing contention.
                    self.reply_events(&query, events_to_send).await;
                }

                let wildcard_updates = self
                    .storage_service
                    .wildcard_updates
                    .read()
                    .await
                    .key_value_pairs()
                    .map(|(wildcard_update_ke, update)| {
                        let action = match update.kind {
                            SampleKind::Put => Action::WildcardPut(wildcard_update_ke.clone()),
                            SampleKind::Delete => {
                                Action::WildcardDelete(wildcard_update_ke.clone())
                            }
                        };

                        let event_metadata = EventMetadata {
                            stripped_key: Some(wildcard_update_ke.clone()),
                            timestamp: update.data.timestamp,
                            action,
                        };

                        (event_metadata, update.data.value.clone())
                    })
                    .collect::<HashMap<EventMetadata, Value>>();

                for (wildcard_update_metadata, update_value) in wildcard_updates {
                    reply_to_query(
                        &query,
                        AlignmentReply::Retrieval(wildcard_update_metadata),
                        Some(update_value),
                    )
                    .await;
                }
            }
            AlignmentQuery::Diff(digest_diff) => {
                tracing::trace!("Processing `AlignmentQuery::Diff`");
                if digest_diff.cold_eras_differ {
                    self.reply_cold_era(&query).await;
                }

                if !digest_diff.warm_eras_differences.is_empty() {
                    self.reply_sub_intervals(&query, digest_diff.warm_eras_differences)
                        .await;
                }

                if !digest_diff.hot_eras_differences.is_empty() {
                    self.reply_events_metadata(&query, digest_diff.hot_eras_differences)
                        .await;
                }
            }
            AlignmentQuery::Intervals(different_intervals) => {
                tracing::trace!("Processing `AlignmentQuery::Intervals`");
                if !different_intervals.is_empty() {
                    self.reply_sub_intervals(&query, different_intervals).await;
                }
            }
            AlignmentQuery::SubIntervals(different_sub_intervals) => {
                tracing::trace!("Processing `AlignmentQuery::SubIntervals`");
                if !different_sub_intervals.is_empty() {
                    self.reply_events_metadata(&query, different_sub_intervals)
                        .await;
                }
            }
            AlignmentQuery::Events(events_to_retrieve) => {
                tracing::trace!("Processing `AlignmentQuery::Events`");
                if !events_to_retrieve.is_empty() {
                    self.reply_events(&query, events_to_retrieve).await;
                }
            }
        }
    }

    /// Replies to the provided [Query] with a hash map containing the index of the [Interval] in
    /// the Cold era and their [Fingerprint].
    ///
    /// The Replica will use this response to assess which [Interval]s differ.
    ///
    /// # Temporality
    ///
    /// There is no guarantee that the Replica indicating a difference in the Cold era is "aligned":
    /// it is possible that its Cold era is either ahead or late (i.e. it has more or less
    /// Interval(s) in its Replication Log in the Cold era).
    ///
    /// We believe this is not important: the Replication Log does not separate the Intervals based
    /// on their era so performing this comparison will still be relevant — even if an Interval is
    /// in the Cold era on one end and in the Warm era in the other.
    pub(crate) async fn reply_cold_era(&self, query: &Query) {
        let log = self.replication_log.read().await;
        let configuration = log.configuration();
        let last_elapsed_interval = match configuration.last_elapsed_interval() {
            Ok(last_elapsed_idx) => last_elapsed_idx,
            Err(e) => {
                tracing::error!(
                    "Fatal error: failed to obtain the index of the last elapsed interval: {e:?}"
                );
                return;
            }
        };
        let warm_era_lower_bound = configuration.warm_era_lower_bound(last_elapsed_interval);

        let reply = AlignmentReply::Intervals({
            log.intervals
                .iter()
                .filter(|(&idx, _)| idx < warm_era_lower_bound)
                .map(|(idx, interval)| (*idx, interval.fingerprint()))
                .collect::<HashMap<_, _>>()
        });

        reply_to_query(query, reply, None).await;
    }

    /// Replies to the [Query] with a structure containing, for each interval index present in the
    /// `different_intervals`, all the [SubInterval]s [Fingerprint].
    ///
    /// The Replica will use this structure to assess which [SubInterval]s differ.
    pub(crate) async fn reply_sub_intervals(
        &self,
        query: &Query,
        different_intervals: HashSet<IntervalIdx>,
    ) {
        let mut sub_intervals_fingerprints = HashMap::with_capacity(different_intervals.len());

        {
            let log = self.replication_log.read().await;
            different_intervals.iter().for_each(|interval_idx| {
                if let Some(interval) = log.intervals.get(interval_idx) {
                    sub_intervals_fingerprints
                        .insert(*interval_idx, interval.sub_intervals_fingerprints());
                }
            });
        }

        let reply = AlignmentReply::SubIntervals(sub_intervals_fingerprints);
        reply_to_query(query, reply, None).await;
    }

    /// Replies to the [Query] with all the [EventMetadata] of the [Event]s present in the
    /// [SubInterval]s listed in `different_sub_intervals`.
    ///
    /// The Replica will use this structure to assess which [Event] (and its associated payload) are
    /// missing in its Replication Log and connected Storage.
    ///
    /// # TODO Performance improvement
    ///
    /// Although the Replica we are answering has to find if, for each provided [EventMetadata],
    /// there is a more recent one, it does not need to go through all its Replication Log. It only
    /// needs, for each [EventMetadata], to go through the Intervals that are greater than the one
    /// it is contained in.
    ///
    /// The rationale is that the Intervals are already sorted in increasing order, so if no Event,
    /// for the same key expression, can be found in any greater Interval, then by definition the
    /// Replication Log does not contain a more recent Event.
    ///
    /// That would require the following changes:
    /// - Change the `sub_intervals` field of the `Interval` structure to a BTreeMap.
    /// - In the `reply_events_metadata` method (just below), send out a `HashMap<IntervalIdx,
    ///   HashMap<SubIntervalIdx, HashSet<EventMetadata>>>` instead of a `Vec<EventMetadata>`.
    /// - In the `process_alignment_reply` method, implement the searching algorithm described
    ///   above.
    pub(crate) async fn reply_events_metadata(
        &self,
        query: &Query,
        different_sub_intervals: HashMap<IntervalIdx, HashSet<SubIntervalIdx>>,
    ) {
        let mut events = Vec::default();
        {
            let log = self.replication_log.read().await;
            different_sub_intervals
                .iter()
                .for_each(|(interval_idx, sub_intervals)| {
                    if let Some(interval) = log.intervals.get(interval_idx) {
                        sub_intervals.iter().for_each(|sub_interval_idx| {
                            if let Some(sub_interval) = interval.get(sub_interval_idx) {
                                sub_interval
                                    .events()
                                    .for_each(|(_, event)| events.push(event.into()))
                            }
                        });
                    }
                });
        }

        let reply = AlignmentReply::Events(events);
        reply_to_query(query, reply, None).await;
    }

    /// Replies to the [Query] with the [EventMetadata] and [Value] that were identified as missing.
    ///
    /// This method will fetch the [StoredData] from the Storage for each provided [EventMetadata],
    /// making a distinct reply for each. The fact that multiple replies are sent to the same Query
    /// is the reason why we need the consolidation to set to be `None` (⚠️).
    pub(crate) async fn reply_events(&self, query: &Query, events_to_retrieve: Vec<EventMetadata>) {
        for event_metadata in events_to_retrieve {
            if matches!(
                event_metadata.action,
                Action::Delete | Action::WildcardDelete(_)
            ) {
                reply_to_query(query, AlignmentReply::Retrieval(event_metadata), None).await;
                continue;
            }

            if let Action::WildcardPut(ref wildcard_ke) = event_metadata.action {
                let wildcard_updates_guard = self.storage_service.wildcard_updates.read().await;

                if let Some(update_payload) = wildcard_updates_guard.weight_at(wildcard_ke) {
                    reply_to_query(
                        query,
                        AlignmentReply::Retrieval(event_metadata),
                        Some(update_payload.data.value.clone()),
                    )
                    .await;
                    continue;
                }
            }

            let stored_data = {
                let mut storage = self.storage_service.storage.lock().await;
                match storage.get(event_metadata.stripped_key.clone(), "").await {
                    Ok(stored_data) => stored_data,
                    Err(e) => {
                        tracing::error!(
                            "Failed to retrieve data associated to key < {:?} >: {e:?}",
                            event_metadata.key_expr()
                        );
                        continue;
                    }
                }
            };

            let requested_data = stored_data
                .into_iter()
                .find(|data| data.timestamp == *event_metadata.timestamp());
            match requested_data {
                Some(data) => {
                    tracing::trace!("Sending Sample: {:?}", event_metadata.stripped_key);
                    reply_to_query(
                        query,
                        AlignmentReply::Retrieval(event_metadata),
                        Some(data.value),
                    )
                    .await;
                }
                None => {
                    // NOTE: This is not necessarily an error. There is a possibility that the data
                    //       associated with this specific key was updated between the time the
                    //       [AlignmentQuery] was sent and when it is processed.
                    //
                    //       Hence, at the time it was "valid" but it no longer is.
                    tracing::debug!(
                        "Found no data in the Storage associated to key < {:?} > with a Timestamp \
                         equal to: {}",
                        event_metadata.key_expr(),
                        event_metadata.timestamp()
                    );
                }
            }
        }
    }

    /// Spawns a new task to query the Aligner of the Replica which potentially has data this
    /// Storage is missing.
    ///
    /// This method will:
    /// 1. Serialise the AlignmentQuery.
    /// 2. Send a Query to the Aligner of the Replica, adding the serialised AlignmentQuery as an
    ///    attachment.
    /// 3. Process all replies.
    ///
    /// Note that the processing of a reply can trigger a new query (requesting additional
    /// information), spawning a new task.
    ///
    /// This process is stateless and all the required information are carried in the query / reply.
    pub(crate) fn spawn_query_replica_aligner(
        &self,
        replica_aligner_ke: OwnedKeyExpr,
        alignment_query: AlignmentQuery,
    ) -> JoinHandle<()> {
        let replication = self.clone();
        tokio::task::spawn(async move {
            let attachment = match bincode::serialize(&alignment_query) {
                Ok(attachment) => attachment,
                Err(e) => {
                    tracing::error!("Failed to serialize AlignmentQuery: {e:?}");
                    return;
                }
            };

            // NOTE: We need to put the Consolidation to `None` as otherwise if multiple replies are
            //       sent, they will be "consolidated" and only one of them will make it through.
            //
            //       When we retrieve Samples from a Replica, each Sample is sent in a separate
            //       reply. Hence the need to have no consolidation.
            let mut consolidation = ConsolidationMode::None;

            if matches!(alignment_query, AlignmentQuery::Discovery) {
                // NOTE: `Monotonic` means that Zenoh will forward the first answer it receives (and
                //       ensure that later answers are with a higher timestamp — we do not care
                //       about that last aspect).
                //
                //       By setting the consolidation to this value when performing the initial
                //       alignment, we select the most reactive Replica (hopefully the closest as
                //       well).
                consolidation = ConsolidationMode::Monotonic;
            }

            match replication
                .zenoh_session
                .get(Into::<Selector>::into(replica_aligner_ke.clone()))
                .attachment(attachment)
                .consolidation(consolidation)
                .await
            {
                Err(e) => {
                    tracing::error!("Failed to query Aligner < {replica_aligner_ke} >: {e:?}");
                }
                Ok(reply_receiver) => {
                    while let Ok(reply) = reply_receiver.recv_async().await {
                        let sample = match reply.into_result() {
                            Ok(sample) => sample,
                            Err(e) => {
                                tracing::warn!(
                                    "Skipping reply to query to < {replica_aligner_ke} >: {e:?}"
                                );
                                continue;
                            }
                        };

                        let alignment_reply = match sample.attachment() {
                            None => {
                                tracing::debug!("Skipping reply without attachment");
                                continue;
                            }
                            Some(attachment) => {
                                match bincode::deserialize::<AlignmentReply>(&attachment.to_bytes())
                                {
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to deserialize attachment as AlignmentReply: \
                                             {e:?}"
                                        );
                                        continue;
                                    }
                                    Ok(alignment_reply) => alignment_reply,
                                }
                            }
                        };

                        replication
                            .process_alignment_reply(
                                replica_aligner_ke.clone(),
                                alignment_reply,
                                sample,
                            )
                            .await;

                        // The consolidation mode `Monotonic`, used for sending out an
                        // `AlignmentQuery::Discovery`, will keep on sending replies. We only want
                        // to discover / align with a single Replica so we break here.
                        if matches!(alignment_query, AlignmentQuery::Discovery) {
                            return;
                        }
                    }
                }
            }
        })
    }

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
                                    .filter(|(sub_idx, sub_fp)| match interval.get(sub_idx) {
                                        None => true,
                                        Some(sub_interval) => sub_interval.fingerprint != *sub_fp,
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
            AlignmentReply::Events(replica_events) => {
                let mut diff_events = Vec::default();

                for replica_event in replica_events {
                    tracing::trace!(
                        "Processing `AlignmentReply::Events` on: < {:?} >",
                        replica_event.stripped_key
                    );

                    if self
                        .latest_updates
                        .read()
                        .await
                        .get(&replica_event.stripped_key)
                        .is_some_and(|latest_event| {
                            latest_event.timestamp >= replica_event.timestamp
                        })
                    {
                        continue;
                    }

                    let mut maybe_wildcard_update = None;
                    if matches!(replica_event.action, Action::Put | Action::Delete) {
                        let Ok(non_stripped_ke) = crate::prefix(
                            self.storage_service.configuration.strip_prefix.as_ref(),
                            replica_event.stripped_key.as_ref(),
                        ) else {
                            tracing::error!(
                                "Internal error while attempting to prefix < {:?} > with < {:?} >",
                                self.storage_service.configuration.strip_prefix,
                                replica_event.stripped_key
                            );
                            continue;
                        };

                        maybe_wildcard_update = self
                            .storage_service
                            .overriding_wild_update(&non_stripped_ke, &replica_event.timestamp)
                            .await;
                    }

                    // NOTE: We do not need to hold a write lock in all situations. However, we
                    // cannot know ahead of time if that's going to be the case. The scenario that
                    // is particularly problematic is in case a PUT event is received for which
                    // there is a more recent wildcard update.
                    //
                    // In that scenario we need to check if there isn't yet another more recent
                    // event in the Replication Log.  If we take a read lock and there is no event
                    // more recent then we need to change the read lock to a write lock to add the
                    // wildcard update. To switch from a read lock to a write lock we need to
                    // drop the read lock, and take a write lock.
                    //
                    // By directly taking a write lock we skip that extra complexity.
                    let mut replication_log_guard = self.replication_log.write().await;
                    match (
                        maybe_wildcard_update,
                        replication_log_guard.lookup(&replica_event.stripped_key),
                    ) {
                        (Some(wildcard_update), Some(latest_event)) => {
                            if latest_event.timestamp >= wildcard_update.data.timestamp {
                                if latest_event.timestamp >= replica_event.timestamp {
                                    continue;
                                }
                            } else {
                                tracing::trace!(
                                    "Event on < {:?} > is overridden by Wildcard Update",
                                    replica_event.stripped_key
                                );
                                self.store_event_overridden_wildcard_update(
                                    replication_log_guard,
                                    replica_event,
                                    wildcard_update,
                                )
                                .await;
                                continue;
                            }
                        }
                        (None, Some(latest_event)) => {
                            if latest_event.timestamp >= replica_event.timestamp {
                                continue;
                            }
                        }
                        (Some(wildcard_update), None) => {
                            tracing::trace!(
                                "Event on < {:?} > is overridden by Wildcard Update",
                                replica_event.stripped_key
                            );
                            self.store_event_overridden_wildcard_update(
                                replication_log_guard,
                                replica_event,
                                wildcard_update,
                            )
                            .await;
                            continue;
                        }
                        (None, None) => {}
                    }

                    match replica_event.action {
                        Action::Put | Action::WildcardPut(_) => {
                            diff_events.push(replica_event);
                            continue;
                        }
                        Action::Delete => {
                            if matches!(
                                self.storage_service
                                    .storage
                                    .lock()
                                    .await
                                    .delete(
                                        replica_event.stripped_key.clone(),
                                        replica_event.timestamp
                                    )
                                    .await,
                                // NOTE: In some of our backend implementation, a deletion on a
                                //       non-existing key will return an error. Given that we cannot
                                //       distinguish an error from a missing key, we will assume the
                                //       latter and move forward.
                                //
                                // FIXME: Once the behaviour described above is fixed, check for
                                //        errors.
                                Ok(StorageInsertionResult::Outdated)
                            ) {
                                continue;
                            }
                        }
                        Action::WildcardDelete(ref wildcard_delete_ke) => {
                            self.storage_service
                                .register_wildcard_update(
                                    wildcard_delete_ke.clone(),
                                    SampleKind::Delete,
                                    replica_event.timestamp,
                                    Value::empty(),
                                )
                                .await;

                            match replication_log_guard.apply_wildcard_update(
                                wildcard_delete_ke,
                                &replica_event.timestamp,
                                SampleKind::Delete,
                            ) {
                                Err(e) => {
                                    tracing::error!(
                                        "Fatal error trying to apply wildcard delete < \
                                         {wildcard_delete_ke} >: {e:?}"
                                    );
                                    return;
                                }
                                Ok(deleted_entries) => {
                                    let mut storage_guard =
                                        self.storage_service.storage.lock().await;
                                    for deleted_event in deleted_entries {
                                        if matches!(
                                            storage_guard
                                                .delete(
                                                    deleted_event.maybe_stripped_key.clone(),
                                                    deleted_event.timestamp,
                                                )
                                                .await,
                                            Ok(StorageInsertionResult::Outdated)
                                        ) {
                                            tracing::error!(
                                                "Internal error detected: Wildcard Delete < {} > \
                                                 applied on < {:?} > was flagged as Outdated",
                                                wildcard_delete_ke,
                                                deleted_event.maybe_stripped_key
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    replication_log_guard.insert_event(replica_event.into());
                }

                if !diff_events.is_empty() {
                    self.spawn_query_replica_aligner(
                        replica_aligner_ke,
                        AlignmentQuery::Events(diff_events),
                    );
                }
            }
            AlignmentReply::Retrieval(replica_event) => {
                tracing::trace!("Processing `AlignmentReply::Retrieval`");
                {
                    let span = tracing::Span::current();
                    span.record(
                        "sample",
                        replica_event
                            .stripped_key
                            .as_ref()
                            .map_or("", |key| key.as_str()),
                    );
                    span.record("t", replica_event.timestamp.to_string());
                }

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
                if let Some(latest_event) =
                    replication_log_guard.lookup(&replica_event.stripped_key)
                {
                    if latest_event.timestamp >= replica_event.timestamp {
                        return;
                    }
                }

                match replica_event.action {
                    // NOTE: This code can only be called with `action` set to `delete` on an
                    // initial alignment, in which case the Storage of the receiving Replica is
                    // empty => there is no need to actually call `storage.delete`.
                    //
                    // Outside of an initial alignment, the `delete` action will be performed at the
                    // step above, in `AlignmentReply::Events`.
                    Action::Delete => {}
                    Action::Put => {
                        if matches!(
                            self.storage_service
                                .storage
                                .lock()
                                .await
                                .put(
                                    replica_event.stripped_key.clone(),
                                    sample.into(),
                                    replica_event.timestamp,
                                )
                                .await,
                            Ok(StorageInsertionResult::Outdated) | Err(_)
                        ) {
                            return;
                        }
                    }
                    Action::WildcardDelete(ref wildcard_delete_ke) => {
                        self.storage_service
                            .register_wildcard_update(
                                wildcard_delete_ke.clone(),
                                SampleKind::Delete,
                                replica_event.timestamp,
                                sample,
                            )
                            .await;
                    }
                    Action::WildcardPut(ref wildcard_update_ke) => {
                        self.storage_service
                            .register_wildcard_update(
                                wildcard_update_ke.clone(),
                                SampleKind::Put,
                                replica_event.timestamp,
                                sample.clone(),
                            )
                            .await;

                        match replication_log_guard.apply_wildcard_update(
                            wildcard_update_ke,
                            &replica_event.timestamp,
                            SampleKind::Put,
                        ) {
                            Err(e) => {
                                tracing::error!(
                                    "Internal error attempting to apply Wildcard Put < \
                                     {wildcard_update_ke} >: {e:?}"
                                );
                                return;
                            }
                            Ok(entries_to_reinsert) => {
                                for entry in entries_to_reinsert {
                                    let mut storage_guard =
                                        self.storage_service.storage.lock().await;

                                    let _ = storage_guard
                                        .delete(entry.maybe_stripped_key.clone(), entry.timestamp)
                                        .await;

                                    let _ = storage_guard
                                        .put(
                                            entry.maybe_stripped_key,
                                            sample.clone().into(),
                                            replica_event.timestamp,
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }

                replication_log_guard.insert_event(replica_event.into());
            }
        }
    }

    /// Stores in the Storage and/or the Replication Log an [Event] on the key expression associated
    /// to provided `replica_event`.
    ///
    /// A payload will be pushed to the Storage if the `wildcard_update` is a put.
    //
    // NOTE: There is no need to attempt to delete an event in the Storage if the `wildcard_update`
    //       is a delete. Indeed, if the wildcard update is a delete then it is impossible to have
    //       a previous event associated to the same key expression as, by definition of a wildcard
    //       update, it would have been deleted.
    async fn store_event_overridden_wildcard_update(
        &self,
        mut replication_log_guard: RwLockWriteGuard<'_, LogLatest>,
        replica_event: EventMetadata,
        wildcard_update: Update,
    ) {
        if wildcard_update.kind == SampleKind::Put
            && matches!(
                self.storage_service
                    .storage
                    .lock()
                    .await
                    .put(
                        replica_event.stripped_key.clone(),
                        wildcard_update.data.value,
                        wildcard_update.data.timestamp
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

        replication_log_guard.insert_event(replica_event.into());
    }
}

/// Replies to a Query, adding the [AlignmentReply] as an attachment and, if provided, the [Value]
/// as the payload (not forgetting to set the Encoding!).
async fn reply_to_query(query: &Query, reply: AlignmentReply, value: Option<Value>) {
    let mut timestamp = None;
    if let AlignmentReply::Retrieval(ref event_metadata) = reply {
        if matches!(
            event_metadata.action,
            Action::WildcardDelete(_) | Action::WildcardPut(_)
        ) {
            timestamp = Some(event_metadata.timestamp);
        }
    }

    let attachment = match bincode::serialize(&reply) {
        Ok(attachment) => attachment,
        Err(e) => {
            tracing::error!("Failed to serialize AlignmentReply: {e:?}");
            return;
        }
    };

    let mut reply_builder = if let Some(value) = value {
        query
            .reply(query.key_expr(), value.payload)
            .encoding(value.encoding)
            .attachment(attachment)
    } else {
        query
            .reply(query.key_expr(), ZBytes::new())
            .attachment(attachment)
    };

    if let Some(timestamp) = timestamp {
        reply_builder = reply_builder.timestamp(timestamp);
    }

    if let Err(e) = reply_builder.await {
        tracing::error!("Failed to reply to Query: {e:?}");
    }
}
