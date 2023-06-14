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

// This module extends Storage with alignment protocol that aligns storages subscribing to the same key_expr

use crate::backends_mgt::StoreIntercept;
use crate::storages_mgt::StorageMessage;
use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_std::task::sleep;
use flume::{Receiver, Sender};
use futures::{pin_mut, select, FutureExt};
use std::collections::{HashMap, HashSet};
use std::str;
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use urlencoding::encode;
use zenoh::prelude::r#async::*;
use zenoh::time::Timestamp;
use zenoh::Session;
use zenoh_backend_traits::config::{ReplicaConfig, StorageConfig};

pub mod align_queryable;
pub mod aligner;
pub mod digest;
pub mod snapshotter;
pub mod storage;

pub use align_queryable::AlignQueryable;
pub use aligner::Aligner;
pub use digest::{Digest, DigestConfig, EraType, LogEntry};
pub use snapshotter::{ReplicationInfo, Snapshotter};
pub use storage::{ReplicationService, StorageService};

const ERA: &str = "era";
const INTERVALS: &str = "intervals";
const SUBINTERVALS: &str = "subintervals";
const CONTENTS: &str = "contents";
pub const EPOCH_START: SystemTime = SystemTime::UNIX_EPOCH;

pub const ALIGN_PREFIX: &str = "@-digest";
pub const SUBINTERVAL_CHUNKS: usize = 10;

// A replica consists of a storage service and services required for anti-entropy
// To perform anti-entropy, we need a `Digest` that contains the state of the datastore
// `Snapshotter` computes the `Digest` and maintains all related information
// `DigestPublisher` reads the latest `Digest` from the `Snapshotter` and publishes to the network
// `DigestSubscriber` subscribes to `Digest`s from other replicas having the same key_expr
// If the incoming digest is valid, the `DigestSubscriber` forwards it to the `Aligner`
// The `Aligner` identifies mismatches in the contents of the storage with respect to the other storage
// `Aligner` generates a list of missing updates that is then send to the `StorageService`
// When a `StorageService` receives an update, it sends a log to the `Snapshotter`

pub struct Replica {
    // TODO: Discuss if we need to add -<storage_type> for uniqueness
    name: String, // name of replica  -- ID(zenoh)-<storage_name>
    session: Arc<Session>,
    key_expr: OwnedKeyExpr,
    replica_config: ReplicaConfig,
    digests_published: RwLock<HashSet<u64>>, // checksum of all digests generated and published by this replica
}

impl Replica {
    // This function starts the replica by initializing all the components
    pub async fn start(
        session: Arc<Session>,
        store_intercept: StoreIntercept,
        storage_config: StorageConfig,
        name: &str,
        rx: Receiver<StorageMessage>,
    ) {
        log::trace!("[REPLICA] Opening session...");
        let startup_entries = match store_intercept.storage.get_all_entries().await {
            Ok(entries) => {
                let mut result = Vec::new();
                for entry in entries {
                    if entry.0.is_none() {
                        if let Some(prefix) = storage_config.clone().strip_prefix {
                            result.push((prefix, entry.1));
                        } else {
                            log::error!("Empty key found with timestamp `{}`", entry.1);
                        }
                    } else {
                        result.push((
                            StorageService::get_prefixed(
                                &storage_config.strip_prefix,
                                &entry.0.unwrap().into(),
                            ),
                            entry.1,
                        ));
                    }
                }
                result
            }
            Err(e) => {
                log::error!("[REPLICA] Error fetching entries from storage: {}", e);
                return;
            }
        };

        let replica = Replica {
            name: name.to_string(),
            session,
            key_expr: storage_config.key_expr.clone(),
            replica_config: storage_config.replica_config.clone().unwrap(),
            digests_published: RwLock::new(HashSet::new()),
        };

        // Create channels for communication between components
        // channel to queue digests to be aligned
        let (tx_digest, rx_digest) = flume::unbounded();
        // channel for aligner to send missing samples to storage
        let (tx_sample, rx_sample) = flume::unbounded();
        // channel for storage to send logging information back
        let (tx_log, rx_log) = flume::unbounded();

        let config = replica.replica_config.clone();
        // snapshotter
        let snapshotter = Arc::new(Snapshotter::new(rx_log, &startup_entries, &config).await);
        // digest sub
        let digest_sub = replica.start_digest_sub(tx_digest).fuse();
        // queryable for alignment
        let digest_key = Replica::get_digest_key(&replica.key_expr, ALIGN_PREFIX);
        let align_q = AlignQueryable::start_align_queryable(
            replica.session.clone(),
            digest_key.clone(),
            &replica.name,
            snapshotter.clone(),
        )
        .fuse();
        // aligner
        let aligner = Aligner::start_aligner(
            replica.session.clone(),
            digest_key,
            rx_digest,
            tx_sample,
            snapshotter.clone(),
        )
        .fuse();
        // digest pub
        let digest_pub = replica.start_digest_pub(snapshotter.clone()).fuse();

        //updating snapshot time
        let snapshot_task = snapshotter.start().fuse();

        //actual storage
        let replication = ReplicationService {
            empty_start: startup_entries.is_empty(),
            aligner_updates: rx_sample,
            log_propagation: tx_log,
        };
        // channel to pipe the receiver to storage
        let storage_task = StorageService::start(
            replica.session.clone(),
            storage_config,
            &replica.name,
            store_intercept,
            rx,
            Some(replication),
        )
        .fuse();

        pin_mut!(
            digest_sub,
            align_q,
            aligner,
            digest_pub,
            snapshot_task,
            storage_task
        );

        select!(
            () = digest_sub => log::trace!("[REPLICA] Exiting digest subscriber"),
            () = align_q => log::trace!("[REPLICA] Exiting align queryable"),
            () = aligner => log::trace!("[REPLICA] Exiting aligner"),
            () = digest_pub => log::trace!("[REPLICA] Exiting digest publisher"),
            () = snapshot_task => log::trace!("[REPLICA] Exiting snapshot task"),
            () = storage_task => log::trace!("[REPLICA] Exiting storage task"),
        )
    }

