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
use crate::storages_mgt::{StorageMessage, StoreIntercept};
use async_std::sync::Arc;
use async_std::sync::Mutex;
use flume::{Receiver, Sender};
use futures::select;
use log::{error, trace, warn};
use std::str;
use zenoh::key_expr::OwnedKeyExpr;
use zenoh::prelude::r#async::*;
use zenoh::time::Timestamp;
use zenoh::Session;
use zenoh_backend_traits::{Query, StorageInsertionResult};

pub struct ReplicationService {
    pub empty_start: bool,
    pub aligner_updates: Receiver<Sample>,
    pub log_propagation: Sender<(OwnedKeyExpr, Timestamp)>,
}

pub struct StorageService {
    session: Arc<Session>,
    key_expr: OwnedKeyExpr,
    complete: bool,
    name: String,
    storage: Mutex<Box<dyn zenoh_backend_traits::Storage>>,
    in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    replication: Option<ReplicationService>,
}

impl StorageService {
    pub async fn start(
        session: Arc<Session>,
        key_expr: OwnedKeyExpr,
        complete: bool,
        name: &str,
        store_intercept: StoreIntercept,
        rx: Receiver<StorageMessage>,
        replication: Option<ReplicationService>,
    ) {
        let storage_service = StorageService {
            session,
            key_expr,
            complete,
            name: name.to_string(),
            storage: Mutex::new(store_intercept.storage),
            in_interceptor: store_intercept.in_interceptor,
            out_interceptor: store_intercept.out_interceptor,
            replication,
        };
        storage_service.start_storage_queryable_subscriber(rx).await
    }

    async fn start_storage_queryable_subscriber(&self, rx: Receiver<StorageMessage>) {
        self.initialize_if_empty().await;

        // subscribe on key_expr
        let storage_sub = match self.session.declare_subscriber(&self.key_expr).res().await {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                error!("Error starting storage {}: {}", self.name, e);
                return;
            }
        };

        // answer to queries on key_expr
        let storage_queryable = match self
            .session
            .declare_queryable(&self.key_expr)
            .complete(self.complete)
            .res()
            .await
        {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                error!("Error starting storage {}: {}", self.name, e);
                return;
            }
        };

        if self.replication.is_some() {
            let aligner_updates = &self.replication.as_ref().unwrap().aligner_updates;
            loop {
                select!(
                    // on sample for key_expr
                    sample = storage_sub.recv_async() => {
                        let sample = match sample {
                            Ok(sample) => sample,
                            Err(e) => {
                                error!("Error in sample: {}", e);
                                continue;
                            }
                        };
                        // log error if the sample is not timestamped
                        // This is to reduce down the line inconsistencies of having duplicate samples stored
                        if sample.get_timestamp().is_none() {
                            error!("Sample {} is not timestamped. Please timestamp samples meant for replicated storage.", sample);
                        }
                        else {
                            self.process_sample(sample).await;
                        }
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        self.reply_query(query).await;
                    },
                    // on aligner update
                    update = aligner_updates.recv_async() => {
                        match update {
                            Ok(sample) => self.process_sample(sample).await,
                            Err(e) => {
                                error!("Error in receiving aligner update: {}", e);
                            }
                        }
                    },
                    // on storage handle drop
                    message = rx.recv_async() => {
                        match message {
                            Ok(StorageMessage::Stop) => {
                                trace!("Dropping storage {}", self.name);
                                return
                            },
                            Ok(StorageMessage::GetStatus(tx)) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                            Err(e) => {
                                error!("Storage Message Channel Error: {}", e);
                            },
                        };
                    }
                );
            }
        } else {
            loop {
                select!(
                    // on sample for key_expr
                    sample = storage_sub.recv_async() => {
                        let mut sample = match sample {
                            Ok(sample) => sample,
                            Err(e) => {
                                error!("Error in sample: {}", e);
                                continue;
                            }
                        };
                        sample.ensure_timestamp();
                        self.process_sample(sample).await;
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        self.reply_query(query).await;
                    },
                    // on storage handle drop
                    message = rx.recv_async() => {
                        match message {
                            Ok(StorageMessage::Stop) => {
                                trace!("Dropping storage {}", self.name);
                                return
                            },
                            Ok(StorageMessage::GetStatus(tx)) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                            Err(e) => {
                                error!("Storage Message Channel Error: {}", e);
                            },
                        };
                    },
                );
            }
        }
    }

    async fn process_sample(&self, sample: Sample) {
        trace!("[STORAGE] Processing sample: {}", sample);
        // Call incoming data interceptor (if any)
        let mut sample = if let Some(ref interceptor) = self.in_interceptor {
            interceptor(sample)
        } else {
            sample
        };

        sample.ensure_timestamp();

        let mut storage = self.storage.lock().await;
        let result = storage.on_sample(sample.clone()).await;
        if self.replication.is_some()
            && result.is_ok()
            && !matches!(result.unwrap(), StorageInsertionResult::Outdated)
        {
            let sending = self.replication.as_ref().unwrap().log_propagation.send((
                OwnedKeyExpr::from(sample.key_expr.clone()),
                *sample.get_timestamp().unwrap(),
            ));
            match sending {
                Ok(_) => (),
                Err(e) => {
                    error!("Error in sending the sample to the log: {}", e);
                }
            }
        }
        drop(storage);
    }

    async fn reply_query(&self, query: Result<zenoh::queryable::Query, flume::RecvError>) {
        let q = match query {
            Ok(q) => q,
            Err(e) => {
                error!("Error in query: {}", e);
                return;
            }
        };
        // wrap zenoh::Query in zenoh_backend_traits::Query
        // with outgoing interceptor
        let query = Query::new(q, self.out_interceptor.clone());
        let mut storage = self.storage.lock().await;
        if let Err(e) = storage.on_query(query).await {
            warn!(
                "Storage {} raised an error receiving a query: {}",
                self.name, e
            );
        }
        drop(storage);
    }

    async fn initialize_if_empty(&self) {
        if self.replication.is_some() && self.replication.as_ref().unwrap().empty_start {
            // align with other storages, querying them on key_expr,
            // with `_time=[..]` to get historical data (in case of time-series)
            let replies = match self
                .session
                .get(KeyExpr::from(&self.key_expr).with_parameters("_time=[..]"))
                .target(QueryTarget::All)
                .consolidation(ConsolidationMode::None)
                .res()
                .await
            {
                Ok(replies) => replies,
                Err(e) => {
                    error!("Error aligning storage {}: {}", self.name, e);
                    return;
                }
            };
            while let Ok(reply) = replies.recv_async().await {
                match reply.sample {
                    Ok(sample) => {
                        self.process_sample(sample).await;
                    }
                    Err(e) => warn!(
                        "Storage {} received an error to align query: {}",
                        self.name, e
                    ),
                }
            }
        }
    }
}
