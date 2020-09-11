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
use async_trait::async_trait;
use std::collections::HashMap;

use crate::routing::broker::*;
use zenoh_protocol::core::{
    CongestionControl, PeerId, QueryConsolidation, QueryTarget, Reliability, ResKey, SubInfo,
    WhatAmI, ZInt,
};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::{DataInfo, Primitives};

pub struct FaceState {
    pub(super) id: usize,
    pub(super) whatami: WhatAmI,
    pub(super) primitives: Arc<dyn Primitives + Send + Sync>,
    pub(super) local_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) remote_mappings: HashMap<ZInt, Arc<Resource>>,
    pub(super) subs: Vec<Arc<Resource>>,
    pub(super) qabl: Vec<Arc<Resource>>,
    pub(super) next_qid: ZInt,
    pub(super) pending_queries: HashMap<ZInt, Arc<Query>>,
}

impl FaceState {
    pub(super) fn new(
        id: usize,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Arc<FaceState> {
        Arc::new(FaceState {
            id,
            whatami,
            primitives,
            local_mappings: HashMap::new(),
            remote_mappings: HashMap::new(),
            subs: Vec::new(),
            qabl: Vec::new(),
            next_qid: 0,
            pending_queries: HashMap::new(),
        })
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn get_mapping(&self, prefixid: &ZInt) -> Option<&std::sync::Arc<Resource>> {
        match self.remote_mappings.get(prefixid) {
            Some(prefix) => Some(prefix),
            None => match self.local_mappings.get(prefixid) {
                Some(prefix) => Some(prefix),
                None => None,
            },
        }
    }

    pub(super) fn get_next_local_id(&self) -> ZInt {
        let mut id = 1;
        while self.local_mappings.get(&id).is_some() || self.remote_mappings.get(&id).is_some() {
            id += 1;
        }
        id
    }
}

pub struct Face {
    pub(super) tables: Arc<RwLock<Tables>>,
    pub(super) state: Arc<FaceState>,
}

#[async_trait]
impl Primitives for Face {
    async fn resource(&self, rid: ZInt, reskey: &ResKey) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        declare_resource(&mut tables, &mut self.state.clone(), rid, prefixid, suffix).await;
    }

    async fn forget_resource(&self, rid: ZInt) {
        let mut tables = self.tables.write().await;
        undeclare_resource(&mut tables, &mut self.state.clone(), rid).await;
    }

    async fn subscriber(&self, reskey: &ResKey, sub_info: &SubInfo) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        declare_subscription(
            &mut tables,
            &mut self.state.clone(),
            prefixid,
            suffix,
            sub_info,
        )
        .await;
    }

    async fn forget_subscriber(&self, reskey: &ResKey) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        undeclare_subscription(&mut tables, &mut self.state.clone(), prefixid, suffix).await;
    }

    async fn publisher(&self, _reskey: &ResKey) {}

    async fn forget_publisher(&self, _reskey: &ResKey) {}

    async fn queryable(&self, reskey: &ResKey) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        declare_queryable(&mut tables, &mut self.state.clone(), prefixid, suffix).await;
    }

    async fn forget_queryable(&self, reskey: &ResKey) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        undeclare_queryable(&mut tables, &mut self.state.clone(), prefixid, suffix).await;
    }

    async fn data(
        &self,
        reskey: &ResKey,
        payload: RBuf,
        _reliability: Reliability,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
    ) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        route_data(
            &mut tables,
            &self.state,
            prefixid,
            suffix,
            congestion_control,
            data_info,
            payload,
        )
        .await;
    }

    async fn query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        route_query(
            &mut tables,
            &self.state,
            prefixid,
            suffix,
            predicate,
            qid,
            target,
            consolidation,
        )
        .await;
    }

    async fn reply_data(
        &self,
        qid: ZInt,
        source_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        info: Option<DataInfo>,
        payload: RBuf,
    ) {
        let mut tables = self.tables.write().await;
        route_reply_data(
            &mut tables,
            &mut self.state.clone(),
            qid,
            source_kind,
            replier_id,
            reskey,
            info,
            payload,
        )
        .await;
    }

    async fn reply_final(&self, qid: ZInt) {
        let mut tables = self.tables.write().await;
        route_reply_final(&mut tables, &mut self.state.clone(), qid).await;
    }

    async fn pull(
        &self,
        is_final: bool,
        reskey: &ResKey,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    ) {
        let (prefixid, suffix) = reskey.into();
        let mut tables = self.tables.write().await;
        pull_data(
            &mut tables,
            &self.state.clone(),
            is_final,
            prefixid,
            suffix,
            pull_id,
            max_samples,
        )
        .await;
    }

    async fn close(&self) {
        Tables::close_face(&self.tables, &Arc::downgrade(&self.state)).await;
    }
}
