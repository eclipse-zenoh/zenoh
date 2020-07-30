//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::{Arc, RwLock};
use async_std::task;
use futures::prelude::*;
use futures::select;
use log::warn;
use std::collections::HashMap;
use zenoh::net::queryable::STORAGE;
use zenoh::net::{Reliability, Session, SubInfo, SubMode};
use zenoh::{Properties, ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

pub(crate) struct Backend {
    pub(crate) storages: HashMap<String, Arc<RwLock<Box<dyn zenoh_backend_core::Storage>>>>,
    pub(crate) backend: Box<dyn zenoh_backend_core::Backend>,
    session: Session,
}

impl Backend {
    pub(crate) fn new(backend: Box<dyn zenoh_backend_core::Backend>, session: Session) -> Backend {
        Backend {
            storages: HashMap::new(),
            backend,
            session,
        }
    }

    pub(crate) fn properties(&self) -> &Properties {
        self.backend.properties()
    }

    pub(crate) async fn create_storage(&mut self, stid: String, props: Properties) -> ZResult<()> {
        if !self.storages.contains_key(&stid) {
            let storage = Arc::new(RwLock::new(self.backend.create_storage(props).await?));
            let path_expr = storage.read().await.path_expr().clone();

            let sub_info = SubInfo {
                reliability: Reliability::Reliable,
                mode: SubMode::Push,
                period: None,
            };
            let mut sub = self
                .session
                .declare_subscriber(&path_expr.to_string().into(), &sub_info)
                .await
                .unwrap();

            let mut queryable = self
                .session
                .declare_queryable(&path_expr.to_string().into(), STORAGE)
                .await
                .unwrap();

            self.storages.insert(stid.clone(), storage.clone());

            task::spawn(async move {
                loop {
                    select!(
                        sample = sub.next().fuse() => {
                            let sample = sample.unwrap();
                            if let Err(e) = storage.write().await.on_sample(sample).await {
                                warn!("Storage {} raised an error receiving a sample: {}", stid, e);
                            }
                        },
                        query = queryable.next().fuse() => {
                            let query = query.unwrap();
                            if let Err(e) = storage.write().await.on_query(query).await {
                                warn!("Storage {} raised an error receiving a query: {}", stid, e);
                            }
                        },
                    );
                }
            });

            Ok(())
        } else {
            zerror!(ZErrorKind::Other {
                descr: format!("Storage '{}' already exists", stid)
            })
        }
    }

    pub(crate) async fn get_storage_properties(&self, stid: &str) -> Option<Properties> {
        match self.storages.get(stid) {
            Some(st) => Some(st.read().await.properties().clone()),
            None => None,
        }
    }
}
