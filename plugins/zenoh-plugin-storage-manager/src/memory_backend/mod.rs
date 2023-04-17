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
use async_std::sync::RwLock;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use zenoh::prelude::r#async::*;
use zenoh::time::Timestamp;
use zenoh_backend_traits::config::{StorageConfig, VolumeConfig};
use zenoh_backend_traits::*;
use zenoh_result::ZResult;

pub fn create_memory_backend(config: VolumeConfig) -> ZResult<Box<dyn Volume>> {
    Ok(Box::new(MemoryBackend { config }))
}

pub struct MemoryBackend {
    config: VolumeConfig,
}

#[async_trait]
impl Volume for MemoryBackend {
    fn get_admin_status(&self) -> serde_json::Value {
        self.config.to_json_value()
    }

    fn get_capability(&self) -> Capability {
        Capability {
            persistence: Persistence::Volatile,
            history: History::Latest,
            read_cost: 0,
        }
    }

    async fn create_storage(&mut self, properties: StorageConfig) -> ZResult<Box<dyn Storage>> {
        log::debug!("Create Memory Storage with configuration: {:?}", properties);
        Ok(Box::new(MemoryStorage::new(properties).await?))
    }

    fn incoming_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>> {
        // By default: no interception point
        None
        // To test interceptors, uncomment this line:
        // Some(Arc::new(|sample| {
        //     trace!(">>>> IN INTERCEPTOR FOR {:?}", sample);
        //     sample
        // }))
    }

    fn outgoing_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>> {
        // By default: no interception point
        None
        // To test interceptors, uncomment this line:
        // Some(Arc::new(|sample| {
        //     trace!("<<<< OUT INTERCEPTOR FOR {:?}", sample);
        //     sample
        // }))
    }
}

impl Drop for MemoryBackend {
    fn drop(&mut self) {
        // nothing to do in case of memory backend
        log::trace!("MemoryBackend::drop()");
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
    fn get_admin_status(&self) -> serde_json::Value {
        self.config.to_json_value()
    }

    async fn put(
        &mut self,
        key: Option<OwnedKeyExpr>,
        value: Value,
        timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        log::trace!("put for {:?}", key);
        let mut map = self.map.write().await;
        match map.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(StoredData { value, timestamp });
                return Ok(StorageInsertionResult::Replaced);
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(StoredData { value, timestamp });
                return Ok(StorageInsertionResult::Inserted);
            }
        }
    }

    async fn delete(
        &mut self,
        key: Option<OwnedKeyExpr>,
        _timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        log::trace!("delete for {:?}", key);
        self.map.write().await.remove_entry(&key);
        return Ok(StorageInsertionResult::Deleted);
    }

    async fn get(
        &mut self,
        key: Option<OwnedKeyExpr>,
        _parameters: &str,
    ) -> ZResult<Vec<StoredData>> {
        log::trace!("get for {:?}", key);
        // @TODO: use parameters???
        match self.map.read().await.get(&key) {
            Some(v) => Ok(vec![v.clone()]),
            None => Err(format!("Key {:?} is not present", key).into()),
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
        log::trace!("MemoryStorage::drop()");
    }
}
