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
use std::{any::Any, collections::HashMap, ops::Deref, sync::Arc};

use zenoh_config::WhatAmI;
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
use crate::{
    key_expr::KeyExpr,
    net::{
        protocol::{linkstate::LinkInfo, network::SuccessorEntry},
        routing::dispatcher::{
            gateway::{Bound, BoundMap},
            interests::{CurrentInterest, RemoteInterest},
        },
        runtime::Runtime,
    },
};

mod client;
mod peer;
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

impl BaseContext<'_> {
    /// Reborrows [`BaseContext`] to avoid moving it.
    pub(crate) fn reborrow(&mut self) -> BaseContext<'_> {
        BaseContext {
            tables_lock: self.tables_lock,
            tables: &mut *self.tables,
            src_face: &mut *self.src_face,
            send_declare: &mut *self.send_declare,
        }
    }
}

/// A party with which a hat exchanges messages.
///
/// This type exists to generalize [`crate:::net::routing::dispatcher::face::Face`] ̦
/// to nodes that don't have a face but still need to be identified in hat interfaces.
///
/// Named after Git remotes.
pub(crate) struct Remote(Box<dyn Any + Send + Sync>);

impl Deref for Remote {
    type Target = dyn Any;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) trait HatBaseTrait: Any {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    // REVIEW(regions): it would be betetr if this returned a Result instead
    // Currently errors are logged in router::Hat::get_router.

    fn new_remote(&self, face: &Arc<FaceState>, nid: NodeId) -> Option<Remote>;

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
        zid: &ZenohIdProto,
        whatami: WhatAmI,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()>;

    fn map_routing_context(
        &self,
        tables: &TablesData, // TODO(fuzzpixelz): can this be removed?
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId;

    fn ingress_filter(&self, tables: &TablesData, face: &FaceState, expr: &RoutingExpr) -> bool;

    fn egress_filter(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &RoutingExpr,
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

    #[allow(dead_code)] // FIXME(regions)
    fn route_successor(&self, _src: ZenohIdProto, _dst: ZenohIdProto) -> Option<ZenohIdProto> {
        None
    }

    #[allow(dead_code)] // FIXME(regions)
    fn route_successors(&self) -> Vec<SuccessorEntry> {
        Vec::new()
    }

    #[allow(dead_code)]
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn whatami(&self) -> WhatAmI;
    fn bound(&self) -> Bound;

    /// Returns `true` if the hat should route data or queries between `src` and `dst`.
    fn should_route_between(&self, src: &FaceState, dst: &FaceState) -> bool {
        // REVIEW(regions): not sure
        !src.local_bound.is_north() ^ !dst.local_bound.is_north()
            || (src.whatami.is_client() && !src.local_bound.is_north())
            || (dst.whatami.is_client() && !dst.local_bound.is_north())
    }

    fn assert_proper_ownership(&self, ctx: &BaseContext) {
        // TODO(regions): remove this

        if self.bound() != ctx.src_face.local_bound {
            unreachable!(
                "Hat doesn't own source face (bound={},face={})",
                self.bound(),
                ctx.src_face
            )
        }

        if self.whatami() != ctx.src_face.whatami {
            unreachable!(
                "Hat owns source face but whatami's don't match (whatami={},face={})",
                self.whatami(),
                ctx.src_face
            )
        }
    }
}

// REVIEW(regions): do resources need to be &mut Arc<Resource> instead of &Arc<Resource>?
// REVIEW(regions): use dispatcher calls instead of passing `south_hats`?

/// Hat interest protocol interface.
///
/// Below is the typical code-path for each message type.
///
/// # Interest (current and/or future)
///   1. [`HatInterestTrait::route_interest`] on the north hat.
///   2. [`HatInterestTrait::register_interest`] on the owner south hat, iff it owns the src face.
///   2. [`HatInterestTrait::send_declarations`] on the owner south hat, iff it owns the src face.
///   3. [`HatInterestTrait::send_final_declaration`] on the owner south hat,
///      iff it owns the src face and there is no gateway in the north region.
///
/// # Interest (final)
///   1. [`HatInterestTrait::route_interest_final`] on the north hat.
///   2. [`HatInterestTrait::unregister_interest`] on the owner south hat, iff it owns the src face.
///
/// # Declare (final)
///   1. [`HatInterestTrait::route_final_declaration`] on the north hat.
///   2. [`HatInterestTrait::send_final_declaration`] on the owner south hat, iff the msg is intended for it.
pub(crate) trait HatInterestTrait {
    /// Handles interest messages.
    ///
    /// This method is only called on the north-bound hat.
    ///
    /// Will use the dowstream hat reference to call
    /// [`HatInterestTrait::propagate_declarations`].
    fn route_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
        south_hats: BoundMap<&mut dyn HatTrait>,
    );

    /// Handles interest finalization messages.
    ///
    /// This method is only called on the north-bound hat.
    fn route_interest_final(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        south_hats: BoundMap<&mut dyn HatTrait>,
    );

    /// Handles declaration finalization messages.
    ///
    /// This method is only called on the north hat.
    fn route_final_declaration(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
        south_hats: BoundMap<&mut dyn HatTrait>,
    );

    // FIXME(regions): only the _declaration_ owner hat should store inbound entities in its specific way.
    // Thus the _interest_ owner hat doesn't have all declarations and the .propagate_declarations(..) fn
    // should be reworked.

    /// Propagates current declarations to the interested subregion.
    ///
    /// This method is only called on the owner south hat.
    fn send_declarations(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    );

    /// Informs the interest source that all declarations have been transmitted.
    ///
    /// This method is only called on south hats.
    fn send_final_declaration(
        &mut self,
        ctx: BaseContext,
        id: InterestId, // TODO(regions*): change to &Interest (?)
        src: &Remote,
    );

    /// Register remote interests.
    ///
    /// This method is only called on the owner south hat.
    fn register_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<&mut Arc<Resource>>,
    );

    /// Unregister remote interests.
    ///
    /// This method is only called on the owner south hat.
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest>;
}

