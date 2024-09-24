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
use tokio::sync::{broadcast::Receiver, Mutex, RwLock, RwLockWriteGuard};
use zenoh::{
    internal::{
        bail,
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
    Result as ZResult,
};
use zenoh_backend_traits::{
    config::{GarbageCollectionConfig, StorageConfig},
    Capability, History, Persistence, StorageInsertionResult, StoredData,
};

use super::LatestUpdates;
use crate::{
    replication::Event,
    storages_mgt::{CacheLatest, StorageMessage},
};

pub const WILDCARD_UPDATES_FILENAME: &str = "wildcard_updates";
pub const TOMBSTONE_FILENAME: &str = "tombstones";

#[derive(Clone)]
struct Update {
    kind: SampleKind,
    data: StoredData,
}

#[derive(Clone)]
pub struct StorageService {
    session: Arc<Session>,
    configuration: StorageConfig,
    name: String,
    pub(crate) storage: Arc<Mutex<Box<dyn zenoh_backend_traits::Storage>>>,
    capability: Capability,
    tombstones: Arc<RwLock<KeBoxTree<Timestamp, NonWild, KeyedSetProvider>>>,
    wildcard_updates: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
    cache_latest: CacheLatest,
}

impl StorageService {
    pub async fn new(
        session: Arc<Session>,
        config: StorageConfig,
        name: &str,
        storage: Arc<Mutex<Box<dyn zenoh_backend_traits::Storage>>>,
        capability: Capability,
        cache_latest: CacheLatest,
    ) -> Self {
        let storage_service = StorageService {
            session,
            configuration: config,
            name: name.to_string(),
            storage,
            capability,
            tombstones: Arc::new(RwLock::new(KeBoxTree::default())),
            wildcard_updates: Arc::new(RwLock::new(KeBoxTree::default())),
            cache_latest,
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
    }

    pub(crate) async fn start_storage_queryable_subscriber(self, mut rx: Receiver<StorageMessage>) {
        // start periodic GC event
        let t = Timer::default();

        let gc_config = self.configuration.garbage_collection_config.clone();

        let latest_updates = if self.cache_latest.replication_log.is_none() {
            Some(self.cache_latest.latest_updates.clone())
        } else {
            None
        };

        let gc = TimedEvent::periodic(
            gc_config.period,
            GarbageCollectionEvent {
                config: gc_config,
                tombstones: self.tombstones.clone(),
                wildcard_updates: self.wildcard_updates.clone(),
                latest_updates,
            },
        );
        t.add_async(gc).await;

        let storage_key_expr = &self.configuration.key_expr;

        // subscribe on key_expr
        let storage_sub = match self.session.declare_subscriber(storage_key_expr).await {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                tracing::error!("Error starting storage '{}': {}", self.name, e);
                return;
            }
        };

        // answer to queries on key_expr
        let storage_queryable = match self
            .session
            .declare_queryable(storage_key_expr)
            .complete(self.configuration.complete)
            .await
        {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                tracing::error!("Error starting storage '{}': {}", self.name, e);
                return;
            }
        };

        tracing::debug!(
            "Starting storage '{}' on keyexpr '{}'",
            self.name,
            storage_key_expr
        );

        tokio::task::spawn(async move {
            loop {
                tokio::select!(
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
                        if let Err(e) = self.process_sample(sample).await {
                            tracing::error!("{e:?}");
                        }
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        self.reply_query(query).await;
                    },
                    // on storage handle drop
                    Ok(message) = rx.recv() => {
                        match message {
                            StorageMessage::Stop => {
                                tracing::trace!("Dropping storage '{}'", self.name);
                                return
                            },
                            StorageMessage::GetStatus(tx) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                        };
                    },
                );
            }
        });
    }

    // The storage should only simply save the key, sample pair while put and retrieve the same
    // during get the trimming during PUT and GET should be handled by the plugin
    pub(crate) async fn process_sample(&self, sample: Sample) -> ZResult<()> {
        tracing::trace!("[STORAGE] Processing sample: {:?}", sample);

        // A Sample, in theory, will not arrive to a Storage without a Timestamp. This check (which,
        // again, should never enter the `None` branch) ensures that the Storage Manager
        // does not panic even if it ever happens.
        let sample_timestamp = match sample.timestamp() {
            Some(timestamp) => timestamp,
            None => {
                bail!("Discarding Sample without a Timestamp: {:?}", sample);
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

        let prefix = self.configuration.strip_prefix.as_ref();

        for k in matching_keys {
            if self.is_deleted(&k, sample_timestamp).await {
                tracing::trace!("Skipping Sample < {} > deleted later on", k);
                continue;
            }

            tracing::trace!(
                "Sample `{:?}` identified as needed processing for key {}",
                sample,
                k
            );

            // there might be the case that the actual update was outdated due to a wild card
            // update, but not stored yet in the storage. get the relevant wild
            // card entry and use that value and timestamp to update the storage
            let sample_to_store: Sample = if let Some(update) =
                self.overriding_wild_update(&k, sample_timestamp).await
            {
                match update.kind {
                    SampleKind::Put => SampleBuilder::put(k.clone(), update.data.value.payload())
                        .encoding(update.data.value.encoding().clone())
                        .timestamp(update.data.timestamp)
                        .into(),
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

            let stripped_key = match crate::strip_prefix(prefix, sample_to_store.key_expr()) {
                Ok(stripped) => stripped,
                Err(e) => {
                    bail!("{e:?}");
                }
            };

            // If the Storage was declared as only keeping the Latest value, we ensure that, for
            // each received Sample, it is indeed the Latest value that is processed.
            let mut cache_guard = None;
            if self.capability.history == History::Latest {
                match self
                    .guard_cache_if_latest(&stripped_key, &sample_to_store_timestamp)
                    .await
                {
                    Some(guard) => {
                        cache_guard = Some(guard);
                    }
                    None => {
                        tracing::trace!("Skipping outdated Sample < {} >", k);
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
                                sample_to_store.payload(),
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
                    tracing::trace!("Ignoring `Outdated` sample < {} >", k);
                }
                Ok(_) => {
                    if let Some(mut cache_guard) = cache_guard {
                        cache_guard.insert(
                            stripped_key.clone(),
                            Event::new(stripped_key, sample_to_store_timestamp, sample.kind()),
                        );
                    }
                }
                Err(e) => {
                    // TODO In case of a wildcard update, multiple keys can be updated. What should
                    //      be the behaviour if one or more of these updates fail?
                    tracing::error!("`{}` on < {} > failed with: {e:?}", sample.kind(), k);
                }
            }
        }

        Ok(())
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

        let prefix = self.configuration.strip_prefix.as_ref();

        for node in wildcards.intersecting_keys(key_expr) {
            let weight = wildcards.weight_at(&node);
            if weight.is_some() && weight.unwrap().data.timestamp > *ts {
                // if the key matches a wild card update, check whether it was saved in storage
                // remember that wild card updates change only existing keys
                let stripped_key = match crate::strip_prefix(prefix, &key_expr.into()) {
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

    /// Returns a guard over the cache if the provided [Timestamp] is more recent than what is kept
    /// in the Storage for the `stripped_key`. Otherwise returns `None`.
    ///
    /// This method will first look up any cached value and if none is found, it will request the
    /// Storage.
    ///
    /// # ⚠️ Race-condition
    ///
    /// Returning a guard over the cache is not an "innocent" choice: in order to avoid
    /// race-condition, the guard over the cache must be kept until the Storage has processed the
    /// Sample and the Cache has been updated accordingly.
    ///
    /// If the lock is released before both operations are performed, the Cache and Storage could
    /// end up in an inconsistent state (think two updates being processed at the same time).
    async fn guard_cache_if_latest(
        &self,
        stripped_key: &Option<OwnedKeyExpr>,
        received_ts: &Timestamp,
    ) -> Option<RwLockWriteGuard<'_, LatestUpdates>> {
        let cache_guard = self.cache_latest.latest_updates.write().await;
        if let Some(event) = cache_guard.get(stripped_key) {
            if received_ts > event.timestamp() {
                return Some(cache_guard);
            }
        }

        if let Some(replication_log) = &self.cache_latest.replication_log {
            if let Some(event) = replication_log.read().await.lookup(stripped_key) {
                if received_ts <= event.timestamp() {
                    return None;
                }
            }
        } else {
            let mut storage = self.storage.lock().await;
            // FIXME: An actual error from the underlying Storage cannot be distinguished from a
            //        missing entry.
            if let Ok(stored_data) = storage.get(stripped_key.clone(), "").await {
                for data in stored_data {
                    if data.timestamp > *received_ts {
                        return None;
                    }
                }
            }
        }

        Some(cache_guard)
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

        let prefix = self.configuration.strip_prefix.as_ref();

        if q.key_expr().is_wild() {
            // resolve key expr into individual keys
            let matching_keys = self.get_matching_keys(q.key_expr()).await;
            let mut storage = self.storage.lock().await;
            for key in matching_keys {
                let stripped_key = match crate::strip_prefix(prefix, &key.clone().into()) {
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
            let stripped_key = match crate::strip_prefix(prefix, q.key_expr()) {
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

        let prefix = self.configuration.strip_prefix.as_ref();

        match storage.get_all_entries().await {
            Ok(entries) => {
                for (k, _ts) in entries {
                    // @TODO: optimize adding back the prefix (possible inspiration from https://github.com/eclipse-zenoh/zenoh/blob/0.5.0-beta.9/backends/traits/src/utils.rs#L79)
                    let full_key = match k {
                        Some(key) => crate::prefix(prefix, &key),
                        None => {
                            let Some(prefix) = prefix else {
                                // TODO Check if we have anything in place that would prevent such
                                //      an error from happening.
                                tracing::error!(
                                    "Internal bug: empty key with no `strip_prefix` configured"
                                );
                                continue;
                            };
                            prefix.clone()
                        }
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
    latest_updates: Option<Arc<RwLock<LatestUpdates>>>,
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

        if let Some(latest_updates) = &self.latest_updates {
            latest_updates
                .write()
                .await
                .retain(|_, event| event.timestamp().get_time() < &time_limit);
        }

        tracing::trace!("End garbage collection of obsolete data-infos");
    }
}
