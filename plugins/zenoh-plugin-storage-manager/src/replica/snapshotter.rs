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
use super::{Digest, DigestConfig};
use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_std::task::sleep;
use flume::Receiver;
use futures::join;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::time::Duration;
use zenoh::key_expr::OwnedKeyExpr;
use zenoh::time::Timestamp;
use zenoh_backend_traits::config::ReplicaConfig;

pub struct ReplicationInfo {
    stable_log: Arc<RwLock<HashMap<OwnedKeyExpr, Timestamp>>>, // log entries until the snapshot time
    volatile_log: RwLock<HashMap<OwnedKeyExpr, Timestamp>>, // log entries after the snapshot time
    last_snapshot_time: RwLock<Timestamp>,                  // the latest snapshot time
    last_interval: RwLock<u64>,                             // the latest interval
    digest: Arc<RwLock<Digest>>,                            // the current stable digest
}

pub struct Snapshotter {
    storage_update: Receiver<(OwnedKeyExpr, Timestamp)>,
    replica_config: ReplicaConfig,
    content: ReplicationInfo,
}

impl Snapshotter {
    // this class takes care of managing logs, digests and keeps snapshot time and interval updated
    // two roles:
    // 1. receives logs from storage, updates the log
    // 2. when asked by snapshot_timer, update the stable and volatile part of logs  -- , rx_snapshot: Receiver<(Timestamp, u64)>
    // a queue each to listen to storage and snapshotter

    pub async fn new(
        rx_sample: Receiver<(OwnedKeyExpr, Timestamp)>,
        initial_entries: Vec<(OwnedKeyExpr, Timestamp)>,
        replica_config: ReplicaConfig,
    ) -> Self {
        // compute snapshot time and snapshot interval to start with
        // from initial entries, populate the log - stable and volatile
        // compute digest
        let (last_snapshot_time, last_interval) = Snapshotter::compute_snapshot_params(
            replica_config.propagation_delay,
            replica_config.delta,
        );
        let snapshotter = Snapshotter {
            storage_update: rx_sample,
            replica_config: replica_config.clone(),
            content: ReplicationInfo {
                stable_log: Arc::new(RwLock::new(HashMap::new())),
                volatile_log: RwLock::new(HashMap::new()),
                last_snapshot_time: RwLock::new(last_snapshot_time),
                last_interval: RwLock::new(last_interval),
                digest: Arc::new(RwLock::new(Digest::create_digest(
                    last_snapshot_time,
                    DigestConfig {
                        delta: replica_config.delta,
                        sub_intervals: replica_config.subintervals,
                        hot: replica_config.hot,
                        warm: replica_config.warm,
                    },
                    Vec::new(),
                    last_interval,
                ))),
            },
        };
        snapshotter.initialize_log(initial_entries).await;
        snapshotter.initialize_digest().await;
        snapshotter
    }

    pub async fn start(&self) {
        // create a periodical task to compute snapshot time and interval
        let task = self.task_update_snapshot_params();
        // create a listener that listens to new samples and updates logs and digest as seen fit
        let listener = self.listener_log();

        join!(task, listener);
    }

    async fn listener_log(&self) {
        while let Ok((key, ts)) = self.storage_update.recv_async().await {
            self.update_log(key, ts).await;
        }
    }

    async fn task_update_snapshot_params(&self) {
        sleep(Duration::from_secs(2)).await;
        loop {
            sleep(self.replica_config.delta).await;
            let mut last_snapshot_time = self.content.last_snapshot_time.write().await;
            let mut last_interval = self.content.last_interval.write().await;
            let (time, interval) = Snapshotter::compute_snapshot_params(
                self.replica_config.propagation_delay,
                self.replica_config.delta,
            );
            *last_interval = interval;
            *last_snapshot_time = time;
            drop(last_interval);
            drop(last_snapshot_time);
            self.update_stable_log().await;
        }
    }

    pub fn compute_snapshot_params(
        propagation_delay: Duration,
        delta: Duration,
    ) -> (Timestamp, u64) {
        let now = zenoh::time::new_reception_timestamp();
        let latest_interval = (now
            .get_time()
            .to_system_time()
            .duration_since(super::EPOCH_START)
            .unwrap()
            .as_millis()
            - propagation_delay.as_millis())
            / delta.as_millis();
        let latest_snapshot_time = zenoh::time::Timestamp::new(
            zenoh::time::NTP64::from(Duration::from_millis(
                u64::try_from(delta.as_millis() * latest_interval).unwrap(),
            )),
            *now.get_id(),
        );
        (
            latest_snapshot_time,
            u64::try_from(latest_interval).unwrap(),
        )
    }

