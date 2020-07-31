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
use log::{debug, trace, warn};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use zenoh::net::utils::resource_name;
use zenoh::net::{Query, Sample};
use zenoh::{utils, ChangeKind, Properties, Timestamp, Value, ZResult};
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
        // nothing to do in case of memory backend
        log::trace!("MemoryBackend::drop()");
    }
}

struct MemoryStorage {
    admin_status: Value,
    map: HashMap<String, (Sample, Timestamp)>,
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
        let (kind, _, timestamp) = utils::decode_data_info(sample.data_info.clone());
        match kind {
            ChangeKind::PUT => match self.map.entry(sample.res_name.clone()) {
                Entry::Vacant(v) => {
                    v.insert((sample, timestamp));
                }
                Entry::Occupied(mut o) => {
                    if o.get().1 < timestamp {
                        o.insert((sample, timestamp));
                    }
                }
            },
            ChangeKind::DELETE => {
                self.map.remove(&sample.res_name);
            }
            ChangeKind::PATCH => {
                warn!("Received PATCH for {}: not yet supported", sample.res_name);
            }
        }
        Ok(())
    }

    async fn on_query(&mut self, query: Query) -> ZResult<()> {
        debug!("on_query {}", query.res_name);
        if !query.res_name.contains('*') {
            if let Some((sample, _)) = self.map.get(&query.res_name) {
                query.reply(sample.clone()).await;
            }
        } else {
            for (_, (sample, _)) in self.map.iter() {
                if resource_name::intersect(&query.res_name, &sample.res_name) {
                    let s: Sample = sample.clone();
                    query.reply(s).await;
                }
            }
        }
        Ok(())
    }
}

impl Drop for MemoryStorage {
    fn drop(&mut self) {
        // nothing to do in case of memory backend
        trace!("MemoryStorage::drop()");
    }
}
