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

//! Recovery decisions and their effect on ordered delivery.
//!
//! Whole-sample queries recover sequence-number gaps; fragment arrival and
//! timer scans select missing ranges within samples already buffered. Only
//! history and whole-sample query completion authorize skipping unseen samples.
//! Query transport and reservation lifetimes live in the sibling `queries` module.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio_util::task::AbortOnDropHandle;
use uhlc::ID;
use zenoh::{
    internal::{runtime::ZRuntime, zlock},
    query::Selector,
    session::EntityGlobalId,
};

use super::{
    queries::{reserve_queries, FragmentRequest, QueryContext, QueryScope, RepliesHandler},
    SequencedSource, State,
};
use crate::{
    advanced_query::format_range,
    fragmentation::{PendingSample, RecoveryTrigger},
    utils::WrappingSn,
};

impl State {
    /// The last global history query has finished. Apply its delivery authority
    /// to all sources and start their periodic recovery timers.
    pub(super) fn on_initial_queries_finished(&mut self, statesref: &Arc<Mutex<State>>) {
        let source_ids: Vec<_> = self.sequenced_states.iter().map(|(id, _)| *id).collect();
        for source_id in source_ids {
            self.on_history_or_sample_recovery_finished(statesref, &source_id);
            let source = self.sequenced_states.peek_mut(&source_id).unwrap();
            source.periodic_task =
                RepliesHandler::spawn_periodic(statesref, self.period, source_id);
        }
        for (_, source) in self.timestamped_states.iter_mut() {
            source.flush(self.callback.as_ref());
        }
    }

    /// Release timestamp-ordered history only after global history is finished.
    /// The source also waits for its own remaining history queries in `flush`.
    pub(super) fn on_timestamped_query_finished(&mut self, id: &ID) {
        if self.global_pending_queries == 0 {
            if let Some(source) = self.timestamped_states.peek_mut(id) {
                source.flush(self.callback.as_ref());
            }
        }
    }

    /// Authorize delivery past unseen sequence numbers covered by a completed
    /// history or whole-sample recovery query. Missing fragments remain eligible
    /// for recovery, including fragments received only through query replies.
    pub(super) fn on_history_or_sample_recovery_finished(
        &mut self,
        statesref: &Arc<Mutex<State>>,
        source_id: &EntityGlobalId,
    ) {
        if self.global_pending_queries != 0 {
            return;
        }
        let Some(callback) = self.callback.as_ref() else {
            return;
        };
        let Some(source) = self.sequenced_states.peek_mut(source_id) else {
            return;
        };
        source.authorize_gap_skip();
        source.drain_authorized_samples(
            callback,
            &self.miss_handlers,
            *source_id,
            self.retransmission,
        );
        // These fragments may have arrived only through query replies. Start
        // the recovery timer here in case no publication started it earlier.
        if self.retransmission
            && source
                .pending_samples
                .values()
                .any(PendingSample::is_incomplete)
        {
            source.arm_fragment_recovery(statesref, *source_id, self.frag_recovery_delay);
        }
    }

    /// Apply fragment completion to delivery after its reservation is released.
    /// This can consume existing gap-skip authority, but cannot authorize skipping
    /// wholly unseen samples. Return whether to recheck for such a gap after unlocking.
    pub(super) fn on_fragment_recovery_finished(&mut self, source_id: EntityGlobalId) -> bool {
        let Some(source) = self.sequenced_states.peek_mut(&source_id) else {
            return false;
        };
        if self.global_pending_queries == 0 {
            source.abandon_failed_head();
        }
        let last_query = source.pending_queries == 0;
        if last_query && self.global_pending_queries == 0 {
            if let Some(callback) = self.callback.as_ref() {
                source.drain_authorized_samples(
                    callback,
                    &self.miss_handlers,
                    source_id,
                    self.retransmission,
                );
                if let Some((sn, sample)) = source.advance_ready() {
                    source.deliver_and_drain(sample, sn, callback, &self.miss_handlers, source_id);
                }
            }
        }
        last_query
    }
}

impl RepliesHandler {
    /// Check for missing samples and reserve the query under the same lock, so
    /// two callers cannot start the same query. Send it after unlocking.
    pub(super) fn recover_gap(statesref: &Arc<Mutex<State>>, source_id: EntityGlobalId) {
        let (context, start) = {
            let mut states = zlock!(statesref);
            if !states.retransmission || states.callback.is_none() {
                return;
            }
            // Use peek_mut: this check must not recreate a removed source or
            // make an idle source look recently used.
            let Some(source) = states.sequenced_states.peek_mut(&source_id) else {
                return;
            };
            if source.last_delivered.is_none() || source.pending_queries != 0 {
                return;
            }
            let start = source.next_expected().unwrap();
            // If we already have some fragments of the next sample, use
            // fragment recovery rather than a sample-recovery query.
            if source
                .pending_samples
                .iter()
                .find(|(_, s)| !s.is_abandoned())
                .map_or(true, |(sn, _)| *sn <= start)
            {
                return;
            }
            reserve_queries(&mut source.pending_queries, 1);
            (QueryContext::new(&states), start)
        };
        Self {
            scope: QueryScope::Sequenced(source_id),
            statesref: statesref.clone(),
        }
        .send_query(
            &context,
            Selector::from((
                context.source_key(source_id),
                format_range("_sn", Some(start), None),
            )),
        );
    }

    /// Defer the post-completion gap check until the state lock is released.
    /// A locally answered query can finish synchronously, so inline dispatch
    /// from completion could otherwise recursively complete another query.
    pub(super) fn schedule_gap_recheck(statesref: &Arc<Mutex<State>>, source_id: EntityGlobalId) {
        let statesref = Arc::downgrade(statesref);
        ZRuntime::Application.spawn(async move {
            if let Some(statesref) = statesref.upgrade() {
                // Re-evaluate the current source before reserving any query.
                Self::recover_gap(&statesref, source_id);
            }
        });
    }

