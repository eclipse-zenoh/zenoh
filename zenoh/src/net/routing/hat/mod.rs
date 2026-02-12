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
    fmt::Debug,
    sync::Arc,
};

use zenoh_config::WhatAmI;
use zenoh_protocol::{
    core::{Region, ZenohIdProto},
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
use crate::net::{
    protocol::{linkstate::LinkInfo, network::SuccessorEntry},
    routing::dispatcher::{
        interests::{CurrentInterest, RemoteInterest},
        region::RegionMap,
    },
    runtime::Runtime,
};

pub(crate) mod broker;
pub(crate) mod client;
pub(crate) mod peer;
pub(crate) mod router;

zconfigurable! {
    pub static ref TREES_COMPUTATION_DELAY_MS: u64 = 100;
}

#[derive(Debug, Default, serde::Serialize)]
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

    pub(crate) fn extend(&mut self, other: &Sources) {
        self.routers.extend(other.routers.iter().copied());
        self.peers.extend(other.peers.iter().copied());
        self.clients.extend(other.clients.iter().copied());
    }
}

pub(crate) type SendDeclare<'a> = dyn FnMut(&Arc<dyn crate::net::primitives::EPrimitives + Send + Sync>, RoutingContext<Declare>)
    + 'a;
pub(crate) trait HatTrait:
    HatBaseTrait + HatInterestTrait + HatPubSubTrait + HatQueriesTrait + HatTokenTrait
{
}

pub(crate) struct DispatcherContext<'ctx> {
    pub(crate) tables_lock: &'ctx Arc<TablesLock>,
    pub(crate) tables: &'ctx mut TablesData,
    pub(crate) src_face: &'ctx mut Arc<FaceState>,
    pub(crate) send_declare: &'ctx mut SendDeclare<'ctx>,
}

impl DispatcherContext<'_> {
    /// Reborrows [`DispatcherContext`] to avoid moving it.
    pub(crate) fn reborrow(&mut self) -> DispatcherContext<'_> {
        DispatcherContext {
            tables_lock: self.tables_lock,
            tables: &mut *self.tables,
            src_face: &mut *self.src_face,
            send_declare: &mut *self.send_declare,
        }
    }
}

/// A party with which a hat exchanges messages.
///
/// This type exists to generalize [`super::dispatcher::face::Face`] to nodes that don't have a face
/// but still need to be identified in hat interfaces.
///
/// Named after Git remotes.
#[derive(Debug)]
pub(crate) struct Remote(Box<dyn RemoteTrait>);

impl Remote {
    fn as_any(&self) -> &dyn Any {
        self.0.as_any()
    }

    // FIXME(regions): remove this
    pub(crate) fn downcast_ref_to_face(&self) -> &Arc<FaceState> {
        self.as_any().downcast_ref().unwrap()
    }
}

pub(crate) trait RemoteTrait: Any + Send + Sync + Debug {
    fn clone_box(&self) -> Box<dyn RemoteTrait>;
    fn as_any(&self) -> &dyn Any;
}

impl<T> RemoteTrait for T
where
    T: Any + Send + Sync + Debug + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn RemoteTrait> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clone for Remote {
    fn clone(&self) -> Self {
        Remote(self.0.clone_box())
    }
}

pub(crate) trait HatBaseTrait: Any {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()>;

    fn new_face(&self) -> Box<dyn Any + Send + Sync>;

    fn new_resource(&self) -> Box<dyn Any + Send + Sync>;

    // REVIEW(regions): it would be better if this returned a Result instead
    // Currently errors are logged in router::Hat::get_router.

    fn new_remote(&self, face: &Arc<FaceState>, nid: NodeId) -> Option<Remote>;

    fn new_local_face(
        &mut self,
        ctx: DispatcherContext,
        tables_ref: &Arc<TablesLock>,
    ) -> ZResult<()>;

    fn new_transport_unicast_face(
        &mut self,
        ctx: DispatcherContext,
        transport: &TransportUnicast,
        other_hats: RegionMap<&dyn HatTrait>,
    ) -> ZResult<()>;

    fn handle_oam(
        &mut self,
        ctx: DispatcherContext,
        oam: &mut Oam,
        zid: &ZenohIdProto,
        whatami: WhatAmI,
        other_hats: RegionMap<&mut dyn HatTrait>,
    ) -> ZResult<()>;

    fn map_routing_context(
        &self,
        tables: &TablesData, // TODO(fuzzpixelz): can this be removed?
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId;

    fn ingress_filter(&self, tables: &TablesData, face: &FaceState, expr: &RoutingExpr) -> bool;

    // TODO(regions): review multicast-related logic
    fn egress_filter(
        &self,
        tables: &TablesData,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &RoutingExpr,
    ) -> bool;

    fn info(&self, kind: WhatAmI) -> String; // FIXME(regions*): remove `kind`.

    fn close_face(&mut self, ctx: DispatcherContext);

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

    #[allow(dead_code)] // FIXME(regions)
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn whatami(&self) -> WhatAmI;

    fn region(&self) -> Region;

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
// FIXME(regions): update comments below

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
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        src: &Remote,
    ) -> Option<CurrentInterest>; // TODO(regions): it's odd that this needs to be `Some` for "resolved interest" (i.e. doing nothing).

    /// Handles interest finalization messages.
    ///
    /// This method is only called on the north-bound hat.
    fn route_interest_final(
        &mut self,
        ctx: DispatcherContext,
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
        ctx: DispatcherContext,
        interest_id: InterestId,
    ) -> Option<CurrentInterest>;

