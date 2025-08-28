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
        interest::{InterestId, InterestMode},
        Declare, Interest, Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_transport::unicast::TransportUnicast;

use super::{
    dispatcher::{
        face::FaceState,
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
    routing::dispatcher::{gateway::Bound, interests::RemoteInterest},
    runtime::Runtime,
};

mod client;
mod p2p_peer;
mod router;

zconfigurable! {
    pub static ref TREES_COMPUTATION_DELAY_MS: u64 = 100;
}

// TODO(now)
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

pub(crate) struct BaseContext<'ctx> {
    pub(crate) tables_lock: &'ctx Arc<TablesLock>,
    pub(crate) tables: &'ctx mut TablesData,
    pub(crate) src_face: &'ctx mut Arc<FaceState>,
    pub(crate) send_declare: &'ctx mut SendDeclare<'ctx>,
}

impl<'ctx> BaseContext<'ctx> {
    /// Reborrows [`BaseContext`] to avoid moving it.
    pub(crate) fn reborrow(&mut self) -> BaseContext<'_> {
        BaseContext {
            tables_lock: &*self.tables_lock,
            tables: &mut *self.tables,
            src_face: &mut *self.src_face,
            send_declare: &mut *self.send_declare,
        }
    }
}

pub(crate) trait HatBaseTrait: Any {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    fn new_local_face(&mut self, ctx: BaseContext, tables_ref: &Arc<TablesLock>) -> ZResult<()>;

    fn new_transport_unicast_face(
        &mut self,
        ctx: BaseContext,
        tables_ref: &Arc<TablesLock>,
        transport: &TransportUnicast,
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

    fn node_id_to_zid(&self, face: &FaceState, _node_id: NodeId) -> Option<ZenohIdProto> {
        // FIXME(regions): remove default impl
        Some(face.zid.clone())
    }

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

    fn close_face(&mut self, ctx: BaseContext, tables_ref: &Arc<TablesLock>);

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

pub(crate) trait HatInterestTrait {
    /// Handles interest originating downstream where this hat is the subregion gateway
    /// so we should send all known declarations in the immediate upstream region
    /// to the source.
    fn propagate_declarations(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        upstream_hat: &mut dyn HatTrait,
    );

    /// Handles interest messages for north-bound hats.
    ///
    /// Will use the dowstream hat reference to call
    /// [`HatInterestTrait::finalize_current_interest`].
    ///
    /// There can be one of two behaviors depending on the bound of the source
    /// face:
    ///
    /// # Relay
    /// Interest originates upstream and we are not the subregion's gateway
    /// so we should forward it point-to-point to the gateway; this is only
    /// relevant for router hats.
    ///
    /// # Exit relay
    /// Interest originates downstream and we are this gateway's exit point
    /// so we should propagate it upstream to the next gateway.
    fn propagate_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        zid: &ZenohIdProto,
    ) -> bool;

    /// Handles interest finalization where this hat is the exit relay.
    ///
    /// Will use the downstream hat reference to call
    /// [`HatInterestTrait::finalize_current_interest`].
    fn unregister_current_interest(
        &mut self,
        ctx: BaseContext,
        id: InterestId,
        downstream_hat: &mut dyn HatTrait,
    );

    /// Informs the interest source that all declarations have been transmitted.
    fn finalize_current_interest(&mut self, ctx: BaseContext, id: InterestId, zid: &ZenohIdProto);

    /// Handles interest finalization where this hat is the subregion gateway.
    ///
    /// Will use the upstream hat reference to call
    /// [`HatInterestTrait::finalize_interest`].
    fn unregister_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        upstream_hat: &mut dyn HatTrait,
    );

    /// Handles interest finalization messages for north-bound hats.
    ///
    /// There can be one of two behaviors depending on the bound of the source
    /// face:
    ///
    /// # Relay
    /// Interest originates upstream and we are not the subregion's gateway
    /// so we should forward it point-to-point to the gateway; this is only
    /// relevant for router hats.
    ///
    /// # Exit relay
    /// Interest originates downstream and we are this gateway's exit point
    /// so we should propagate it upstream to the next gateway.
    fn finalize_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        inbound_interest: RemoteInterest,
    );
}

pub(crate) trait HatPubSubTrait {
    /// Handles subscriber declaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn declare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        sub_info: &SubscriberInfo,
        profile: InterestProfile,
    );

    /// Handles subscriber undeclaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn undeclare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
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
        ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        qabl_info: &QueryableInfoType,
        profile: InterestProfile,
    );

    /// Handles queryable undeclaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn undeclare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
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
        ctx: BaseContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        interest_id: Option<InterestId>,
        profile: InterestProfile,
    );

    /// Handles token undeclaration.
    ///
    /// The undeclaration is pushed this hat's subregion if `ctx.is_owned` is `true`
    /// or if `profile` is [`InterestProfile::Push`].
    fn undeclare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
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
