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

//! Per-source arrival, buffering, and delivery policy.
//!
//! Arrivals are classified relative to the last delivered sequence number. The
//! next eligible delivery can be later: eviction and abandonment allow skipping
//! retired samples, while completed history/sample queries grant bounded authority
//! to skip unseen samples. Only delivery reports misses and advances the baseline.

use std::{
    collections::{btree_map, BTreeMap, HashMap},
    sync::Arc,
    time::Instant,
};

use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};
use zenoh::{
    handlers::Callback,
    sample::{Sample, SourceInfo},
    session::EntityGlobalId,
    time::Timestamp,
};

use super::Miss;
use crate::{
    fragmentation::{validate_fragment_metadata, FragInsertError, PendingSample},
    utils::WrappingSn,
};

/// Valid duplicates count as activity even when nothing is inserted. Explicitly
/// retired input does not; malformed fragments are returned as validation errors.
pub(super) enum ReceiveOutcome {
    Accepted {
        /// A new fragment slot that can arm recovery, not progress within an assembly.
        new_fragment_slot: bool,
    },
    Ignored,
}

pub(super) struct DeliveryContext<'a> {
    pub(super) callback: &'a Callback<Sample>,
    pub(super) miss_handlers: &'a HashMap<usize, Callback<Miss>>,
    pub(super) source_id: EntityGlobalId,
}

pub(super) struct ArrivalContext<'a> {
    pub(super) delivery: DeliveryContext<'a>,
    pub(super) history_pending: bool,
    pub(super) retransmission: bool,
    pub(super) max_pending_samples: usize,
    pub(super) max_fragments: u32,
}

enum Arrival {
    Retired,
    InitialHistory,
    OldOrDuplicate,
    FirstOrConsecutive,
    Ahead,
}

pub(super) struct SequencedSource {
    /// Identifies this source state so old fragment queries cannot affect a
    /// source that was removed and later added again.
    pub(super) generation: Arc<()>,
    pub(super) last_delivered: Option<WrappingSn>,
    /// Reject samples at or below this sequence number, even if delivery has
    /// not reached them yet. These samples were removed to limit buffer size.
    pub(super) last_evicted: Option<WrappingSn>,
    /// Number of unfinished history queries, sample-recovery queries, and fragment
    /// recovery attempts for this source.
    /// An attempt counts once, even if it sends queries for several fragment ranges.
    pub(super) pending_queries: u64,
    /// Newest complete sample buffered when a history or sample-recovery query finished.
    /// Delivery can skip missing sequence numbers up to this value, but still
    /// waits for unfinished queries and fragments it can recover.
    pub(super) gap_skip_through: Option<WrappingSn>,
    pub(super) pending_samples: BTreeMap<WrappingSn, PendingSample>,
    pub(super) latest_access: Instant,
    pub(super) periodic_task: Option<AbortOnDropHandle<()>>,
    /// Periodically checks all samples for missing fragments. Stops when none
    /// are missing.
    pub(super) frag_recovery_task: Option<AbortOnDropHandle<()>>,
    /// Shared cancellation for this source generation's fragment-query dispatches.
    pub(super) frag_dispatch_cancel: CancellationToken,
    pub(super) alive: bool,
}

impl Default for SequencedSource {
    fn default() -> Self {
        Self {
            generation: Arc::new(()),
            last_delivered: None,
            last_evicted: None,
            pending_queries: 0,
            gap_skip_through: None,
            pending_samples: BTreeMap::new(),
            latest_access: Instant::now(),
            periodic_task: None,
            frag_recovery_task: None,
            frag_dispatch_cancel: CancellationToken::new(),
            alive: false,
        }
    }
}

impl Drop for SequencedSource {
    fn drop(&mut self) {
        self.frag_dispatch_cancel.cancel();
    }
}

