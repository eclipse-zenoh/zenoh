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

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use rand::Rng;
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tracing::{debug_span, Instrument};
use zenoh::{
    key_expr::{
        format::{kedefine, keformat},
        OwnedKeyExpr,
    },
    sample::Locality,
    Session,
};
use zenoh_backend_traits::Storage;

use super::{
    digest::Digest,
    log::LogLatest,
    service::{MAX_RETRY, WAIT_PERIOD_SECS},
};
use crate::{replication::aligner::AlignmentQuery, storages_mgt::LatestUpdates};

kedefine!(
    pub digest_key_expr_formatter: "@-digest/${zid:*}/${storage_ke:**}",
    pub aligner_key_expr_formatter: "@zid/${zid:*}/${storage_ke:**}/aligner",
);

#[derive(Clone)]
pub(crate) struct Replication {
    pub(crate) zenoh_session: Arc<Session>,
    pub(crate) replication_log: Arc<RwLock<LogLatest>>,
    pub(crate) storage_key_expr: OwnedKeyExpr,
    pub(crate) latest_updates: Arc<RwLock<LatestUpdates>>,
    pub(crate) storage: Arc<Mutex<Box<dyn Storage>>>,
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

            // Scope to not forget to release the lock.
            let (publication_interval, propagation_delay, last_elapsed_interval) = {
                let replication_log_guard = replication_log.read().await;
                let configuration = replication_log_guard.configuration();
                let last_elapsed_interval = match configuration.last_elapsed_interval() {
                    Ok(idx) => idx,
                    Err(e) => {
                        tracing::error!(
                            "Fatal error, call to `last_elapsed_interval` failed with: {e:?}"
                        );
                        return;
                    }
                };

                (
                    configuration.interval,
                    configuration.propagation_delay,
                    last_elapsed_interval,
                )
            };

            // We have no control over when a replica is going to be started. The purpose is here
            // is to try to align its publications and make it so that they happen more or less
            // at every interval (+ δ).
            let duration_until_next_interval = {
                let millis_last_elapsed =
                    *last_elapsed_interval as u128 * publication_interval.as_millis();

                if millis_last_elapsed > u64::MAX as u128 {
                    tracing::error!(
                        "Fatal error, the last elapsed interval converted to milliseconds is \
                         higher than u64::MAX. The host is likely misconfigured (internal clock \
                         far ahead in the future?)."
                    );
                    return;
                }

                let millis_since_now =
                    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
                        Ok(duration) => duration.as_millis(),
                        Err(e) => {
                            tracing::error!(
                                "Fatal error, failed to obtain the Duration until `now()`: {e:?}"
                            );
                            return;
                        }
                    };

