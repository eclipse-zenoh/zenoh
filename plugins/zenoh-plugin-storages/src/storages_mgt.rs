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
use async_std::channel::{bounded, Sender};
use async_std::sync::{Arc, RwLock};
use async_std::task;
use futures::prelude::*;
use futures::select;
use log::{debug, error, trace, warn};
use zenoh::net::{
    queryable, QueryConsolidation, QueryTarget, Reliability, SubInfo, SubMode, Target,
};
use zenoh::{Path, PathExpr, ZResult, Zenoh};
use zenoh_backend_traits::{IncomingDataInterceptor, OutgoingDataInterceptor, Query};

pub(crate) async fn start_storage(
    mut storage: Box<dyn zenoh_backend_traits::Storage>,
    admin_path: Path,
    path_expr: PathExpr,
    in_interceptor: Option<Arc<RwLock<Box<dyn IncomingDataInterceptor>>>>,
    out_interceptor: Option<Arc<RwLock<Box<dyn OutgoingDataInterceptor>>>>,
    zenoh: Arc<Zenoh>,
) -> ZResult<Sender<bool>> {
    debug!("Start storage {} on {}", admin_path, path_expr);

    let (tx, rx) = bounded::<bool>(1);
    task::spawn(async move {
        let workspace = zenoh.workspace(Some(admin_path.clone())).await.unwrap();

        // subscribe on path_expr
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };
        let mut storage_sub = match workspace
            .session()
            .declare_subscriber(&path_expr.to_string().into(), &sub_info)
            .await
        {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                error!("Error starting storage {} : {}", admin_path, e);
                return;
            }
        };

        // align with other storages, querying them on path_expr,
        // with starttime to get historical data (in case of time-series)
        let query_target = QueryTarget {
            kind: queryable::STORAGE,
            target: Target::All,
        };
        let mut replies = match workspace
            .session()
            .query(
                &path_expr.to_string().into(),
                "?(starttime=0)",
                query_target,
                QueryConsolidation::none(),
            )
            .await
        {
            Ok(replies) => replies,
            Err(e) => {
                error!("Error aligning storage {} : {}", admin_path, e);
                return;
            }
        };
        while let Some(reply) = replies.next().await {
            log::trace!("Storage {} aligns data {}", admin_path, reply.data.res_name);
            // Call incoming data interceptor (if any)
            let sample = if let Some(ref interceptor) = in_interceptor {
                interceptor.read().await.on_sample(reply.data).await
            } else {
                reply.data
            };
            // Call storage
            if let Err(e) = storage.on_sample(sample).await {
                warn!(
                    "Storage {} raised an error aligning a sample: {}",
                    admin_path, e
                );
            }
        }

        // admin_path is "/@/.../storage/<stid>"
        // answer to GET on 'admin_path'
        let mut storage_admin = match workspace.register_eval(&PathExpr::from(&admin_path)).await {
            Ok(storages_admin) => storages_admin,
            Err(e) => {
                error!("Error starting storage {} : {}", admin_path, e);
                return;
            }
        };

        // answer to queries on path_expr
        let mut storage_queryable = match workspace
            .session()
            .declare_queryable(&path_expr.to_string().into(), queryable::STORAGE)
            .await
        {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                error!("Error starting storage {} : {}", admin_path, e);
                return;
            }
        };

        loop {
            select!(
                // on get request on storage_admin
                get = storage_admin.next().fuse() => {
                    let get = get.unwrap();
                    get.reply(admin_path.clone(), storage.get_admin_status().await).await;
                },
                // on sample for path_expr
                sample = storage_sub.stream().next().fuse() => {
                    // Call incoming data interceptor (if any)
                    let sample = if let Some(ref interceptor) = in_interceptor {
                        interceptor.read().await.on_sample(sample.unwrap()).await
                    } else {
                        sample.unwrap()
                    };
                    // Call storage
                    if let Err(e) = storage.on_sample(sample).await {
                        warn!("Storage {} raised an error receiving a sample: {}", admin_path, e);
                    }
                },
                // on query on path_expr
                query = storage_queryable.stream().next().fuse() => {
                    let q = query.unwrap();
                    // wrap zenoh::net::Query in zenoh_backend_traits::Query
                    // with outgoing interceptor
                    let query = Query::new(q, out_interceptor.clone());
                    if let Err(e) = storage.on_query(query).await {
                        warn!("Storage {} raised an error receiving a query: {}", admin_path, e);
                    }
                },
                // on storage handle drop
                _ = rx.recv().fuse() => {
                    trace!("Dropping storage {}", admin_path);
                    return
                }
            );
        }
    });

    Ok(tx)
}
