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
use super::super::TransportUnicast;
use super::Primitives;
use zenoh_buffers::ZBuf;
use zenoh_protocol::{
    core::{Channel, CongestionControl, ExprId, WireExpr, ZenohId},
    zenoh::{
        zmsg, ConsolidationMode, DataInfo, Declaration, ForgetPublisher, ForgetQueryable,
        ForgetResource, ForgetSubscriber, Publisher, QueryBody, QueryId, QueryTarget, Queryable,
        QueryableInfo, ReplierInfo, ReplyContext, Resource, RoutingContext, SubInfo, Subscriber,
        ZenohMessage,
    },
};

pub struct Mux {
    handler: TransportUnicast,
}

impl Mux {
    pub fn new(handler: TransportUnicast) -> Mux {
        Mux { handler }
    }
}

impl Primitives for Mux {
    fn decl_resource(&self, expr_id: ExprId, key_expr: &WireExpr) {
        let d = Declaration::Resource(Resource {
            expr_id,
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, None));
    }

    fn forget_resource(&self, expr_id: ExprId) {
        let d = Declaration::ForgetResource(ForgetResource { expr_id });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, None));
    }

    fn decl_subscriber(
        &self,
        key_expr: &WireExpr,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let d = Declaration::Subscriber(Subscriber {
            key: key_expr.to_owned(),
            info: sub_info.clone(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, routing_context));
    }

    fn forget_subscriber(&self, key_expr: &WireExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetSubscriber(ForgetSubscriber {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, routing_context));
    }

    fn decl_publisher(&self, key_expr: &WireExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::Publisher(Publisher {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, routing_context));
    }

    fn forget_publisher(&self, key_expr: &WireExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetPublisher(ForgetPublisher {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, routing_context));
    }

    fn decl_queryable(
        &self,
        key_expr: &WireExpr,
        qabl_info: &QueryableInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let d = Declaration::Queryable(Queryable {
            key: key_expr.to_owned(),
            info: qabl_info.clone(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, routing_context));
    }

    fn forget_queryable(&self, key_expr: &WireExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetQueryable(ForgetQueryable {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, routing_context));
    }

    fn send_data(
        &self,
        key_expr: &WireExpr,
        payload: ZBuf,
        channel: Channel,
        cogestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_data(
            key_expr.to_owned(),
            payload,
            channel,
            cogestion_control,
            data_info,
            routing_context,
            None,
        ));
    }

    fn send_query(
        &self,
        key_expr: &WireExpr,
        parameters: &str,
        qid: QueryId,
        target: QueryTarget,
        consolidation: ConsolidationMode,
        body: Option<QueryBody>,
        routing_context: Option<RoutingContext>,
    ) {
        let target_opt = if target == QueryTarget::default() {
            None
        } else {
            Some(target)
        };
        let _ = self.handler.handle_message(ZenohMessage::make_query(
            key_expr.to_owned(),
            parameters.to_owned(),
            qid,
            target_opt,
            consolidation,
            body,
            routing_context,
        ));
    }

    fn send_reply_data(
        &self,
        qid: QueryId,
        replier_id: ZenohId,
        key_expr: WireExpr,
        data_info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_data(
            key_expr.to_owned(),
            payload,
            zmsg::default_channel::REPLY,
            zmsg::default_congestion_control::REPLY,
            data_info,
            None,
            Some(ReplyContext::new(qid, Some(ReplierInfo { id: replier_id }))),
        ));
    }

    fn send_reply_final(&self, qid: QueryId) {
        let _ = self.handler.handle_message(ZenohMessage::make_unit(
            zmsg::default_channel::REPLY,
            zmsg::default_congestion_control::REPLY,
            Some(ReplyContext::new(qid, None)),
        ));
    }

    fn send_pull(
        &self,
        is_final: bool,
        key_expr: &WireExpr,
        pull_id: u64,
        max_samples: &Option<u64>,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_pull(
            is_final,
            key_expr.to_owned(),
            pull_id,
            *max_samples,
        ));
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}
