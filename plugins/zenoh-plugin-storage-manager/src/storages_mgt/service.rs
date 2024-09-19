//
// Copyright (c) 2023 ZettaScale Technology
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
    collections::{HashMap, HashSet},
    str::{self, FromStr},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use flume::Receiver;
use futures::select;
use tokio::sync::{Mutex, RwLock};
use zenoh::{
    internal::{
        buffers::{SplitBuffer, ZBuf},
        zenoh_home, Timed, TimedEvent, Timer, Value,
    },
    key_expr::{
        keyexpr_tree::{
            IKeyExprTree, IKeyExprTreeMut, KeBoxTree, KeyedSetProvider, NonWild, UnknownWildness,
        },
        KeyExpr, OwnedKeyExpr,
    },
    sample::{Sample, SampleBuilder, SampleKind},
    session::Session,
    time::{Timestamp, NTP64},
};
use zenoh_backend_traits::{
    config::{GarbageCollectionConfig, StorageConfig},
    Capability, History, Persistence, StorageInsertionResult, StoredData,
};

use crate::storages_mgt::StorageMessage;

pub const WILDCARD_UPDATES_FILENAME: &str = "wildcard_updates";
pub const TOMBSTONE_FILENAME: &str = "tombstones";

#[derive(Clone)]
struct Update {
    kind: SampleKind,
    data: StoredData,
}

pub struct StorageService {
    session: Arc<Session>,
    key_expr: OwnedKeyExpr,
    complete: bool,
    name: String,
    strip_prefix: Option<OwnedKeyExpr>,
    storage: Arc<Mutex<Box<dyn zenoh_backend_traits::Storage>>>,
    capability: Capability,
    tombstones: Arc<RwLock<KeBoxTree<Timestamp, NonWild, KeyedSetProvider>>>,
    wildcard_updates: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
    latest_updates: Arc<Mutex<HashMap<Option<OwnedKeyExpr>, Timestamp>>>,
}

impl StorageService {
    pub async fn start(
        session: Arc<Session>,
        config: StorageConfig,
        name: &str,
        storage: Arc<Mutex<Box<dyn zenoh_backend_traits::Storage>>>,
        capability: Capability,
        rx: Receiver<StorageMessage>,
    ) {
        // @TODO: optimization: if read_cost is high for the storage, initialize a cache for the
        // latest value
        let mut storage_service = StorageService {
            session,
            key_expr: config.key_expr,
            complete: config.complete,
            name: name.to_string(),
            strip_prefix: config.strip_prefix,
            storage,
            capability,
            tombstones: Arc::new(RwLock::new(KeBoxTree::default())),
            wildcard_updates: Arc::new(RwLock::new(KeBoxTree::default())),
            latest_updates: Arc::new(Mutex::new(HashMap::new())),
        };
        if storage_service
            .capability
            .persistence
            .eq(&Persistence::Durable)
        {
            // update tombstones and wild card updates from persisted file if it exists
            if zenoh_home().join(TOMBSTONE_FILENAME).exists() {
                let saved_ts =
                    std::fs::read_to_string(zenoh_home().join(TOMBSTONE_FILENAME)).unwrap();
                let saved_ts: HashMap<OwnedKeyExpr, Timestamp> =
                    serde_json::from_str(&saved_ts).unwrap();
                let mut tombstones = storage_service.tombstones.write().await;
                for (k, ts) in saved_ts {
                    tombstones.insert(&k, ts);
                }
            }
            if zenoh_home().join(WILDCARD_UPDATES_FILENAME).exists() {
                let saved_wc =
                    std::fs::read_to_string(zenoh_home().join(WILDCARD_UPDATES_FILENAME)).unwrap();
                let saved_wc: HashMap<OwnedKeyExpr, String> =
                    serde_json::from_str(&saved_wc).unwrap();
                let mut wildcard_updates = storage_service.wildcard_updates.write().await;
                for (k, data) in saved_wc {
                    wildcard_updates.insert(&k, construct_update(data));
                }
            }
        }
        storage_service
            .start_storage_queryable_subscriber(rx, config.garbage_collection_config)
            .await
    }

