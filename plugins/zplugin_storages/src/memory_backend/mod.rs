//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_trait::async_trait;
use log::debug;
use std::collections::HashMap;
use zenoh::net::utils::resource_name;
use zenoh::net::{Query, Sample};
use zenoh::{utils, Properties, Value, ZResult};
use zenoh_backend_core::{Backend, Storage};

pub fn create_backend(_unused: Properties) -> ZResult<Box<dyn Backend>> {
    // For now admin status is static and only contains a "kind"
    let admin_status = Value::Json(r#"{"kind"="memory"}"#.to_string());
    Ok(Box::new(MemoryBackend { admin_status }))
}

pub struct MemoryBackend {
    admin_status: Value,
}

#[async_trait]
impl Backend for MemoryBackend {
    async fn get_admin_status(&self) -> Value {
        self.admin_status.clone()
    }

    async fn create_storage(&mut self, properties: Properties) -> ZResult<Box<dyn Storage>> {
        debug!("Create Memory Storage with properties: {}", properties);
        Ok(Box::new(MemoryStorage::new(properties).await?))
    }
}

impl Drop for MemoryBackend {
    fn drop(&mut self) {
        log::debug!("MemoryBackend::drop()");
    }
}

struct MemoryStorage {
    admin_status: Value,
    map: HashMap<String, Sample>,
}

impl MemoryStorage {
    async fn new(properties: Properties) -> ZResult<MemoryStorage> {
        let admin_status = utils::properties_to_json_value(&properties);

        Ok(MemoryStorage {
            admin_status,
            map: HashMap::new(),
        })
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn get_admin_status(&self) -> Value {
        self.admin_status.clone()
    }

    async fn on_sample(&mut self, sample: Sample) -> ZResult<()> {
        debug!("on_sample {}", sample.res_name);
        self.map.insert(sample.res_name.clone(), sample);
        Ok(())
    }

    async fn on_query(&mut self, query: Query) -> ZResult<()> {
        debug!("on_query {}", query.res_name);
        for (_stored_name, sample) in self.map.iter() {
            if resource_name::intersect(&query.res_name, &sample.res_name) {
                let s: Sample = sample.clone();
                query.reply(s).await;
            }
        }
        Ok(())
    }
}

impl Drop for MemoryStorage {
    fn drop(&mut self) {
        debug!("MemoryStorage::drop()");
    }
}
