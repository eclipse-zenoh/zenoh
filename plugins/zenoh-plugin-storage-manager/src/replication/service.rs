//
// Copyright (c) 2024 ZettaScale Technology
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

use std::sync::Arc;

use tokio::{
    sync::{broadcast::Receiver, RwLock},
    task::JoinHandle,
};
use zenoh::{key_expr::OwnedKeyExpr, session::Session};

use super::{core::Replication, LogLatest};
use crate::storages_mgt::{LatestUpdates, StorageMessage, StorageService};

pub(crate) struct ReplicationService {
    digest_publisher_handle: JoinHandle<()>,
    digest_subscriber_handle: JoinHandle<()>,
    aligner_queryable_handle: JoinHandle<()>,
}

impl ReplicationService {
    /// Starts the `ReplicationService`, spawning multiple tasks.
    ///
    /// # Initial alignment
    ///
    /// To optimise network resources, if the Storage is empty an "initial alignment" will be
    /// performed: if a Replica is detected, a query will be made to retrieve the entire content of
    /// its Storage.
    ///
    /// # Tasks spawned
    ///
    /// This function will spawn four long-lived tasks:
    /// 1. One to publish the [Digest].
    /// 2. One to receive the [Digest] of other Replica.
    /// 3. One to receive alignment queries of other Replica.
    /// 4. One to wait on the provided [Receiver] in order to stop the Replication Service,
    ///    attempting to abort all the tasks that were spawned, once a Stop message has been
    ///    received.
    pub async fn spawn_start(
        zenoh_session: Arc<Session>,
        storage_service: Arc<StorageService>,
        storage_key_expr: OwnedKeyExpr,
        replication_log: Arc<RwLock<LogLatest>>,
        latest_updates: Arc<RwLock<LatestUpdates>>,
        mut rx: Receiver<StorageMessage>,
    ) {
        let replication = Replication {
            zenoh_session,
            replication_log,
            storage_key_expr,
            latest_updates,
            storage_service,
        };

        if replication
            .replication_log
            .read()
            .await
            .intervals
            .is_empty()
        {
            replication.initial_alignment().await;
        }

        tokio::task::spawn(async move {
            let replication_service = Self {
                digest_publisher_handle: replication.spawn_digest_publisher(),
                digest_subscriber_handle: replication.spawn_digest_subscriber(),
                aligner_queryable_handle: replication.spawn_aligner_queryable(),
            };

            while let Ok(storage_message) = rx.recv().await {
                if matches!(storage_message, StorageMessage::Stop) {
                    replication_service.stop();
                    return;
                }
            }
        });
    }

    /// Stops all the long-lived tasks spawned by the `ReplicationService`.
    pub fn stop(self) {
        self.digest_publisher_handle.abort();
        self.digest_subscriber_handle.abort();
        self.aligner_queryable_handle.abort();
    }
}
