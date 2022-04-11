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
mod demux;
mod mux;

use super::protocol;
use super::protocol::core::{
    Channel, CongestionControl, ConsolidationStrategy, KeyExpr, PeerId, QueryTAK, QueryableInfo,
    SubInfo, ZInt,
};
use super::protocol::io::ZBuf;
use super::protocol::proto::{DataInfo, RoutingContext};
pub use demux::*;
pub use mux::*;

pub trait Primitives: Send + Sync {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &KeyExpr);
    fn forget_resource(&self, expr_id: ZInt);

    fn decl_publisher(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>);
    fn forget_publisher(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>);

    fn decl_subscriber(
        &self,
        key_expr: &KeyExpr,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    );
    fn forget_subscriber(&self, key_expr: &KeyExpr, routing_context: Option<RoutingContext>);

    fn decl_queryable(
        &self,
        key_expr: &KeyExpr,
        kind: ZInt,
        qabl_info: &QueryableInfo,
        routing_context: Option<RoutingContext>,
    );
    fn forget_queryable(
        &self,
        key_expr: &KeyExpr,
        kind: ZInt,
        routing_context: Option<RoutingContext>,
    );

    fn send_data(
        &self,
        key_expr: &KeyExpr,
        payload: ZBuf,
        channel: Channel,
        cogestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    );

    fn send_query(
        &self,
        key_expr: &KeyExpr,
        value_selector: &str,
        qid: ZInt,
        target: QueryTAK,
        consolidation: ConsolidationStrategy,
        routing_context: Option<RoutingContext>,
    );

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: PeerId,
        key_expr: KeyExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
    );

    fn send_reply_final(&self, qid: ZInt);

    fn send_pull(
        &self,
        is_final: bool,
        key_expr: &KeyExpr,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    );

    fn send_close(&self);
}

#[derive(Default)]
pub struct DummyPrimitives;

impl DummyPrimitives {
    pub fn new() -> Self {
        Self
    }
}

impl Primitives for DummyPrimitives {
    fn decl_resource(&self, _expr_id: ZInt, _key_expr: &KeyExpr) {}
    fn forget_resource(&self, _expr_id: ZInt) {}

    fn decl_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {}
    fn forget_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {}

    fn decl_subscriber(
        &self,
        _key_expr: &KeyExpr,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn forget_subscriber(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {}

    fn decl_queryable(
        &self,
        _key_expr: &KeyExpr,
        _kind: ZInt,
        _qable_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn forget_queryable(
        &self,
        _key_expr: &KeyExpr,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
    }

    fn send_data(
        &self,
        _key_expr: &KeyExpr,
        _payload: ZBuf,
        _channel: Channel,
        _cogestion_control: CongestionControl,
        _info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn send_query(
        &self,
        _key_expr: &KeyExpr,
        _value_selector: &str,
        _qid: ZInt,
        _target: QueryTAK,
        _consolidation: ConsolidationStrategy,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn send_reply_data(
        &self,
        _qid: ZInt,
        _replier_kind: ZInt,
        _replier_id: PeerId,
        _key_expr: KeyExpr,
        _info: Option<DataInfo>,
        _payload: ZBuf,
    ) {
    }
    fn send_reply_final(&self, _qid: ZInt) {}
    fn send_pull(
        &self,
        _is_final: bool,
        _key_expr: &KeyExpr,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
    }

    fn send_close(&self) {}
}
