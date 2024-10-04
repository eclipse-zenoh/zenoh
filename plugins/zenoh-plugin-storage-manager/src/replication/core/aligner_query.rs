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
use zenoh::{bytes::ZBytes, internal::Value, query::Query, sample::SampleKind};

use super::aligner_reply::AlignmentReply;
use crate::replication::{
    classification::{IntervalIdx, SubIntervalIdx},
    core::Replication,
    digest::DigestDiff,
    log::EventMetadata,
};

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
                    let mut events_to_retrieve = Vec::default();
                    if let Some(interval) = self
                        .replication_log
                        .read()
                        .await
                        .intervals
                        .get(&interval_idx)
                    {
                        interval.sub_intervals().for_each(|sub_interval| {
                            events_to_retrieve.extend(sub_interval.events().map(Into::into));
                        });
                    }

                    // NOTE: As we took the lock in the `if let` block, it is released here,
                    // diminishing contention.
                    for event_to_retrieve in events_to_retrieve {
                        self.reply_event_retrieval(&query, event_to_retrieve).await;
                    }
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
                for event_to_retrieve in events_to_retrieve {
                    self.reply_event_retrieval(&query, event_to_retrieve).await;
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
                            if let Some(sub_interval) = interval.sub_interval_at(sub_interval_idx) {
                                events.extend(sub_interval.events().map(Into::into));
                            }
                        });
                    }
                });
        }

        let reply = AlignmentReply::EventsMetadata(events);
        reply_to_query(query, reply, None).await;
    }

    /// Replies to the [Query] with the [EventMetadata] and [Value] identified as missing.
    ///
    /// This method will fetch the [StoredData] from the Storage.
    pub(crate) async fn reply_event_retrieval(
        &self,
        query: &Query,
        event_to_retrieve: EventMetadata,
    ) {
        let mut value = None;

        if event_to_retrieve.action == SampleKind::Put {
            let stored_data = {
                let mut storage = self.storage.lock().await;
                match storage
                    .get(event_to_retrieve.stripped_key.clone(), "")
                    .await
                {
                    Ok(stored_data) => stored_data,
                    Err(e) => {
                        tracing::error!(
                            "Failed to retrieve data associated to key < {:?} >: {e:?}",
                            event_to_retrieve.key_expr()
                        );
                        return;
                    }
                }
            };

            let requested_data = stored_data
                .into_iter()
                .find(|data| data.timestamp == *event_to_retrieve.timestamp());
            match requested_data {
                Some(data) => {
                    value = Some(data.value);
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
                        event_to_retrieve.key_expr(),
                        event_to_retrieve.timestamp()
                    );
                }
            }
        }

        reply_to_query(query, AlignmentReply::Retrieval(event_to_retrieve), value).await;
    }
}

/// Replies to a Query, adding the [AlignmentReply] as an attachment and, if provided, the [Value]
/// as the payload (not forgetting to set the Encoding!).
async fn reply_to_query(query: &Query, reply: AlignmentReply, value: Option<Value>) {
    let attachment = match bincode::serialize(&reply) {
        Ok(attachment) => attachment,
        Err(e) => {
            tracing::error!("Failed to serialize AlignmentReply: {e:?}");
            return;
        }
    };

    let reply_fut = if let Some(value) = value {
        query
            .reply(query.key_expr(), value.payload)
            .encoding(value.encoding)
            .attachment(attachment)
    } else {
        query
            .reply(query.key_expr(), ZBytes::new())
            .attachment(attachment)
    };

    if let Err(e) = reply_fut.await {
        tracing::error!("Failed to reply to Query: {e:?}");
    }
}
