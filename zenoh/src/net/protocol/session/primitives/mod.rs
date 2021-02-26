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
    CongestionControl, PeerId, QueryConsolidation, QueryTarget, Reliability, ResKey, SubInfo, ZInt,
};
use super::io::RBuf;
use super::proto::{DataInfo, RoutingContext};
use async_trait::async_trait;
pub use demux::*;
pub use mux::*;

#[async_trait]
pub trait Primitives {
    async fn decl_resource(&self, rid: ZInt, reskey: &ResKey);
    async fn forget_resource(&self, rid: ZInt);

    async fn decl_publisher(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);
    async fn forget_publisher(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);

    async fn decl_subscriber(
        &self,
        reskey: &ResKey,
        sub_info: &SubInfo,
        routing_context: Option<RoutingContext>,
    );
    async fn forget_subscriber(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);

    async fn decl_queryable(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);
    async fn forget_queryable(&self, reskey: &ResKey, routing_context: Option<RoutingContext>);

    async fn send_data(
        &self,
        reskey: &ResKey,
        payload: RBuf,
        reliability: Reliability,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
    );

    async fn send_query(
        &self,
        reskey: &ResKey,
        predicate: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
    );

    async fn send_reply_data(
        &self,
        qid: ZInt,
        source_kind: ZInt,
        replier_id: PeerId,
        reskey: ResKey,
        info: Option<DataInfo>,
        payload: RBuf,
    );

    async fn send_reply_final(&self, qid: ZInt);

    async fn send_pull(
        &self,
        is_final: bool,
        reskey: &ResKey,
        pull_id: ZInt,
        max_samples: &Option<ZInt>,
    );

    async fn send_close(&self);
}

#[derive(Default)]
pub struct DummyPrimitives;

impl DummyPrimitives {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Primitives for DummyPrimitives {
    async fn decl_resource(&self, _rid: ZInt, _reskey: &ResKey) {}
    async fn forget_resource(&self, _rid: ZInt) {}

    async fn decl_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}
    async fn forget_publisher(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    async fn decl_subscriber(
        &self,
        _reskey: &ResKey,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    async fn forget_subscriber(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    async fn decl_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}
    async fn forget_queryable(&self, _reskey: &ResKey, _routing_context: Option<RoutingContext>) {}

    async fn send_data(
        &self,
        _reskey: &ResKey,
        _payload: RBuf,
        _reliability: Reliability,
        _congestion_control: CongestionControl,
        _info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    async fn send_query(
        &self,
        _reskey: &ResKey,
        _predicate: &str,
        _qid: ZInt,
        _target: QueryTarget,
        _consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
    }
    async fn send_reply_data(
        &self,
        _qid: ZInt,
        _source_kind: ZInt,
        _replier_id: PeerId,
        _reskey: ResKey,
        _info: Option<DataInfo>,
        _payload: RBuf,
    ) {
    }
    async fn send_reply_final(&self, _qid: ZInt) {}
    async fn send_pull(
        &self,
        _is_final: bool,
        _reskey: &ResKey,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
    }

    async fn send_close(&self) {}
}
