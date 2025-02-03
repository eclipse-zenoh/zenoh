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
use zenoh::{internal::bail, session::Session, Result as ZResult};
use zenoh_backend_traits::{config::StorageConfig, History, VolumeInstance};

use crate::replication::{Action, Event, LogLatest, LogLatestKey, ReplicationService};

pub(crate) mod service;
pub(crate) use service::StorageService;

#[derive(Clone)]
pub enum StorageMessage {
    Stop,
    GetStatus(tokio::sync::mpsc::Sender<serde_json::Value>),
}

pub(crate) type LatestUpdates = HashMap<LogLatestKey, Event>;

#[derive(Clone)]
pub(crate) struct CacheLatest {
    pub(crate) latest_updates: Arc<RwLock<LatestUpdates>>,
    pub(crate) replication_log: Option<Arc<RwLock<LogLatest>>>,
}

impl CacheLatest {
    pub fn new(
        latest_updates: Arc<RwLock<LatestUpdates>>,
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

    let (tx, rx_storage) = tokio::sync::broadcast::channel(1);
    let rx_replication = tx.subscribe();

    let mut entries = match storage.get_all_entries().await {
        Ok(entries) => entries
            .into_iter()
            .map(|(stripped_key, ts)| {
                let event = Event::new(stripped_key, ts, &Action::Put);
                (event.log_key(), event)
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

    let latest_updates = Arc::new(RwLock::new(latest_updates));

    let storage = Arc::new(Mutex::new(storage));

    // NOTE The StorageService method `start_storage_queryable_subscriber` does not spawn its own
    //      task to loop/wait on the Subscriber and Queryable it creates. Thus we spawn the task
    //      here.
    //
    //      Doing so also allows us to return early from the creation of the Storage, creation which
    //      blocks populating the routing tables.
    tokio::task::spawn(async move {
        let storage_service = Arc::new(
            StorageService::new(
                zenoh_session.clone(),
                config.clone(),
                &name,
                storage,
                capability,
                CacheLatest::new(latest_updates.clone(), replication_log.clone()),
            )
            .await,
        );

        // Testing if the `replication_log` is set is equivalent to testing if the `replication` is
        // set: the `replication_log` is only set when the latter is.
        if let Some(replication_log) = replication_log {
            tracing::debug!(
                "Starting replication of storage '{}' on keyexpr '{}'",
                name,
                config.key_expr,
            );

            ReplicationService::spawn_start(
                zenoh_session,
                storage_service.clone(),
                config.key_expr,
                replication_log,
                latest_updates,
                rx_replication,
            )
            .await;
        }

        storage_service
            .start_storage_queryable_subscriber(rx_storage)
            .await;
    });

    Ok(tx)
}
