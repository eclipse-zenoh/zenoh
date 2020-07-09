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
use async_std::sync::Arc;

use zenoh_protocol::core::{ZInt, ResKey, PeerId, SubInfo, QueryTarget, QueryConsolidation, };
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::Primitives;


#[derive(Clone)]
pub struct OwnedPrimitives {
    primitives: Arc<dyn Primitives + Send + Sync>,
}

impl OwnedPrimitives {
    pub fn new(primitives: Arc<dyn Primitives + Send + Sync>) -> OwnedPrimitives {OwnedPrimitives {primitives}}

    pub async fn resource(self, rid: ZInt, reskey: ResKey) {
        self.primitives.resource(rid, &reskey).await
    }
    pub async fn forget_resource(self, rid: ZInt) {
        self.primitives.forget_resource(rid).await
    }
    
    pub async fn publisher(self, reskey: ResKey) {
        self.primitives.publisher(&reskey).await
    }
    pub async fn forget_publisher(self, reskey: ResKey) {
        self.primitives.forget_publisher(&reskey).await
    }
    
    pub async fn subscriber(self, reskey: ResKey, sub_info: SubInfo) {
        self.primitives.subscriber(&reskey, &sub_info).await
    }
    pub async fn forget_subscriber(self, reskey: ResKey) {
        self.primitives.forget_subscriber(&reskey).await
    }
    
    pub async fn queryable(self, reskey: ResKey) {
        self.primitives.queryable(&reskey).await
    }
    pub async fn forget_queryable(self, reskey: ResKey) {
        self.primitives.forget_queryable(&reskey).await
    }

    pub async fn data(self, reskey: ResKey, reliable: bool, info: Option<RBuf>, payload: RBuf) {
        self.primitives.data(&reskey, reliable, &info, payload).await
    }
    pub async fn query(self, reskey: ResKey, predicate: String, qid: ZInt, target: QueryTarget, consolidation: QueryConsolidation) {
        self.primitives.query(&reskey, &predicate, qid, target, consolidation).await
    }
    pub async fn reply_data(self, qid: ZInt, source_kind: ZInt, replier_id: PeerId, reskey: ResKey, info: Option<RBuf>, payload: RBuf) {
        self.primitives.reply_data(qid, source_kind, replier_id, reskey, info, payload).await
    }
    pub async fn reply_final(self, qid: ZInt) {
        self.primitives.reply_final(qid).await
    }
    pub async fn pull(self, is_final: bool, reskey: ResKey, pull_id: ZInt, max_samples: Option<ZInt>){
        self.primitives.pull(is_final, &reskey, pull_id, &max_samples).await
    }

    pub async fn close(self) {
        self.primitives.close().await
    }
}