impl SequencedSource {
    fn classify_arrival(&self, sn: WrappingSn, history_pending: bool) -> Arrival {
        if self.last_evicted.is_some_and(|last| sn <= last)
            || self
                .pending_samples
                .get(&sn)
                .is_some_and(PendingSample::is_abandoned)
        {
            return Arrival::Retired;
        }
        match self.last_delivered {
            None if history_pending => Arrival::InitialHistory,
            None => Arrival::FirstOrConsecutive,
            Some(last) if sn == last + 1 => Arrival::FirstOrConsecutive,
            Some(last) if sn > last => Arrival::Ahead,
            Some(_) => Arrival::OldOrDuplicate,
        }
    }

    /// Process a sequenced arrival once. Accepted activity is recorded by the
    /// caller alongside LRU promotion. The `new_fragment_slot` flag describes only
    /// fragmented input; whole samples never arm fragment recovery.
    pub(super) fn receive(
        &mut self,
        sample: Sample,
        info: &SourceInfo,
        context: &ArrivalContext<'_>,
    ) -> Result<ReceiveOutcome, FragInsertError> {
        let sn = info.source_sn().into();
        let arrival = self.classify_arrival(sn, context.history_pending);
        let is_fragmented = sample.frag_info().is_some();
        let deliver_directly = !is_fragmented
            && match arrival {
                // A pending-buffer limit of one delivers whole samples immediately,
                // even while initial history is pending.
                Arrival::InitialHistory => context.max_pending_samples == 1,
                // With recovery, even whole samples wait for buffered predecessors.
                Arrival::FirstOrConsecutive => {
                    !context.retransmission || self.pending_samples.is_empty()
                }
                Arrival::Ahead => !context.retransmission,
                Arrival::Retired | Arrival::OldOrDuplicate => false,
            };

        let new_fragment_slot = match arrival {
            Arrival::Retired => return Ok(ReceiveOutcome::Ignored),
            // Delivered duplicates have no assembly left to validate them.
            // Check their advertised metadata before counting them as activity.
            Arrival::OldOrDuplicate => match sample.frag_info() {
                Some(fi) => validate_fragment_metadata(
                    fi.frag_num(),
                    fi.frag_count(),
                    context.max_fragments,
                )
                .map(|()| false),
                None => Ok(false),
            },
            _ if deliver_directly => {
                self.deliver_and_drain(
                    sample,
                    sn,
                    context.delivery.callback,
                    context.delivery.miss_handlers,
                    context.delivery.source_id,
                );
                Ok(false)
            }
            _ => self
                .insert_sample(sample, info, context.max_fragments)
                .map(|new_entry| {
                    new_entry
                        && is_fragmented
                        && (context.retransmission || !matches!(arrival, Arrival::Ahead))
                }),
        };

        if !deliver_directly {
            match (&arrival, &new_fragment_slot) {
                // Invalid history fragments neither enforce the bound nor pop.
                (Arrival::InitialHistory, Ok(_)) => self.enforce_arrival_limit(context),
                (Arrival::Ahead, _) if context.retransmission => {
                    // Ahead arrivals advance BEFORE enforcing the bound, even
                    // after rejection: a completion can unblock an evicted or failed head.
                    let ready = self.advance_ready();
                    self.deliver_ready(ready, &context.delivery);
                    self.enforce_arrival_limit(context);
                }
                (Arrival::FirstOrConsecutive, Ok(_)) | (Arrival::Ahead, _) => {
                    // In-order arrivals enforce BEFORE advancing delivery.
                    // Without recovery, ahead fragments also enforce first,
                    // including rejected fragments, then check their own slot.
                    self.enforce_arrival_limit(context);
                    let ready = if context.retransmission {
                        self.advance_ready()
                    } else {
                        self.take_complete(sn).map(|sample| (sn, sample))
                    };
                    self.deliver_ready(ready, &context.delivery);
                }
                _ => {}
            }
        }
        if !context.history_pending {
            self.drain_authorized_samples(
                context.delivery.callback,
                context.delivery.miss_handlers,
                context.delivery.source_id,
                context.retransmission,
            );
        }
        new_fragment_slot.map(|new_fragment_slot| ReceiveOutcome::Accepted { new_fragment_slot })
    }

