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
use zenoh::{session::Session, Result as ZResult};
use zenoh_backend_traits::{config::StorageConfig, Capability, VolumeInstance};

use super::storages_mgt::*;

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
