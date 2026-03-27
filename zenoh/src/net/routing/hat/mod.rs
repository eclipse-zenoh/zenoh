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
    fmt::{Debug, Display},
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
use zenoh_sync::get_mut_unchecked;
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

    pub(crate) fn is_empty(&self) -> bool {
        self.routers.is_empty() && self.peers.is_empty() && self.clients.is_empty()
    }

    pub(crate) fn with_mode(
        mut self,
        zids: impl IntoIterator<Item = ZenohIdProto>,
        mode: WhatAmI,
    ) -> Self {
        match mode {
            WhatAmI::Router => self.routers.extend(zids),
            WhatAmI::Peer => self.peers.extend(zids),
            WhatAmI::Client => self.clients.extend(zids),
        }
        self
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

impl Display for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Remote {
    fn as_any(&self) -> &dyn Any {
        self.0.as_any()
    }

    pub(crate) fn downcast_ref_to_face(&self) -> Option<&Arc<FaceState>> {
        self.as_any().downcast_ref()
    }
}

pub(crate) trait RemoteTrait: Any + Send + Sync + Debug + Display {
    fn clone_box(&self) -> Box<dyn RemoteTrait>;
    fn as_any(&self) -> &dyn Any;
}

impl<T> RemoteTrait for T
where
    T: Any + Send + Sync + Debug + Display + Clone + 'static,
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
        other_hats: RegionMap<&mut dyn HatTrait>,
    ) -> ZResult<()>;

    fn map_routing_context(
        &self,
        tables: &TablesData, // TODO(fuzzpixelz): can this be removed?
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId;

    fn info(&self) -> String;

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

    #[allow(dead_code)]
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn mode(&self) -> WhatAmI;

    fn region(&self) -> Region;

    /// Converts a message source [`NodeId`] and its source [`FaceState`] into the source [`ZenohIdProto`].
    fn remote_node_id_to_zid(&self, src: &FaceState, node_id: NodeId) -> Option<ZenohIdProto>;

    /// Returns the list of `zid`'s gateways. Assumes the hat is south-bound.
    fn gateways_of(&self, tables: &TablesData, zid: &ZenohIdProto) -> Option<Vec<ZenohIdProto>>;

    /// Returns the list of this region's gateways. Assumes the hat is south-bound.
    fn gateways(&self, tables: &TablesData) -> Option<Vec<ZenohIdProto>>;

    /// Returns `true` if `face` belongs to this [`Hat`].
    fn owns(&self, face: &FaceState) -> bool {
        if self.region() == face.region && face.remote_bound.is_north() {
            debug_assert_eq!(self.mode(), face.whatami);
            debug_assert_eq!(face.is_local, self.region() == Region::Local);
            debug_assert_implies!(face.is_local, face.whatami.is_client());
        }

        self.region() == face.region
    }

    /// Disables this hat's data routes for the given resource and all its matches.
    fn disable_data_routes(&mut self, res: &mut Arc<Resource>) {
        if res.ctx.is_some() {
            get_mut_unchecked(res).context_mut().hats[self.region()].disable_data_routes();
            get_mut_unchecked(res).context_mut().disable_data_routes();

            for match_ in &res.context().matches {
                let mut match_ = match_.upgrade().unwrap();
                if !Arc::ptr_eq(&match_, res) {
                    get_mut_unchecked(&mut match_).context_mut().hats[self.region()]
                        .disable_data_routes();
                    get_mut_unchecked(&mut match_)
                        .context_mut()
                        .disable_data_routes();
                }
            }
        }
    }

    /// Disables this hat's query routes for the given resource and all its matches.
    fn disable_query_routes(&mut self, res: &mut Arc<Resource>) {
        if res.ctx.is_some() {
            get_mut_unchecked(res).context_mut().hats[self.region()].disable_query_routes();

            for match_ in &res.context().matches {
                let mut match_ = match_.upgrade().unwrap();
                if !Arc::ptr_eq(&match_, res) {
                    get_mut_unchecked(&mut match_).context_mut().hats[self.region()]
                        .disable_query_routes();
                }
            }
        }
    }

    /// Disables this hat's data and query routes **for all resources**.
    fn disable_all_routes(&mut self, tables: &mut TablesData) {
        let routes_version = &mut tables.hats[self.region()].routes_version;
        *routes_version = routes_version.saturating_add(1);

        tables.disable_all_routes();
    }
}

