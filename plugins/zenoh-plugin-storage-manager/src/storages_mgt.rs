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
use async_std::channel::{bounded, Sender};
use async_std::task;
use futures::select;
use futures::stream::StreamExt;
use futures::FutureExt;
use log::{debug, error, trace, warn};
use std::sync::Arc;
use zenoh::prelude::*;
use zenoh::query::{QueryConsolidation, QueryTarget, Target};
use zenoh::queryable;
use zenoh::Session;
use zenoh_backend_traits::Query;
use zenoh_core::Result as ZResult;

pub(crate) enum StorageMessage {
    Stop,
    GetStatus(Sender<serde_json::Value>),
}

pub(crate) async fn start_storage(
    mut storage: Box<dyn zenoh_backend_traits::Storage>,
    admin_key: String,
    key_expr: String,
    in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    zenoh: Arc<Session>,
) -> ZResult<Sender<StorageMessage>> {
    debug!("Start storage {} on {}", admin_key, key_expr);

    let (tx, rx) = bounded(1);
    task::spawn(async move {
        // subscribe on key_expr
        let mut storage_sub = match zenoh.subscribe(&key_expr).await {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                error!("Error starting storage {} : {}", admin_key, e);
                return;
            }
        };

        // align with other storages, querying them on key_expr,
        // with starttime to get historical data (in case of time-series)
        let query_target = QueryTarget {
            kind: queryable::STORAGE,
            target: Target::All,
        };
        let mut replies = match zenoh
            .get(&Selector::from(&key_expr).with_value_selector("?(starttime=0)"))
            .target(query_target)
            .consolidation(QueryConsolidation::none())
            .await
        {
            Ok(replies) => replies,
            Err(e) => {
                error!("Error aligning storage {} : {}", admin_key, e);
                return;
            }
        };
        while let Some(reply) = replies.next().await {
            log::trace!(
                "Storage {} aligns data {}",
                admin_key,
                reply.sample.key_expr
            );
            // Call incoming data interceptor (if any)
            let sample = if let Some(ref interceptor) = in_interceptor {
                interceptor(reply.sample)
            } else {
                reply.sample
            };
            // Call storage
            if let Err(e) = storage.on_sample(sample).await {
                warn!(
                    "Storage {} raised an error aligning a sample: {}",
                    admin_key, e
                );
            }
        }

        // answer to queries on key_expr
        let mut storage_queryable = match zenoh.queryable(&key_expr).kind(queryable::STORAGE).await
        {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                error!("Error starting storage {} : {}", admin_key, e);
                return;
            }
        };

        loop {
            select!(
                // on sample for key_expr
                sample = storage_sub.next() => {
                    // Call incoming data interceptor (if any)
                    let sample = if let Some(ref interceptor) = in_interceptor {
                        interceptor(sample.unwrap())
                    } else {
                        sample.unwrap()
                    };
                    // Call storage
                    if let Err(e) = storage.on_sample(sample).await {
                        warn!("Storage {} raised an error receiving a sample: {}", admin_key, e);
                    }
                },
                // on query on key_expr
                query = storage_queryable.next() => {
                    let q = query.unwrap();
                    // wrap zenoh::Query in zenoh_backend_traits::Query
                    // with outgoing interceptor
                    let query = Query::new(q, out_interceptor.clone());
                    if let Err(e) = storage.on_query(query).await {
                        warn!("Storage {} raised an error receiving a query: {}", admin_key, e);
                    }
                },
                // on storage handle drop
                message = rx.recv().fuse() => {
                    match message {
                        Ok(StorageMessage::Stop) => {
                            trace!("Dropping storage {}", admin_key);
                            return
                        },
                        Ok(StorageMessage::GetStatus(tx)) => {
                            std::mem::drop(tx.send(storage.get_admin_status()).await);
                        }
                        Err(e) => {log::error!("Storage Message Channel Error: {}", e); return},
                    };
                }
            );
        }
    });

    Ok(tx)
}
