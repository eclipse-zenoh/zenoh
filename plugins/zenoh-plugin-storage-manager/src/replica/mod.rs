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

// This module extends Storage with alignment protocol that aligns storages subscribing to the same key_expr

use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_std::task::sleep;
use flume::Sender;
use futures::join;
use log::{debug, error, trace};
use std::collections::{HashMap, HashSet};
use std::str;
use std::str::FromStr;
use std::time::SystemTime;
use urlencoding::encode;
use zenoh::prelude::r#async::AsyncResolve;
use zenoh::prelude::Sample;
use zenoh::prelude::*;
use zenoh::time::Timestamp;
use zenoh::Session;
use zenoh_backend_traits::config::ReplicaConfig;
use zenoh_core::Result as ZResult;

pub mod align_queryable;
pub mod aligner;
pub mod digest;
pub mod snapshotter;
pub mod storage;

pub use align_queryable::AlignQueryable;
pub use aligner::Aligner;
pub use digest::{Digest, DigestConfig, EraType};
pub use snapshotter::{ReplicationInfo, Snapshotter};
pub use storage::{ReplicationService, StorageService};

const ERA: &str = "era";
const INTERVALS: &str = "intervals";
const SUBINTERVALS: &str = "subintervals";
const CONTENTS: &str = "contents";
pub const EPOCH_START: SystemTime = SystemTime::UNIX_EPOCH;

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
    name: String, // name of replica  -- UUID(zenoh)-<storage_name>
    session: Arc<Session>,
    key_expr: OwnedKeyExpr,
    replica_config: ReplicaConfig,
    digests_published: RwLock<HashSet<u64>>, // checksum of all digests generated and published by this replica
}

impl Replica {
    // This function starts the replica by initializing all the components
    pub async fn start(
        config: ReplicaConfig,
        session: Arc<Session>,
        storage: Box<dyn zenoh_backend_traits::Storage>,
        in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
        out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
        key_expr: OwnedKeyExpr,
        name: &str,
    ) -> ZResult<Sender<crate::StorageMessage>> {
        trace!("[REPLICA]Opening session...");
        let startup_entries = storage.get_all_entries().await?;

        let replica = Replica {
            name: name.to_string(),
            session,
            key_expr,
            replica_config: config,
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
        let snapshotter = Arc::new(Snapshotter::new(rx_log, startup_entries, config.clone()).await);
        // digest sub
        let digest_sub = replica.start_digest_sub(tx_digest);
        // queryable for alignment
        let digest_key = Replica::get_digest_key(replica.key_expr.clone(), config.align_prefix);
        let align_queryable = AlignQueryable::start_align_queryable(
            replica.session.clone(),
            digest_key.clone(),
            &replica.name,
            snapshotter.clone(),
        );
        // aligner
        let aligner = Aligner::start_aligner(
            replica.session.clone(),
            digest_key,
            rx_digest,
            tx_sample,
            snapshotter.clone(),
        );
        // digest pub
        let digest_pub = replica.start_digest_pub(snapshotter.clone());

        //updating snapshot time
        let snapshot_task = snapshotter.start();

        //actual storage
        let replication = ReplicationService {
            aligner_updates: rx_sample,
            log_propagation: tx_log,
        };
        let storage_task = StorageService::start(
            replica.session.clone(),
            replica.key_expr.clone(),
            &replica.name,
            storage,
            in_interceptor,
            out_interceptor,
            Some(replication),
        );

        let result = join!(
            digest_sub,
            align_queryable,
            aligner,
            digest_pub,
            snapshot_task,
            storage_task,
        );

        result.5
    }

    // Create a subscriber to get digests of remote replicas
    // Subscribe on <align_prefix>/<encoded_key_expr>/**
    pub async fn start_digest_sub(&self, tx: Sender<(String, Digest)>) {
        let mut received = HashMap::<String, Timestamp>::new();

        let digest_key = Replica::get_digest_key(
            self.key_expr.clone(),
            self.replica_config.align_prefix.to_string(),
        )
        .join("**")
        .unwrap();

        debug!(
            "[DIGEST_SUB] Creating Subscriber named {} on '{}'",
            self.name, digest_key
        );
        let subscriber = self
            .session
            .declare_subscriber(&digest_key)
            .res_async()
            .await
            .unwrap();
        loop {
            let sample = subscriber.recv_async().await;
            let sample = sample.unwrap();
            let from = &sample.key_expr.as_str()[Replica::get_digest_key(
                self.key_expr.clone(),
                self.replica_config.align_prefix.to_string(),
            )
            .len()
                + 1..];
            trace!(
                "[DIGEST_SUB] From {} Received {} ('{}': '{}')",
                from,
                sample.kind,
                sample.key_expr.as_str(),
                sample.value
            );
            let digest: Digest = serde_json::from_str(&format!("{}", sample.value)).unwrap();
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
                trace!("[DIGEST_SUB] sending {} to aligner", digest.checksum);
                tx.send_async((from.to_string(), digest)).await.unwrap();
            };
            received.insert(from.to_string(), ts);
        }
    }

    // Create a publisher to periodically publish digests from the snapshotter
    // Publish on <align_prefix>/<encoded_key_expr>/<replica_name>
    pub async fn start_digest_pub(&self, snapshotter: Arc<Snapshotter>) {
        let digest_key = Replica::get_digest_key(
            self.key_expr.clone(),
            self.replica_config.align_prefix.to_string(),
        )
        .join(&self.name)
        .unwrap();

        // let expr_id = self.session.declare_keyexpr(&digest_key).res().await.unwrap();
        // debug!("[DIGEST_PUB] => ExprId {}", expr_id);

        debug!("[DIGEST_PUB] Declaring publication on '{}'...", digest_key);
        let publisher = self
            .session
            .declare_publisher(digest_key)
            .local_routing(false)
            .res_async()
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

            trace!("[DIGEST_PUB] Putting Digest : {} ...", digest_json);
            publisher.put(digest_json).res_async().await.unwrap();
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
        // TODO: test this part
        if received.contains_key(from) && *received.get(from).unwrap() > ts {
            // not the latest from that replica
            trace!("[DIGEST_SUB] Dropping older digest at {} from {}", ts, from);
            return false;
        }
        // TODO: test this part
        if config.delta != self.replica_config.delta
            || config.sub_intervals != self.replica_config.subintervals
            || config.hot != self.replica_config.hot
            || config.warm != self.replica_config.warm
        {
            error!("[DIGEST_SUB] mismatching digest configs, cannot be aligned");
            return false;
        }
        true
    }

    fn get_digest_key(key_expr: OwnedKeyExpr, align_prefix: String) -> OwnedKeyExpr {
        let key_expr = encode(&key_expr).to_string();
        OwnedKeyExpr::from_str(&align_prefix)
            .unwrap()
            .join(&key_expr)
            .unwrap()
    }
}
