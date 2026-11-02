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

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zenoh::{
    bytes::ZBytes,
    config::{Config, WhatAmI},
    internal::{runtime::ZRuntime, ztimeout},
    query::{ConsolidationMode, Query, Reply, ReplyKeyExpr, Selector},
    sample::{FragInfo, SampleBuilder, SourceInfo},
    time::Timestamp,
};
use zenoh_config::ModeDependentValue;

use super::{
    queries::{FragRecoveryAttempt, QueryContext, QueryScope, RepliesHandler},
    *,
};
use crate::{
    fragmentation::{FragRange, PendingSample, RecoveryTrigger},
    utils::WrappingSn,
    AdvancedPublisherBuilderExt, AdvancedSubscriberBuilderExt,
};

mod assembly_recovery;
mod delivery_eviction_misses;
mod history_query_barriers;
mod metadata_config;
mod retries_abandonment;

/// 12-byte payload, 3 fragments of 4 bytes with `fragmentation(4)`.
const PAYLOAD: &str = "0123456789AB";
const TIMEOUT: Duration = Duration::from_secs(60);

async fn isolated_session() -> Session {
    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    ztimeout!(zenoh::open(config)).unwrap()
}

fn peer_config() -> Config {
    let mut config = Config::default();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config
}

/// Build recovery state without a running GC timer. Query transport tests also
/// use this fixture through `advanced_subscriber::tests::frag_recovery_state`.
pub(super) fn frag_recovery_state(
    session: &Session,
    key_expr: &KeyExpr<'static>,
    received: Arc<Mutex<Vec<Sample>>>,
) -> Arc<Mutex<State>> {
    Arc::new(Mutex::new(State {
        next_id: 0,
        global_pending_queries: 0,
        sequenced_states: LruCache::unbounded(),
        timestamped_states: LruCache::unbounded(),
        session: session.downgrade(),
        key_expr: key_expr.clone().into_owned(),
        retransmission: true,
        frag_recovery_delay: RecoveryConfig::<true>::default().frag_recovery_delay,
        period: None,
        max_pending_samples: 10,
        query_target: QueryTarget::All,
        query_timeout: Duration::from_secs(10),
        max_fragments: MAX_FRAGMENTS_DEFAULT,
        callback: Some(Callback::from(move |sample: Sample| {
            received.lock().unwrap().push(sample)
        })),
        miss_handlers: HashMap::new(),
        subscriber_liveliness_token: None,
        _gc_task: AbortOnDropHandle::new(ZRuntime::Application.spawn(std::future::pending())),
    }))
}

/// Synchronous source-policy fixture with recorded deliveries and misses.
struct BufferedSource {
    source: SequencedSource,
    callback: Callback<Sample>,
    miss_handlers: HashMap<usize, Callback<Miss>>,
    received: Arc<Mutex<Vec<Sample>>>,
    misses: Arc<Mutex<Vec<u32>>>,
    id: EntityGlobalId,
    key: KeyExpr<'static>,
    history_pending: bool,
    retransmission: bool,
    max_pending_samples: usize,
}

impl BufferedSource {
    fn new(depth: usize) -> Self {
        let received = Arc::new(Mutex::new(Vec::new()));
        let misses = Arc::new(Mutex::new(Vec::new()));
        Self {
            source: SequencedSource::default(),
            callback: Callback::from({
                let received = received.clone();
                move |sample: Sample| received.lock().unwrap().push(sample)
            }),
            miss_handlers: HashMap::from([(
                0,
                Callback::from({
                    let misses = misses.clone();
                    move |miss: Miss| misses.lock().unwrap().push(miss.nb())
                }),
            )]),
            received,
            misses,
            id: EntityGlobalId::new("1".parse().unwrap(), 7),
            key: KeyExpr::try_from("test/ext/pending").unwrap(),
            history_pending: false,
            retransmission: false,
            max_pending_samples: depth,
        }
    }

    fn whole(&self, sn: u32, payload: &str) -> Sample {
        whole(&self.key, self.id, sn, payload)
    }

    fn fragment(&self, sn: u32, num: u32, count: u32, payload: &str) -> Sample {
        fragment(&self.key, self.id, sn, num, count, payload)
    }

    fn receive(&mut self, sample: Sample) {
        let info = sample.source_info().unwrap().clone();
        self.source
            .receive(
                sample,
                &info,
                &ArrivalContext {
                    delivery: DeliveryContext {
                        callback: &self.callback,
                        miss_handlers: &self.miss_handlers,
                        source_id: self.id,
                    },
                    history_pending: self.history_pending,
                    retransmission: self.retransmission,
                    max_pending_samples: self.max_pending_samples,
                    max_fragments: MAX_FRAGMENTS_DEFAULT,
                },
            )
            .unwrap();
    }

    fn delivered(&self) -> Vec<u32> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.source_info().unwrap().source_sn())
            .collect()
    }

    fn payloads(&self) -> Vec<String> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.payload().try_to_string().unwrap().into_owned())
            .collect()
    }
}

fn whole(key: &KeyExpr<'static>, source: EntityGlobalId, sn: u32, payload: &str) -> Sample {
    SampleBuilder::put(key.clone(), payload)
        .source_info(SourceInfo::new(source, sn))
        .into()
}

fn fragment(
    key: &KeyExpr<'static>,
    source: EntityGlobalId,
    sn: u32,
    num: u32,
    count: u32,
    payload: &str,
) -> Sample {
    SampleBuilder::put(key.clone(), payload)
        .source_info(SourceInfo::new(source, sn))
        .frag_info(FragInfo::new(count, num))
        .into()
}

async fn wait_for(mut ready: impl FnMut() -> bool) {
    ztimeout!(async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
}

async fn queries_finished(states: &Mutex<State>, id: EntityGlobalId) {
    wait_for(|| {
        zlock!(states)
            .sequenced_states
            .peek(&id)
            .unwrap()
            .pending_queries
            == 0
    })
    .await;
}

async fn recovery_failed(states: &Mutex<State>, id: EntityGlobalId, sn: u32) {
    wait_for(|| {
        let states = zlock!(states);
        let source = states.sequenced_states.peek(&id).unwrap();
        source.pending_queries == 0
            && source
                .pending_samples
                .get(&WrappingSn(sn))
                .is_some_and(PendingSample::can_abandon)
    })
    .await;
}

/// Buffer and reserve recovery under the lock, dispatch after unlocking, just
/// like the publication callback. No sample-gap query is introduced here.
#[allow(clippy::too_many_arguments)]
fn feed_fragment(
    statesref: &Arc<Mutex<State>>,
    key_expr: &KeyExpr<'static>,
    source_id: EntityGlobalId,
    payload: &str,
    frag_count: u32,
    frag_num: u32,
    sn: u32,
    delay: Duration,
) {
    let mut states = zlock!(statesref);
    let inserted = handle_sample(
        &mut states,
        fragment(key_expr, source_id, sn, frag_num, frag_count, payload),
    );
    let source = states.sequenced_states.get_mut(&source_id).unwrap();
    let request = source.on_fragment(
        statesref,
        source_id,
        sn.into(),
        inserted.new_fragment_slot,
        delay,
    );
    let generation = states
        .sequenced_states
        .peek(&source_id)
        .unwrap()
        .generation
        .clone();
    let context = QueryContext::new(&states);
    drop(states);
    if let Some(request) = request {
        context.spawn_fragment_query(statesref, source_id, &generation, request);
    }
}
