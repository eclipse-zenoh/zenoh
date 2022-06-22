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
use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_std::task::sleep;
use flume::Sender;
use futures::join;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::str;
use std::time::SystemTime;
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

pub struct Replica {
    name: String, // name of replica  -- UUID(zenoh)-<storage_name>((-<storage_type>??))
    session: Arc<Session>, // zenoh session used by the replica
    key_expr: String, // key expression of the storage to be functioning as a replica
    replica_config: ReplicaConfig, // replica configuration - if some, replica, if none, normal storage
    digests_published: RwLock<HashSet<u64>>, // checksum of all digests generated and published by this replica
}

impl Replica {
    pub async fn start(
        config: ReplicaConfig,
        session: Arc<Session>,
        storage: Box<dyn zenoh_backend_traits::Storage>,
        in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
        out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
        key_expr: &str,
        name: &str,
    ) -> ZResult<Sender<crate::StorageMessage>> {
        info!("[REPLICA]Opening session...");
        let startup_entries = storage.get_all_entries().await?;

        let replica = Replica {
            name: name.to_string(),
            session,
            key_expr: key_expr.to_string(),
            replica_config: config,
            digests_published: RwLock::new(HashSet::new()),
        };

        // channel to queue digests to be aligned
        let (tx_digest, rx_digest) = flume::unbounded();
        // channel for aaligner to send missing samples to storage
        let (tx_sample, rx_sample) = flume::unbounded();
        // channel for storage to send loggin information back
        let (tx_log, rx_log) = flume::unbounded();
        let config = replica.replica_config.clone();
        // snapshotter
        let snapshotter = Arc::new(Snapshotter::new(rx_log, startup_entries, config.clone()).await);
        // digest sub
        let digest_sub = replica.start_digest_sub(tx_digest);
        // eval for align
        let digest_key = Replica::get_digest_key(key_expr.to_string(), config.align_prefix);
        let align_queryable = AlignQueryable::start_align_queryable(
            replica.session.clone(),
            &digest_key,
            &replica.name,
            snapshotter.clone(),
        );
        // aligner
        let aligner = Aligner::start_aligner(
            replica.session.clone(),
            &digest_key,
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
            &replica.key_expr,
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

    pub async fn start_digest_sub(&self, tx: Sender<(String, Digest)>) {
        let mut received = HashMap::<String, Timestamp>::new();

        let digest_key = format!(
            "{}/**",
            Replica::get_digest_key(
                self.key_expr.to_string(),
                self.replica_config.align_prefix.to_string()
            )
        );

        debug!(
            "[DIGEST_SUB]Creating Subscriber named {} on '{}'... repeating .. '{}'....",
            self.name, digest_key, digest_key
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
                self.key_expr.to_string(),
                self.replica_config.align_prefix.to_string(),
            )
            .len()..];
            debug!(
                "[DIGEST_SUB]>> [Digest Subscriber] From {} Received {} ('{}': '{}')",
                from,
                sample.kind,
                sample.key_expr.as_str(),
                sample.value
            );
            let digest: Digest = serde_json::from_str(&format!("{}", sample.value)).unwrap();
            let ts = digest.timestamp;
            let to_be_processed = self
                .processing_needed(from, digest.timestamp, digest.checksum, received.clone())
                .await;
            if to_be_processed {
                debug!("[DIGEST_SUB] sending {} to aligner", digest.checksum);
                tx.send_async((from.to_string(), digest)).await.unwrap();
            };
            received.insert(from.to_string(), ts);
        }
    }

    pub async fn start_digest_pub(&self, snapshotter: Arc<Snapshotter>) {
        let digest_key = format!(
            "{}/{}",
            Replica::get_digest_key(
                self.key_expr.to_string(),
                self.replica_config.align_prefix.to_string()
            ),
            self.name
        );

        debug!(
            "[DIGEST_PUB]Declaring digest on key expression '{}'...",
            digest_key
        );
        // let expr_id = self.session.declare_keyexpr(&digest_key).res().await.unwrap();
        // debug!("[DIGEST_PUB] => ExprId {}", expr_id);

        debug!("[DIGEST_PUB]Declaring publication on '{}'...", digest_key);
        let publisher = self
            .session
            .declare_publisher(digest_key)
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

            debug!("[DIGEST_PUB]Putting Digest : {} ...", digest_json);
            publisher.put(digest_json).res_async().await.unwrap();
        }
    }

    async fn processing_needed(
        &self,
        from: &str,
        ts: Timestamp,
        checksum: u64,
        received: HashMap<String, Timestamp>,
    ) -> bool {
        if *from == self.name {
            debug!("[DIGEST_SUB]Dropping own digest with checksum {}", checksum);
            return false;
        }
        // TODO: test this part
        if received.contains_key(from) && *received.get(from).unwrap() > ts {
            // not the latest from that replica
            debug!("[DIGEST_SUB]Dropping older digest at {} from {}", ts, from);
            return false;
        }
        //
        if checksum == 0 {
            return false;
        }

        true
    }

    fn get_digest_key(key_expr: String, align_prefix: String) -> String {
        let mut key_expr = key_expr;
        if key_expr.ends_with("**") {
            key_expr = key_expr.strip_suffix("**").unwrap().to_string();
        }
        if key_expr.ends_with('/') {
            key_expr = key_expr.strip_suffix('/').unwrap().to_string();
        }
        format!("{}/{}", align_prefix, key_expr)
    }
}
