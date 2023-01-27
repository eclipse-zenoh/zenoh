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
use async_std::sync::Arc;
use log::trace;
use zenoh::Session;
use zenoh_backend_traits::config::StorageConfig;
use zenoh_result::ZResult;

pub use super::replica::{Replica, StorageService};

pub enum StorageMessage {
    Stop,
    GetStatus(async_std::channel::Sender<serde_json::Value>),
}

pub(crate) async fn start_storage(
    store_intercept: super::StoreIntercept,
    config: StorageConfig,
    admin_key: String,
    zenoh: Arc<Session>,
) -> ZResult<flume::Sender<StorageMessage>> {
    // Ex: @/router/390CEC11A1E34977A1C609A35BC015E6/status/plugins/storage_manager/storages/demo1 -> 390CEC11A1E34977A1C609A35BC015E6/demo1 (/<type> needed????)
    let parts: Vec<&str> = admin_key.split('/').collect();
    let uuid = parts[2];
    let storage_name = parts[7];
    let name = format!("{uuid}/{storage_name}");

    trace!("Start storage {} on {}", name, config.key_expr);

    let (tx, rx) = flume::bounded(1);

    async_std::task::spawn(async move {
        // If a configuration for replica is present, we initialize a replica, else only a storage service
        // A replica contains a storage service and all metadata required for anti-entropy
        match config.replica_config {
            Some(replica_config) => {
                Replica::start(
                    replica_config,
                    zenoh.clone(),
                    store_intercept,
                    config.key_expr,
                    config.complete,
                    &name,
                    rx,
                )
                .await
            }
            None => {
                StorageService::start(
                    zenoh.clone(),
                    config.key_expr,
                    config.complete,
                    &name,
                    store_intercept,
                    rx,
                    None,
                )
                .await
            }
        }
    });

    Ok(tx)
}