    fn route_current_token(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId,
        res: Arc<Resource>,
    ) -> Option<CurrentInterest>;

    // FIXME(regions): only the _declaration_ owner hat should store inbound entities in its specific way.
    // Thus the _interest_ owner hat doesn't have all declarations and the .propagate_declarations(..) fn
    // should be reworked.

    /// Propagates current subscribers to the interested subregion.
    ///
    /// This method is only called on the owner south hat.
    fn send_current_subscribers(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    );

    /// Propagates current queryables to the interested subregion.
    ///
    /// This method is only called on the owner south hat.
    fn send_current_queryables(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, QueryableInfoType>,
    );

    fn send_current_tokens(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashSet<Arc<Resource>>,
    );

    fn propagate_current_token(
        &self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        interest: CurrentInterest,
    );

    /// Informs the interest source that all declarations have been transmitted.
    ///
    /// This method is only called on south hats.
    fn send_declare_final(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId, // TODO(regions): change to &Interest (?)
        dst: &Remote,
    );

    /// Register remote interests.
    ///
    /// This method is only called on the owner south hat.
    fn register_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
    );

    /// Unregister remote interests.
    ///
    /// This method is only called on the owner south hat.
    fn unregister_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
    ) -> Option<RemoteInterest>;

    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest>;
}

/// Return value of entity unregistration methods.
#[derive(Debug)]
pub(crate) enum UnregisterResult {
    /// Indicates that the unregisration was a no-op (e.g. the entity has duplicates).
    Noop,
    /// Indicates that the entity info changed (e.g. an update to queryable completeness).
    InfoUpdate { res: Arc<Resource> },
    /// Indicates that the last entity on the given [`Resource`] was unregistered.
    LastUnregistered { res: Arc<Resource> },
}

pub(crate) trait HatPubSubTrait {
    /// Register a subscriber entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn register_subscriber(
        &mut self,
        ctx: DispatcherContext,
        id: SubscriberId,
        res: Arc<Resource>,
        nid: NodeId,
        info: &SubscriberInfo,
    );

    /// Unregister a subscriber entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn unregister_subscriber(
        &mut self,
        ctx: DispatcherContext,
        id: SubscriberId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>>;

    fn unregister_face_subscriber(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>>;

    /// Propagate a subscriber entity.
    ///
    /// The callee hat will only push the subscriber if is the north hat.
    fn propagate_subscriber(
        &mut self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        other_info: Option<SubscriberInfo>,
    );

    /// Unpropagate a subscriber entity.
    fn unpropagate_subscriber(&mut self, ctx: DispatcherContext, res: Arc<Resource>);

    /// Unpropagate the last remaining subscriber entity which the callee hat doesn't own.
    ///
    /// This implies that the callee hat owns the last remaining subscriber and that the penultimate
    /// subscriber was unregistered.
    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_subscriber(
        &mut self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
    ) {
        self.unpropagate_subscriber(ctx, res);
    }

    fn remote_subscribers_of(&self, res: &Resource) -> Option<SubscriberInfo>;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_subscribers(&self, tables: &TablesData) -> HashMap<Arc<Resource>, SubscriberInfo> {
        self.remote_subscribers_matching(tables, None)
    }

    fn remote_subscribers_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, SubscriberInfo>;

    fn sourced_subscribers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn sourced_publishers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_data_route(
        &self,
        tables: &TablesData,
        face: &FaceState,
        expr: &RoutingExpr,
        node_id: NodeId,
    ) -> Arc<Route>;
}

pub(crate) trait HatQueriesTrait {
    /// Register a queryable entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn register_queryable(
        &mut self,
        ctx: DispatcherContext,
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
        ctx: DispatcherContext,
        id: QueryableId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> UnregisterResult;

    fn unregister_face_queryables(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>>;

    /// Propagate a queryable entity.
    ///
    /// The callee hat will only push the subscriber if is the north hat.
    fn propagate_queryable(
        &mut self,
        ctx: DispatcherContext,
        res: Arc<Resource>,
        other_info: Option<QueryableInfoType>,
    );

    /// Unpropagate a queryable entity.
    fn unpropagate_queryable(&mut self, ctx: DispatcherContext, res: Arc<Resource>);

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_queryable(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
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

    // TODO(region): replace return type with map
    fn sourced_queryables(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn sourced_queriers(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;

    fn compute_query_route(
        &self,
        tables: &TablesData,
        face: &FaceState,
        expr: &RoutingExpr,
        source: NodeId,
    ) -> Arc<QueryTargetQablSet>;
}

pub(crate) trait HatTokenTrait {
    /// Register a token entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn register_token(
        &mut self,
        ctx: DispatcherContext,
        id: TokenId,
        res: Arc<Resource>,
        nid: NodeId,
    );

    /// Unregister a token entity.
    ///
    /// The callee hat assumes that it owns the source face.
    fn unregister_token(
        &mut self,
        ctx: DispatcherContext,
        id: TokenId,
        res: Option<Arc<Resource>>,
        nid: NodeId,
    ) -> Option<Arc<Resource>>;

    fn unregister_face_tokens(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>>;

    /// Propagate a token entity.
    ///
    /// The callee hat will only push the subscriber if is the north hat.
    fn propagate_token(&mut self, ctx: DispatcherContext, res: Arc<Resource>, other_tokens: bool);

    /// Unpropagate a queryable entity.
    fn unpropagate_token(&mut self, ctx: DispatcherContext, res: Arc<Resource>);

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_token(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
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

    fn sourced_tokens(&self, tables: &TablesData) -> Vec<(Arc<Resource>, Sources)>;
}
