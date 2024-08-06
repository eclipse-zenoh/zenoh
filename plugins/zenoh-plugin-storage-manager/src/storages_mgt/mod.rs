//
// Copyright (c) 2023 ZettaScale Technology
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

use std::sync::Arc;

use flume::Sender;
use tokio::sync::Mutex;
use zenoh::{session::Session, Result as ZResult};
use zenoh_backend_traits::{config::StorageConfig, VolumeInstance};

mod service;
use service::StorageService;

pub enum StorageMessage {
    Stop,
    GetStatus(tokio::sync::mpsc::Sender<serde_json::Value>),
}

pub(crate) async fn create_and_start_storage(
    admin_key: String,
    config: StorageConfig,
    backend: &VolumeInstance,
    zenoh: Arc<Session>,
) -> ZResult<Sender<StorageMessage>> {
    tracing::trace!("Create storage '{}'", &admin_key);
    let capability = backend.get_capability();
    let storage = backend.create_storage(config.clone()).await?;

    // Ex: @/390CEC11A1E34977A1C609A35BC015E6/router/status/plugins/storage_manager/storages/demo1
    // -> 390CEC11A1E34977A1C609A35BC015E6/demo1 (/<type> needed????)
    let parts: Vec<&str> = admin_key.split('/').collect();
    let uuid = parts[2];
    let storage_name = parts[7];
    let name = format!("{uuid}/{storage_name}");

    tracing::trace!("Start storage '{}' on keyexpr '{}'", name, config.key_expr);

    let (tx, rx) = flume::bounded(1);

    let storage = Arc::new(Mutex::new(storage));
    tokio::task::spawn(async move {
        StorageService::start(zenoh.clone(), config, &name, storage, capability, rx).await;
    });

    Ok(tx)
}
