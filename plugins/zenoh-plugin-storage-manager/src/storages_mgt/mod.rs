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

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{broadcast::Sender, Mutex, RwLock};
use zenoh::{
    internal::bail, key_expr::OwnedKeyExpr, sample::SampleKind, session::Session, Result as ZResult,
};
use zenoh_backend_traits::{config::StorageConfig, History, VolumeInstance};

use crate::replication::{Event, LogLatest, ReplicationService};

pub(crate) mod service;
pub(crate) use service::StorageService;

#[derive(Clone)]
pub enum StorageMessage {
    Stop,
    GetStatus(tokio::sync::mpsc::Sender<serde_json::Value>),
}

pub(crate) type LatestUpdates = HashMap<Option<OwnedKeyExpr>, Event>;

#[derive(Clone)]
pub(crate) struct CacheLatest {
    pub(crate) latest_updates: Arc<Mutex<LatestUpdates>>,
    pub(crate) replication_log: Option<Arc<RwLock<LogLatest>>>,
}

impl CacheLatest {
    pub fn new(
        latest_updates: Arc<Mutex<LatestUpdates>>,
        replication_log: Option<Arc<RwLock<LogLatest>>>,
    ) -> Self {
        Self {
            latest_updates,
            replication_log,
        }
    }
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

    let (tx, rx_storage) = tokio::sync::broadcast::channel(1);

    let mut entries = match storage.get_all_entries().await {
        Ok(entries) => entries
            .into_iter()
            .map(|(stripped_key, ts)| {
                (
                    stripped_key.clone(),
                    Event::new(stripped_key, ts, SampleKind::Put),
                )
            })
            .collect::<HashMap<_, _>>(),
        Err(e) => bail!("`get_all_entries` failed with: {e:?}"),
    };

    let mut replication_log = None;
    let mut latest_updates = HashMap::default();
    if let Some(replica_config) = &config.replication {
        if capability.history != History::Latest {
            bail!(
                "Replication was enabled for storage {name} but its history capability is not \
                 supported: found < {:?} >, expected < {:?} >",
                capability.history,
                History::Latest
            );
        }
        let mut log_latest = LogLatest::new(
            config.key_expr.clone(),
            config.strip_prefix.clone(),
            replica_config.clone(),
        );
        log_latest.update(entries.drain().map(|(_, event)| event));

        replication_log = Some(Arc::new(RwLock::new(log_latest)));
    } else {
        latest_updates = entries;
    }

    let latest_updates = Arc::new(Mutex::new(latest_updates));

    let storage = Arc::new(Mutex::new(storage));
    let storage_service = StorageService::start(
        zenoh_session.clone(),
        config.clone(),
        &name,
        storage,
        capability,
        rx_storage,
        CacheLatest::new(latest_updates.clone(), replication_log.clone()),
    )
    .await;

    // Testing if the `replication_log` is set is equivalent to testing if the `replication` is
    // set: the `replication_log` is only set when the latter is.
    if let Some(replication_log) = replication_log {
        let rx_replication = tx.subscribe();

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
        tokio::task::spawn(async move {
            tracing::debug!(
                "Starting replication of storage '{}' on keyexpr '{}'",
                name,
                config.key_expr,
            );
            ReplicationService::spawn_start(
                zenoh_session,
                storage_service,
                config.key_expr,
                replication_log,
                latest_updates,
                rx_replication,
            )
            .await;
        });
    }

    Ok(tx)
}
