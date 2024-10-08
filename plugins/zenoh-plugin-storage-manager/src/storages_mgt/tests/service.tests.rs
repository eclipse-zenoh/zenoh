//
// Copyright (c) 2024 ZettaScale Technology
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

use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use zenoh::{
    internal::Value,
    key_expr::{
        keyexpr_tree::{KeBoxTree, KeyedSetProvider, NonWild, UnknownWildness},
        OwnedKeyExpr,
    },
    time::Timestamp,
    Result as ZResult,
};
use zenoh_backend_traits::{
    config::{GarbageCollectionConfig, StorageConfig},
    Capability, Storage, StorageInsertionResult, StoredData,
};

use super::{StorageService, Update};
use crate::storages_mgt::CacheLatest;

struct DummyStorage;

#[async_trait::async_trait]
impl Storage for DummyStorage {
    fn get_admin_status(&self) -> serde_json::Value {
        todo!()
    }

    async fn put(
        &mut self,
        _key: Option<OwnedKeyExpr>,
        _value: Value,
        _timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        todo!()
    }

    async fn delete(
        &mut self,
        _key: Option<OwnedKeyExpr>,
        _timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        todo!()
    }

    async fn get(
        &mut self,
        _key: Option<OwnedKeyExpr>,
        _parameters: &str,
    ) -> ZResult<Vec<StoredData>> {
        todo!()
    }

    async fn get_all_entries(&self) -> ZResult<Vec<(Option<OwnedKeyExpr>, Timestamp)>> {
        todo!()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_overriding_wild_update() {
    let _storage_service = StorageService {
        session: Arc::new(zenoh::open(zenoh::Config::default()).await.unwrap()),
        configuration: StorageConfig {
            name: "test".to_string(),
            key_expr: OwnedKeyExpr::new("test/**").unwrap(),
            complete: true,
            strip_prefix: Some(OwnedKeyExpr::new("test").unwrap()),
            volume_id: "test-volume".to_string(),
            volume_cfg: serde_json::json!({}),
            garbage_collection_config: GarbageCollectionConfig::default(),
            replication: None,
        },
        name: "test-storage".to_string(),
        storage: Arc::new(Mutex::new(Box::new(DummyStorage {}) as Box<dyn Storage>)),
        capability: Capability {
            persistence: zenoh_backend_traits::Persistence::Volatile,
            history: zenoh_backend_traits::History::Latest,
        },
        tombstones: Arc::new(RwLock::new(
            KeBoxTree::<Timestamp, NonWild, KeyedSetProvider>::default(),
        )),
        wildcard_updates: Arc::new(RwLock::new(KeBoxTree::<
            Update,
            UnknownWildness,
            KeyedSetProvider,
        >::default())),
        cache_latest: CacheLatest::default(),
    };
}
