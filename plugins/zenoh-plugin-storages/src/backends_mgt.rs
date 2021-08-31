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
use zenoh::{encoding, Sample, SampleKind, Session, Value, ZError, ZErrorKind, ZResult};
use zenoh_backend_traits::{
    IncomingDataInterceptor, OutgoingDataInterceptor, PROP_STORAGE_PATH_EXPR,
};
use zenoh_util::{zerror, zerror2};

pub(crate) async fn start_backend(
    backend: Box<dyn zenoh_backend_traits::Backend>,
    admin_path: String,
    zenoh: Arc<Session>,
) -> ZResult<Sender<bool>> {
    let backend_name = admin_path.clone();
    trace!("Starting backend {}", backend_name);

    // Channel for the task to advertise when ready to receive requests
    let (ready_tx, ready_rx) = bounded::<bool>(1);
    // Channel to stop the task
    let (stop_tx, stop_rx) = bounded::<bool>(1);

    task::spawn(async move {
        // admin_path is "/@/.../backend/<beid>"
        // answer to GET on 'admin_path'
        let mut backend_admin = match zenoh.register_queryable(&admin_path.as_str().into()).await {
            Ok(backend_admin) => backend_admin,
            Err(e) => {
                error!("Error starting backend {} : {}", admin_path, e);
                return;
            }
        };
        // subscribe to PUT/DELETE on 'admin_path'/storage/*
        let storages_admin_selector = format!("{}/storage/*", admin_path);
        let mut storages_admin = match zenoh.subscribe(&storages_admin_selector.into()).await {
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
        let mut storages_handles: HashMap<String, Sender<bool>> = HashMap::new();
        loop {
            select!(
                // on query on backend_admin
                query = backend_admin.receiver().next().fuse() => {
                    let query = query.unwrap();
                    query.reply_async(Sample::new(admin_path.to_string(), backend.get_admin_status().await)).await;
                },

                // on sample for storages_admin
                sample = storages_admin.receiver().next().fuse() => {
                    let sample = sample.unwrap();
                    trace!("{} received change for {}", admin_path, sample.res_name);
                    match sample.kind {
                        SampleKind::Put => {
                            #[allow(clippy::map_entry)]
                            if !storages_handles.contains_key(&sample.res_name) {
                                match create_and_start_storage(sample.res_name.clone(), sample.value, &mut backend, in_interceptor.clone(), out_interceptor.clone(), zenoh.clone()).await {
                                    Ok(handle) => {
                                        let _ = storages_handles.insert(sample.res_name.clone(), handle);
                                    }
                                    Err(e) => warn!("{}", e),
                                }
                            } else {
                                warn!("Storage {} already exists", sample.res_name);
                            }
                        }
                        SampleKind::Delete =>  {
                            debug!("Delete storage {}", sample.res_name);
                            let _ = storages_handles.remove(&sample.res_name);
                        }
                        SampleKind::Patch => warn!("PATCH not supported on {}", sample.res_name),
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
    admin_path: String,
    mut value: Value,
    backend: &mut Box<dyn zenoh_backend_traits::Backend>,
    in_interceptor: Option<Arc<RwLock<Box<dyn IncomingDataInterceptor>>>>,
    out_interceptor: Option<Arc<RwLock<Box<dyn OutgoingDataInterceptor>>>>,
    zenoh: Arc<Session>,
) -> ZResult<Sender<bool>> {
    trace!("Create storage {}", admin_path);
    if value.encoding == encoding::APP_PROPERTIES {
        if let Ok(props) = String::from_utf8(value.payload.read_vec()).map(crate::Properties::from)
        {
            let path_expr = props
                .get(PROP_STORAGE_PATH_EXPR)
                .ok_or_else(|| {
                    zerror2!(ZErrorKind::Other {
                        descr: format!(
                            "Can't create storage {}: no {} property",
                            admin_path, PROP_STORAGE_PATH_EXPR
                        )
                    })
                })?
                .to_string();
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
                    "Received a PUT on {}, unable to decode properties from value: {:?}",
                    admin_path, value
                )
            })
        }
    } else {
        zerror!(ZErrorKind::Other {
            descr: format!(
                "Received a PUT on {} with invalid value encoding: {:?}",
                admin_path, value
            )
        })
    }
}
