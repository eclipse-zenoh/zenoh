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
    collections::HashSet,
    str::{self},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use tokio::sync::{broadcast::Receiver, Mutex, RwLock, RwLockWriteGuard};
use zenoh::{
    bytes::{Encoding, ZBytes},
    internal::{bail, Timed, TimedEvent, Timer},
    key_expr::{
        keyexpr,
        keyexpr_tree::{
            IKeyExprTree, IKeyExprTreeMut, KeBoxTree, KeyedSetProvider, UnknownWildness,
        },
        OwnedKeyExpr,
    },
    sample::{Sample, SampleBuilder, SampleFields, SampleKind},
    session::Session,
    time::{Timestamp, NTP64},
    Result as ZResult,
};
use zenoh_backend_traits::{
    config::{GarbageCollectionConfig, StorageConfig},
    Capability, History, StorageInsertionResult, StoredData,
};

use super::LatestUpdates;
use crate::{
    replication::{Action, Event},
    storages_mgt::{CacheLatest, StorageMessage},
};

#[derive(Clone)]
pub(crate) struct Update {
    kind: SampleKind,
    data: StoredData,
}

impl Update {
    pub(crate) fn timestamp(&self) -> &Timestamp {
        &self.data.timestamp
    }

    pub(crate) fn kind(&self) -> SampleKind {
        self.kind
    }

    pub(crate) fn payload(&self) -> &ZBytes {
        &self.data.payload
    }

    pub(crate) fn encoding(&self) -> &Encoding {
        &self.data.encoding
    }
}

impl From<Update> for StoredData {
    fn from(update: Update) -> Self {
        update.data
    }
}

