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
use zenoh::{internal::bail, session::Session, Result as ZResult};
use zenoh_backend_traits::{config::StorageConfig, History, VolumeInstance};

use crate::replication::ReplicationService;

pub(crate) mod service;
pub(crate) use service::StorageService;

pub enum StorageMessage {
    Stop,
    GetStatus(tokio::sync::mpsc::Sender<serde_json::Value>),
}

pub(crate) async fn create_and_start_storage(
    admin_key: String,
    config: StorageConfig,
    backend: &VolumeInstance,
    zenoh_session: Arc<Session>,
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

    // If the Replication is enabled but the Storage capability is not compatible with it (i.e. not
    // `Latest`), we return an error early keeping in mind "containerised execution environment"
    // where simply looking at the logs is not trivial.
    if config.replication.is_some() && capability.history != History::Latest {
        bail!(
            "Replication was enabled for storage {name} but its history capability is not \
             supported: found < {:?} >, expected < {:?} >",
            capability.history,
            History::Latest
        );
    }

    let latest_updates = match storage.get_all_entries().await {
        Ok(entries) => entries.into_iter().collect(),
        Err(e) => {
            bail!("Failed to retrieve entries from Storage < {storage_name} >: {e:?}");
        }
    };

    let storage = Arc::new(Mutex::new(storage));
    let storage_service = StorageService::start(
        zenoh_session.clone(),
        config.clone(),
        &name,
        storage,
        capability,
        rx,
        Arc::new(Mutex::new(latest_updates)),
    )
    .await;

    if config.replication.is_some() {
        // NOTE Although the function `ReplicationService::spawn_start` spawns its own tasks, we
        //      still need to call it within a dedicated task because the Zenoh routing tables are
        //      populated only after the plugins have been loaded.
        //
        //      If we don't wait for the routing tables to be populated the initial alignment
        //      (i.e. querying any Storage on the network handling the same key expression), will
        //      never work.
        //
        // TODO Do we really want to perform such an initial alignment? Because this query will
        //      target any Storage that matches the same key expression, regardless of if they have
        //      been configured to be replicated.
        tokio::task::spawn({
            let storage_key_expr = config.key_expr.clone();
            let zenoh_session = zenoh_session.clone();

            async move {
                tracing::debug!(
                    "Starting replication of storage '{}' on keyexpr '{}'",
                    name,
                    storage_key_expr
                );
                ReplicationService::start(zenoh_session, storage_service, storage_key_expr).await;
            }
        });
    }

    Ok(tx)
}