    // Create a subscriber to get digests of remote replicas
    // Subscribe on <align_prefix>/<encoded_key_expr>/**
    pub async fn start_digest_sub(&self, tx: Sender<(String, Digest)>) {
        let mut received = HashMap::<String, Timestamp>::new();

        let digest_key = Replica::get_digest_key(&self.key_expr, ALIGN_PREFIX)
            .join("**")
            .unwrap();

        log::debug!(
            "[DIGEST_SUB] Declaring Subscriber named {} on '{}'",
            self.name,
            digest_key
        );
        let subscriber = self
            .session
            .declare_subscriber(&digest_key)
            .allowed_origin(Locality::Remote)
            .res()
            .await
            .unwrap();
        loop {
            let sample = match subscriber.recv_async().await {
                Ok(sample) => sample,
                Err(e) => {
                    log::error!("[DIGEST_SUB] Error receiving sample: {}", e);
                    continue;
                }
            };
            let from = &sample.key_expr.as_str()
                [Replica::get_digest_key(&self.key_expr, ALIGN_PREFIX).len() + 1..];
            log::trace!(
                "[DIGEST_SUB] From {} Received {} ('{}': '{}')",
                from,
                sample.kind,
                sample.key_expr.as_str(),
                sample.value
            );
            let digest: Digest = match serde_json::from_str(&format!("{}", sample.value)) {
                Ok(digest) => digest,
                Err(e) => {
                    log::error!("[DIGEST_SUB] Error in decoding the digest: {}", e);
                    continue;
                }
            };
            let ts = digest.timestamp;
            let to_be_processed = self
                .processing_needed(
                    from,
                    digest.timestamp,
                    digest.checksum,
                    received.clone(),
                    digest.config.clone(),
                )
                .await;
            if to_be_processed {
                log::trace!("[DIGEST_SUB] sending {} to aligner", digest.checksum);
                match tx.send_async((from.to_string(), digest)).await {
                    Ok(()) => {}
                    Err(e) => log::error!("[DIGEST_SUB] Error sending digest to aligner: {}", e),
                }
            };
            received.insert(from.to_string(), ts);
        }
    }

    // Create a publisher to periodically publish digests from the snapshotter
    // Publish on <align_prefix>/<encoded_key_expr>/<replica_name>
    pub async fn start_digest_pub(&self, snapshotter: Arc<Snapshotter>) {
        let digest_key = Replica::get_digest_key(&self.key_expr, ALIGN_PREFIX)
            .join(&self.name)
            .unwrap();

        log::debug!("[DIGEST_PUB] Declaring Publisher on '{}'...", digest_key);
        let publisher = self
            .session
            .declare_publisher(digest_key)
            .res()
            .await
            .unwrap();

        loop {
            sleep(self.replica_config.publication_interval).await;

            let digest = snapshotter.get_digest().await;
            let digest = digest.compress();
            let digest_json = serde_json::to_string(&digest).unwrap();
            let mut digests_published = self.digests_published.write().await;
            digests_published.insert(digest.checksum);
            drop(digests_published);
            drop(digest);

            log::trace!("[DIGEST_PUB] Putting Digest: {} ...", digest_json);
            match publisher.put(digest_json).res().await {
                Ok(()) => {}
                Err(e) => log::error!("[DIGEST_PUB] Digest publication failed: {}", e),
            }
        }
    }

    async fn processing_needed(
        &self,
        from: &str,
        ts: Timestamp,
        checksum: u64,
        received: HashMap<String, Timestamp>,
        config: DigestConfig,
    ) -> bool {
        if checksum == 0 {
            // no values to align
            return false;
        }
        let digests_published = self.digests_published.read().await;
        if digests_published.contains(&checksum) {
            log::trace!("[DIGEST_SUB] Dropping since matching digest already seen");
            return false;
        }
        // TODO: test this part
        if received.contains_key(from) && *received.get(from).unwrap() > ts {
            // not the latest from that replica
            log::trace!("[DIGEST_SUB] Dropping older digest at {} from {}", ts, from);
            return false;
        }
        // TODO: test this part
        if config.delta != self.replica_config.delta
            || config.hot
                != Replica::get_hot_interval_number(
                    self.replica_config.publication_interval,
                    self.replica_config.delta,
                )
        {
            log::error!("[DIGEST_SUB] Mismatching digest configs, cannot be aligned");
            return false;
        }
        true
    }

    fn get_digest_key(key_expr: &OwnedKeyExpr, align_prefix: &str) -> OwnedKeyExpr {
        let key_expr = encode(key_expr).to_string();
        OwnedKeyExpr::from_str(align_prefix)
            .unwrap()
            .join(&key_expr)
            .unwrap()
    }

    pub fn get_hot_interval_number(publication_interval: Duration, delta: Duration) -> usize {
        ((publication_interval.as_nanos() / delta.as_nanos()) as usize) + 1
    }

    pub fn get_warm_interval_number(publication_interval: Duration, delta: Duration) -> usize {
        Replica::get_hot_interval_number(publication_interval, delta) * 5
    }
}