pub(crate) trait HatPubSubTrait {
    /// Handles subscriber declaration.
    fn declare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        sub_info: &SubscriberInfo,
    );

    /// Handles subscriber undeclaration.
    fn undeclare_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>>;

    fn get_subscriptions(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn get_publications(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_data_route(
        &self,
        tables: &TablesData,
        face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
        dst_node_id: NodeId,
    ) -> Arc<Route>;

    fn get_matching_subscriptions(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
    ) -> HashMap<usize, Arc<FaceState>>;
}

pub(crate) trait HatQueriesTrait {
    /// Handles queryable declaration.
    fn declare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        qabl_info: &QueryableInfoType,
    );

    /// Handles queryable undeclaration.
    fn undeclare_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>>;

    fn get_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn get_queriers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_query_route(
        &self,
        tables: &TablesData,
        face: &FaceState,
        expr: &RoutingExpr,
        source: NodeId,
    ) -> Arc<QueryTargetQablSet>;

    fn get_matching_queryables(
        &self,
        tables: &TablesData,
        key_expr: &KeyExpr<'_>,
        complete: bool,
    ) -> HashMap<usize, Arc<FaceState>>;
}

pub(crate) fn new_hat(whatami: WhatAmI, bound: Bound) -> Box<dyn HatTrait + Send + Sync> {
    match whatami {
        WhatAmI::Client => Box::new(client::Hat::new(bound)),
        WhatAmI::Peer => Box::new(peer::Hat::new(bound)),
        WhatAmI::Router => Box::new(router::Hat::new(bound)),
    }
}

pub(crate) trait HatTokenTrait {
    /// Handles token declaration.
    fn declare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: &mut Arc<Resource>,
        node_id: NodeId,
        interest_id: Option<InterestId>,
    );

    /// Handles token declaration.
    fn declare_current_token(
        &mut self,
        ctx: BaseContext,
        res: &mut Arc<Resource>,
        interest_id: InterestId,
        downstream_hats: BoundMap<&mut dyn HatTrait>,
    );

    fn propagate_current_token(
        &mut self,
        ctx: BaseContext,
        res: &mut Arc<Resource>,
        interest: &CurrentInterest,
    );

    /// Handles token undeclaration.
    fn undeclare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
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
