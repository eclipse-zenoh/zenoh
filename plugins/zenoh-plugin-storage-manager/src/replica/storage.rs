//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::backends_mgt::StoreIntercept;
use crate::storages_mgt::StorageMessage;
use async_std::sync::Arc;
use async_std::sync::{Mutex, RwLock};
use flume::{Receiver, Sender};
use futures::select;
use log::{error, info, trace, warn};
use std::str;
use zenoh::key_expr::OwnedKeyExpr;
use zenoh::prelude::r#async::*;
use zenoh::time::Timestamp;
use zenoh::Session;
use zenoh_backend_traits::{Capability, History, StorageInsertionResult};
use zenoh_util::keyexpr_tree::impls::KeyedSetProvider;
use zenoh_util::keyexpr_tree::{
    IKeyExprTreeExt, IKeyExprTreeExtMut, KeBoxTree, NonWild, UnknownWildness,
};

pub struct ReplicationService {
    pub empty_start: bool,
    pub aligner_updates: Receiver<Sample>,
    pub log_propagation: Sender<(OwnedKeyExpr, Timestamp)>,
}

pub struct StorageService {
    session: Arc<Session>,
    key_expr: OwnedKeyExpr,
    complete: bool,
    name: String,
    storage: Mutex<Box<dyn zenoh_backend_traits::Storage>>,
    capability: Capability,
    tombstones: RwLock<KeBoxTree<Timestamp, NonWild, KeyedSetProvider>>,
    wildcard_updates: RwLock<KeBoxTree<Sample, UnknownWildness, KeyedSetProvider>>,
    in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    replication: Option<ReplicationService>,
}

impl StorageService {
    pub async fn start(
        session: Arc<Session>,
        key_expr: OwnedKeyExpr,
        complete: bool,
        name: &str,
        store_intercept: StoreIntercept,
        rx: Receiver<StorageMessage>,
        replication: Option<ReplicationService>,
    ) {
        // @TODO: if storage is persistent, check if tombstones and wildcard updates are already present
        // @TODO: optimization: if read_cost is high for the storage, initialize a cache for the latest value
        let mut storage_service = StorageService {
            session,
            key_expr,
            complete,
            name: name.to_string(),
            storage: Mutex::new(store_intercept.storage),
            capability: store_intercept.capability,
            tombstones: RwLock::new(KeBoxTree::new()),
            wildcard_updates: RwLock::new(KeBoxTree::new()),
            in_interceptor: store_intercept.in_interceptor,
            out_interceptor: store_intercept.out_interceptor,
            replication,
        };
        storage_service.start_storage_queryable_subscriber(rx).await
    }

    async fn start_storage_queryable_subscriber(&mut self, rx: Receiver<StorageMessage>) {
        self.initialize_if_empty().await;

        // subscribe on key_expr
        let storage_sub = match self.session.declare_subscriber(&self.key_expr).res().await {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                error!("Error starting storage {}: {}", self.name, e);
                return;
            }
        };

