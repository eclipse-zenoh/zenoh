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
use async_std::sync::{Arc, RwLock};
use async_trait::async_trait;
use log::{debug, trace, warn};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zenoh::prelude::*;
use zenoh::time::Timestamp;
use zenoh::utils::key_expr;
use zenoh_backend_traits::config::{StorageConfig, VolumeConfig};
use zenoh_backend_traits::*;
use zenoh_collections::{Timed, TimedEvent, TimedHandle, Timer};
use zenoh_core::Result as ZResult;

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

    async fn create_storage(&mut self, properties: StorageConfig) -> ZResult<Box<dyn Storage>> {
        debug!("Create Memory Storage with configuration: {:?}", properties);
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
        trace!("MemoryBackend::drop()");
    }
}

#[allow(clippy::large_enum_variant)]
enum StoredValue {
    Present {
        ts: Timestamp,
        sample: Sample,
    },
    Removed {
        ts: Timestamp,
        // handle of the TimedEvent that will eventually remove the entry from the map
        cleanup_handle: TimedHandle,
    },
}

impl StoredValue {
    fn ts(&self) -> &Timestamp {
        match self {
            Present { ts, sample: _ } => ts,
            Removed {
                ts,
                cleanup_handle: _,
            } => ts,
        }
    }
}
use StoredValue::{Present, Removed};

struct MemoryStorage {
    config: StorageConfig,
    map: Arc<RwLock<HashMap<String, StoredValue>>>,
    timer: Timer,
}

impl MemoryStorage {
    async fn new(properties: StorageConfig) -> ZResult<MemoryStorage> {
        Ok(MemoryStorage {
            config: properties,
            map: Arc::new(RwLock::new(HashMap::new())),
            timer: Timer::new(false),
        })
    }
}

impl MemoryStorage {
    async fn schedule_cleanup(&self, key: String) -> TimedHandle {
        let event = TimedEvent::once(
            Instant::now() + Duration::from_millis(CLEANUP_TIMEOUT_MS),
            TimedCleanup {
                map: self.map.clone(),
                key,
            },
        );
        let handle = event.get_handle();
        self.timer.add_async(event).await;
        handle
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    fn get_admin_status(&self) -> serde_json::Value {
        self.config.to_json_value()
    }

    async fn on_sample(&mut self, mut sample: Sample) -> ZResult<()> {
        trace!("on_sample for {}", sample.key_expr);
        sample.ensure_timestamp();
        let timestamp = sample.timestamp.unwrap();
        match sample.kind {
            SampleKind::Put => match self.map.write().await.entry(sample.key_expr.to_string()) {
                Entry::Vacant(v) => {
                    v.insert(Present {
                        sample,
                        ts: timestamp,
                    });
                }
                Entry::Occupied(mut o) => {
                    let old_val = o.get();
                    if old_val.ts() < &timestamp {
                        if let Removed {
                            ts: _,
                            cleanup_handle,
                        } = old_val
                        {
                            // cancel timed cleanup
                            cleanup_handle.clone().defuse();
                        }
                        o.insert(Present {
                            sample,
                            ts: timestamp,
                        });
                    } else {
                        debug!("PUT on {} dropped: out-of-date", sample.key_expr);
                    }
                }
            },
            SampleKind::Delete => match self.map.write().await.entry(sample.key_expr.to_string()) {
                Entry::Vacant(v) => {
                    // NOTE: even if key is not known yet, we need to store the removal time:
                    // if ever a put with a lower timestamp arrive (e.g. msg inversion between put and remove)
                    // we must drop the put.
                    let cleanup_handle = self.schedule_cleanup(sample.key_expr.to_string()).await;
                    v.insert(Removed {
                        ts: timestamp,
                        cleanup_handle,
                    });
                }
                Entry::Occupied(mut o) => {
                    match o.get() {
                        Removed {
                            ts: _,
                            cleanup_handle: _,
                        } => (), // nothing to do
                        Present { sample: _, ts } => {
                            if ts < &timestamp {
                                let cleanup_handle =
                                    self.schedule_cleanup(sample.key_expr.to_string()).await;
                                o.insert(Removed {
                                    ts: timestamp,
                                    cleanup_handle,
                                });
                            } else {
                                debug!("DEL on {} dropped: out-of-date", sample.key_expr);
                            }
                        }
                    }
                }
            },
            SampleKind::Patch => {
                warn!("Received PATCH for {}: not yet supported", sample.key_expr);
            }
        }
        Ok(())
    }

    async fn on_query(&mut self, query: Query) -> ZResult<()> {
        trace!("on_query for {}", query.key_selector());
        if !query.key_selector().as_str().contains('*') {
            if let Some(Present { sample, ts: _ }) =
                self.map.read().await.get(query.key_selector().as_str())
            {
                query.reply(sample.clone()).await;
            }
        } else {
            for (_, stored_value) in self.map.read().await.iter() {
                if let Present { sample, ts: _ } = stored_value {
                    if key_expr::intersect(query.key_selector().as_str(), sample.key_expr.as_str())
                    {
                        let s: Sample = sample.clone();
                        query.reply(s).await;
                    }
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

const CLEANUP_TIMEOUT_MS: u64 = 5000;

struct TimedCleanup {
    map: Arc<RwLock<HashMap<String, StoredValue>>>,
    key: String,
}

#[async_trait]
impl Timed for TimedCleanup {
    async fn run(&mut self) {
        self.map.write().await.remove(&self.key);
    }
}
