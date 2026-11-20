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

//! Subscriber callbacks and their runtime wiring. Recovery decisions and timer
//! loops live in `recovery`; query transport and lifetimes live in `queries`.

use std::{
    cmp::min,
    hash::Hash,
    str::FromStr,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use lru::LruCache;
use uhlc::ID;
use zenoh::{
    config::ZenohId,
    handlers::CallbackDrop,
    internal::zlock,
    key_expr::KeyExpr,
    pubsub::Subscriber,
    query::Selector,
    sample::{Locality, Sample, SampleKind},
    session::{EntityGlobalId, EntityId},
    Result as ZResult, Session, Wait, KE_ADV_PREFIX, KE_EMPTY, KE_PUB, KE_STARSTAR, KE_SUB,
};

use super::{
    handle_sample,
    queries::{reserve_queries, QueryContext, QueryScope, RepliesHandler},
    HistoryConfig, State,
};
use crate::{
    advanced_cache::{ke_liveliness, KE_UHLC},
    advanced_query::format_range,
    utils::WrappingSn,
    z_deserialize,
};

pub(super) fn declare_data_subscriber(
    session: &Session,
    key_expr: &KeyExpr<'_>,
    origin: Locality,
    statesref: &Arc<Mutex<State>>,
) -> ZResult<Subscriber<()>> {
    let publications = statesref.clone();
    let undeclared = statesref.clone();
    session
        .declare_subscriber(key_expr)
        .with(CallbackDrop {
            callback: move |sample| on_data_sample(&publications, sample),
            drop: move || on_data_subscriber_undeclare(&undeclared),
        })
        .allowed_origin(origin)
        .wait()
}

fn on_data_sample(statesref: &Arc<Mutex<State>>, sample: Sample) {
    let mut lock = zlock!(statesref);
    let states = &mut *lock;
    let source_info = sample.source_info().cloned();
    let is_fragmented = sample.frag_info().is_some();
    let inserted = handle_sample(states, sample);
    let Some(info) = source_info else { return };
    let source_id = *info.source_id();
    // Arrival processing already recorded valid activity. Recovery bookkeeping
    // must not promote ignored or malformed input with an unchanged access time.
    let Some(source) = states.sequenced_states.peek_mut(&source_id) else {
        return;
    };
    let request = if is_fragmented && states.retransmission {
        source.on_fragment(
            statesref,
            source_id,
            info.source_sn().into(),
            inserted.new_fragment_slot,
            states.frag_recovery_delay,
        )
    } else {
        None
    };
    if inserted.new_source {
        source.periodic_task = RepliesHandler::spawn_periodic(statesref, states.period, source_id);
    }
    let needs_gap_check = !source.pending_samples.is_empty();
    let work = request.map(|request| (request, source.generation.clone()));
    let work = work.map(|(request, generation)| (request, generation, QueryContext::new(states)));
    drop(lock);
    if let Some((request, generation, context)) = work {
        context.spawn_fragment_query(statesref, source_id, &generation, request);
    }
    if needs_gap_check {
        RepliesHandler::recover_gap(statesref, source_id);
    }
}

/// Closing the underlying subscriber must drop the callback to close the receiver.
fn on_data_subscriber_undeclare(statesref: &Mutex<State>) {
    let mut states = statesref.lock().unwrap();
    states.callback.take();
    states.miss_handlers.clear();
}

pub(super) fn send_initial_history_query(
    statesref: &Arc<Mutex<State>>,
    key_expr: &KeyExpr<'_>,
    history: &HistoryConfig,
) {
    let handler = RepliesHandler {
        scope: QueryScope::Global,
        statesref: statesref.clone(),
    };
    let context = QueryContext::new(&zlock!(statesref));
    handler.send_query(
        &context,
        Selector::from((key_expr / KE_ADV_PREFIX / KE_STARSTAR, history.parameters())),
    );
}

pub(super) fn declare_source_liveliness_subscriber(
    session: &Session,
    key_expr: &KeyExpr<'_>,
    statesref: &Arc<Mutex<State>>,
    history: &HistoryConfig,
) -> ZResult<Subscriber<()>> {
    let context = QueryContext::new(&zlock!(statesref));
    let statesref = statesref.clone();
    let history = history.clone();
    tracing::debug!(
        "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers on {}",
        key_expr,
        key_expr / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR
    );
    session
        .liveliness()
        .declare_subscriber(key_expr / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR)
        .history(true)
        .callback(move |sample| on_source_liveliness(&statesref, &context, &history, sample))
        .wait()
}

fn on_source_liveliness(
    statesref: &Arc<Mutex<State>>,
    context: &QueryContext,
    history: &HistoryConfig,
    sample: Sample,
) {
    let Ok(parsed) = ke_liveliness::parse(sample.key_expr().as_keyexpr()) else {
        tracing::warn!(
            "AdvancedSubscriber{{}}: Received malformed liveliness token key expression: {}",
            sample.key_expr()
        );
        return;
    };
    // TODO: For an already known source, query only the appropriate range.
    if let Ok(zid) = ZenohId::from_str(parsed.zid().as_str()) {
        if parsed.eid() == KE_UHLC {
            on_timestamped_source_liveliness(statesref, context, history, &sample, zid);
        } else if let Ok(eid) = EntityId::from_str(parsed.eid().as_str()) {
            on_sequenced_source_liveliness(
                statesref,
                context,
                history,
                &sample,
                EntityGlobalId::new(zid, eid),
            );
        }
    } else if sample.kind() == SampleKind::Put {
        let mut states = zlock!(statesref);
        tracing::trace!(
            "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers with zid={}",
            states.key_expr,
            parsed.zid().as_str()
        );
        reserve_queries(&mut states.global_pending_queries, 1);
        drop(states);
        RepliesHandler {
            scope: QueryScope::Global,
            statesref: statesref.clone(),
        }
        .send_query(
            context,
            Selector::from((sample.key_expr(), history.parameters())),
        );
    }
}

fn on_timestamped_source_liveliness(
    statesref: &Arc<Mutex<State>>,
    context: &QueryContext,
    history: &HistoryConfig,
    sample: &Sample,
    zid: ZenohId,
) {
    let mut states = zlock!(statesref);
    if sample.kind() == SampleKind::Delete {
        tracing::trace!(
            "AdvancedSubscriber{{key_expr: {}}}: Liveliness loss for publishers with zid={}",
            states.key_expr,
            zid
        );
        if let Some(source) = states.timestamped_states.peek_mut(&ID::from(zid)) {
            source.alive = false;
        }
        return;
    }
    tracing::trace!(
        "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers with zid={}",
        states.key_expr,
        zid
    );
    let source = states
        .timestamped_states
        .get_or_insert_mut(ID::from(zid), Default::default);
    reserve_queries(&mut source.pending_queries, 1);
    source.alive = true;
    source.latest_access = Instant::now();
    drop(states);
    RepliesHandler {
        scope: QueryScope::Timestamped(ID::from(zid)),
        statesref: statesref.clone(),
    }
    .send_query(
        context,
        Selector::from((sample.key_expr(), history.parameters())),
    );
}

fn on_sequenced_source_liveliness(
    statesref: &Arc<Mutex<State>>,
    context: &QueryContext,
    history: &HistoryConfig,
    sample: &Sample,
    source_id: EntityGlobalId,
) {
    let mut lock = zlock!(statesref);
    let states = &mut *lock;
    if sample.kind() == SampleKind::Delete {
        tracing::trace!(
            "AdvancedSubscriber{{key_expr: {}}}: Liveliness loss for publishers with zid={}",
            states.key_expr,
            source_id.zid()
        );
        if let Some(source) = states.sequenced_states.peek_mut(&source_id) {
            source.alive = false;
        }
        return;
    }
    tracing::trace!(
        "AdvancedSubscriber{{key_expr: {}}}: Detect late joiner publishers with zid={}",
        states.key_expr,
        source_id.zid()
    );
    let mut new = false;
    let source = states.sequenced_states.get_or_insert_mut(source_id, || {
        new = true;
        Default::default()
    });
    if new {
        source.periodic_task = RepliesHandler::spawn_periodic(statesref, states.period, source_id);
    }
    reserve_queries(&mut source.pending_queries, 1);
    source.alive = true;
    source.latest_access = Instant::now();
    drop(lock);
    RepliesHandler {
        scope: QueryScope::Sequenced(source_id),
        statesref: statesref.clone(),
    }
    .send_query(
        context,
        Selector::from((sample.key_expr(), history.parameters())),
    );
}

pub(super) fn declare_source_heartbeat_subscriber(
    session: &Session,
    key_expr: &KeyExpr<'_>,
    origin: Locality,
    statesref: &Arc<Mutex<State>>,
) -> ZResult<Subscriber<()>> {
    let heartbeat_key = key_expr / KE_ADV_PREFIX / KE_PUB / KE_STARSTAR;
    let statesref = statesref.clone();
    tracing::debug!(
        "AdvancedSubscriber{{key_expr: {}}}: Enable heartbeat subscriber on {}",
        key_expr,
        heartbeat_key
    );
    session
        .declare_subscriber(heartbeat_key)
        .callback(move |sample| on_source_heartbeat(&statesref, sample))
        .allowed_origin(origin)
        .wait()
}

fn on_source_heartbeat(statesref: &Arc<Mutex<State>>, sample: Sample) {
    if sample.kind() != SampleKind::Put {
        return;
    }
    let heartbeat_key = sample.key_expr().as_keyexpr();
    let Ok(parsed) = ke_liveliness::parse(heartbeat_key) else {
        return;
    };
    let Ok(zid) = ZenohId::from_str(parsed.zid().as_str()) else {
        return;
    };
    let Ok(eid) = EntityId::from_str(parsed.eid().as_str()) else {
        return;
    };
    let source_id = EntityGlobalId::new(zid, eid);
    let Ok(heartbeat_sn) = z_deserialize::<WrappingSn>(sample.payload()) else {
        tracing::debug!(
            "AdvancedSubscriber{{}}: Skipping invalid heartbeat payload on '{}'",
            heartbeat_key
        );
        return;
    };

    let mut lock = zlock!(statesref);
    let states = &mut *lock;
    let mut new = false;
    let source = states.sequenced_states.get_or_insert_mut(source_id, || {
        new = true;
        Default::default()
    });
    source.latest_access = Instant::now();
    if new {
        // NOTE: API does not allow both heartbeat and periodic_queries.
        source.periodic_task = RepliesHandler::spawn_periodic(statesref, states.period, source_id);
        if states.global_pending_queries > 0 {
            tracing::trace!("AdvancedSubscriber{{key_expr: {}}}: Skipping heartbeat on '{}' from publisher that is currently being pulled by global query", states.key_expr, heartbeat_key);
            return;
        }
    }
    if (source.last_delivered.is_none()
        || source.last_delivered.is_some_and(|sn| heartbeat_sn > sn))
        && source.pending_queries == 0
    {
        let range = format_range(
            "_sn",
            source.last_delivered.map(|sn| sn + 1),
            Some(heartbeat_sn),
        );
        reserve_queries(&mut source.pending_queries, 1);
        let context = QueryContext::new(states);
        drop(lock);
        RepliesHandler {
            scope: QueryScope::Sequenced(source_id),
            statesref: statesref.clone(),
        }
        .send_query(&context, Selector::from((heartbeat_key, range)));
    }
}

pub(super) fn declare_subscriber_liveliness_token(
    session: &Session,
    key_expr: &KeyExpr<'_>,
    subscriber: &Subscriber<()>,
    statesref: &Mutex<State>,
    meta: Option<KeyExpr<'_>>,
) -> ZResult<()> {
    let suffix = KE_ADV_PREFIX
        / KE_SUB
        / &subscriber.id().zid().into_keyexpr()
        / &KeyExpr::try_from(subscriber.id().eid().to_string()).unwrap();
    let suffix = match meta {
        Some(meta) => suffix / &meta,
        // We need this empty chunk because of a routing matching bug.
        _ => suffix / KE_EMPTY,
    };
    tracing::debug!(
        "AdvancedSubscriber{{key_expr: {}}}: Declare liveliness token {}",
        key_expr,
        key_expr / &suffix
    );
    let token = session
        .liveliness()
        .declare_token(key_expr / &suffix)
        .wait()?;
    zlock!(statesref).subscriber_liveliness_token = Some(token);
    Ok(())
}

/// Reclamation is based on last access; alive publishers are not reclaimed.
pub(super) async fn gc_task(statesref: Weak<Mutex<State>>, retention_period: Duration) {
    tokio::time::sleep(retention_period).await;
    loop {
        let oldest_access = {
            let Some(states) = statesref.upgrade() else {
                // Abortion may race scheduling; new_cyclic may not have returned yet.
                tokio::time::sleep(retention_period).await;
                continue;
            };
            let mut states = states.lock().unwrap();
            let now = Instant::now();
            min(
                garbage_collect(&mut states.sequenced_states, retention_period, now, |s| {
                    (&mut s.latest_access, s.alive)
                }),
                garbage_collect(&mut states.timestamped_states, retention_period, now, |s| {
                    (&mut s.latest_access, s.alive)
                }),
            )
        };
        tokio::time::sleep_until((oldest_access + retention_period).into()).await;
    }
}

/// Garbage collect a source list and return its oldest access.
fn garbage_collect<K: Copy + Eq + Hash, S>(
    states: &mut LruCache<K, S>,
    retention_period: Duration,
    now: Instant,
    retention: impl Fn(&mut S) -> (&mut Instant, bool),
) -> Instant {
    while let Some((&key, state)) = states.iter_mut().next_back() {
        let (latest_access, alive) = retention(state);
        if now.duration_since(*latest_access) <= retention_period {
            return *latest_access;
        } else if alive {
            // Refreshing an alive source also moves it in the LRU list.
            *retention(states.get_mut(&key).unwrap()).0 = now;
        } else {
            states.pop_lru();
        }
    }
    now
}

#[cfg(test)]
#[path = "tests/retention.rs"]
mod retention_tests;
