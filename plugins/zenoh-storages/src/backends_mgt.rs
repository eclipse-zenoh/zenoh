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
use super::storages_mgt::*;
use async_std::channel::{bounded, Sender};
use async_std::sync::{Arc, RwLock};
use async_std::task;
use futures::prelude::*;
use futures::select;
use log::{debug, error, trace, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use zenoh::{ChangeKind, Path, PathExpr, Selector, Value, ZError, ZErrorKind, ZResult, Zenoh};
use zenoh_backend_traits::{
    IncomingDataInterceptor, OutgoingDataInterceptor, PROP_STORAGE_PATH_EXPR,
};
use zenoh_util::{zerror, zerror2};

pub(crate) async fn start_backend(
    backend: Box<dyn zenoh_backend_traits::Backend>,
    admin_path: Path,
    zenoh: Arc<Zenoh>,
) -> ZResult<Sender<bool>> {
    let backend_name = admin_path.clone();
    trace!("Starting backend {}", backend_name);

    // Channel for the task to advertise when ready to receive requests
    let (ready_tx, ready_rx) = bounded::<bool>(1);
    // Channel to stop the task
    let (stop_tx, stop_rx) = bounded::<bool>(1);

    task::spawn(async move {
        let workspace = zenoh.workspace(Some(admin_path.clone())).await.unwrap();
        // admin_path is "/@/.../backend/<beid>"
        // answer to GET on 'admin_path'
        let mut backend_admin = match workspace.register_eval(&PathExpr::from(&admin_path)).await {
            Ok(backend_admin) => backend_admin,
            Err(e) => {
                error!("Error starting backend {} : {}", admin_path, e);
                return;
            }
        };
        // subscribe to PUT/DELETE on 'admin_path'/storage/*
        let storages_admin_selector = Selector::try_from("storage/*").unwrap();
        let mut storages_admin = match workspace.subscribe(&storages_admin_selector).await {
            Ok(storages_admin) => storages_admin,
            Err(e) => {
                error!("Error starting backend {} : {}", admin_path, e);
                return;
            }
        };

        // now that the backend is ready to receive GET/PUT/DELETE,
        // unblock the start_backend() operation below
        if let Err(e) = ready_tx.send(true).await {
            error!("Error starting backend {} : {}", admin_path, e);
            return;
        }

        let in_interceptor: Option<Arc<RwLock<Box<dyn IncomingDataInterceptor>>>> =
            backend.incoming_data_interceptor().map(|i| {
                debug!("Backend {} as an IncomingDataInterceptor", admin_path);
                Arc::new(RwLock::new(i))
            });
        let out_interceptor: Option<Arc<RwLock<Box<dyn OutgoingDataInterceptor>>>> =
            backend.outgoing_data_interceptor().map(|i| {
                debug!("Backend {} as an OutgoingDataInterceptor", admin_path);
                Arc::new(RwLock::new(i))
            });

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
                    trace!("{} received change for {}", admin_path, change.path);
                    match change.kind {
                        ChangeKind::Put => {
                            if let Some(value) = change.value {
                                #[allow(clippy::map_entry)]
                                if !storages_handles.contains_key(&change.path) {
                                    match create_and_start_storage(change.path.clone(), value, &mut backend, in_interceptor.clone(), out_interceptor.clone(), zenoh.clone()).await {
                                        Ok(handle) => {
                                            let _ = storages_handles.insert(change.path, handle);
                                        }
                                        Err(e) => warn!("{}", e),
                                    }
                                } else {
                                    warn!("Storage {} already exists", change.path);
                                }
                            } else {
                                warn!("Received a PUT on {} with invalid value: {:?}", change.path, change.value);
                            }
                        }
                        ChangeKind::Delete =>  {
                            debug!("Delete storage {}", change.path);
                            let _ = storages_handles.remove(&change.path);
                        }
                        ChangeKind::Patch => warn!("PATCH not supported on {}", change.path),
                    }
                },
                _ = stop_rx.recv().fuse() => {
                    trace!("Dropping backend {}", admin_path);
                    return
                }
            );
        }
    });

    // wait for the above task to be ready to receive GET/PUT/DELETE
    let _ = ready_rx.recv().await;
    trace!("Backend {} ready", backend_name);

    Ok(stop_tx)
}

async fn create_and_start_storage(
    admin_path: Path,
    value: Value,
    backend: &mut Box<dyn zenoh_backend_traits::Backend>,
    in_interceptor: Option<Arc<RwLock<Box<dyn IncomingDataInterceptor>>>>,
    out_interceptor: Option<Arc<RwLock<Box<dyn OutgoingDataInterceptor>>>>,
    zenoh: Arc<Zenoh>,
) -> ZResult<Sender<bool>> {
    trace!("Create storage {}", admin_path);
    if let Value::Properties(props) = value {
        let path_expr_str = props.get(PROP_STORAGE_PATH_EXPR).ok_or_else(|| {
            zerror2!(ZErrorKind::Other {
                descr: format!(
                    "Can't create storage {}: no {} property",
                    admin_path, PROP_STORAGE_PATH_EXPR
                )
            })
        })?;
        let path_expr = PathExpr::try_from(path_expr_str.as_str())?;
        let storage = backend.create_storage(props).await?;
        start_storage(
            storage,
            admin_path.clone(),
            path_expr,
            in_interceptor,
            out_interceptor,
            zenoh,
        )
        .await
    } else {
        zerror!(ZErrorKind::Other {
            descr: format!(
                "Received a PUT on {} with invalid value: {:?}",
                admin_path, value
            )
        })
    }
}
