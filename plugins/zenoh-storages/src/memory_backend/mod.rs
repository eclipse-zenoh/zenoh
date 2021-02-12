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
use zenoh::net::utils::resource_name;
use zenoh::net::Sample;
use zenoh::{utils, ChangeKind, Properties, Timestamp, Value, ZResult};
use zenoh_backend_traits::*;
use zenoh_util::collections::{Timed, TimedEvent, TimedHandle, Timer};

pub fn create_backend(_unused: Properties) -> ZResult<Box<dyn Backend>> {
    // For now admin status is static and only contains a PROP_BACKEND_TYPE entry
    let properties = Properties::from(&[(PROP_BACKEND_TYPE, "memory")][..]);
    let admin_status = utils::properties_to_json_value(&properties);
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

    fn incoming_data_interceptor(&self) -> Option<Box<dyn IncomingDataInterceptor>> {
        // By default: no interception point
        None
        // To test interceptors, uncomment this line:
        // Some(Box::new(InTestInterceptor {}))
    }

    fn outgoing_data_interceptor(&self) -> Option<Box<dyn OutgoingDataInterceptor>> {
        // By default: no interception point
        None
        // To test interceptors, uncomment this line:
        // Some(Box::new(OutTestInterceptor {}))
    }
}

struct InTestInterceptor {}

#[async_trait]
impl IncomingDataInterceptor for InTestInterceptor {
    async fn on_sample(&self, sample: Sample) -> Sample {
        println!(">>>> IN INTERCEPTOR FOR {:?}", sample);
        sample
    }
}

struct OutTestInterceptor {}

#[async_trait]
impl OutgoingDataInterceptor for OutTestInterceptor {
    async fn on_reply(&self, sample: Sample) -> Sample {
        println!("<<<< OUT INTERCEPTOR FOR {:?}", sample);
        sample
    }
}

impl Drop for MemoryBackend {
    fn drop(&mut self) {
        // nothing to do in case of memory backend
        log::trace!("MemoryBackend::drop()");
    }
}

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
            Present { ts, sample: _ } => &ts,
            Removed {
                ts,
                cleanup_handle: _,
            } => &ts,
        }
    }
}
use StoredValue::{Present, Removed};

struct MemoryStorage {
    admin_status: Value,
    map: Arc<RwLock<HashMap<String, StoredValue>>>,
    timer: Timer,
}

impl MemoryStorage {
    async fn new(properties: Properties) -> ZResult<MemoryStorage> {
        let admin_status = utils::properties_to_json_value(&properties);

        Ok(MemoryStorage {
            admin_status,
            map: Arc::new(RwLock::new(HashMap::new())),
            timer: Timer::new(),
        })
    }
}

impl MemoryStorage {
    async fn schedule_cleanup(&self, path: String) -> TimedHandle {
        let event = TimedEvent::once(
            Instant::now() + Duration::from_millis(CLEANUP_TIMEOUT_MS),
            TimedCleanup {
                map: self.map.clone(),
                path,
            },
        );
        let handle = event.get_handle();
        self.timer.add(event).await;
        handle
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn get_admin_status(&self) -> Value {
        self.admin_status.clone()
    }

    async fn on_sample(&mut self, sample: Sample) -> ZResult<()> {
        trace!("on_sample for {}", sample.res_name);
        let (kind, timestamp) = if let Some(ref info) = sample.data_info {
            (
                info.kind.map_or(ChangeKind::Put, ChangeKind::from),
                match &info.timestamp {
                    Some(ts) => ts.clone(),
                    None => utils::new_reception_timestamp(),
                },
            )
        } else {
            (ChangeKind::Put, utils::new_reception_timestamp())
        };
        match kind {
            ChangeKind::Put => match self.map.write().await.entry(sample.res_name.clone()) {
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
                        debug!("PUT on {} dropped: out-of-date", sample.res_name);
                    }
                }
            },
            ChangeKind::Delete => match self.map.write().await.entry(sample.res_name.clone()) {
                Entry::Vacant(v) => {
                    // NOTE: even if path is not known yet, we need to store the removal time:
                    // if ever a put with a lower timestamp arrive (e.g. msg inversion between put and remove)
                    // we must drop the put.
                    let cleanup_handle = self.schedule_cleanup(sample.res_name.clone()).await;
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
                                    self.schedule_cleanup(sample.res_name.clone()).await;
                                o.insert(Removed {
                                    ts: timestamp,
                                    cleanup_handle,
                                });
                            } else {
                                debug!("DEL on {} dropped: out-of-date", sample.res_name);
                            }
                        }
                    }
                }
            },
            ChangeKind::Patch => {
                warn!("Received PATCH for {}: not yet supported", sample.res_name);
            }
        }
        Ok(())
    }

    async fn on_query(&mut self, query: Query) -> ZResult<()> {
        trace!("on_query for {}", query.res_name());
        if !query.res_name().contains('*') {
            if let Some(Present { sample, ts: _ }) = self.map.read().await.get(query.res_name()) {
                query.reply(sample.clone()).await;
            }
        } else {
            for (_, stored_value) in self.map.read().await.iter() {
                if let Present { sample, ts: _ } = stored_value {
                    if resource_name::intersect(query.res_name(), &sample.res_name) {
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
    path: String,
}

#[async_trait]
impl Timed for TimedCleanup {
    async fn run(&mut self) {
        self.map.write().await.remove(&self.path);
    }
}
