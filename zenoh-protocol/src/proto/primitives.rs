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
use async_trait::async_trait;
use crate::core::{ZInt, PeerId, ResKey};
use crate::io::RBuf;
use crate::proto::{SubInfo, QueryTarget, QueryConsolidation};

#[derive(Debug, Clone)]
pub enum Reply {
    ReplyData {source_kind: ZInt, replier_id: PeerId, reskey: ResKey, info: Option<RBuf>, payload: RBuf, },
    SourceFinal {source_kind: ZInt, replier_id: PeerId, },
    ReplyFinal,
} 

#[async_trait]
pub trait Primitives {
    async fn resource(&self, rid: ZInt, reskey: &ResKey);
    async fn forget_resource(&self, rid: ZInt);
    
    async fn publisher(&self, reskey: &ResKey);
    async fn forget_publisher(&self, reskey: &ResKey);
    
    async fn subscriber(&self, reskey: &ResKey, sub_info: &SubInfo);
    async fn forget_subscriber(&self, reskey: &ResKey);
    
    async fn queryable(&self, reskey: &ResKey);
    async fn forget_queryable(&self, reskey: &ResKey);

    async fn data(&self, reskey: &ResKey, reliable: bool, info: &Option<RBuf>, payload: RBuf);
    async fn query(&self, reskey: &ResKey, predicate: &str, qid: ZInt, target: QueryTarget, consolidation: QueryConsolidation);
    async fn reply(&self, qid: ZInt, reply: &Reply);
    async fn pull(&self, is_final: bool, reskey: &ResKey, pull_id: ZInt, max_samples: &Option<ZInt>);

    async fn close(&self);
}