    fn enforce_arrival_limit(&mut self, context: &ArrivalContext<'_>) {
        self.enforce_pending_limit(
            context.delivery.callback,
            context.delivery.miss_handlers,
            context.delivery.source_id,
            context.max_pending_samples,
        );
    }

    fn deliver_ready(
        &mut self,
        ready: Option<(WrappingSn, Sample)>,
        context: &DeliveryContext<'_>,
    ) {
        if let Some((sn, sample)) = ready {
            self.deliver_and_drain(
                sample,
                sn,
                context.callback,
                context.miss_handlers,
                context.source_id,
            );
        }
    }

    /// Add a sample or fragment to the buffer. Return whether a sequence-number
    /// entry was created. Validation errors go to the arrival boundary, which
    /// logs each rejection once.
    pub(super) fn insert_sample(
        &mut self,
        sample: Sample,
        source_info: &SourceInfo,
        max_fragments: u32,
    ) -> Result<bool, FragInsertError> {
        let sn = source_info.source_sn().into();
        let Some(fi) = sample.frag_info() else {
            return Ok(self
                .pending_samples
                .insert(sn, PendingSample::single(sample))
                .is_none());
        };
        let (frag_num, frag_count) = (fi.frag_num(), fi.frag_count());
        self.insert_fragment(sample, sn, frag_num, frag_count, max_fragments)
    }

