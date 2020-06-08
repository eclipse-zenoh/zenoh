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
use async_trait::async_trait;
use crate::core::{ZInt, ResKey};
use crate::io::RBuf;
use crate::proto::{channel, ZenohMessage, SubInfo, Declaration, Primitives,  
    QueryTarget, QueryConsolidation, ReplyContext, Reply};
use crate::session::MsgHandler;

pub struct Mux<T: MsgHandler + Send + Sync + ?Sized> {
    handler: Arc<T>,
}

impl<T: MsgHandler + Send + Sync + ?Sized> Mux<T> {
    pub fn new(handler: Arc<T>) -> Mux<T> {
        Mux {handler}
    }
}

#[allow(unused_must_use)] // TODO
#[async_trait]
impl<T: MsgHandler + Send + Sync + ?Sized> Primitives for Mux<T> {
    async fn resource(&self, rid: u64, reskey: &ResKey) {
        let mut decls = Vec::new();
        decls.push(Declaration::Resource{rid, key: reskey.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }

    async fn forget_resource(&self, rid: u64) {
        let mut decls = Vec::new();
        decls.push(Declaration::ForgetResource{rid});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }
    
    async fn subscriber(&self, reskey: &ResKey, sub_info: &SubInfo) {
        let mut decls = Vec::new();
        decls.push(Declaration::Subscriber{key: reskey.clone(), info: sub_info.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }

    async fn forget_subscriber(&self, reskey: &ResKey) {
        let mut decls = Vec::new();
        decls.push(Declaration::ForgetSubscriber{key: reskey.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }
    
    async fn publisher(&self, reskey: &ResKey) {
        let mut decls = Vec::new();
        decls.push(Declaration::Publisher{key: reskey.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }

    async fn forget_publisher(&self, reskey: &ResKey) {
        let mut decls = Vec::new();
        decls.push(Declaration::ForgetPublisher{key: reskey.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }
    
    async fn queryable(&self, reskey: &ResKey) {
        let mut decls = Vec::new();
        decls.push(Declaration::Queryable{key: reskey.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }

    async fn forget_queryable(&self, reskey: &ResKey) {
        let mut decls = Vec::new();
        decls.push(Declaration::ForgetQueryable{key: reskey.clone()});
        self.handler.handle_message(ZenohMessage::make_declare(decls, None)).await;
    }

    async fn data(&self, reskey: &ResKey, reliability: bool, info: &Option<RBuf>, payload: RBuf) {
        self.handler.handle_message(ZenohMessage::make_data(reliability, reskey.clone(), info.clone(), payload, None, None)).await;
    }

    async fn query(&self, reskey: &ResKey, predicate: &str, qid: ZInt, target: QueryTarget, consolidation: QueryConsolidation) {
        let target_opt = if target == QueryTarget::default() { None } else { Some(target) };
        self.handler.handle_message(ZenohMessage::make_query(reskey.clone(), predicate.to_string(), qid, target_opt, consolidation.clone(), None)).await;
    }

    async fn reply(&self, qid: ZInt, reply: &Reply) {
        match reply {
            Reply::ReplyData { source_kind, replier_id, reskey, info, payload } => {
                self.handler.handle_message(ZenohMessage::make_data(
                    channel::RELIABLE, reskey.clone(), info.clone(), payload.clone(), 
                    Some(ReplyContext::make(qid, *source_kind, Some(replier_id.clone()))), None)).await;
            }
            Reply::SourceFinal { source_kind, replier_id } => {
                self.handler.handle_message(ZenohMessage::make_unit(
                    channel::RELIABLE, Some(ReplyContext::make(qid, *source_kind, Some(replier_id.clone()))), None)).await;
            }
            Reply::ReplyFinal {} => {
                self.handler.handle_message(ZenohMessage::make_unit(
                    channel::RELIABLE, Some(ReplyContext::make(qid, 0, None)), None)).await;
            }
        }
    }

    async fn pull(&self, is_final: bool, reskey: &ResKey, pull_id: ZInt, max_samples: &Option<ZInt>) {
        self.handler.handle_message(ZenohMessage::make_pull(is_final, reskey.clone(), pull_id, *max_samples, None)).await;
    }

    async fn close(&self) {
        self.handler.close().await;
    }
}