#[derive(Clone)]
pub struct StorageService {
    session: Arc<Session>,
    pub(crate) configuration: StorageConfig,
    name: String,
    pub(crate) storage: Arc<Mutex<Box<dyn zenoh_backend_traits::Storage>>>,
    capability: Capability,
    pub(crate) wildcard_deletes: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
    pub(crate) wildcard_puts: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
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
        StorageService {
            session,
            configuration: config,
            name: name.to_string(),
            storage,
            capability,
            wildcard_deletes: Arc::new(RwLock::new(KeBoxTree::default())),
            wildcard_puts: Arc::new(RwLock::new(KeBoxTree::default())),
            cache_latest,
        }
    }

    pub(crate) async fn start_storage_queryable_subscriber(
        self: Arc<Self>,
        mut rx: Receiver<StorageMessage>,
    ) {
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
                wildcard_deletes: self.wildcard_deletes.clone(),
                wildcard_puts: self.wildcard_puts.clone(),
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
                                std::mem::drop(tx.send(storage.get_admin_status().into()).await);
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
        tracing::trace!("[STORAGE] Processing sample: {:?}", sample.key_expr());
        let SampleFields {
            key_expr,
            timestamp,
            payload,
            encoding,
            kind,
            ..
        } = sample.clone().into();

        // A Sample, in theory, will not arrive to a Storage without a Timestamp. This check (which,
        // again, should never enter the `None` branch) ensures that the Storage Manager
        // does not panic even if it ever happens.
        let Some(timestamp) = timestamp else {
            bail!("Discarding Sample without a Timestamp: {:?}", sample);
        };

        let mut action: Action = kind.into();
        // if wildcard, update wildcard_updates
        if key_expr.is_wild() {
            self.register_wildcard_update(
                key_expr.clone().into(),
                kind,
                timestamp,
                payload,
                encoding,
            )
            .await;

            action = match kind {
                SampleKind::Put => Action::WildcardPut(key_expr.clone().into()),
                SampleKind::Delete => Action::WildcardDelete(key_expr.clone().into()),
            };

            let event = Event::new(Some(key_expr.clone().into()), timestamp, &action);

            self.cache_latest
                .latest_updates
                .write()
                .await
                .insert(event.log_key(), event);
        }

        let matching_keys = if key_expr.is_wild() {
            self.get_matching_keys(&key_expr).await
        } else {
            vec![key_expr.clone().into()]
        };
        tracing::trace!(
            "The list of keys matching `{}` is : {:?}",
            &key_expr,
            matching_keys
        );

        let prefix = self.configuration.strip_prefix.as_ref();

        for k in matching_keys {
            // there might be the case that the actual update was outdated due to a wild card
            // update, but not stored yet in the storage. get the relevant wild
            // card entry and use that value and timestamp to update the storage
            let sample_to_store: Sample = if let Some((_, update)) = self
                .overriding_wild_update(&k, &timestamp, &None, &kind.into())
                .await
            {
                match update.kind {
                    SampleKind::Put => SampleBuilder::put(k.clone(), update.data.payload.clone())
                        .encoding(update.data.encoding.clone())
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
            let new_event = Event::new(stripped_key.clone(), sample_to_store_timestamp, &action);
            let mut cache_guard = None;
            if self.capability.history == History::Latest {
                match self.guard_cache_if_latest(&new_event).await {
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
                            sample_to_store.payload().clone(),
                            sample_to_store.encoding().clone(),
                            sample_to_store_timestamp,
                        )
                        .await
                }
                SampleKind::Delete => {
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
                        cache_guard.insert(new_event.log_key(), new_event);
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

    /// Registers a Wildcard Update, storing it in a dedicated in-memory structure and on disk if
    /// the Storage persistence capability is set to `Durable`.
    ///
    /// The `key_expr` and `timestamp` cannot be extracted from the received Sample when aligning
    /// and hence must be manually passed.
    ///
    /// # ⚠️ Cache with Replication
    ///
    /// It is the *responsibility of the caller* to insert a Wildcard Update event in the Cache. If
    /// the Replication is enabled, depending on where this method is called, the event should
    /// either be inserted in the Cache (to be later added in the Replication Log) or in the
    /// Replication Log.
    pub(crate) async fn register_wildcard_update(
        &self,
        key_expr: OwnedKeyExpr,
        kind: SampleKind,
        timestamp: Timestamp,
        payload: ZBytes,
        encoding: Encoding,
    ) {
        let update = Update {
            kind,
            data: StoredData {
                payload,
                encoding,
                timestamp,
            },
        };

        match kind {
            SampleKind::Put => {
                self.wildcard_puts.write().await.insert(&key_expr, update);
            }
            SampleKind::Delete => {
                self.wildcard_deletes
                    .write()
                    .await
                    .insert(&key_expr, update);
            }
        }
    }

    /// Returns an [Update] if the provided key expression is overridden by a Wildcard Update.
    pub(crate) async fn overriding_wild_update(
        &self,
        key_expr: &OwnedKeyExpr,
        timestamp: &Timestamp,
        timestamp_last_non_wildcard_update: &Option<Timestamp>,
        action: &Action,
    ) -> Option<(OwnedKeyExpr, Update)> {
        // First, check for a delete *if and only if the action is not a Wildcard Put*: if there are
        // Wildcard Delete that match this key expression, we want to keep the lowest delete
        // (i.e. the first that applies) as a Delete does not override another Delete -- except if
        // it's a Wildcard Delete that overrides another Wildcard Delete but that's another story.
        if matches!(
            action,
            Action::Put | Action::Delete | Action::WildcardDelete(_)
        ) {
            let mut wildcard_ke = None;
            let wildcard_deletes_guard = self.wildcard_deletes.read().await;
            let lowest_event_ts = timestamp_last_non_wildcard_update.unwrap_or(*timestamp);
            let mut lowest_wildcard_delete_ts = None;

            for wildcard_delete_ke in wildcard_deletes_guard.intersecting_keys(key_expr) {
                if let Some(wildcard_delete_update) =
                    wildcard_deletes_guard.weight_at(&wildcard_delete_ke)
                {
                    // Wildcard Delete with a greater timestamp than the lowest timestamp of the
                    // Event are the only one that should apply.
                    if wildcard_delete_update.data.timestamp >= lowest_event_ts {
                        match lowest_wildcard_delete_ts {
                            None => {
                                lowest_wildcard_delete_ts =
                                    Some(*wildcard_delete_update.timestamp());
                                wildcard_ke = Some(wildcard_delete_ke);
                            }
                            Some(current_lowest_ts) => {
                                if current_lowest_ts > wildcard_delete_update.data.timestamp {
                                    lowest_wildcard_delete_ts =
                                        Some(*wildcard_delete_update.timestamp());
                                    wildcard_ke = Some(wildcard_delete_ke);
                                }
                            }
                        }
                    }
                }
            }

            if let Some(wildcard_delete_ke) = wildcard_ke {
                if let Some(wildcard_delete_update) =
                    wildcard_deletes_guard.weight_at(&wildcard_delete_ke)
                {
                    return Some((wildcard_delete_ke, wildcard_delete_update.clone()));
                }
            }
        }

        // A Wildcard Put can only override a Put or another Wildcard Put. If several match, this
        // time we want to keep the Update with the latest timestamp.
        if matches!(action, Action::Put | Action::WildcardPut(_)) {
            let mut wildcard_ke = None;

            let wildcards = self.wildcard_puts.read().await;
            let mut latest_wildcard_ts = *timestamp;

            for node in wildcards.intersecting_keys(key_expr) {
                if let Some(wildcard_update) = wildcards.weight_at(&node) {
                    if wildcard_update.data.timestamp >= latest_wildcard_ts {
                        latest_wildcard_ts = wildcard_update.data.timestamp;
                        wildcard_ke = Some(node);
                    }
                }
            }

            if let Some(wildcard_ke) = wildcard_ke {
                if let Some(wildcard_update) = wildcards.weight_at(&wildcard_ke) {
                    return Some((wildcard_ke, wildcard_update.clone()));
                }
            }
        }

        None
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
        new_event: &Event,
    ) -> Option<RwLockWriteGuard<'_, LatestUpdates>> {
        let cache_guard = self.cache_latest.latest_updates.write().await;
        if let Some(event) = cache_guard.get(&new_event.log_key()) {
            if new_event.timestamp > event.timestamp {
                return Some(cache_guard);
            }
        }

        if let Some(replication_log) = &self.cache_latest.replication_log {
            if replication_log
                .read()
                .await
                .lookup_newer(new_event)
                .is_some()
            {
                return None;
            }
        } else {
            let mut storage = self.storage.lock().await;
            // FIXME: An actual error from the underlying Storage cannot be distinguished from a
            //        missing entry.
            if let Ok(stored_data) = storage.get(new_event.stripped_key.clone(), "").await {
                for data in stored_data {
                    if data.timestamp > new_event.timestamp {
                        return None;
                    }
                }
            }
        }

        Some(cache_guard)
    }

    async fn reply_query(&self, query: ZResult<zenoh::query::Query>) {
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
                                .reply(key.clone(), entry.payload.clone())
                                .encoding(entry.encoding.clone())
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
                            .reply(q.key_expr().clone(), entry.payload.clone())
                            .encoding(entry.encoding.clone())
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

    async fn get_matching_keys(&self, key_expr: &keyexpr) -> Vec<OwnedKeyExpr> {
        let mut result = Vec::new();
        // @TODO: if cache exists, use that to get the list
        let storage = self.storage.lock().await;

        let prefix = self.configuration.strip_prefix.as_ref();

        match storage.get_all_entries().await {
            Ok(entries) => {
                for (k, _ts) in entries {
                    // @TODO: optimize adding back the prefix (possible inspiration from https://github.com/eclipse-zenoh/zenoh/blob/0.5.0-beta.9/backends/traits/src/utils.rs#L79)
                    let Ok(full_key) = crate::prefix(prefix, k.as_ref()) else {
                        tracing::error!(
                            "Internal error: empty key with no `strip_prefix` configured"
                        );
                        continue;
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

// Periodic event cleaning-up data info for old metadata
struct GarbageCollectionEvent {
    config: GarbageCollectionConfig,
    wildcard_deletes: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
    wildcard_puts: Arc<RwLock<KeBoxTree<Update, UnknownWildness, KeyedSetProvider>>>,
    latest_updates: Option<Arc<RwLock<LatestUpdates>>>,
}

#[async_trait]
impl Timed for GarbageCollectionEvent {
    async fn run(&mut self) {
        tracing::trace!("Start garbage collection");
        let time_limit = NTP64::from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
            - NTP64::from(self.config.lifespan);

        // Get lock on fields
        let mut wildcard_deletes_guard = self.wildcard_deletes.write().await;
        let mut wildcard_updates_guard = self.wildcard_puts.write().await;

        let mut to_be_removed = HashSet::new();
        for (k, update) in wildcard_deletes_guard.key_value_pairs() {
            let ts = update.data.timestamp;
            if ts.get_time() < &time_limit {
                // mark key to be removed
                to_be_removed.insert(k);
            }
        }
        for k in to_be_removed {
            wildcard_deletes_guard.remove(&k);
        }

        let mut to_be_removed = HashSet::new();
        for (k, update) in wildcard_updates_guard.key_value_pairs() {
            let ts = update.data.timestamp;
            if ts.get_time() < &time_limit {
                // mark key to be removed
                to_be_removed.insert(k);
            }
        }
        for k in to_be_removed {
            wildcard_updates_guard.remove(&k);
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
