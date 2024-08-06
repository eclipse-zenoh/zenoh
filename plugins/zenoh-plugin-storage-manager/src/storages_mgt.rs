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
use zenoh::{prelude::ZResult, session::Session};
use zenoh_backend_traits::{config::StorageConfig, Capability, VolumeInstance};

pub use super::replica::{Replica, StorageService};

pub enum StorageMessage {
    Stop,
    GetStatus(tokio::sync::mpsc::Sender<serde_json::Value>),
}

pub struct StoreIntercept {
    pub storage: Box<dyn zenoh_backend_traits::Storage>,
    pub capability: Capability,
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
    let store_intercept = StoreIntercept {
        storage,
        capability,
    };

    start_storage(store_intercept, config, admin_key, zenoh).await
}

pub(crate) async fn start_storage(
    store_intercept: StoreIntercept,
    config: StorageConfig,
    admin_key: String,
    zenoh: Arc<Session>,
) -> ZResult<flume::Sender<StorageMessage>> {
    // Ex: @/390CEC11A1E34977A1C609A35BC015E6/router/status/plugins/storage_manager/storages/demo1
    // -> 390CEC11A1E34977A1C609A35BC015E6/demo1 (/<type> needed????)
    let parts: Vec<&str> = admin_key.split('/').collect();
    let uuid = parts[2];
    let storage_name = parts[7];
    let name = format!("{uuid}/{storage_name}");

    tracing::trace!("Start storage '{}' on keyexpr '{}'", name, config.key_expr);

    let (tx, rx) = flume::bounded(1);

    tokio::task::spawn(async move {
        // If a configuration for replica is present, we initialize a replica, else only a storage
        // service A replica contains a storage service and all metadata required for
        // anti-entropy
        if config.replica_config.is_some() {
            Replica::start(zenoh.clone(), store_intercept, config, &name, rx).await;
        } else {
            StorageService::start(zenoh.clone(), config, &name, store_intercept, rx, None).await;
        }
    });

    Ok(tx)
}
