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

use std::{sync::Arc, time::Duration};

use tokio::{
    sync::{broadcast::Receiver, RwLock},
    task::JoinHandle,
};
use zenoh::{key_expr::OwnedKeyExpr, query::QueryTarget, sample::Locality, session::Session};

use super::{core::Replication, LogLatest};
use crate::storages_mgt::{LatestUpdates, StorageMessage, StorageService};

pub(crate) struct ReplicationService {
    digest_publisher_handle: JoinHandle<()>,
    digest_subscriber_handle: JoinHandle<()>,
    aligner_queryable_handle: JoinHandle<()>,
}

pub(crate) const MAX_RETRY: usize = 2;
pub(crate) const WAIT_PERIOD_SECS: u64 = 4;

impl ReplicationService {
    /// Starts the `ReplicationService`, spawning multiple tasks.
    ///
    /// # Tasks spawned
    ///
    /// This function will spawn two tasks:
    /// 1. One to publish the [Digest].
    /// 2. One to wait on the provided [Receiver] in order to stop the Replication Service,
    ///    attempting to abort all the tasks that were spawned, once a Stop message has been
    ///    received.
    pub async fn spawn_start(
        zenoh_session: Arc<Session>,
        storage_service: StorageService,
        storage_key_expr: OwnedKeyExpr,
        replication_log: Arc<RwLock<LogLatest>>,
        latest_updates: Arc<RwLock<LatestUpdates>>,
        mut rx: Receiver<StorageMessage>,
    ) {
        // We perform a "wait-try" policy because Zenoh needs some time to propagate the routing
        // information and, here, we need to have the queryables propagated.
        //
        // 4 seconds is an arbitrary value.
        let mut attempt = 0;
        let mut received_reply = false;

        while attempt < MAX_RETRY {
            attempt += 1;
            tokio::time::sleep(Duration::from_secs(WAIT_PERIOD_SECS)).await;

            match zenoh_session
                .get(&storage_key_expr)
                // `BestMatching`, the default option for `target`, will try to minimise the storage
                // that are queried and their distance while trying to maximise the key space
                // covered.
                //
                // In other words, if there is a close and complete storage, it will only query this
                // one.
                .target(QueryTarget::BestMatching)
                // The value `Remote` is self-explanatory but why it is needed deserves an
                // explanation: we do not want to query the local database as the purpose is to get
                // the data from other replicas (if there is one).
                .allowed_destination(Locality::Remote)
                .await
            {
                Ok(replies) => {
                    while let Ok(reply) = replies.recv_async().await {
                        received_reply = true;
                        if let Ok(sample) = reply.into_result() {
                            if let Err(e) = storage_service.process_sample(sample).await {
                                tracing::error!("{e:?}");
                            }
                        }
                    }
                }
                Err(e) => tracing::error!("Initial alignment Query failed with: {e:?}"),
            }

            if received_reply {
                break;
            }

            tracing::debug!(
                "Found no Queryable matching '{storage_key_expr}'. Attempt {attempt}/{MAX_RETRY}."
            );
        }

        tokio::task::spawn(async move {
            let replication = Replication {
                zenoh_session,
                replication_log,
                storage_key_expr,
                latest_updates,
                storage: storage_service.storage.clone(),
            };

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

    /// Stops all the tasks spawned by the `ReplicationService`.
    pub fn stop(self) {
        self.digest_publisher_handle.abort();
        self.digest_subscriber_handle.abort();
        self.aligner_queryable_handle.abort();
    }
}
