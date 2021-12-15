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
use super::super::TransportUnicast;
use super::protocol::core::{
    Channel, CongestionControl, KeyExpr, PeerId, QueryConsolidation, QueryTarget, QueryableInfo,
    SubInfo, ZInt,
};
use super::protocol::io::ZBuf;
use super::protocol::message::{
    default_channel, default_congestion_control, DataInfo, Declaration, ForgetPublisher,
    ForgetQueryable, ForgetResource, ForgetSubscriber, Publisher, Queryable, ReplierInfo,
    ReplyContext, Resource, RoutingContext, Subscriber, ZenohMessage,
};
use super::Primitives;

pub struct Mux {
    handler: TransportUnicast,
}

impl Mux {
    pub(crate) fn new(handler: TransportUnicast) -> Mux {
        Mux { handler }
    }
}

impl Primitives for Mux {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &KeyExpr) {
        let d = Declaration::Resource(Resource {
            expr_id,
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, None, None));
    }

    fn forget_resource(&self, expr_id: ZInt) {
        let d = Declaration::ForgetResource(ForgetResource { expr_id });
        let decls = vec![d];
        let _ = self
            .handler
            .handle_message(ZenohMessage::make_declare(decls, None, None));
    }

    fn decl_subscriber(
        &self,
        key_expr: &KeyExpr,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let d = Declaration::Subscriber(Subscriber {
            key: key_expr.to_owned(),
            info: sub_info.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn forget_subscriber(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetSubscriber(ForgetSubscriber {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn decl_publisher(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::Publisher(Publisher {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn forget_publisher(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>) {
        let d = Declaration::ForgetPublisher(ForgetPublisher {
            key: key_expr.to_owned(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn decl_queryable(
        &self,
        key_expr: &KeyExpr,
        kind: ZInt,
        qabl_info: &QueryableInfo,
        routing_context: Option<RoutingContext>,
    ) {
        let d = Declaration::Queryable(Queryable {
            key: key_expr.to_owned(),
            kind,
            info: qabl_info.clone(),
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn forget_queryable(
        &self,
        key_expr: &KeyExpr,
        kind: ZInt,
        routing_context: Option<RoutingContext>,
    ) {
        let d = Declaration::ForgetQueryable(ForgetQueryable {
            key: key_expr.to_owned(),
            kind,
        });
        let decls = vec![d];
        let _ =
            self.handler
                .handle_message(ZenohMessage::make_declare(decls, routing_context, None));
    }

    fn send_data(
        &self,
        key_expr: &KeyExpr,
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
            None,
        ));
    }

    fn send_query(
        &self,
        key_expr: &KeyExpr,
        value_selector: &str,
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
            key_expr.to_owned(),
            value_selector.to_string(),
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
        replier_kind: ZInt,
        replier_id: PeerId,
        key_expr: KeyExpr,
        data_info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_data(
            key_expr.to_owned(),
            payload,
            default_channel::REPLY,
            default_congestion_control::REPLY,
            data_info,
            None,
            Some(ReplyContext::new(
                qid,
                Some(ReplierInfo {
                    kind: replier_kind,
                    id: replier_id,
                }),
            )),
            None,
        ));
    }

    fn send_reply_final(&self, qid: ZInt) {
        let _ = self.handler.handle_message(ZenohMessage::make_unit(
            default_channel::REPLY,
            default_congestion_control::REPLY,
            Some(ReplyContext::new(qid, None)),
            None,
        ));
    }

    fn send_pull(
        &self,
        is_final: bool,
        key_expr: &KeyExpr,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    ) {
        let _ = self.handler.handle_message(ZenohMessage::make_pull(
            is_final,
            key_expr.to_owned(),
            pull_id,
            *max_samples,
            None,
        ));
    }

    fn send_close(&self) {
        // self.handler.closing().await;
    }
}
