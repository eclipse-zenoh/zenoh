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
use async_std::channel::Sender;
use async_std::sync::Arc;
use log::trace;
use zenoh::prelude::*;
use zenoh::Session;
use zenoh_backend_traits::config::StorageConfig;
use zenoh_core::Result as ZResult;

pub(crate) async fn create_and_start_storage(
    admin_key: String,
    config: StorageConfig,
    backend: &mut Box<dyn zenoh_backend_traits::Volume>,
    in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    zenoh: Arc<Session>,
) -> ZResult<Sender<StorageMessage>> {
    trace!("Create storage {}", &admin_key);
    let key_expr = config.key_expr.clone();
    let storage = backend.create_storage(config).await?;
    start_storage(
        storage,
        admin_key,
        key_expr,
        in_interceptor,
        out_interceptor,
        zenoh,
    )
    .await
}
