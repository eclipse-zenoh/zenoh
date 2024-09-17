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

use std::{collections::HashMap, sync::Arc, time::Instant};

use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use zenoh::{
    key_expr::{
        format::{kedefine, keformat},
        OwnedKeyExpr,
    },
    Session,
};

use super::{digest::Digest, log::LogLatest};
use crate::storages_mgt::LatestUpdates;

kedefine!(
    pub digest_key_expr_formatter: "@-digest/${zid:*}/${storage_ke:**}",
);

#[derive(Clone)]
pub(crate) struct Replication {
    pub(crate) zenoh_session: Arc<Session>,
    pub(crate) replication_log: Arc<RwLock<LogLatest>>,
    pub(crate) storage_key_expr: OwnedKeyExpr,
    pub(crate) latest_updates: Arc<Mutex<LatestUpdates>>,
}

impl Replication {
    /// Spawns a task that periodically publishes the [Digest] of the Replication [Log].
    ///
    /// This task will perform the following steps:
    /// 1. It will swap the `latest_updates` structure with an empty one -- with the sole purpose of
    ///    minimising the contention on the StorageService.
    /// 2. With the content from the `latest_updates`, it will update the Replication [Log].
    /// 3. It will recompute the [Digest].
    /// 4. It will publish the [Digest]. The periodicity of this publication is dictated by the
    ///    `interval` configuration option.
    ///
    /// [Log]: crate::replication::log::LogLatest
    pub(crate) fn spawn_digest_publisher(&self) -> JoinHandle<()> {
        let zenoh_session = self.zenoh_session.clone();
        let storage_key_expr = self.storage_key_expr.clone();
        let replication_log = self.replication_log.clone();
        let latest_updates = self.latest_updates.clone();

        tokio::task::spawn(async move {
            let digest_key_put = match keformat!(
                digest_key_expr_formatter::formatter(),
                zid = zenoh_session.zid(),
                storage_ke = storage_key_expr
            ) {
                Ok(key) => key,
                Err(e) => {
                    tracing::error!(
                        "Failed to generate a key expression to publish the digest: {e:?}"
                    );
                    return;
                }
            };

            let replication_log_guard = replication_log.read().await;
            let publication_interval = replication_log_guard.configuration().interval;
            let propagation_delay = replication_log_guard.configuration().propagation_delay;
            drop(replication_log_guard);

            let mut serialization_buffer = Vec::default();
            let mut events = HashMap::default();

            let mut digest_update_start: Instant;
            let mut digest: Digest;
            loop {
                digest_update_start = Instant::now();

                // The publisher will be awoken every multiple of `publication_interval`.
                //
                // Except that we want to take into account the time it takes for a publication to
                // reach this Zenoh node. Hence, we sleep for `propagation_delay` to, hopefully,
                // catch the publications that are in transit.
                tokio::time::sleep(propagation_delay).await;

                {
                    let mut latest_updates_guard = latest_updates.lock().await;
                    std::mem::swap(&mut events, &mut latest_updates_guard);
                }

                {
                    let mut replication_guard = replication_log.write().await;
                    replication_guard.update(events.drain().map(|(_, event)| event));
                    digest = match replication_guard.digest() {
                        Ok(digest) => digest,
                        Err(e) => {
                            tracing::error!("Fatal error, failed to compute the Digest: {e:?}");
                            return;
                        }
                    };
                }

                if let Err(e) = bincode::serialize_into(&mut serialization_buffer, &digest) {
                    tracing::warn!("Failed to serialise the replication Digest: {e:?}");
                    continue;
                }

                // To try to minimise the allocations performed, we extract the current capacity
                // of the buffer (capacity >= len) to later call `std::mem::replace` with a
                // buffer that, hopefully, has enough memory.
                let buffer_capacity = serialization_buffer.capacity();

                match zenoh_session
                    .put(
                        &digest_key_put,
                        std::mem::replace(
                            &mut serialization_buffer,
                            Vec::with_capacity(buffer_capacity),
                        ),
                    )
                    .await
                {
                    Ok(_) => tracing::trace!("Published Digest: {digest:?}"),
                    Err(e) => tracing::error!("Failed to publish the replication Digest: {e:?}"),
                }

                let digest_update_duration = digest_update_start.elapsed();
                if digest_update_duration > publication_interval {
                    tracing::warn!(
                        "The duration it took to update and publish the Digest is superior to the \
                         duration of an Interval ({} ms), we recommend increasing the duration of \
                         the latter. Digest update: {} ms (incl. delay: {} ms)",
                        publication_interval.as_millis(),
                        digest_update_duration.as_millis(),
                        propagation_delay.as_millis(),
                    );
                } else {
                    tokio::time::sleep(publication_interval - digest_update_duration).await;
                }
            }
        })
    }
}