                Duration::from_millis(
                    (publication_interval.as_millis() - (millis_since_now - millis_last_elapsed))
                        as u64,
                )
            };
            tokio::time::sleep(duration_until_next_interval).await;

            let mut serialization_buffer = Vec::default();
            let mut events = HashMap::default();

            // Internal delay to avoid an "update storm".
            let max_publication_delay = (publication_interval.as_millis() / 3) as u64;

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
                    let mut latest_updates_guard = latest_updates.write().await;
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

                // We do not want to create a "coordinated update storm" with all replicas
                // publishing at the same time, hence we wait some variable additional time.
                let publication_delay = rand::thread_rng().gen_range(0..max_publication_delay);
                tokio::time::sleep(Duration::from_millis(publication_delay)).await;

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
                        publication_delay + propagation_delay.as_millis() as u64
                    );
                } else {
                    tokio::time::sleep(publication_interval - digest_update_duration).await;
                }
            }
        })
    }

    /// Spawns a task that subscribes to the [Digest] published by other Replicas.
    ///
    /// Upon reception of a [Digest], it is compared with the local Replication Log. If this
    /// comparison generates a [DigestDiff], the Aligner of the Replica that generated the [Digest]
    /// that was processed is queried to start an alignment.
    ///
    /// [DigestDiff]: super::digest::DigestDiff
    pub(crate) fn spawn_digest_subscriber(&self) -> JoinHandle<()> {
        let zenoh_session = self.zenoh_session.clone();
        let storage_key_expr = self.storage_key_expr.clone();
        let replication_log = self.replication_log.clone();
        let replication = self.clone();

        tokio::task::spawn(async move {
            let digest_key_sub = match keformat!(
                digest_key_expr_formatter::formatter(),
                zid = "*",
                storage_ke = &storage_key_expr
            ) {
                Ok(key) => key,
                Err(e) => {
                    tracing::error!(
                        "Fatal error, failed to generate a key expression to subscribe to \
                         Digests: {e:?}. The storage will not receive the Replication Digest of \
                         other replicas."
                    );
                    return;
                }
            };

            let mut retry = 0;
            let subscriber = loop {
                match zenoh_session
                    .declare_subscriber(&digest_key_sub)
                    // NOTE: We need to explicitly set the locality to `Remote` as otherwise the
                    //       Digest subscriber will also receive the Digest published by its own
                    //       Digest publisher.
                    .allowed_origin(Locality::Remote)
                    .await
                {
                    Ok(subscriber) => break subscriber,
                    Err(e) => {
                        if retry < MAX_RETRY {
                            retry += 1;
                            tracing::warn!(
                                "Failed to declare Digest subscriber: {e:?}. Attempt \
                                 {retry}/{MAX_RETRY}."
                            );
                            tokio::time::sleep(Duration::from_secs(WAIT_PERIOD_SECS)).await;
                        } else {
                            tracing::error!(
                                "Could not declare Digest subscriber. The storage will not \
                                 receive the Replication Digest of other replicas."
                            );
                            return;
                        }
                    }
                }
            };

            tracing::debug!("Subscribed to {digest_key_sub}");

            loop {
                if let Ok(sample) = subscriber.recv_async().await {
                    let parsed_ke = match digest_key_expr_formatter::parse(sample.key_expr()) {
                        Ok(parsed_ke) => parsed_ke,
                        Err(e) => {
                            tracing::error!(
                                "Failed to parse key expression associated with Digest \
                                 publication < {} >: {e:?}",
                                sample.key_expr()
                            );
                            continue;
                        }
                    };
                    let source_zid = parsed_ke.zid();

                    let span = debug_span!(
                        "Digest subscriber",
                        source_zid = source_zid.as_str(),
                        request_id = uuid::Uuid::new_v4().simple().to_string()
                    );

                    // Async block such that we can `instrument` it in an asynchronous compatible
                    // manner using the `span` we created just above.
                    async {
                        let other_digest = match bincode::deserialize::<Digest>(
                            &sample.payload().deserialize::<Cow<[u8]>>(),
                        ) {
                            Ok(other_digest) => other_digest,
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to deserialize Payload as Digest: {e:?}. Skipping."
                                );
                                return;
                            }
                        };

                        tracing::debug!("Replication digest received");

                        let digest = match replication_log.read().await.digest() {
                            Ok(digest) => digest,
                            Err(e) => {
                                tracing::error!(
                                    "Fatal error, failed to compute local Digest: {e:?}"
                                );
                                return;
                            }
                        };

                        if let Some(digest_diff) = digest.diff(other_digest) {
                            tracing::debug!("Potential misalignment detected");

                            let replica_aligner_ke = match keformat!(
                                aligner_key_expr_formatter::formatter(),
                                storage_ke = &storage_key_expr,
                                zid = source_zid,
                            ) {
                                Ok(key) => key,
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to generate a key expression to contact aligner: \
                                         {e:?}"
                                    );
                                    return;
                                }
                            };

                            replication.spawn_query_replica_aligner(
                                replica_aligner_ke,
                                AlignmentQuery::Diff(digest_diff),
                            );
                        }
                    }
                    .instrument(span)
                    .await;
                }
            }
        })
    }

    /// Spawns a task that handles alignment queries.
    ///
    /// An alignment query will always come from a Replica. Hence, as multiple Replicas could query
    /// at the same time, for each received query a new task is spawned. This newly spawned task is
    /// responsible for fetching in the Replication Log or in the Storage the relevant information
    /// to send to the Replica such that it can align its own Storage.
    pub(crate) fn spawn_aligner_queryable(&self) -> JoinHandle<()> {
        let zenoh_session = self.zenoh_session.clone();
        let storage_key_expr = self.storage_key_expr.clone();
        let replication = self.clone();

        tokio::task::spawn(async move {
            let aligner_ke = match keformat!(
                aligner_key_expr_formatter::formatter(),
                zid = zenoh_session.zid(),
                storage_ke = storage_key_expr,
            ) {
                Ok(ke) => ke,
                Err(e) => {
                    tracing::error!(
                        "Fatal error, failed to generate a key expression for the Aligner \
                         queryable: {e:?}. The storage will NOT align with other replicas."
                    );
                    return;
                }
            };

            let mut retry = 0;
            let queryable = loop {
                match zenoh_session
                    .declare_queryable(&aligner_ke)
                    .allowed_origin(Locality::Remote)
                    .await
                {
                    Ok(queryable) => break queryable,
                    Err(e) => {
                        if retry < MAX_RETRY {
                            retry += 1;
                            tracing::warn!(
                                "Failed to declare the Aligner queryable: {e:?}. Attempt \
                                 {retry}/{MAX_RETRY}."
                            );
                            tokio::time::sleep(Duration::from_secs(WAIT_PERIOD_SECS)).await;
                        } else {
                            tracing::error!(
                                "Could not declare the Aligner queryable. This storage will NOT \
                                 align with other replicas."
                            );
                            return;
                        }
                    }
                }
            };

            tracing::debug!("Declared Aligner queryable: {aligner_ke}");

            while let Ok(query) = queryable.recv_async().await {
                if query.attachment().is_none() {
                    tracing::debug!("Skipping query with empty Attachment");
                    continue;
                }

                tracing::trace!("Received Alignment Query");

                let replication = replication.clone();
                tokio::task::spawn(async move { replication.aligner(query).await });
            }
        })
    }
}
