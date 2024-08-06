//
// Copyright (c) 2024 ZettaScale Technology
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

use std::{sync::Arc, time::Duration};

use zenoh::{key_expr::OwnedKeyExpr, query::QueryTarget, sample::Locality, session::Session};

use crate::storages_mgt::StorageService;

pub(crate) struct ReplicationService;

const MAX_RETRY: usize = 2;
const WAIT_PERIOD_SECS: u64 = 4;

impl ReplicationService {
    pub async fn start(
        zenoh_session: Arc<Session>,
        storage_service: StorageService,
        storage_key_expr: OwnedKeyExpr,
    ) {
        // We perform a "wait-try" policy because Zenoh needs some time to propagate the routing
        // information and, here, we need to have the queryables propagated.
        //
        // 4 seconds is an arbitrary value.
        let mut attempt = 0;
        let mut received_reply = false;

        while attempt < MAX_RETRY {
            attempt += 1;
            tokio::time::sleep(Duration::from_secs(WAIT_PERIOD_SECS)).await;

            match zenoh_session
                .get(&storage_key_expr)
                // `BestMatching`, the default option for `target`, will try to minimise the storage
                // that are queried and their distance while trying to maximise the key space
                // covered.
                //
                // In other words, if there is a close and complete storage, it will only query this
                // one.
                .target(QueryTarget::BestMatching)
                // The value `Remote` is self-explanatory but why it is needed deserves an
                // explanation: we do not want to query the local database as the purpose is to get
                // the data from other replicas (if there is one).
                .allowed_destination(Locality::Remote)
                .await
            {
                Ok(replies) => {
                    while let Ok(reply) = replies.recv_async().await {
                        received_reply = true;
                        if let Ok(sample) = reply.into_result() {
                            if let Err(e) = storage_service.process_sample(sample).await {
                                tracing::error!("{e:?}");
                            }
                        }
                    }
                }
                Err(e) => tracing::error!("Initial alignment Query failed with: {e:?}"),
            }

            if received_reply {
                break;
            }

            tracing::debug!(
                "Found no Queryable matching '{storage_key_expr}'. Attempt {attempt}/{MAX_RETRY}."
            );
        }
    }
}
