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
mod demux;
mod mux;

use super::core;
use super::io;
use super::link;
use super::proto;
use super::session;

use super::core::{
    Channel, CongestionControl, PeerId, QueryConsolidation, QueryTarget, ResKey, SubInfo, ZInt,
};
use super::io::ZBuf;
use super::proto::{DataInfo, RoutingContext};
pub use demux::*;
pub use mux::*;

pub trait Primitives {
    fn decl_resource(&self, rid: ZInt, reskey: &ResKey);
    fn forget_resource(&self, rid: ZInt);

    fn decl_publisher(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);
    fn forget_publisher(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);

    fn decl_subscriber(
        &self,
        reskey: &ResKey,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    );
    fn forget_subscriber(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);

    fn decl_queryable(&self, reskey: &ResKey, kind: ZInt, routing_context: Option<RoutingContext>);
    fn forget_queryable(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);

    fn send_data(
        &self,
        reskey: &ResKey,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    );

    fn send_query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
    );

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        info: Option<DataInfo>,
        payload: ZBuf,
    );

    fn send_reply_final(&self, qid: ZInt);

    fn send_pull(&self, is_final: bool, reskey: &ResKey, pull_id: ZInt, max_samples: &Option<ZInt>);

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
    fn decl_resource(&self, _rid: ZInt, _reskey: &ResKey) {}
    fn forget_resource(&self, _rid: ZInt) {}

    fn decl_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}
    fn forget_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn decl_subscriber(
        &self,
        _reskey: &ResKey,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn forget_subscriber(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn decl_queryable(
        &self,
        _reskey: &ResKey,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn forget_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    fn send_data(
        &self,
        _reskey: &ResKey,
        _payload: ZBuf,
        _channel: Channel,
        _congestion_control: CongestionControl,
        _info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn send_query(
        &self,
        _reskey: &ResKey,
        _predicate: &str,
        _qid: ZInt,
        _target: QueryTarget,
        _consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    fn send_reply_data(
        &self,
        _qid: ZInt,
        _replier_kind: ZInt,
        _replier_id: PeerId,
        _reskey: ResKey,
        _info: Option<DataInfo>,
        _payload: ZBuf,
    ) {
    }
    fn send_reply_final(&self, _qid: ZInt) {}
    fn send_pull(
        &self,
        _is_final: bool,
        _reskey: &ResKey,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
    }

    fn send_close(&self) {}
}