        // answer to queries on key_expr
        let storage_queryable = match self
            .session
            .declare_queryable(&self.key_expr)
            .complete(self.complete)
            .res()
            .await
        {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                error!("Error starting storage {}: {}", self.name, e);
                return;
            }
        };

        if self.replication.is_some() {
            let aligner_updates = &self.replication.as_ref().unwrap().aligner_updates;
            loop {
                select!(
                    // on sample for key_expr
                    sample = storage_sub.recv_async() => {
                        let sample = match sample {
                            Ok(sample) => sample,
                            Err(e) => {
                                error!("Error in sample: {}", e);
                                continue;
                            }
                        };
                        // log error if the sample is not timestamped
                        // This is to reduce down the line inconsistencies of having duplicate samples stored
                        if sample.get_timestamp().is_none() {
                            error!("Sample {} is not timestamped. Please timestamp samples meant for replicated storage.", sample);
                        }
                        else {
                            self.process_sample(sample).await;
                        }
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        self.reply_query(query).await;
                    },
                    // on aligner update
                    update = aligner_updates.recv_async() => {
                        match update {
                            Ok(sample) => self.process_sample(sample).await,
                            Err(e) => {
                                error!("Error in receiving aligner update: {}", e);
                            }
                        }
                    },
                    // on storage handle drop
                    message = rx.recv_async() => {
                        match message {
                            Ok(StorageMessage::Stop) => {
                                trace!("Dropping storage {}", self.name);
                                return
                            },
                            Ok(StorageMessage::GetStatus(tx)) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                            Err(e) => {
                                error!("Storage Message Channel Error: {}", e);
                            },
                        };
                    }
                );
            }
        } else {
            loop {
                select!(
                    // on sample for key_expr
                    sample = storage_sub.recv_async() => {
                        let mut sample = match sample {
                            Ok(sample) => sample,
                            Err(e) => {
                                error!("Error in sample: {}", e);
                                continue;
                            }
                        };
                        sample.ensure_timestamp();
                        self.process_sample(sample).await;
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        self.reply_query(query).await;
                    },
                    // on storage handle drop
                    message = rx.recv_async() => {
                        match message {
                            Ok(StorageMessage::Stop) => {
                                trace!("Dropping storage {}", self.name);
                                return
                            },
                            Ok(StorageMessage::GetStatus(tx)) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                            Err(e) => {
                                error!("Storage Message Channel Error: {}", e);
                            },
                        };
                    },
                );
            }
        }
    }

    // @TODO: handle the case where there is a strip_prefix specified for the storage
    // The storage should only simply save the key, sample pair while put and retrieve the same during get
    // the trimming during PUT and GET should be handled by the plugin
    async fn process_sample(&self, sample: Sample) {
        trace!("[STORAGE] Processing sample: {}", sample);
        // Call incoming data interceptor (if any)
        let sample = if let Some(ref interceptor) = self.in_interceptor {
            interceptor(sample)
        } else {
            sample
        };

        // if wildcard, update wildcard_updates
        if sample.key_expr.is_wild() {
            self.register_wildcard_update(sample.clone()).await;
        }

        let matching_keys = if sample.key_expr.is_wild() {
            self.get_matching_keys(&sample.key_expr).await
        } else {
            vec![sample.key_expr.clone().into()]
        };
        trace!(
            "The list of keys matching `{}` is : {:?}",
            sample.key_expr,
            matching_keys
        );

        for k in matching_keys {
            if !self
                .is_deleted(&k.clone(), sample.get_timestamp().unwrap())
                .await
                && (self.capability.history.eq(&History::All)
                    || (self.capability.history.eq(&History::Latest)
                        && self.is_latest(&k, sample.get_timestamp().unwrap()).await))
            {
                trace!(
                    "Sample `{}` identified as neded processing for key {}",
                    sample,
                    k
                );
                // there might be the case that the actual update was outdated due to a wild card update, but not stored yet in the storage.
                // get the relevant wild card entry and use that value and timestamp to update the storage
                let sample_to_store = match self
                    .ovderriding_wild_update(&k, sample.get_timestamp().unwrap())
                    .await
                {
                    Some(overriding_sample) => {
                        let mut sample_to_store =
                            Sample::new(KeyExpr::from(k.clone()), overriding_sample.value)
                                .with_timestamp(overriding_sample.timestamp.unwrap());
                        sample_to_store.kind = overriding_sample.kind;
                        sample_to_store
                    }
                    None => {
                        let mut sample_to_store =
                            Sample::new(KeyExpr::from(k.clone()), sample.value.clone())
                                .with_timestamp(sample.timestamp.unwrap());
                        sample_to_store.kind = sample.kind;
                        sample_to_store
                    }
                };

                // @TODO: if strip_prefix present, perform the stripping
                let mut storage = self.storage.lock().await;
                let result = if sample.kind == SampleKind::Put {
                    storage.put(k.clone(), sample_to_store.clone()).await
                } else if sample.kind == SampleKind::Delete {
                    // register a tombstone
                    self.mark_tombstone(&k, sample_to_store.timestamp.unwrap())
                        .await;
                    storage
                        .delete(k.clone(), sample_to_store.timestamp.unwrap())
                        .await
                } else {
                    Err("sample kind not implemented".into())
                };
                if self.replication.is_some()
                    && result.is_ok()
                    && !matches!(result.unwrap(), StorageInsertionResult::Outdated)
                {
                    // @TODO: if strip_prefix present, add back the prefix
                    let sending = self
                        .replication
                        .as_ref()
                        .unwrap()
                        .log_propagation
                        .send((k.clone(), *sample_to_store.get_timestamp().unwrap()));
                    match sending {
                        Ok(_) => (),
                        Err(e) => {
                            error!("Error in sending the sample to the log: {}", e);
                        }
                    }
                }
                drop(storage);
            }
        }
    }

    async fn mark_tombstone(&self, key_expr: &OwnedKeyExpr, timestamp: Timestamp) {
        // @TODO:change into a better store
        let mut tombstones = self.tombstones.write().await;
        tombstones.insert(key_expr, timestamp);
        // @TODO: implement this
        // if self.capability.persistence.eq(&Persistence::Durable) {
        //     // flush to disk to makeit durable
        //     todo!("yet to be implemented");
        // }
        drop(tombstones);
    }

    async fn register_wildcard_update(&self, sample: Sample) {
        // @TODO: change to a better store
        let key = sample.clone().key_expr;
        let mut wildcards = self.wildcard_updates.write().await;
        wildcards.insert(&key, sample);
        // @TODO: implement this
        // if self.capability.persistence.eq(&Persistence::Durable) {
        //     // flush to disk to makeit durable
        //     todo!("yet to be implemented");
        // }
        drop(wildcards);
    }

    async fn is_deleted(&self, key_expr: &OwnedKeyExpr, timestamp: &Timestamp) -> bool {
        // check tombstones to see if it is deleted in the future
        let tombstones = self.tombstones.read().await;
        let weight = tombstones.weight_at(key_expr);
        weight.is_some() && weight.unwrap() > timestamp
    }

    async fn ovderriding_wild_update(
        &self,
        key_expr: &OwnedKeyExpr,
        timestamp: &Timestamp,
    ) -> Option<Sample> {
        // check wild card store for any futuristic update
        let wildcards = self.wildcard_updates.read().await;
        for node in wildcards.intersecting_keys(key_expr) {
            let weight = wildcards.weight_at(&node);
            if weight.is_some() && weight.unwrap().timestamp.unwrap() > *timestamp {
                // if the key matches a wild card update, check whether it was saved in storage
                // remember that wild card updates change only existing keys
                let mut storage = self.storage.lock().await;
                match storage.get(key_expr.clone(), "").await {
                    Ok(stored_samples) => {
                        for stored_sample in stored_samples {
                            if stored_sample.get_timestamp().is_some()
                                && stored_sample.get_timestamp().unwrap() > timestamp
                            {
                                return None;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Storage {} raised an error fetching a query on key {} : {}",
                            self.name, key_expr, e
                        );
                        return Some(weight.unwrap().clone());
                    }
                }
            }
        }
        None
    }

    async fn is_latest(&self, key_expr: &OwnedKeyExpr, timestamp: &Timestamp) -> bool {
        // @TODO: if cache exists, read from there
        let mut storage = self.storage.lock().await;
        if let Ok(samples) = storage.get(key_expr.clone(), "").await {
            for sample in samples {
                if sample.get_timestamp().unwrap() > timestamp {
                    return false;
                }
            }
        }
        true
    }

    async fn reply_query(&self, query: Result<zenoh::queryable::Query, flume::RecvError>) {
        let q = match query {
            Ok(q) => q,
            Err(e) => {
                error!("Error in query: {}", e);
                return;
            }
        };
        if q.key_expr().is_wild() {
            // resolve key expr into individual keys
            let matching_keys = self.get_matching_keys(q.key_expr()).await;
            let mut storage = self.storage.lock().await;
            for key in matching_keys {
                // @TODO: if strip_prefix present, perform the stripping
                match storage.get(key, q.parameters()).await {
                    Ok(samples) => {
                        for sample in samples {
                            // apply outgoing interceptor on results
                            let sample = if let Some(ref interceptor) = self.out_interceptor {
                                interceptor(sample)
                            } else {
                                sample
                            };
                            // @TODO: if strip_prefix present, add back the prefix
                            if let Err(e) = q.reply(Ok(sample)).res().await {
                                warn!(
                                    "Storage {} raised an error replying a query: {}",
                                    self.name, e
                                )
                            }
                        }
                    }
                    Err(e) => warn!("Storage {} raised an error on query: {}", self.name, e),
                };
            }
            drop(storage);
        } else {
            // @TODO: if strip_prefix present, perform the stripping
            let mut storage = self.storage.lock().await;
            match storage
                .get(
                    OwnedKeyExpr::new(q.key_expr().as_str()).unwrap(),
                    q.parameters(),
                )
                .await
            {
                Ok(samples) => {
                    // if key is not available, return Error
                    if samples.is_empty() {
                        info!("Requested key `{}` not found", q.key_expr());
                        if let Err(e) = q.reply(Err("Key not found".into())).res().await {
                            warn!(
                                "Storage {} raised an error replying a query: {}",
                                self.name, e
                            )
                        }
                        return;
                    }
                    for sample in samples {
                        // apply outgoing interceptor on results
                        let sample = if let Some(ref interceptor) = self.out_interceptor {
                            interceptor(sample)
                        } else {
                            sample
                        };
                        // @TODO: if strip_prefix present, add back the prefix
                        if let Err(e) = q.reply(Ok(sample)).res().await {
                            warn!(
                                "Storage {} raised an error replying a query: {}",
                                self.name, e
                            )
                        }
                    }
                }
                Err(e) => {
                    let err_message =
                        format!("Storage {} raised an error on query: {}", self.name, e);
                    warn!("{}", err_message);
                    if let Err(e) = q.reply(Err(err_message.into())).res().await {
                        warn!(
                            "Storage {} raised an error replying a query: {}",
                            self.name, e
                        )
                    }
                }
            };
        }
    }

    async fn get_matching_keys(&self, key_expr: &KeyExpr<'_>) -> Vec<OwnedKeyExpr> {
        let mut result = Vec::new();
        // @TODO: if cache exists, use that to get the list
        let storage = self.storage.lock().await;
        match storage.get_all_entries().await {
            Ok(entries) => {
                for (k, _ts) in entries {
                    if k.intersects(key_expr) {
                        result.push(k);
                    }
                }
            }
            Err(e) => warn!(
                "Storage {} raised an error while retrieving keys: {}",
                self.name, e
            ),
        }
        result
    }

    async fn initialize_if_empty(&mut self) {
        if self.replication.is_some() && self.replication.as_ref().unwrap().empty_start {
            // align with other storages, querying them on key_expr,
            // with `_time=[..]` to get historical data (in case of time-series)
            let replies = match self
                .session
                .get(KeyExpr::from(&self.key_expr).with_parameters("_time=[..]"))
                .target(QueryTarget::All)
                .consolidation(ConsolidationMode::None)
                .res()
                .await
            {
                Ok(replies) => replies,
                Err(e) => {
                    error!("Error aligning storage {}: {}", self.name, e);
                    return;
                }
            };
            while let Ok(reply) = replies.recv_async().await {
                match reply.sample {
                    Ok(sample) => {
                        self.process_sample(sample).await;
                    }
                    Err(e) => warn!(
                        "Storage {} received an error to align query: {}",
                        self.name, e
                    ),
                }
            }
        }
    }
}
