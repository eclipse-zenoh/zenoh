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

use zenoh_config::{Config, WhatAmI};
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
        tables::{
            NodeId, QueryTargetQablSet, Resource, Route, RoutingExpr, TablesData, TablesLock,
        },
    },
    RoutingContext,
};
#[cfg(feature = "unstable")]
use crate::key_expr::KeyExpr;
use crate::net::{
    protocol::{linkstate::LinkInfo, network::SuccessorEntry},
    routing::dispatcher::gateway::Bound,
    runtime::Runtime,
};

mod client;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InterestProfile {
    Push,
    Pull,
}

impl InterestProfile {
    pub(crate) fn is_push(&self) -> bool {
        matches!(self, InterestProfile::Push)
    }

    pub(crate) fn is_pull(&self) -> bool {
        matches!(self, InterestProfile::Pull)
    }

    /// Computes [`InterestProfile`] from source and destination [`Bound`]s for a given entity.
    pub(crate) fn with_bound_flow((src, dst): (&Bound, &Bound)) -> Self {
        if src.is_north() && !dst.is_north() {
            Self::Pull
        } else {
            Self::Push
        }
    }
}

pub(crate) type SendDeclare<'a> = dyn FnMut(&Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>, RoutingContext<Declare>)
    + 'a;
pub(crate) trait HatTrait:
    HatBaseTrait + HatInterestTrait + HatPubSubTrait + HatQueriesTrait + HatTokenTrait
{
}

pub(crate) trait HatBaseTrait: Any {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    fn new_local_face(
        &mut self,
        tables: &mut TablesData,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn new_transport_unicast_face(
        &mut self,
        tables: &mut TablesData,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        transport: &TransportUnicast,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn handle_oam(
        &mut self,
        tables: &mut TablesData,
        tables_ref: &Arc<TablesLock>,
        oam: &mut Oam,
        transport: &TransportUnicast,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn map_routing_context(
        &self,
        tables: &TablesData, // TODO(fuzzpixelz): can this be removed?
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId;

    fn ingress_filter(&self, tables: &TablesData, face: &FaceState, expr: &mut RoutingExpr)
        -> bool;

    fn egress_filter(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &mut RoutingExpr,
    ) -> bool;

    fn info(&self, kind: WhatAmI) -> String;

    fn close_face(
        &mut self,
        tables: &mut TablesData,
        tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    );

    fn update_from_config(
        &mut self,

        _tables_ref: &Arc<TablesLock>,
        _runtime: &Runtime,
    ) -> ZResult<()> {
        Ok(())
    }

    fn links_info(&self) -> HashMap<ZenohIdProto, LinkInfo> {
        HashMap::new()
    }

    fn route_successor(&self, _src: ZenohIdProto, _dst: ZenohIdProto) -> Option<ZenohIdProto> {
        None
    }

    fn route_successors(&self) -> Vec<SuccessorEntry> {
        Vec::new()
    }

    #[allow(dead_code)]
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(crate) struct DeclarationContext<'ctx> {
    pub(crate) tables: &'ctx mut TablesData,
    pub(crate) src_face: &'ctx mut Arc<FaceState>,
    pub(crate) send_declare: &'ctx mut SendDeclare<'ctx>,
    pub(crate) node_id: NodeId,
}

pub(crate) trait HatInterestTrait {
    fn declare_interest(
        &self,
        ctx: DeclarationContext,
        tables_ref: &Arc<TablesLock>,
        id: InterestId,
        res: Option<&mut Arc<Resource>>,
        mode: InterestMode,
        options: InterestOptions,
    );

    fn undeclare_interest(&self, ctx: DeclarationContext, id: InterestId);

    fn declare_final(&self, ctx: DeclarationContext, id: InterestId);
}

pub(crate) trait HatPubSubTrait {
    /// Handles subscriber declaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn declare_subscription(
        &mut self,
        ctx: DeclarationContext,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        sub_info: &SubscriberInfo,
        profile: InterestProfile,
    );

    /// Handles subscriber undeclaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn undeclare_subscription(
        &mut self,
        ctx: DeclarationContext,
        id: SubscriberId,
        res: Option<Arc<Resource>>, // FIXME(fuzzypixelz): can this be a borrow
        profile: InterestProfile,
    ) -> Option<Arc<Resource>>;

    fn get_subscriptions(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn get_publications(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_data_route(
        &self,
        tables: &TablesData,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<Route>;

    #[zenoh_macros::unstable]
    fn get_matching_subscriptions(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>>;
}

pub(crate) trait HatQueriesTrait {
    /// Handles queryable declaration.
    ///
    /// The declaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn declare_queryable(
        &mut self,
        ctx: DeclarationContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        qabl_info: &QueryableInfoType,
        profile: InterestProfile,
    );

    /// Handles queryable undeclaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn undeclare_queryable(
        &mut self,
        ctx: DeclarationContext,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        profile: InterestProfile,
    ) -> Option<Arc<Resource>>;

    fn get_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn get_queriers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_query_route(
        &self,
        tables: &TablesData,
        expr: &mut RoutingExpr,
        source: NodeId,
        source_type: WhatAmI,
    ) -> Arc<QueryTargetQablSet>;

    #[zenoh_macros::unstable]
    fn get_matching_queryables(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<usize, Arc<FaceState>>;
}

pub(crate) fn new_hat(
    whatami: WhatAmI,
    _config: &Config,
    bound: Bound,
) -> Box<dyn HatTrait + Send + Sync> {
    match whatami {
        WhatAmI::Client => Box::new(client::Hat::new(bound)),
        WhatAmI::Peer => Box::new(p2p_peer::Hat::new(bound)),
        WhatAmI::Router => Box::new(router::Hat::new(bound)),
    }
}

pub(crate) trait HatTokenTrait {
    /// Handles token declaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn declare_token(
        &mut self,
        ctx: DeclarationContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        interest_id: Option<InterestId>,
        profile: InterestProfile,
    );

    /// Handles token undeclaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn undeclare_token(
        &mut self,
        ctx: DeclarationContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        profile: InterestProfile,
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
