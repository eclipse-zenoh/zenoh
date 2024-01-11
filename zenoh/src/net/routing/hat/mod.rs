//
// Copyright (c) 2023 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use super::{
    dispatcher::{
        face::{Face, FaceState},
        tables::{
            NodeId, PullCaches, QueryTargetQablSet, Resource, Route, RoutingExpr, Tables,
            TablesLock,
        },
    },
    router::RoutesIndexes,
};
use crate::runtime::Runtime;
use std::{any::Any, sync::Arc};
use zenoh_buffers::ZBuf;
use zenoh_config::{unwrap_or_default, Config, WhatAmI};
use zenoh_protocol::{
    core::WireExpr,
    network::{
        declare::{queryable::ext::QueryableInfo, subscriber::ext::SubscriberInfo},
        Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::unicast::TransportUnicast;

mod client;
mod linkstate_peer;
mod p2p_peer;
mod router;

zconfigurable! {
    static ref TREES_COMPUTATION_DELAY: u64 = 100;
}

pub(crate) trait HatTrait: HatBaseTrait + HatPubSubTrait + HatQueriesTrait {}

pub(crate) trait HatBaseTrait {
    fn as_any(&self) -> &dyn Any;

    #[allow(clippy::too_many_arguments)]
    fn init(&self, tables: &mut Tables, runtime: Runtime);

    fn new_tables(&self, router_peers_failover_brokering: bool) -> Box<dyn Any + Send + Sync>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    fn new_local_face(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
    ) -> ZResult<()>;

    fn new_transport_unicast_face(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        transport: &TransportUnicast,
    ) -> ZResult<()>;

    fn handle_oam(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        oam: Oam,
        transport: &TransportUnicast,
    ) -> ZResult<()>;

    fn map_routing_context(
        &self,
        tables: &Tables,
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId;

    fn ingress_filter(&self, tables: &Tables, face: &FaceState, expr: &mut RoutingExpr) -> bool;

    fn egress_filter(
        &self,
        tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &mut RoutingExpr,
    ) -> bool;

    fn closing(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        transport: &TransportUnicast,
    ) -> ZResult<()>;

    fn close_face(&self, tables: &TablesLock, face: &mut Arc<FaceState>);
}

pub(crate) trait HatPubSubTrait {
    fn declare_subscription(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
    );
    fn forget_subscription(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        node_id: NodeId,
    );

    fn compute_data_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route>;

    fn compute_matching_pulls_(
        &self,
        tables: &Tables,
        pull_caches: &mut PullCaches,
        expr: &mut RoutingExpr,
    );

    fn compute_matching_pulls(&self, tables: &Tables, expr: &mut RoutingExpr) -> Arc<PullCaches> {
        let mut pull_caches = PullCaches::default();
        self.compute_matching_pulls_(tables, &mut pull_caches, expr);
        Arc::new(pull_caches)
    }

    fn update_matching_pulls(&self, tables: &Tables, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            let mut res_mut = res.clone();
            let res_mut = get_mut_unchecked(&mut res_mut);
            if res_mut.context_mut().matching_pulls.is_none() {
                res_mut.context_mut().matching_pulls = Some(Arc::new(PullCaches::default()));
            }
            self.compute_matching_pulls_(
                tables,
                get_mut_unchecked(res_mut.context_mut().matching_pulls.as_mut().unwrap()),
                &mut RoutingExpr::new(res, ""),
            );
        }
    }

    fn get_data_routes_entries(&self, tables: &Tables) -> RoutesIndexes;
}

pub(crate) trait HatQueriesTrait {
    fn declare_queryable(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        qabl_info: &QueryableInfo,
        node_id: NodeId,
    );
    fn forget_queryable(
        &self,
        tables: &TablesLock,
        face: &mut Arc<FaceState>,
        expr: &WireExpr,
        node_id: NodeId,
    );
    fn compute_query_route(
        &self,
        tables: &Tables,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<QueryTargetQablSet>;

    fn get_query_routes_entries(&self, tables: &Tables) -> RoutesIndexes;

    fn compute_local_replies(
        &self,
        tables: &Tables,
        prefix: &Arc<Resource>,
        suffix: &str,
        face: &Arc<FaceState>,
    ) -> Vec<(WireExpr<'static>, ZBuf)>;
}

pub(crate) fn new_hat(whatami: WhatAmI, config: &Config) -> Box<dyn HatTrait + Send + Sync> {
    match whatami {
        WhatAmI::Client => Box::new(client::HatCode {}),
        WhatAmI::Peer => {
            if unwrap_or_default!(config.routing().peer().mode()) == *"linkstate" {
                Box::new(linkstate_peer::HatCode {})
            } else {
                Box::new(p2p_peer::HatCode {})
            }
        }
        WhatAmI::Router => Box::new(router::HatCode {}),
    }
}
