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
use crate::storages_mgt::StorageMessage;
use async_std::sync::Arc;
use async_std::sync::Mutex;
use flume::{Receiver, Sender};
use futures::select;
use log::{debug, error, trace, warn};
use std::str;
use zenoh::key_expr::OwnedKeyExpr;
use zenoh::prelude::r#async::AsyncResolve;
use zenoh::prelude::Sample;
use zenoh::prelude::*;
use zenoh::time::Timestamp;
use zenoh::Session;
use zenoh_backend_traits::{Query, StorageInsertionResult};
use zenoh_core::Result as ZResult;

pub struct ReplicationService {
    pub aligner_updates: Receiver<Sample>,
    pub log_propagation: Sender<(OwnedKeyExpr, Timestamp)>,
}

pub struct StorageService {
    session: Arc<Session>,
    key_expr: String,
    name: String,
    storage: Mutex<Box<dyn zenoh_backend_traits::Storage>>,
    in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
    replication: Option<ReplicationService>,
}

impl StorageService {
    pub async fn start(
        session: Arc<Session>,
        key_expr: &str,
        name: &str,
        storage: Box<dyn zenoh_backend_traits::Storage>,
        in_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
        out_interceptor: Option<Arc<dyn Fn(Sample) -> Sample + Send + Sync>>,
        replication: Option<ReplicationService>,
    ) -> ZResult<Sender<StorageMessage>> {
        let storage_service = StorageService {
            session,
            key_expr: key_expr.to_string(),
            name: name.to_string(),
            storage: Mutex::new(storage),
            in_interceptor,
            out_interceptor,
            replication,
        };
        storage_service.start_storage_queryable_subscriber().await
    }

    async fn start_storage_queryable_subscriber(&self) -> ZResult<Sender<StorageMessage>> {
        let (tx, rx) = flume::bounded(1);

        // TODO:do we need a task here? if yes, why?
        // task::spawn(async move {
        // subscribe on key_expr
        let storage_sub = match self
            .session
            .declare_subscriber(&self.key_expr)
            .res_async()
            .await
        {
            Ok(storage_sub) => storage_sub,
            Err(e) => {
                error!("Error starting storage {} : {}", self.name, e);
                return Err(e);
            }
        };

        // answer to queries on key_expr
        let storage_queryable = match self.session.declare_queryable(&self.key_expr).res().await {
            Ok(storage_queryable) => storage_queryable,
            Err(e) => {
                error!("Error starting storage {} : {}", self.name, e);
                return Err(e);
            }
        };

        // TODO: refactor
        if self.replication.is_some() {
            let aligner_updates = &self.replication.as_ref().unwrap().aligner_updates;
            loop {
                select!(
                    // on sample for key_expr
                    sample = storage_sub.recv_async() => {
                        self.process_sample(sample.unwrap()).await;
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        let q = query.unwrap();
                        // wrap zenoh::Query in zenoh_backend_traits::Query
                        // with outgoing interceptor
                        let query = Query::new(q, self.out_interceptor.clone());
                        let mut storage = self.storage.lock().await;
                        if let Err(e) = storage.on_query(query).await {
                            warn!("Storage {} raised an error receiving a query: {}", self.name, e);
                        }
                        drop(storage);
                    },
                    // on storage handle drop
                    message = rx.recv_async() => {
                        match message {
                            Ok(StorageMessage::Stop) => {
                                trace!("Dropping storage {}", self.name);
                                return Ok(tx);
                            },
                            Ok(StorageMessage::GetStatus(tx)) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                            Err(e) => {
                                error!("Storage Message Channel Error: {}", e);
                                return Err(e.into());
                            },
                        };
                    },
                    // on aligner update
                    update = aligner_updates.recv_async() => {
                        match update {
                            Ok(sample) => self.process_sample(sample).await,
                            Err(e) => {
                                error!("Error in receiving aligner update: {}", e);
                                return Err(e.into());
                            }
                        }

                    }
                );
            }
        } else {
            loop {
                select!(
                    // on sample for key_expr
                    sample = storage_sub.recv_async() => {
                        self.process_sample(sample.unwrap()).await;
                    },
                    // on query on key_expr
                    query = storage_queryable.recv_async() => {
                        let q = query.unwrap();
                        // wrap zenoh::Query in zenoh_backend_traits::Query
                        // with outgoing interceptor
                        let query = Query::new(q, self.out_interceptor.clone());
                        let mut storage = self.storage.lock().await;
                        if let Err(e) = storage.on_query(query).await {
                            warn!("Storage {} raised an error receiving a query: {}", self.name, e);
                        }
                        drop(storage);
                    },
                    // on storage handle drop
                    message = rx.recv_async() => {
                        match message {
                            Ok(StorageMessage::Stop) => {
                                trace!("Dropping storage {}", self.name);
                                return Ok(tx);
                            },
                            Ok(StorageMessage::GetStatus(tx)) => {
                                let storage = self.storage.lock().await;
                                std::mem::drop(tx.send(storage.get_admin_status()).await);
                                drop(storage);
                            }
                            Err(e) => {
                                error!("Storage Message Channel Error: {}", e);
                                return Err(e.into());
                            },
                        };
                    },
                );
            }
        }
    }

    async fn process_sample(&self, sample: Sample) {
        debug!("[STORAGE] Processing sample: {}", sample);
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
}
