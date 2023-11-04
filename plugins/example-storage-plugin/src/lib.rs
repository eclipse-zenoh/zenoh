use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use async_std::sync::RwLock;
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
use async_trait::async_trait;
use zenoh::{prelude::OwnedKeyExpr, sample::Sample, time::Timestamp, value::Value};
use zenoh_backend_traits::{
    config::{StorageConfig, VolumeConfig},
    Capability, History, Persistence, Storage, StorageInsertionResult, StoredData, Volume,
    VolumePlugin,
};
use zenoh_plugin_trait::Plugin;
use zenoh_result::ZResult;

zenoh_plugin_trait::declare_plugin!(ExampleBackend);

impl Plugin for ExampleBackend {
    type StartArgs = VolumeConfig;
    type Instance = VolumePlugin;

    fn start(_name: &str, _args: &Self::StartArgs) -> ZResult<Self::Instance> {
        let volume = ExampleBackend {};
        Ok(Box::new(volume))
    }

    const STATIC_NAME: &'static str = "example_backend";
}

#[no_mangle]
pub fn create_volume(_config: VolumeConfig) -> ZResult<Box<dyn Volume>> {
    let volume = ExampleBackend {};
    Ok(Box::new(volume))
}

pub struct ExampleBackend {}

pub struct ExampleStorage {
    map: RwLock<HashMap<Option<OwnedKeyExpr>, StoredData>>,
}

impl Default for ExampleStorage {
    fn default() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Volume for ExampleBackend {
    fn get_admin_status(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    fn get_capability(&self) -> Capability {
        Capability {
            persistence: Persistence::Volatile,
            history: History::Latest,
            read_cost: 0,
        }
    }
    async fn create_storage(&self, _props: StorageConfig) -> ZResult<Box<dyn Storage>> {
        Ok(Box::<ExampleStorage>::default())
    }
    fn incoming_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>> {
        None
    }
    fn outgoing_data_interceptor(&self) -> Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>> {
        None
    }
}

#[async_trait]
impl Storage for ExampleStorage {
    fn get_admin_status(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    async fn put(
        &mut self,
        key: Option<OwnedKeyExpr>,
        value: Value,
        timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        let mut map = self.map.write().await;
        match map.entry(key) {
            Entry::Occupied(mut e) => {
                e.insert(StoredData { value, timestamp });
                return Ok(StorageInsertionResult::Replaced);
            }
            Entry::Vacant(e) => {
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
        self.map.write().await.remove_entry(&key);
        Ok(StorageInsertionResult::Deleted)
    }

    async fn get(
        &mut self,
        key: Option<OwnedKeyExpr>,
        _parameters: &str,
    ) -> ZResult<Vec<StoredData>> {
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