    async fn start_storage_queryable_subscriber(
        &mut self,
        rx: Receiver<StorageMessage>,
        gc_config: GarbageCollectionConfig,
    ) {
        // start periodic GC event
        let t = Timer::default();
        let gc = TimedEvent::periodic(
            gc_config.period,
            GarbageCollectionEvent {
                config: gc_config,
                tombstones: self.tombstones.clone(),
                wildcard_updates: self.wildcard_updates.clone(),
                latest_updates: self.latest_updates.clone(),
            },
        );
        t.add_async(gc).await;

        // subscribe on key_expr
        let storage_sub = match self.session.declare_subscriber(&self.key_expr).await {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                tracing::error!("Error starting storage '{}': {}", self.name, e);
                return;
            }
        };

        // answer to queries on key_expr
        let storage_queryable = match self
            .session
            .declare_queryable(&self.key_expr)
            .complete(self.complete)
            .await
        {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                tracing::error!("Error starting storage '{}': {}", self.name, e);
                return;
            }
        };

        loop {
            select!(
                // on sample for key_expr
                sample = storage_sub.recv_async() => {
                    let sample = match sample {
                        Ok(sample) => sample,
                        Err(e) => {
                            tracing::error!("Error in sample: {}", e);
                            continue;
                        }
                    };
                    let timestamp = sample.timestamp().cloned().unwrap_or(self.session.new_timestamp());
                    let sample = SampleBuilder::from(sample).timestamp(timestamp).into();
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
                            tracing::trace!("Dropping storage '{}'", self.name);
                            return
                        },
                        Ok(StorageMessage::GetStatus(tx)) => {
                            let storage = self.storage.lock().await;
                            std::mem::drop(tx.send(storage.get_admin_status()).await);
                            drop(storage);
                        }
                        Err(e) => {
                            tracing::error!("Storage Message Channel Error: {}", e);
                        },
                    };
                },
            );
        }
    }

    // The storage should only simply save the key, sample pair while put and retrieve the same
    // during get the trimming during PUT and GET should be handled by the plugin
    pub(crate) async fn process_sample(&self, sample: Sample) {
        tracing::trace!("[STORAGE] Processing sample: {:?}", sample);

        // A Sample, in theory, will not arrive to a Storage without a Timestamp. This check (which,
        // again, should never enter the `None` branch) ensures that the Storage Manager
        // does not panic even if it ever happens.
        let sample_timestamp = match sample.timestamp() {
            Some(timestamp) => timestamp,
            None => {
                tracing::error!("Discarding Sample that has no Timestamp: {:?}", sample);
                return;
            }
        };

        // if wildcard, update wildcard_updates
        if sample.key_expr().is_wild() {
            self.register_wildcard_update(sample.clone()).await;
        }

        let matching_keys = if sample.key_expr().is_wild() {
            self.get_matching_keys(sample.key_expr()).await
        } else {
            vec![sample.key_expr().clone().into()]
        };
        tracing::trace!(
            "The list of keys matching `{}` is : {:?}",
            sample.key_expr(),
            matching_keys
        );

        for k in matching_keys {
            if !self.is_deleted(&k.clone(), sample_timestamp).await
                && (self.capability.history.eq(&History::All)
                    || (self.capability.history.eq(&History::Latest)
                        && self.is_latest(&k, sample_timestamp).await))
            {
                tracing::trace!(
                    "Sample `{:?}` identified as needed processing for key {}",
                    sample,
                    k
                );
                // there might be the case that the actual update was outdated due to a wild card
                // update, but not stored yet in the storage. get the relevant wild
                // card entry and use that value and timestamp to update the storage
                let sample_to_store: Sample =
                    if let Some(update) = self.overriding_wild_update(&k, sample_timestamp).await {
                        match update.kind {
                            SampleKind::Put => {
                                SampleBuilder::put(k.clone(), update.data.value.payload().clone())
                                    .encoding(update.data.value.encoding().clone())
                                    .timestamp(update.data.timestamp)
                                    .into()
                            }
                            SampleKind::Delete => SampleBuilder::delete(k.clone())
                                .timestamp(update.data.timestamp)
                                .into(),
                        }
                    } else {
                        SampleBuilder::from(sample.clone())
                            .keyexpr(k.clone())
                            .into()
                    };

                // A Sample that is to be stored **must** have a Timestamp. In theory, the Sample
                // generated should have a Timestamp and, in theory, this check is
                // unneeded.
                let sample_to_store_timestamp = match sample_to_store.timestamp() {
                    Some(timestamp) => *timestamp,
                    None => {
                        tracing::error!(
                            "Discarding `Sample` generated through `SampleBuilder` that has no \
                             Timestamp: {:?}",
                            sample_to_store
                        );
                        continue;
                    }
                };

                let stripped_key = match crate::strip_prefix(
                    self.strip_prefix.as_ref(),
                    sample_to_store.key_expr(),
                ) {
                    Ok(stripped) => stripped,
                    Err(e) => {
                        tracing::error!("{}", e);
                        return;
                    }
                };

                // If the Storage was declared as only keeping the Latest value, we ensure that, for
                // each received Sample, it is indeed the Latest value that is processed.
                let mut latest_updates_guard = self.latest_updates.lock().await;
                if self.capability.history == History::Latest {
                    if let Some(stored_timestamp) = latest_updates_guard.get(&stripped_key) {
                        if sample_to_store_timestamp < *stored_timestamp {
                            tracing::debug!(
                                "Skipping Sample for < {:?} >, a Value with a more recent \
                                 Timestamp is stored: (received) {} vs (stored) {}",
                                stripped_key,
                                sample_to_store_timestamp,
                                stored_timestamp
                            );
                            continue;
                        }
                    }
                }

                let mut storage = self.storage.lock().await;
                let storage_result = match sample.kind() {
                    SampleKind::Put => {
                        storage
                            .put(
                                stripped_key.clone(),
                                Value::new(
                                    sample_to_store.payload().clone(),
                                    sample_to_store.encoding().clone(),
                                ),
                                sample_to_store_timestamp,
                            )
                            .await
                    }
                    SampleKind::Delete => {
                        // register a tombstone
                        self.mark_tombstone(&k, sample_to_store_timestamp).await;
                        storage
                            .delete(stripped_key.clone(), sample_to_store_timestamp)
                            .await
                    }
                };

                drop(storage);

                match storage_result {
                    Ok(StorageInsertionResult::Outdated) => {
                        tracing::trace!(
                            "Ignoring `Outdated` sample < {} >",
                            sample_to_store.key_expr()
                        );
                    }
                    Ok(_) => {
                        if self.capability.history == History::Latest {
                            latest_updates_guard.insert(stripped_key, sample_to_store_timestamp);
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "`{}` on < {} > failed with: {e:?}",
                            sample.kind(),
                            sample_to_store.key_expr()
                        );
                    }
                }
            }
        }
    }

    async fn mark_tombstone(&self, key_expr: &OwnedKeyExpr, timestamp: Timestamp) {
        // @TODO: change into a better store that does incremental writes
        let mut tombstones = self.tombstones.write().await;
        tombstones.insert(key_expr, timestamp);
        if self.capability.persistence.eq(&Persistence::Durable) {
            // flush to disk to make it durable
            let mut serialized_data = HashMap::new();
            for (k, ts) in tombstones.key_value_pairs() {
                serialized_data.insert(k, *ts);
            }
            if let Err(e) = std::fs::write(
                zenoh_home().join(TOMBSTONE_FILENAME),
                serde_json::to_string_pretty(&serialized_data).unwrap(),
            ) {
                tracing::error!("Saving tombstones failed: {}", e);
            }
        }
    }

    async fn register_wildcard_update(&self, sample: Sample) {
        // @TODO: change into a better store that does incremental writes
        let key = sample.key_expr().clone();
        let mut wildcards = self.wildcard_updates.write().await;
        let timestamp = *sample.timestamp().unwrap();
        wildcards.insert(
            &key,
            Update {
                kind: sample.kind(),
                data: StoredData {
                    value: Value::from(sample),
                    timestamp,
                },
            },
        );
        if self.capability.persistence.eq(&Persistence::Durable) {
            // flush to disk to make it durable
            let mut serialized_data = HashMap::new();
            for (k, update) in wildcards.key_value_pairs() {
                serialized_data.insert(k, serialize_update(update));
            }
            if let Err(e) = std::fs::write(
                zenoh_home().join(WILDCARD_UPDATES_FILENAME),
                serde_json::to_string_pretty(&serialized_data).unwrap(),
            ) {
                tracing::error!("Saving wildcard updates failed: {}", e);
            }
        }
    }

    async fn is_deleted(&self, key_expr: &OwnedKeyExpr, timestamp: &Timestamp) -> bool {
        // check tombstones to see if it is deleted in the future
        let tombstones = self.tombstones.read().await;
        let weight = tombstones.weight_at(key_expr);
        weight.is_some() && weight.unwrap() > timestamp
    }

    async fn overriding_wild_update(
        &self,
        key_expr: &OwnedKeyExpr,
        timestamp: &Timestamp,
    ) -> Option<Update> {
        // check wild card store for any futuristic update
        let wildcards = self.wildcard_updates.read().await;
        let mut ts = timestamp;
        let mut update = None;
        for node in wildcards.intersecting_keys(key_expr) {
            let weight = wildcards.weight_at(&node);
            if weight.is_some() && weight.unwrap().data.timestamp > *ts {
                // if the key matches a wild card update, check whether it was saved in storage
                // remember that wild card updates change only existing keys
                let stripped_key =
                    match crate::strip_prefix(self.strip_prefix.as_ref(), &key_expr.into()) {
                        Ok(stripped) => stripped,
                        Err(e) => {
                            tracing::error!("{}", e);
                            break;
                        }
                    };
                let mut storage = self.storage.lock().await;
                match storage.get(stripped_key, "").await {
                    Ok(stored_data) => {
                        for entry in stored_data {
                            if entry.timestamp > *ts {
                                return None;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Storage '{}' raised an error fetching a query on key {} : {}",
                            self.name,
                            key_expr,
                            e
                        );
                        ts = &weight.unwrap().data.timestamp;
                        update = Some(weight.unwrap().clone());
                    }
                }
            }
        }
        update
    }

    async fn is_latest(&self, key_expr: &OwnedKeyExpr, timestamp: &Timestamp) -> bool {
        // @TODO: if cache exists, read from there
        let mut storage = self.storage.lock().await;
        let stripped_key = match crate::strip_prefix(self.strip_prefix.as_ref(), &key_expr.into()) {
            Ok(stripped) => stripped,
            Err(e) => {
                tracing::error!("{}", e);
                return false;
            }
        };
        if let Ok(stored_data) = storage.get(stripped_key, "").await {
            for entry in stored_data {
                if entry.timestamp > *timestamp {
                    return false;
                }
            }
        }
        true
    }

    async fn reply_query(&self, query: Result<zenoh::query::Query, flume::RecvError>) {
        let q = match query {
            Ok(q) => q,
            Err(e) => {
                tracing::error!("Error in query: {}", e);
                return;
            }
        };
        tracing::trace!("[STORAGE] Processing query on key_expr: {}", q.key_expr());
        if q.key_expr().is_wild() {
            // resolve key expr into individual keys
            let matching_keys = self.get_matching_keys(q.key_expr()).await;
            let mut storage = self.storage.lock().await;
            for key in matching_keys {
                let stripped_key =
                    match crate::strip_prefix(self.strip_prefix.as_ref(), &key.clone().into()) {
                        Ok(k) => k,
                        Err(e) => {
                            tracing::error!("{}", e);
                            // @TODO: return error when it is supported
                            return;
                        }
                    };
                match storage.get(stripped_key, q.parameters().as_str()).await {
                    Ok(stored_data) => {
                        for entry in stored_data {
                            if let Err(e) = q
                                .reply(key.clone(), entry.value.payload().clone())
                                .encoding(entry.value.encoding().clone())
                                .timestamp(entry.timestamp)
                                .await
                            {
                                tracing::warn!(
                                    "Storage '{}' raised an error replying a query: {}",
                                    self.name,
                                    e
                                )
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Storage'{}' raised an error on query: {}", self.name, e)
                    }
                };
            }
            drop(storage);
        } else {
            let stripped_key = match crate::strip_prefix(self.strip_prefix.as_ref(), q.key_expr()) {
                Ok(k) => k,
                Err(e) => {
                    tracing::error!("{}", e);
                    // @TODO: return error when it is supported
                    return;
                }
            };
            let mut storage = self.storage.lock().await;
            match storage.get(stripped_key, q.parameters().as_str()).await {
                Ok(stored_data) => {
                    for entry in stored_data {
                        if let Err(e) = q
                            .reply(q.key_expr().clone(), entry.value.payload().clone())
                            .encoding(entry.value.encoding().clone())
                            .timestamp(entry.timestamp)
                            .await
                        {
                            tracing::warn!(
                                "Storage '{}' raised an error replying a query: {}",
                                self.name,
                                e
                            )
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Storage '{}' raised an error on query: {e}", self.name);
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
                    // @TODO: optimize adding back the prefix (possible inspiration from https://github.com/eclipse-zenoh/zenoh/blob/0.5.0-beta.9/backends/traits/src/utils.rs#L79)
                    let full_key = match k {
                        Some(key) => crate::prefix(self.strip_prefix.as_ref(), &key),
                        None => self.strip_prefix.clone().unwrap(),
                    };
                    if key_expr.intersects(&full_key.clone()) {
                        result.push(full_key);
                    }
                }
            }
            Err(e) => tracing::warn!(
                "Storage '{}' raised an error while retrieving keys: {}",
                self.name,
                e
            ),
        }
        result
    }
}

fn serialize_update(update: &Update) -> String {
    let Update {
        kind,
        data: StoredData { value, timestamp },
    } = update;
    let zbuf: ZBuf = value.payload().into();

    let result = (
        kind.to_string(),
        timestamp.to_string(),
        value.encoding().to_string(),
        zbuf.slices().collect::<Vec<&[u8]>>(),
    );
    serde_json::to_string_pretty(&result).unwrap()
}

fn construct_update(data: String) -> Update {
    let result: (String, String, String, Vec<&[u8]>) = serde_json::from_str(&data).unwrap(); // @TODO: remove the unwrap()
    let mut payload = ZBuf::default();
    for slice in result.3 {
        payload.push_zslice(slice.to_vec().into());
    }
    let value = Value::new(payload, result.2);
    let data = StoredData {
        value,
        timestamp: Timestamp::from_str(&result.1).unwrap(), // @TODO: remove the unwrap()
    };
    let kind = if result.0.eq(&(SampleKind::Put).to_string()) {
        SampleKind::Put
    } else {
        SampleKind::Delete
    };
    Update { kind, data }
}

// Periodic event cleaning-up data info for old metadata
struct GarbageCollectionEvent {
    config: GarbageCollectionConfig,
    tombstones: Arc<RwLock<KeBoxTree<Timestamp, NonWild, KeyedSetProvider>>>,
    wildcard_updates: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
    latest_updates: Arc<Mutex<HashMap<Option<OwnedKeyExpr>, Timestamp>>>,
}

#[async_trait]
impl Timed for GarbageCollectionEvent {
    async fn run(&mut self) {
        tracing::trace!("Start garbage collection");
        let time_limit = NTP64::from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
            - NTP64::from(self.config.lifespan);

        // Get lock on fields
        let mut tombstones = self.tombstones.write().await;
        let mut wildcard_updates = self.wildcard_updates.write().await;

        let mut to_be_removed = HashSet::new();
        for (k, ts) in tombstones.key_value_pairs() {
            if ts.get_time() < &time_limit {
                // mark key to be removed
                to_be_removed.insert(k);
            }
        }
        for k in to_be_removed {
            tombstones.remove(&k);
        }

        let mut to_be_removed = HashSet::new();
        for (k, update) in wildcard_updates.key_value_pairs() {
            let ts = update.data.timestamp;
            if ts.get_time() < &time_limit {
                // mark key to be removed
                to_be_removed.insert(k);
            }
        }
        for k in to_be_removed {
            wildcard_updates.remove(&k);
        }

        self.latest_updates
            .lock()
            .await
            .retain(|_, timestamp| timestamp.get_time() < &time_limit);

        tracing::trace!("End garbage collection of obsolete data-infos");
    }
}