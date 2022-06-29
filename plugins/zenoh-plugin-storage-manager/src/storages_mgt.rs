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
use zenoh::prelude::*;
use zenoh::Session;
use zenoh_backend_traits::config::ReplicaConfig;
use zenoh_core::Result as ZResult;

pub use super::replica::{Replica, StorageService};

pub enum StorageMessage {
    Stop,
    GetStatus(async_std::channel::Sender<serde_json::Value>),
}

pub(crate) async fn start_storage(
    storage: Box<dyn zenoh_backend_traits::Storage>,
    config: Option<ReplicaConfig>,
    admin_key: String,
    key_expr: OwnedKeyExpr,
    in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    zenoh: Arc<Session>,
) -> ZResult<flume::Sender<StorageMessage>> {
    // Ex: @/router/390CEC11A1E34977A1C609A35BC015E6/status/plugins/storage_manager/storages/demo1 -> 390CEC11A1E34977A1C609A35BC015E6/demo1 (/memory needed????)
    let parts: Vec<&str> = admin_key.split('/').collect();
    let uuid = parts[2];
    let storage_name = parts[7];
    let name = format!("{}/{}", uuid, storage_name);

    trace!("Start storage {} on {}", name, key_expr);

    // If a configuration for replica is present, we initialize a replica, else only a storage service
    // A replica contains a storage service and all metadata required for anti-entropy
    if config.is_some() {
        Replica::start(
            config.unwrap(),
            zenoh.clone(),
            storage,
            in_interceptor,
            out_interceptor,
            &key_expr,
            &name,
        )
        .await
    } else {
        StorageService::start(
            zenoh.clone(),
            &key_expr,
            &name,
            storage,
            in_interceptor,
            out_interceptor,
            None,
        )
        .await
    }

    // // align with other storages, querying them on key_expr,
    // // with `_time=[..]` to get historical data (in case of time-series)
    // let replies = match zenoh
    //     .get(KeyExpr::from(&key_expr).with_value_selector("_time=[..]"))
    //     .target(QueryTarget::All)
    //     .consolidation(ConsolidationMode::None)
    //     .res_async()
    //     .await
    // {
    //     Ok(replies) => replies,
    //     Err(e) => {
    //         error!("Error aligning storage {} : {}", admin_key, e);
    //         return;
    //     }
    // };
}
