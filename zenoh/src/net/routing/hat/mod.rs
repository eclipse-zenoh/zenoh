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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::{any::Any, collections::HashMap, sync::Arc};

use zenoh_config::{unwrap_or_default, Config, WhatAmI};
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId, TokenId},
        interest::{InterestId, InterestMode, InterestOptions},
        Declare, Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_transport::unicast::TransportUnicast;

use super::{
    dispatcher::{
        face::{Face, FaceState},
        pubsub::SubscriberInfo,
        tables::{NodeId, QueryTargetQablSet, Resource, Route, RoutingExpr, Tables, TablesLock},
    },
    RoutingContext,
};
use crate::{
    key_expr::KeyExpr,
    net::{
        protocol::{linkstate::LinkInfo, network::SuccessorEntry},
        runtime::Runtime,
    },
};

mod client;
mod linkstate_peer;
mod p2p_peer;
mod router;

zconfigurable! {
    pub static ref TREES_COMPUTATION_DELAY_MS: u64 = 100;
}

#[derive(Default, serde::Serialize)]
pub(crate) struct Sources {
    routers: Vec<ZenohIdProto>,
    peers: Vec<ZenohIdProto>,
    clients: Vec<ZenohIdProto>,
}

impl Sources {
    pub(crate) fn empty() -> Self {
        Self {
            routers: vec![],
            peers: vec![],
            clients: vec![],
        }
    }
}

pub(crate) type SendDeclare<'a> = dyn FnMut(&Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>, RoutingContext<Declare>)
    + 'a;
pub(crate) trait HatTrait:
    HatBaseTrait + HatInterestTrait + HatPubSubTrait + HatQueriesTrait + HatTokenTrait
{
}

pub(crate) trait HatBaseTrait {
    fn init(&self, tables: &mut Tables, runtime: Runtime) -> ZResult<()>;

    fn new_tables(&self, router_peers_failover_brokering: bool) -> Box<dyn Any + Send + Sync>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    fn new_local_face(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn new_transport_unicast_face(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        transport: &TransportUnicast,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn handle_oam(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        oam: &mut Oam,
        transport: &TransportUnicast,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn map_routing_context(
        &self,
        tables: &Tables,
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId;

    fn ingress_filter(&self, tables: &Tables, face: &FaceState, expr: &RoutingExpr) -> bool;

    fn egress_filter(
        &self,
        tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &RoutingExpr,
    ) -> bool;

    fn info(&self, tables: &Tables, kind: WhatAmI) -> String;

    fn close_face(
        &self,
        tables: &TablesLock,
        tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    );

    fn update_from_config(
        &self,
        _tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        _runtime: &Runtime,
    ) -> ZResult<()> {
        Ok(())
    }

    fn links_info(&self, _tables: &Tables) -> HashMap<ZenohIdProto, LinkInfo> {
        HashMap::new()
    }

    fn route_successor(
        &self,
        _tables: &Tables,
        _src: ZenohIdProto,
        _dst: ZenohIdProto,
    ) -> Option<ZenohIdProto> {
        None
    }

    fn route_successors(&self, _tables: &Tables) -> Vec<SuccessorEntry> {
        Vec::new()
    }
}

pub(crate) trait HatInterestTrait {
    #[allow(clippy::too_many_arguments)]
    fn declare_interest(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        options: InterestOptions,
        send_declare: &mut SendDeclare,
    );
    fn undeclare_interest(&self, tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId);
    fn declare_final(&self, tables: &mut Tables, face: &mut Arc<FaceState>, id: InterestId);
}

pub(crate) trait HatPubSubTrait {
    #[allow(clippy::too_many_arguments)]
    fn declare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    );
    fn undeclare_subscription(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>>;

    fn get_subscriptions(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)>;

    fn get_publications(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_data_route(
        &self,
        tables: &Tables,
        expr: &RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route>;

    fn get_matching_subscriptions(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>>;
}

pub(crate) trait HatQueriesTrait {
    #[allow(clippy::too_many_arguments)]
    fn declare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    );
    fn undeclare_queryable(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>>;

    fn get_queryables(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)>;

    fn get_queriers(&self, tables: &Tables) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_query_route(
        &self,
        tables: &Tables,
        expr: &RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<QueryTargetQablSet>;

    fn get_matching_queryables(
        &self,
        tables: &Tables,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<usize, Arc<FaceState>>;
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

pub(crate) trait HatTokenTrait {
    #[allow(clippy::too_many_arguments)]
    fn declare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        interest_id: Option<InterestId>,
        send_declare: &mut SendDeclare,
    );

    fn undeclare_token(
        &self,
        tables: &mut Tables,
        face: &mut Arc<FaceState>,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
        send_declare: &mut SendDeclare,
    ) -> Option<Arc<Resource>>;
}

trait CurrentFutureTrait {
    fn future(&self) -> bool;
    fn current(&self) -> bool;
}

impl CurrentFutureTrait for InterestMode {
    #[inline]
    fn future(&self) -> bool {
        self == &InterestMode::Future || self == &InterestMode::CurrentFuture
    }

    #[inline]
    fn current(&self) -> bool {
        self == &InterestMode::Current || self == &InterestMode::CurrentFuture
    }
}
