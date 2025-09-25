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
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tokio::sync::RwLock;
use zenoh::{
    bytes::{Encoding, ZBytes},
    key_expr::OwnedKeyExpr,
    time::Timestamp,
    Result as ZResult,
};
use zenoh_backend_traits::{
    config::{StorageConfig, VolumeConfig},
    *,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin};
use zenoh_util::ffi::JsonValue;

use crate::MEMORY_BACKEND_NAME;

pub struct MemoryBackend {
    config: VolumeConfig,
}

impl Plugin for MemoryBackend {
    type StartArgs = VolumeConfig;
    type Instance = VolumeInstance;

    const DEFAULT_NAME: &'static str = MEMORY_BACKEND_NAME;
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(_: &str, args: &VolumeConfig) -> ZResult<VolumeInstance> {
        Ok(Box::new(MemoryBackend {
            config: args.clone(),
        }))
    }
}

#[async_trait]
impl Volume for MemoryBackend {
    fn get_admin_status(&self) -> JsonValue {
        self.config.to_json_value().into()
    }

    fn get_capability(&self) -> Capability {
        Capability {
            persistence: Persistence::Volatile,
            history: History::Latest,
        }
    }

    async fn create_storage(&self, properties: StorageConfig) -> ZResult<Box<dyn Storage>> {
        tracing::debug!("Create Memory Storage with configuration: {:?}", properties);
        Ok(Box::new(MemoryStorage::new(properties).await?))
    }
}

impl Drop for MemoryBackend {
    fn drop(&mut self) {
        // nothing to do in case of memory backend
        tracing::trace!("MemoryBackend::drop()");
    }
}

struct MemoryStorage {
    config: StorageConfig,
    map: Arc<RwLock<HashMap<Option<OwnedKeyExpr>, StoredData>>>,
}

impl MemoryStorage {
    async fn new(properties: StorageConfig) -> ZResult<MemoryStorage> {
        Ok(MemoryStorage {
            config: properties,
            map: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    fn get_admin_status(&self) -> JsonValue {
        self.config.to_json_value().into()
    }

    async fn put(
        &mut self,
        key: Option<OwnedKeyExpr>,
        payload: ZBytes,
        encoding: Encoding,
        timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        tracing::trace!("put for {:?}", key);
        let mut map = self.map.write().await;
        match map.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(StoredData {
                    payload,
                    encoding,
                    timestamp,
                });
                return Ok(StorageInsertionResult::Replaced);
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(StoredData {
                    payload,
                    encoding,
                    timestamp,
                });
                return Ok(StorageInsertionResult::Inserted);
            }
        }
    }

    async fn delete(
        &mut self,
        key: Option<OwnedKeyExpr>,
        _timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        tracing::trace!("delete for {:?}", key);
        self.map.write().await.remove_entry(&key);
        return Ok(StorageInsertionResult::Deleted);
    }

    async fn get(
        &mut self,
        key: Option<OwnedKeyExpr>,
        _parameters: &str,
    ) -> ZResult<Vec<StoredData>> {
        tracing::trace!("get for {:?}", key);
        // @TODO: use parameters???
        match self.map.read().await.get(&key) {
            Some(v) => Ok(vec![v.clone()]),
            None => Err(format!("Key {key:?} is not present").into()),
        }
    }

    async fn get_all_entries(&self) -> ZResult<Vec<(Option<OwnedKeyExpr>, Timestamp)>> {
        let map = self.map.read().await;
        let mut result = Vec::with_capacity(map.len());
        for (k, v) in map.iter() {
            result.push((k.clone(), v.timestamp));
        }
        Ok(result)
    }
}

impl Drop for MemoryStorage {
    fn drop(&mut self) {
        // nothing to do in case of memory backend
        tracing::trace!("MemoryStorage::drop()");
    }
}
