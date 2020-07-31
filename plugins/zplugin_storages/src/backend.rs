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
use async_std::sync::{channel, Sender};
use async_std::task;
use futures::prelude::*;
use futures::select;
use log::debug;
use log::warn;
use std::collections::HashMap;
use std::convert::TryFrom;
use zenoh::net::{queryable, Reliability, SubInfo, SubMode};
use zenoh::{ChangeKind, Path, PathExpr, Selector, Value, Workspace, ZError, ZErrorKind, ZResult};
use zenoh_util::{zerror, zerror2};

pub(crate) const STORAGE_PATH_EXPR_PROPERTY: &str = "path_expr";

pub(crate) async fn start_backend(
    backend: Box<dyn zenoh_backend_core::Backend>,
    admin_path: Path,
    workspace: Workspace,
) -> ZResult<Sender<bool>> {
    debug!("Start backend {}", admin_path);
    // admin_path is "/@/.../backend/<beid>"
    // answer to GET on 'admin_path'
    let mut backend_admin = workspace
        .register_eval(&PathExpr::from(&admin_path))
        .await?;
    // subscribe to PUT/DELETE on 'admin_path'/storage/*
    let storages_admin_selector = Selector::try_from(format!("{}/storage/*", &admin_path)).unwrap();
    let mut storages_admin = workspace.subscribe(&storages_admin_selector).await?;

    let (tx, rx) = channel::<bool>(1);
    task::spawn(async move {
        let mut backend = backend;
        // Map owning handles on alive storages for this backend.
        // Once dropped, a handle will release/stop the backend.
        let mut storages_handles: HashMap<Path, Sender<bool>> = HashMap::new();
        loop {
            select!(
                // on get request on backend_admin
                get = backend_admin.next().fuse() => {
                    let get = get.unwrap();
                    get.reply(admin_path.clone(), backend.get_admin_status().await).await;
                },

                // on change for storages_admin
                change = storages_admin.next().fuse() => {
                    let change = change.unwrap();
                    debug!("{} received change for {}", admin_path, change.path);
                    match change.kind {
                        ChangeKind::PUT => {
                            if let Some(value) = change.value {
                                match create_and_start_storage(change.path.clone(), value, &mut backend, workspace.clone()).await {
                                    Ok(handle) => {
                                        let _ = storages_handles.insert(change.path, handle);
                                    }
                                    Err(e) => warn!("{}", e),
                                }
                            } else {
                                warn!("Received a PUT on {} with invalid value: {:?}", change.path, change.value);
                            }
                        }
                        ChangeKind::DELETE =>  {
                            debug!("Delete storage {}", change.path);
                            let _ = storages_handles.remove(&change.path);
                        }
                        ChangeKind::PATCH => warn!("PATCH not supported on {}", change.path),
                    }
                },
                _ = rx.recv().fuse() => {
                    debug!("Dropping backend {}", admin_path);
                    return ()
                }
            );
        }
    });

    Ok(tx)
}

async fn create_and_start_storage(
    admin_path: Path,
    value: Value,
    backend: &mut Box<dyn zenoh_backend_core::Backend>,
    workspace: Workspace,
) -> ZResult<Sender<bool>> {
    if let Value::Properties(props) = value {
        let path_expr_str = props.get(STORAGE_PATH_EXPR_PROPERTY).ok_or_else(|| {
            zerror2!(ZErrorKind::Other {
                descr: format!(
                    "Can't create storage {}: no {} property",
                    admin_path, STORAGE_PATH_EXPR_PROPERTY
                )
            })
        })?;
        let path_expr = PathExpr::try_from(path_expr_str.as_str())?;
        let storage = backend.create_storage(props).await?;
        start_storage(storage, admin_path.clone(), path_expr, workspace.clone()).await
    } else {
        zerror!(ZErrorKind::Other {
            descr: format!(
                "Received a PUT on {} with invalid value: {:?}",
                admin_path, value
            )
        })
    }
}

async fn start_storage(
    storage: Box<dyn zenoh_backend_core::Storage>,
    admin_path: Path,
    path_expr: PathExpr,
    workspace: Workspace,
) -> ZResult<Sender<bool>> {
    debug!("Start storage {} on {}", admin_path, path_expr);
    // admin_path is "/@/.../storage/<stid>"
    // answer to GET on 'admin_path'
    let mut storage_admin = workspace
        .register_eval(&PathExpr::from(&admin_path))
        .await?;

    // subscribe on path_expr
    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    let mut storage_sub = workspace
        .session()
        .declare_subscriber(&path_expr.to_string().into(), &sub_info)
        .await
        .unwrap();

    // answer to queries on path_expr
    let mut storage_queryable = workspace
        .session()
        .declare_queryable(&path_expr.to_string().into(), queryable::STORAGE)
        .await
        .unwrap();

    let (tx, rx) = channel::<bool>(1);
    task::spawn(async move {
        let mut storage = storage;
        loop {
            select!(
                // on get request on storage_admin
                get = storage_admin.next().fuse() => {
                    let get = get.unwrap();
                    get.reply(admin_path.clone(), storage.get_admin_status().await).await;
                },
                // on sample for path_expr
                sample = storage_sub.next().fuse() => {
                    let sample = sample.unwrap();
                    if let Err(e) = storage.on_sample(sample).await {
                        warn!("Storage {} raised an error receiving a sample: {}", admin_path, e);
                    }
                },
                // on query on path_expr
                query = storage_queryable.next().fuse() => {
                    let query = query.unwrap();
                    if let Err(e) = storage.on_query(query).await {
                        warn!("Storage {} raised an error receiving a query: {}", admin_path, e);
                    }
                },
                // on storage handle drop
                _ = rx.recv().fuse() => {
                    debug!("Dropping storage {}", admin_path);
                    return ()
                }
            );
        }
    });

    Ok(tx)
}
