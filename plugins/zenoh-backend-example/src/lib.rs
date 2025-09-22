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
use std::collections::{hash_map::Entry, HashMap};

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
    Capability, History, Persistence, Storage, StorageInsertionResult, StoredData, Volume,
    VolumeInstance,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin};
use zenoh_util::ffi::JsonValue;

#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(ExampleBackend);

impl Plugin for ExampleBackend {
    type StartArgs = VolumeConfig;
    type Instance = VolumeInstance;
    fn start(_name: &str, _args: &Self::StartArgs) -> ZResult<Self::Instance> {
        let volume = ExampleBackend {};
        Ok(Box::new(volume))
    }

    const DEFAULT_NAME: &'static str = "example_backend";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();
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
    fn get_admin_status(&self) -> JsonValue {
        serde_json::Value::Null.into()
    }
    fn get_capability(&self) -> Capability {
        Capability {
            persistence: Persistence::Volatile,
            history: History::Latest,
        }
    }
    async fn create_storage(&self, _props: StorageConfig) -> ZResult<Box<dyn Storage>> {
        Ok(Box::<ExampleStorage>::default())
    }
}

#[async_trait]
impl Storage for ExampleStorage {
    fn get_admin_status(&self) -> JsonValue {
        serde_json::Value::Null.into()
    }
    async fn put(
        &mut self,
        key: Option<OwnedKeyExpr>,
        payload: ZBytes,
        encoding: Encoding,
        timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        let mut map = self.map.write().await;
        match map.entry(key) {
            Entry::Occupied(mut e) => {
                e.insert(StoredData {
                    payload,
                    encoding,
                    timestamp,
                });
                return Ok(StorageInsertionResult::Replaced);
            }
            Entry::Vacant(e) => {
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
