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
use super::core::{CongestionControl, PeerId, Reliability, ResKey, ZInt};
use super::core::{QueryConsolidation, QueryTarget, SubInfo};
use super::io::ZBuf;
use super::proto::{
    zmsg, DataInfo, Declaration, ForgetPublisher, ForgetQueryable, ForgetResource,
    ForgetSubscriber, Publisher, Queryable, ReplyContext, Resource, RoutingContext, Subscriber,
    ZenohMessage,
};
use super::session::{Primitives, Session};

pub struct Mux {
    handler: Session,
}

#[allow(unused_must_use)] // TODO
#[allow(dead_code)]
impl Mux {
    pub(crate) fn new(handler: Session) -> Mux {
        Mux { handler }
    }
}

impl Primitives for Mux {
    fn decl_resource(&self, rid: ZInt, reskey: &ResKey) {
        let d = Declaration::Resource(Resource {
            rid,
            key: reskey.clone(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, None, None));
    }

    fn forget_resource(&self, rid: ZInt) {
        let d = Declaration::ForgetResource(ForgetResource { rid });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, None, None));
    }

    fn decl_subscriber(
        &self,
        reskey: &ResKey,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let d = Declaration::Subscriber(Subscriber {
            key: reskey.clone(),
            info: sub_info.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn forget_subscriber(&self, reskey: &ResKey, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetSubscriber(ForgetSubscriber {
            key: reskey.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn decl_publisher(&self, reskey: &ResKey, routing_context: Option<RoutingContext>) {
        let d = Declaration::Publisher(Publisher {
            key: reskey.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn forget_publisher(&self, reskey: &ResKey, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetPublisher(ForgetPublisher {
            key: reskey.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn decl_queryable(&self, reskey: &ResKey, kind: ZInt, routing_context: Option<RoutingContext>) {
        let d = Declaration::Queryable(Queryable {
            key: reskey.clone(),
            kind,
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn forget_queryable(&self, reskey: &ResKey, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetQueryable(ForgetQueryable {
            key: reskey.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn send_data(
        &self,
        reskey: &ResKey,
        payload: ZBuf,
        reliability: Reliability,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_data(
            reskey.clone(),
            payload,
            reliability,
            congestion_control,
            data_info,
            routing_context,
            None,
            None,
        ));
    }

    fn send_query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
    ) {
        let target_opt = if target == QueryTarget::default() {
            None
        } else {
            Some(target)
        };
        let _ = self.handler.handle_message(ZenohMessage::make_query(
            reskey.clone(),
            predicate.to_string(),
            qid,
            target_opt,
            consolidation,
            routing_context,
            None,
        ));
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        source_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        data_info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_data(
            reskey,
            payload,
            zmsg::default_reliability::REPLY,
            zmsg::default_congestion_control::REPLY,
            data_info,
            None,
            Some(ReplyContext::make(qid, source_kind, Some(replier_id))),
            None,
        ));
    }

    fn send_reply_final(&self, qid: ZInt) {
        let _ = self.handler.handle_message(ZenohMessage::make_unit(
            zmsg::default_reliability::REPLY,
            zmsg::default_congestion_control::REPLY,
            Some(ReplyContext::make(qid, 0, None)),
            None,
        ));
    }

    fn send_pull(
        &self,
        is_final: bool,
        reskey: &ResKey,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_pull(
            is_final,
            reskey.clone(),
            pull_id,
            *max_samples,
            None,
        ));
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}