    async fn initialize_log(&self, log: Vec<(OwnedKeyExpr, Timestamp)>) {
        let replica_data = &self.content;
        let last_snapshot_time = replica_data.last_snapshot_time.read().await;

        let mut stable_log = replica_data.stable_log.write().await;
        let mut volatile_log = replica_data.volatile_log.write().await;
        for (k, ts) in log {
            // depending on the associated timestamp, either to stable_log or volatile log
            // entries until last_snapshot_time goes to stable
            if ts > *last_snapshot_time {
                if volatile_log.contains_key(&k) {
                    if *volatile_log.get(&k).unwrap() < ts {
                        (*volatile_log).insert(k, ts);
                    }
                } else {
                    (*volatile_log).insert(k, ts);
                }
            } else if stable_log.contains_key(&k) {
                if *stable_log.get(&k).unwrap() < ts {
                    (*stable_log).insert(k, ts);
                }
            } else {
                (*stable_log).insert(k, ts);
            }
        }
        drop(volatile_log);
        drop(stable_log);

        drop(last_snapshot_time);

        self.flush().await;
    }

    async fn initialize_digest(&self) {
        let now = zenoh::time::new_reception_timestamp();
        let replica_data = &self.content;
        let log_locked = replica_data.stable_log.read().await;
        let latest_interval = replica_data.last_interval.read().await;
        let latest_snapshot_time = replica_data.last_snapshot_time.read().await;
        let digest = Digest::create_digest(
            now,
            super::DigestConfig {
                delta: self.replica_config.delta,
                sub_intervals: self.replica_config.subintervals,
                hot: self.replica_config.hot,
                warm: self.replica_config.warm,
            },
            (*log_locked).values().copied().collect(),
            *latest_interval,
        );
        drop(latest_interval);
        drop(latest_snapshot_time);

        let mut digest_lock = replica_data.digest.write().await;
        *digest_lock = digest;
        drop(digest_lock);
    }

    async fn update_log(&self, key: OwnedKeyExpr, ts: Timestamp) {
        let replica_data = &self.content;
        let last_snapshot_time = replica_data.last_snapshot_time.read().await;
        let last_interval = replica_data.last_interval.read().await;
        let mut redundant_content = HashSet::new();
        let mut new_stable_content = HashSet::new();
        if ts > *last_snapshot_time {
            let mut log = replica_data.volatile_log.write().await;
            (*log).insert(key, ts);
            drop(log);
        } else {
            let mut log = replica_data.stable_log.write().await;
            let redundant = (*log).insert(key, ts);
            if redundant.is_some() {
                redundant_content.insert(redundant.unwrap());
            }
            drop(log);
            new_stable_content.insert(ts);
        }
        let mut digest = replica_data.digest.write().await;
        let updated_digest = Digest::update_digest(
            digest.clone(),
            *last_interval,
            *last_snapshot_time,
            new_stable_content,
            redundant_content,
        )
        .await;
        *digest = updated_digest;
    }

    async fn update_stable_log(&self) {
        let replica_data = &self.content;
        let last_snapshot_time = replica_data.last_snapshot_time.read().await;
        let last_interval = replica_data.last_interval.read().await;
        let volatile = replica_data.volatile_log.read().await;
        let mut stable = replica_data.stable_log.write().await;
        let mut still_volatile = HashMap::new();
        let mut new_stable = HashSet::new();
        let mut redundant_stable = HashSet::new();
        for (k, ts) in volatile.clone() {
            if ts > *last_snapshot_time {
                still_volatile.insert(k, ts);
            } else {
                let redundant = stable.insert(k, ts);
                if redundant.is_some() {
                    redundant_stable.insert(redundant.unwrap());
                }
                new_stable.insert(ts);
            }
        }
        drop(stable);
        drop(volatile);

        let mut volatile = replica_data.volatile_log.write().await;
        *volatile = still_volatile;
        drop(volatile);

        let mut digest = replica_data.digest.write().await;
        let updated_digest = Digest::update_digest(
            digest.clone(),
            *last_interval,
            *last_snapshot_time,
            new_stable,
            redundant_stable,
        )
        .await;
        *digest = updated_digest;

        self.flush().await;
    }

    async fn flush(&self) {
        // let log_filename = format!("{}_log.json", self.name.replace('/', "-"));
        // let mut log_file = fs::File::create(log_filename).unwrap();

        let replica_data = &self.content;
        let stable = replica_data.stable_log.read().await;
        let volatile = replica_data.volatile_log.read().await;
        // let l = serde_json::to_string(&(*log)).unwrap();
        // log_file.write_all(l.as_bytes()).unwrap();
        debug!("Stable log updated:: {:?}", stable);
        debug!("Volatile log updated:: {:?}", volatile);
        // drop(log);
    }

    pub async fn get_stable_log(&self) -> HashMap<OwnedKeyExpr, Timestamp> {
        self.content.stable_log.read().await.clone()
    }

    pub async fn get_digest(&self) -> Digest {
        self.content.digest.read().await.clone()
    }
}