    fn insert_fragment(
        &mut self,
        sample: Sample,
        sn: WrappingSn,
        frag_num: u32,
        frag_count: u32,
        max_fragments: u32,
    ) -> Result<bool, FragInsertError> {
        validate_fragment_metadata(frag_num, frag_count, max_fragments)?;
        match self.pending_samples.entry(sn) {
            btree_map::Entry::Vacant(entry) => {
                let pending = PendingSample::from_first_fragment(
                    sample,
                    frag_num,
                    frag_count,
                    max_fragments,
                )?;
                entry.insert(pending);
                Ok(true)
            }
            btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().insert(sample, frag_num, frag_count)?;
                Ok(false)
            }
        }
    }

    /// Remove the oldest samples until the buffer fits the configured limit.
    /// Deliver complete samples and discard samples with missing fragments.
    /// Report discarded samples as missed only when a newer sample is delivered.
    pub(super) fn enforce_pending_limit(
        &mut self,
        callback: &Callback<Sample>,
        miss_handlers: &HashMap<usize, Callback<Miss>>,
        source_id: EntityGlobalId,
        max_pending_samples: usize,
    ) {
        while self.pending_samples.len() > max_pending_samples.max(1) {
            let (sn, sample) = self
                .pending_samples
                .pop_first()
                .expect("non-empty pending_samples");
            if let Some(sample) = sample.into_sample() {
                self.deliver_and_drain(sample, sn, callback, miss_handlers, source_id);
            } else {
                self.last_evicted = Some(sn);
                tracing::debug!(
                    "AdvancedSubscriber: evicted incomplete sample at sn={sn} due to max_pending_samples={max_pending_samples}"
                );
                // Removing an incomplete sample may let us deliver the next one.
                // Try now, even if the buffer is already small enough.
                if let Some((sn, sample)) = self.advance_ready() {
                    self.deliver_and_drain(sample, sn, callback, miss_handlers, source_id);
                }
            }
        }
    }

    /// Find the next sequence number to deliver, skipping samples we discarded
    /// or gave up recovering. Before the first delivery, start with the oldest
    /// buffered sample we have not given up on.
    pub(super) fn next_expected(&self) -> Option<WrappingSn> {
        let Some(last) = self.last_delivered else {
            return self
                .pending_samples
                .iter()
                .find(|(_, sample)| !sample.is_abandoned())
                .map(|(sn, _)| *sn);
        };
        let mut next = last + 1;
        if let Some(evicted) = self.last_evicted {
            if evicted >= next {
                next = evicted + 1;
            }
        }
        while self
            .pending_samples
            .get(&next)
            .is_some_and(PendingSample::is_abandoned)
        {
            next += 1;
        }
        Some(next)
    }

    /// Abandon failed blocking assemblies, then take the next ready sample.
    pub(super) fn advance_ready(&mut self) -> Option<(WrappingSn, Sample)> {
        self.abandon_failed_head();
        let sn = self.next_expected()?;
        self.take_complete(sn).map(|sample| (sn, sample))
    }

    /// Give up recovering the oldest incomplete sample if recovery failed and
    /// a newer complete sample is waiting. Samples we have not received at all
    /// still need a sample-recovery query.
    pub(super) fn abandon_failed_head(&mut self) {
        loop {
            let Some((&sn, sample)) = self.pending_samples.iter().find(|(_, s)| !s.is_abandoned())
            else {
                return;
            };
            if !sample.can_abandon()
                || !self
                    .pending_samples
                    .range((std::ops::Bound::Excluded(sn), std::ops::Bound::Unbounded))
                    .any(|(_, sample)| sample.is_complete())
            {
                return;
            }
            // If an earlier sample is entirely missing, first query for it.
            // A completed query may already allow skipping that sequence number.
            if self.next_expected().is_some_and(|next| sn > next)
                && !self.gap_skip_through.is_some_and(|through| sn <= through)
            {
                return;
            }
            self.pending_samples.insert(sn, PendingSample::Abandoned);
        }
    }

    pub(super) fn take_complete(&mut self, sn: WrappingSn) -> Option<Sample> {
        if let btree_map::Entry::Occupied(entry) = self.pending_samples.entry(sn) {
            if entry.get().is_complete() {
                return Some(
                    entry
                        .remove()
                        .into_sample()
                        .expect("into_sample should not fail when PendingSample is complete"),
                );
            }
        }
        None
    }

    pub(super) fn deliver_and_drain(
        &mut self,
        sample: Sample,
        source_sn: impl Into<WrappingSn>,
        callback: &Callback<Sample>,
        miss_handlers: &HashMap<usize, Callback<Miss>>,
        source_id: EntityGlobalId,
    ) {
        self.deliver_one(sample, source_sn.into(), callback, miss_handlers, source_id);
        while let Some((sn, sample)) = self.advance_ready() {
            self.deliver_one(sample, sn, callback, miss_handlers, source_id);
        }
        if !self
            .pending_samples
            .values()
            .any(PendingSample::is_incomplete)
        {
            self.frag_recovery_task = None;
        }
    }

    /// The sole sequenced-delivery path owns miss accounting, the baseline,
    /// eviction-watermark retirement, and removal of obsolete buffered entries.
    fn deliver_one(
        &mut self,
        sample: Sample,
        sn: WrappingSn,
        callback: &Callback<Sample>,
        miss_handlers: &HashMap<usize, Callback<Miss>>,
        source_id: EntityGlobalId,
    ) {
        // Count discarded and abandoned samples only when delivery crosses them.
        // Before the first delivery, we do not know which samples were missed.
        if let Some(last) = self.last_delivered {
            if sn > last + 1 {
                let missed = sn - last - 1;
                tracing::info!("Sample missed: missed {missed} samples from {source_id:?}.");
                for callback in miss_handlers.values() {
                    callback.call(Miss {
                        source: source_id,
                        nb: missed,
                    });
                }
            }
        }
        callback.call(sample);
        self.last_delivered = Some(sn);
        // The baseline now rejects these old samples. Keeping last_evicted
        // could reject new samples later as the wrapping counter advances.
        if self.last_evicted.is_some_and(|evicted| sn >= evicted) {
            self.last_evicted = None;
        }
        while self
            .pending_samples
            .first_key_value()
            .is_some_and(|(key, _)| *key <= sn)
        {
            self.pending_samples.pop_first();
        }
    }

    /// After a history or sample-recovery query finishes, remember the newest
    /// complete sample in the buffer. Once unfinished queries and earlier missing
    /// fragments are resolved, delivery can skip missing sequence numbers up to
    /// that sample.
    ///
    /// For example, a limit of 13 lets delivery skip a missing sample 12 to
    /// deliver sample 13. If sample 15 arrives later, skipping a missing sample
    /// 14 still requires another history or sample-recovery query.
    pub(super) fn authorize_gap_skip(&mut self) {
        if let Some(last_complete) = self
            .pending_samples
            .iter()
            .rev()
            .find_map(|(sn, sample)| sample.is_complete().then_some(*sn))
        {
            self.gap_skip_through = Some(
                self.gap_skip_through
                    .map_or(last_complete, |old| old.max(last_complete)),
            );
        }
    }

    pub(super) fn drain_authorized_samples(
        &mut self,
        callback: &Callback<Sample>,
        miss_handlers: &HashMap<usize, Callback<Miss>>,
        source_id: EntityGlobalId,
        retransmission: bool,
    ) {
        if self.pending_queries != 0 {
            return;
        }
        let Some(through) = self.gap_skip_through else {
            return;
        };
        loop {
            if retransmission {
                self.abandon_failed_head();
            }
            if self.last_delivered.is_some_and(|last| last >= through)
                || self.last_evicted.is_some_and(|last| last >= through)
            {
                self.gap_skip_through = None;
                return;
            }
            // Keep the Abandoned entries until a newer sample is delivered.
            // They prevent late fragments from recreating samples we gave up on.
            let Some((&sn, sample)) = self
                .pending_samples
                .iter()
                .find(|(sn, sample)| **sn <= through && !sample.is_abandoned())
            else {
                self.gap_skip_through = None;
                return;
            };
            if retransmission && sample.is_incomplete() {
                return;
            }
            let sample = self.pending_samples.remove(&sn).unwrap();
            if let Some(sample) = sample.into_sample() {
                if self.last_delivered.map_or(true, |last| sn > last) {
                    self.deliver_and_drain(sample, sn, callback, miss_handlers, source_id);
                }
            }
        }
    }
}

