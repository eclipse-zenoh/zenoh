use std::sync::Arc;

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
};
use zenoh_result::ZResult;

#[no_mangle]
pub fn create_volume(_config: VolumeConfig) -> ZResult<Box<dyn Volume>> {
    let volume = ExampleBackendStorage {};
    Ok(Box::new(volume))
}

pub struct ExampleBackendStorage {}

pub struct ExampleStorage {}

#[async_trait]
impl Volume for ExampleBackendStorage {
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
    async fn create_storage(&mut self, _props: StorageConfig) -> ZResult<Box<dyn Storage>> {
        Ok(Box::new(ExampleStorage {}))
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
        _key: Option<OwnedKeyExpr>,
        _value: Value,
        _timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        Ok(StorageInsertionResult::Inserted)
    }

    async fn delete(
        &mut self,
        _key: Option<OwnedKeyExpr>,
        _timestamp: Timestamp,
    ) -> ZResult<StorageInsertionResult> {
        Ok(StorageInsertionResult::Deleted)
    }

    async fn get(
        &mut self,
        _key: Option<OwnedKeyExpr>,
        _parameters: &str,
    ) -> ZResult<Vec<StoredData>> {
        Ok(Vec::new())
    }

    async fn get_all_entries(&self) -> ZResult<Vec<(Option<OwnedKeyExpr>, Timestamp)>> {
        Ok(Vec::new())
    }
}