/// Return value of current entity routing methods.
#[derive(Debug)]
pub(crate) enum RouteCurrentDeclareResult {
    /// Indicates that the operation failed or has no effect on the dispatcher (e.g. the interest id is unknown).
    Noop,
    /// The breadcrumb corresponding to a pending current interest for this entity.
    Breadcrumb { interest: CurrentInterest },
    /// Indicates that the entity should be propagated to matching downstream interests—there is no breadcrumb.
    NoBreadcrumb,
}

/// Return value of [`HatInterestTrait::route_interest`].
#[derive(Debug)]
pub(crate) enum RouteInterestResult {
    /// Indicates that the caller should send [`zenoh_protocol::network::declare::DeclareFinal`] to
    /// the interest source.
    ResolvedCurrentInterest,
    /// Indicates that the operation failed or has no effect on the dispatcher (e.g. interest is not
    /// current and/or was not propagated upstream).
    Noop,
}

/// Hat interest protocol interface.
///
/// Below is the typical code-path for each message type.
///
/// # Interest w/ `mode=Current` or `mode=CurrentFuture`
///   1. [`HatInterestTrait::route_interest`] on the north hat.
///   2. [`HatInterestTrait::register_interest`] on the owner south hat, iff it owns the src face.
///   3. [`HatInterestTrait::send_current_tokens`] on the owner south hat, iff it owns the src face.
///   4. [`HatInterestTrait::send_current_queryables`] on the owner south hat, iff it owns the src face.
///   4. [`HatInterestTrait::send_current_tokens`] on the owner south hat, iff it owns the src face.
///   5. [`HatInterestTrait::send_declare_final`] on the owner south hat,
///      iff it owns the src face and there is no gateway in the north region.
///
/// # `Interest` w/ `mode=Final`
///   1. [`HatInterestTrait::route_interest_final`] on the north hat.
///   2. [`HatInterestTrait::unregister_interest`] on the owner south hat, iff it owns the src face.
///
/// # `DeclareFinal` (final)
///   1. [`HatInterestTrait::route_declare_final`] on the north hat.
///   2. [`HatInterestTrait::send_declare_final`] on the owner south hat, iff the msg is intended for it.
///
/// # `DeclareToken` (w/ interest id)
///   1. [`HatInterestTrait::route_current_token`] on the north hat.
///   2. [`HatInterestTrait::propagate_current_token`] on the owner south hat, iff the msg is intended for it.
pub(crate) trait HatInterestTrait {
    fn route_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        src: &Remote,
    ) -> RouteInterestResult;

    fn route_interest_final(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        remote_interest: &RemoteInterest,
    );

    fn route_declare_final(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId,
    ) -> RouteCurrentDeclareResult;

    fn route_current_token(
        &mut self,
        ctx: DispatcherContext,
        interest_id: InterestId,
        res: Arc<Resource>,
    ) -> RouteCurrentDeclareResult;

    fn send_current_subscribers(
        &self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
        other_matches: HashMap<Arc<Resource>, SubscriberInfo>,
    );

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

    fn send_declare_final(&mut self, ctx: DispatcherContext, interest_id: InterestId, dst: &Remote);

    fn register_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
        res: Option<Arc<Resource>>,
    );

    fn unregister_interest(
        &mut self,
        ctx: DispatcherContext,
        msg: &Interest,
    ) -> Option<RemoteInterest>;

    fn remote_interests(&self, tables: &TablesData) -> HashSet<RemoteInterest>;
}

/// Return value of entity unregistration methods.
///
/// This type is currently only used for queryables, i.e. the only entity with non-trivial info.
#[derive(Debug)]
pub(crate) enum UnregisterEntityResult {
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

