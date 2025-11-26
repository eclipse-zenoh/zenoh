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
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use zenoh_config::WhatAmI;
use zenoh_protocol::{
    core::ZenohIdProto,
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId, TokenId},
        interest::InterestId,
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
            interests::{CurrentInterest, RemoteInterest},
            region::{Region, RegionMap},
        },
        runtime::Runtime,
    },
};

pub(crate) mod broker;
pub(crate) mod client;
pub(crate) mod peer;
pub(crate) mod router;

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
pub(crate) type Remote = Arc<dyn Any + Send + Sync>;

pub(crate) trait HatBaseTrait: Any {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    // REVIEW(regions): it would be better if this returned a Result instead
    // Currently errors are logged in router::Hat::get_router.

    fn new_remote(&self, face: &Arc<FaceState>, nid: NodeId) -> Option<Remote>;

    fn new_local_face(&mut self, ctx: BaseContext, tables_ref: &Arc<TablesLock>) -> ZResult<()>;

    fn new_transport_unicast_face(
        &mut self,
        ctx: BaseContext,
        other_hats: RegionMap<&dyn HatTrait>,
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

    fn close_face(&mut self, ctx: BaseContext);

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

    fn region(&self) -> Region;

    /// Returns `true` if the hat should route data or queries between `src` and `dst`.
    fn should_route_between(&self, src: &FaceState, dst: &FaceState) -> bool {
        // REVIEW(regions): not sure
        src.region.bound().is_south() ^ dst.region.bound().is_south()
            || (src.whatami.is_client() && src.region.bound().is_south())
            || (dst.whatami.is_client() && dst.region.bound().is_south())
    }

    /// Returns `true` if `face` belongs to this [`Hat`].
    fn owns(&self, face: &FaceState) -> bool {
        if self.region() == face.region && face.remote_bound.is_north() {
            debug_assert_eq!(self.whatami(), face.whatami);
            debug_assert_eq!(face.is_local, self.region() == Region::Local);
            debug_assert_implies!(face.is_local, face.whatami.is_client());
        }

        self.region() == face.region
    }
}

// REVIEW(regions): do resources need to be &mut Arc<Resource> instead of &Arc<Resource>?

/// Hat interest protocol interface.
///
/// Below is the typical code-path for each message type.
///
/// # Interest (current and/or future)
///   1. [`HatInterestTrait::route_interest`] on the north hat.
///   2. [`HatInterestTrait::register_interest`] on the owner south hat, iff it owns the src face.
///   3. [`HatInterestTrait::propagate_current_subscriptions`] on the owner south hat, iff it owns the src face.
///   4. [`HatInterestTrait::propagate_current_queryables`] on the owner south hat, iff it owns the src face.
///   4. [`HatInterestTrait::propagate_current_tokens`] on the owner south hat, iff it owns the src face.
///   5. [`HatInterestTrait::send_final_declaration`] on the owner south hat,
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
    /// Returns `Some` iff the pending current interest is resolved.
    ///
    /// Will use the dowstream hat reference to call
    /// [`HatInterestTrait::propagate_declarations`].
    fn route_interest(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        src: &Remote,
    ) -> Option<CurrentInterest>;

    /// Handles interest finalization messages.
    ///
    /// This method is only called on the north-bound hat.
    fn route_interest_final(
        &mut self,
        ctx: BaseContext,
        msg: &Interest,
        remote_interest: &RemoteInterest,
    );

    /// Handles declaration finalization messages.
    ///
    /// Returns `Some` iff the pending current interest is resolved.
    ///
    /// This method is only called on the north hat.
    fn route_declare_final(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
    ) -> Option<CurrentInterest>;

    fn route_current_token(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId,
        res: Arc<Resource>,
    ) -> Option<CurrentInterest>;

    // FIXME(regions): only the _declaration_ owner hat should store inbound entities in its specific way.
    // Thus the _interest_ owner hat doesn't have all declarations and the .propagate_declarations(..) fn
    // should be reworked.

    /// Propagates current subscriptions to the interested subregion.
    ///
    /// This method is only called on the owner south hat.
    fn send_current_subscriptions(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    );

    /// Propagates current queryables to the interested subregion.
    ///
    /// This method is only called on the owner south hat.
    fn send_current_queryables(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    );

    fn send_current_tokens(
        &self,
        ctx: BaseContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashSet<Arc<Resource>>,
    );

    fn propagate_current_token(
        &self,
        ctx: BaseContext,
        res: Arc<Resource>,
        interest: CurrentInterest,
    );