    /// Query beyond the delivery baseline on a periodic tick unless global
    /// history, a source query, or a fragment recovery attempt is still running.
    pub(super) fn periodic(statesref: &Arc<Mutex<State>>, source_id: EntityGlobalId) {
        let (context, start) = {
            let mut states = zlock!(statesref);
            if states.global_pending_queries != 0 {
                return;
            }
            let Some(source) = states.sequenced_states.peek_mut(&source_id) else {
                return;
            };
            // Skip this tick if a query or fragment recovery attempt is running.
            if source.pending_queries != 0 {
                return;
            }
            reserve_queries(&mut source.pending_queries, 1);
            let start = source.last_delivered.map(|sn| sn + 1);
            (QueryContext::new(&states), start)
        };
        Self {
            scope: QueryScope::Sequenced(source_id),
            statesref: statesref.clone(),
        }
        .send_query(
            &context,
            Selector::from((
                context.source_key(source_id),
                format_range("_sn", start, None),
            )),
        );
    }

    pub(super) fn spawn_periodic(
        statesref: &Arc<Mutex<State>>,
        period: Option<Duration>,
        source_id: EntityGlobalId,
    ) -> Option<AbortOnDropHandle<()>> {
        let period = period?;
        let statesref = statesref.clone();
        Some(AbortOnDropHandle::new(ZRuntime::Application.spawn(
            async move {
                let mut interval = tokio::time::interval(period);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    Self::periodic(&statesref, source_id);
                }
            },
        )))
    }
}

impl SequencedSource {
    /// Start one timer to check all samples from this source for missing fragments.
    /// Each tick selects queries and reserves attempts under the lock, then sends
    /// the queries after unlocking, without awaiting in between. The captured
    /// source generation prevents this timer from acting on a recreated source.
    pub(super) fn arm_fragment_recovery(
        &mut self,
        statesref: &Arc<Mutex<State>>,
        source_id: EntityGlobalId,
        delay: Duration,
    ) {
        if self
            .frag_recovery_task
            .as_ref()
            .is_some_and(|h| !h.is_finished())
        {
            return;
        }
        let statesref = statesref.clone();
        let generation = self.generation.clone();
        self.frag_recovery_task = Some(AbortOnDropHandle::new(ZRuntime::Application.spawn(
            async move {
                let mut interval = tokio::time::interval(delay);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let (requests, context) = {
                        let mut states = zlock!(statesref);
                        let Some(source) = states.sequenced_states.peek_mut(&source_id) else {
                            return;
                        };
                        if !Arc::ptr_eq(&source.generation, &generation) {
                            return;
                        }
                        if !source
                            .pending_samples
                            .values()
                            .any(PendingSample::is_incomplete)
                        {
                            return;
                        }
                        let requests = source.prepare_fragment_scan(delay);
                        if requests.is_empty() {
                            continue;
                        }
                        (requests, QueryContext::new(&states))
                    };
                    context.issue_fragments(&statesref, source_id, &generation, requests);
                }
            },
        )));
    }

    /// React to a published fragment after inserting it under the state lock.
    /// A new buffered slot arms the source timer. Arrival can immediately query
    /// holes before the highest received fragment; a missing tail waits for a
    /// timer tick. Retries wait for other source queries to finish as well as
    /// the sample's retry cooldown.
    ///
    /// A returned request reserves one attempt regardless of its range count.
    /// After unlocking, pass it and this source's generation to `QueryContext`
    /// before scheduling a task or awaiting.
    pub(super) fn on_fragment(
        &mut self,
        statesref: &Arc<Mutex<State>>,
        source_id: EntityGlobalId,
        sn: WrappingSn,
        new_slot: bool,
        delay: Duration,
    ) -> Option<FragmentRequest> {
        if new_slot {
            self.arm_fragment_recovery(statesref, source_id, delay);
        }
        let sample = self.pending_samples.get_mut(&sn)?;
        if self.pending_queries != 0 && sample.is_retry() {
            return None;
        }
        let (ranges, attempt) = sample.prepare_recovery(delay, RecoveryTrigger::FragmentArrival)?;
        reserve_queries(&mut self.pending_queries, 1);
        Some(FragmentRequest {
            sn,
            ranges,
            attempt,
        })
    }

    /// Select fragment attempts on a timer tick while holding the state lock.
    /// Timer scans include a missing tail only once fragment progress has
    /// stalled for `delay`. Each sample still enforces its own attempt identity
    /// and retry cooldown. Queries already pending before the scan defer retries;
    /// attempts selected during this scan do not block one another.
    ///
    /// Reserve one count per selected sample, then dispatch all requests after
    /// unlocking, without awaiting between selection and ownership by an attempt.
    pub(super) fn prepare_fragment_scan(&mut self, delay: Duration) -> Vec<FragmentRequest> {
        let mut requests = Vec::new();
        // Check once, so queries selected by this tick do not prevent other
        // samples from being retried in the same tick.
        let queries_pending = self.pending_queries != 0;
        for (&sn, sample) in &mut self.pending_samples {
            // Let running queries finish and deliver buffered samples before retrying.
            if queries_pending && sample.is_retry() {
                continue;
            }
            if let Some((ranges, attempt)) =
                sample.prepare_recovery(delay, RecoveryTrigger::TimerTick)
            {
                requests.push(FragmentRequest {
                    sn,
                    ranges,
                    attempt,
                });
            }
        }
        reserve_queries(&mut self.pending_queries, requests.len());
        requests
    }
}