    fn unregister_face_subscribers(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>>;

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

    fn remote_subscribers_of(&self, tables: &TablesData, res: &Resource) -> Option<SubscriberInfo>;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_subscribers(&self, tables: &TablesData) -> HashMap<Arc<Resource>, SubscriberInfo> {
        self.remote_subscribers_matching(tables, None)
    }

    fn remote_subscribers_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, SubscriberInfo>;

    fn sourced_subscribers(&self, tables: &TablesData) -> HashMap<Arc<Resource>, Sources>;

    fn sourced_publishers(&self, tables: &TablesData) -> HashMap<Arc<Resource>, Sources>;

    /// Computes routing destination for `Push` messages.
    ///
    /// Note that it's the responsibility of the dispatcher to ensure that messages are not routed
    /// to their source: this method is agnostic vis-à-vis the source face.
    ///
    /// Note that the computed route contains the wire expression used for a given direction, thus
    /// routes are invalidated when keyexpr mappings change.
    ///
    /// # Dependencies
    /// The result of this method depends on two classes of variables: message properties and hat
    /// state; these variables are documented in each hat's impl. Changes in stateful properties
    /// (i.e. the hats' state) invalidate the computed routes. While message properties are used to
    /// store distinct sets of routes for each possible message property.
    fn compute_data_route(
        &self,
        tables: &TablesData,
        src_region: &Region,
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
    ) -> UnregisterEntityResult;

    fn unregister_face_queryables(&mut self, ctx: DispatcherContext) -> HashSet<Arc<Resource>>;

    /// Propagate a queryable entity.
    ///
    /// The callee hat will only push the queryable if is the north hat.
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

    fn remote_queryables_of(
        &self,
        tables: &TablesData,
        res: &Resource,
    ) -> Option<QueryableInfoType>;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_queryables(&self, tables: &TablesData) -> HashMap<Arc<Resource>, QueryableInfoType> {
        self.remote_queryables_matching(tables, None)
    }

    fn remote_queryables_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashMap<Arc<Resource>, QueryableInfoType>;

    fn sourced_queryables(&self, tables: &TablesData) -> HashMap<Arc<Resource>, Sources>;

    fn sourced_queriers(&self, tables: &TablesData) -> HashMap<Arc<Resource>, Sources>;

    /// Computes routing destination for `Request` messages.
    ///
    /// Note that it's the responsibility of the dispatcher to ensure that messages are not routed
    /// to their source: this method is agnostic vis-à-vis the source face.
    ///
    /// Note that the computed route contains the wire expression used for a given direction, thus
    /// routes are invalidated when keyexpr mappings change.
    ///
    /// # Dependencies
    /// The result of this method depends on two classes of variables: message properties and hat
    /// state; these variables are documented in each hat's impl. Changes in stateful properties
    /// (i.e. the hats' state) invalidate the computed routes. While message properties are used to
    /// store distinct sets of routes for each possible message property.
    fn compute_query_route(
        &self,
        tables: &TablesData,
        src_region: &Region,
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
    fn propagate_token(&mut self, ctx: DispatcherContext, res: Arc<Resource>);

    /// Unpropagate a token entity.
    fn unpropagate_token(&mut self, ctx: DispatcherContext, res: Arc<Resource>);

    #[tracing::instrument(level = "trace", skip_all)]
    fn unpropagate_last_non_owned_token(&mut self, ctx: DispatcherContext, res: Arc<Resource>) {
        self.unpropagate_token(ctx, res);
    }

    fn remote_tokens_of(&self, tables: &TablesData, res: &Resource) -> bool;

    #[tracing::instrument(level = "trace", skip_all)]
    fn remote_tokens(&self, tables: &TablesData) -> HashSet<Arc<Resource>> {
        self.remote_tokens_matching(tables, None)
    }

    fn remote_tokens_matching(
        &self,
        tables: &TablesData,
        res: Option<&Resource>,
    ) -> HashSet<Arc<Resource>>;

    fn sourced_tokens(&self, tables: &TablesData) -> HashMap<Arc<Resource>, Sources>;
}