    /// Informs the interest source that all declarations have been transmitted.
    ///
    /// This method is only called on south hats.
    fn send_declare_final(
        &mut self,
        ctx: BaseContext,
        interest_id: InterestId, // TODO(regions*): change to &Interest (?)
        dst: &Remote,
    );

    /// Register remote interests.
    ///
    /// This method is only called on the owner south hat.
    fn register_interest(&mut self, ctx: BaseContext, msg: &Interest, res: Option<Arc<Resource>>);

    /// Unregister remote interests.
    ///
    /// This method is only called on the owner south hat.
    fn unregister_interest(&mut self, ctx: BaseContext, msg: &Interest) -> Option<RemoteInterest>;

    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest>;
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

    /// Register a subscriber entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn register_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: Arc<Resource>,
        nid: NodeId,
        info: &SubscriberInfo,
    );

    /// Unregister a subscriber entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn unregister_subscription(
        &mut self,
        ctx: BaseContext,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>>;

    fn unregister_face_subscriptions(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>>;

    /// Propagate a subscriber entity.
    ///
    /// The callee hat will only push the subscription if is the north hat.
    fn propagate_subscription(
        &mut self,
        ctx: BaseContext,
        res: Arc<Resource>,
        other_info: Option<SubscriberInfo>,
    );

    /// Unpropagate a subscriber entity.
    fn unpropagate_subscription(&mut self, ctx: BaseContext, res: Arc<Resource>);

    /// Unpropagate the last remaining subscriber entity which the callee hat doesn't own.
    ///
    /// This implies that the callee hat owns the last remaining subscriber and that the penultimate
    /// subscriber was unregistered.
    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_subscription(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        self.unpropagate_subscription(ctx, res);
    }

    fn remote_subscriptions_of(&self, res: &Resource) -> Option<SubscriberInfo>;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_subscriptions(&self, tables: &TablesData) -> HashMap<Arc<Resource>, SubscriberInfo> {
        self.remote_subscriptions_matching(tables, None)
    }

    fn remote_subscriptions_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, SubscriberInfo>;

    fn get_subscriptions(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn get_publications(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_data_route(
        &self,
        tables: &TablesData,
        face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
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

    /// Register a queryable entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn register_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: Arc<Resource>,
        nid: NodeId,
        info: &QueryableInfoType,
    );

    /// Unregister a queryable entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn unregister_queryable(
        &mut self,
        ctx: BaseContext,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>>;

    fn unregister_face_queryables(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>>;

    /// Propagate a queryable entity.
    ///
    /// The callee hat will only push the subscription if is the north hat.
    fn propagate_queryable(
        &mut self,
        ctx: BaseContext,
        res: Arc<Resource>,
        other_info: Option<QueryableInfoType>,
    );

    /// Unpropagate a queryable entity.
    fn unpropagate_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>);

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_queryable(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        self.unpropagate_queryable(ctx, res);
    }

    fn remote_queryables_of(&self, res: &Resource) -> Option<QueryableInfoType>;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_queryables(&self, tables: &TablesData) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.remote_queryables_matching(tables, None)
    }

    fn remote_queryables_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, QueryableInfoType>;

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

    /// Handles token undeclaration.
    fn undeclare_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        node_id: NodeId,
    ) -> Option<Arc<Resource>>;

    /// Register a token entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn register_token(&mut self, ctx: BaseContext, id: TokenId, res: Arc<Resource>, nid: NodeId);

    /// Unregister a token entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn unregister_token(
        &mut self,
        ctx: BaseContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>>;

    fn unregister_face_tokens(&mut self, ctx: BaseContext) -> HashSet<Arc<Resource>>;

    /// Propagate a token entity.
    ///
    /// The callee hat will only push the subscription if is the north hat.
    fn propagate_token(&mut self, ctx: BaseContext, res: Arc<Resource>, other_tokens: bool);

    /// Unpropagate a queryable entity.
    fn unpropagate_token(&mut self, ctx: BaseContext, res: Arc<Resource>);

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_token(&mut self, ctx: BaseContext, res: Arc<Resource>) {
        self.unpropagate_token(ctx, res);
    }

    fn remote_tokens_of(&self, res: &Resource) -> bool;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_tokens(&self, tables: &TablesData) -> HashSet<Arc<Resource>> {
        self.remote_tokens_matching(tables, None)
    }

    fn remote_tokens_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashSet<Arc<Resource>>;
}