/// Buffers samples by timestamp while history queries are running.
/// Fragment recovery is only supported for sources with sequence numbers.
pub(super) struct TimestampedSource {
    pub(super) last_delivered: Option<Timestamp>,
    pub(super) pending_queries: u64,
    pub(super) pending_samples: BTreeMap<Timestamp, Sample>,
    pub(super) latest_access: Instant,
    pub(super) alive: bool,
}

impl Default for TimestampedSource {
    fn default() -> Self {
        Self {
            last_delivered: None,
            pending_queries: 0,
            pending_samples: BTreeMap::new(),
            latest_access: Instant::now(),
            alive: false,
        }
    }
}

impl TimestampedSource {
    pub(super) fn receive(
        &mut self,
        sample: Sample,
        callback: &Callback<Sample>,
        global_pending_queries: u64,
        max_pending_samples: usize,
    ) {
        let timestamp = *sample.timestamp().expect("timestamped source");
        if self.last_delivered.map(|t| t < timestamp).unwrap_or(true) {
            if (global_pending_queries == 0 && self.pending_queries == 0)
                || max_pending_samples == 1
            {
                self.last_delivered = Some(timestamp);
                callback.call(sample);
            } else {
                self.pending_samples.entry(timestamp).or_insert(sample);
                if self.pending_samples.len() >= max_pending_samples {
                    self.flush(Some(callback));
                }
            }
        }
        self.latest_access = Instant::now();
    }

    pub(super) fn flush(&mut self, callback: Option<&Callback<Sample>>) {
        let Some(callback) = callback else {
            return;
        };
        if self.pending_queries == 0 && !self.pending_samples.is_empty() {
            for (timestamp, sample) in std::mem::take(&mut self.pending_samples) {
                if self.last_delivered.map_or(true, |last| timestamp > last) {
                    self.last_delivered = Some(timestamp);
                    callback.call(sample);
                }
            }
        }
    }
}
