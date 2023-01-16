//
// Copyright (c) 2022 ZettaScale Technology
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
use log::{debug, trace};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zenoh::prelude::r#async::*;
use zenoh::time::Timestamp;
use zenoh_backend_traits::config::{StorageConfig, VolumeConfig};
use zenoh_backend_traits::*;
use zenoh_core::Result as ZResult;
use zenoh_util::{Timed, TimedEvent, TimedHandle, Timer};

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
    map: Arc<RwLock<HashMap<OwnedKeyExpr, StoredValue>>>,
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
    async fn schedule_cleanup(&self, key: OwnedKeyExpr) -> TimedHandle {
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

    async fn on_sample(&mut self, mut sample: Sample) -> ZResult<StorageInsertionResult> {
        trace!("on_sample for {}", sample.key_expr);
        sample.ensure_timestamp();
        let timestamp = sample.timestamp.unwrap();
        match sample.kind {
            SampleKind::Put => match self.map.write().await.entry(sample.key_expr.clone().into()) {
                Entry::Vacant(v) => {
                    v.insert(Present {
                        sample,
                        ts: timestamp,
                    });
                    return Ok(StorageInsertionResult::Inserted);
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
                        return Ok(StorageInsertionResult::Replaced);
                    } else {
                        debug!("PUT on {} dropped: out-of-date", sample.key_expr);
                        return Ok(StorageInsertionResult::Outdated);
                    }
                }
            },
            SampleKind::Delete => {
                match self.map.write().await.entry(sample.key_expr.clone().into()) {
                    Entry::Vacant(v) => {
                        // NOTE: even if key is not known yet, we need to store the removal time:
                        // if ever a put with a lower timestamp arrive (e.g. msg inversion between put and remove)
                        // we must drop the put.
                        let cleanup_handle = self.schedule_cleanup(sample.key_expr.into()).await;
                        v.insert(Removed {
                            ts: timestamp,
                            cleanup_handle,
                        });
                        return Ok(StorageInsertionResult::Deleted);
                    }
                    Entry::Occupied(mut o) => match o.get() {
                        Removed {
                            ts,
                            cleanup_handle: _,
                        } => {
                            if ts < &timestamp {
                                let cleanup_handle =
                                    self.schedule_cleanup(sample.key_expr.into()).await;
                                o.insert(Removed {
                                    ts: timestamp,
                                    cleanup_handle,
                                });
                                return Ok(StorageInsertionResult::Deleted);
                            } else {
                                debug!("DEL on {} dropped: out-of-date", sample.key_expr);
                                return Ok(StorageInsertionResult::Outdated);
                            }
                        }
                        Present { sample: _, ts } => {
                            if ts < &timestamp {
                                let cleanup_handle =
                                    self.schedule_cleanup(sample.key_expr.into()).await;
                                o.insert(Removed {
                                    ts: timestamp,
                                    cleanup_handle,
                                });
                                return Ok(StorageInsertionResult::Deleted);
                            } else {
                                debug!("DEL on {} dropped: out-of-date", sample.key_expr);
                                return Ok(StorageInsertionResult::Outdated);
                            }
                        }
                    },
                }
            }
        }
    }

    async fn on_query(&mut self, query: Query) -> ZResult<()> {
        trace!("on_query for {}", query.key_expr());
        if !query.key_expr().is_wild() {
            if let Some(Present { sample, ts: _ }) =
                self.map.read().await.get(query.key_expr().as_keyexpr())
            {
                query.reply(sample.clone()).res().await?;
            }
        } else {
            for (_, stored_value) in self.map.read().await.iter() {
                if let Present { sample, ts: _ } = stored_value {
                    if query.key_expr().intersects(&sample.key_expr) {
                        let s: Sample = sample.clone();
                        query.reply(s).res().await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn get_all_entries(&self) -> ZResult<Vec<(OwnedKeyExpr, Timestamp)>> {
        let map = self.map.read().await;
        let mut result = Vec::with_capacity(map.len());
        for (k, v) in map.iter() {
            result.push((k.clone(), *v.ts()));
        }
        Ok(result)
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
    map: Arc<RwLock<HashMap<OwnedKeyExpr, StoredValue>>>,
    key: OwnedKeyExpr,
}

#[async_trait]
impl Timed for TimedCleanup {
    async fn run(&mut self) {
        self.map.write().await.remove(&self.key);
    }
}
