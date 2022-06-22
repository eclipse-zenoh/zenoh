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
use super::storages_mgt::*;
use flume::Sender;
use log::trace;
use std::sync::Arc;
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
    let storage = backend.create_storage(config.clone()).await?;
    start_storage(
        storage,
        config.replica_config,
        admin_key,
        key_expr,
        in_interceptor,
        out_interceptor,
        zenoh,
    )
    .await
}
