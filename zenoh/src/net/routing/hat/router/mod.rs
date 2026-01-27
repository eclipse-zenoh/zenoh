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
    iter, mem,
    sync::Arc,
};

use itertools::Itertools;
use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI};
use zenoh_protocol::{
    common::ZExtBody,
    core::{Region, ZenohIdProto},
    network::{
        declare::queryable::ext::QueryableInfoType, interest::InterestId, oam::id::OAM_LINKSTATE,
        Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_task::{TaskController, TerminatableTask};
use zenoh_transport::unicast::TransportUnicast;

use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, TablesData, TablesLock},
    },
    HatBaseTrait, HatTrait, SendDeclare,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::{
        linkstate::{link_weights_from_config, LinkStateList},
        network::{LinkId, Network},
        ROUTERS_NET_NAME,
    },
    routing::{
        dispatcher::{
            self,
            face::InterestState,
            interests::{PendingCurrentInterest, RemoteInterest},
            queries::merge_qabl_infos,
            region::RegionMap,
        },
        hat::{BaseContext, Remote, TREES_COMPUTATION_DELAY_MS},
        router::DEFAULT_NODE_ID,
    },
    runtime::Runtime,
};

mod interests;
mod pubsub;
mod queries;
mod token;

use crate::net::{common::AutoConnect, protocol::network::SuccessorEntry};

struct TreesComputationWorker {
    _task: TerminatableTask,
    tx: flume::Sender<Arc<TablesLock>>,
}

impl TreesComputationWorker {
    fn new(region: Region) -> Self {
        let (tx, rx) = flume::bounded::<Arc<TablesLock>>(1);
        let task = TerminatableTask::spawn_abortable(zenoh_runtime::ZRuntime::Net, async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(
                    *TREES_COMPUTATION_DELAY_MS,
                ))
                .await;
                if let Ok(tables_ref) = rx.recv_async().await {
                    let mut wtables = zwrite!(tables_ref.tables);
                    let tables = &mut *wtables;
                    let hat = tables.hats[region]
                        .as_any_mut()
                        .downcast_mut::<Hat>()
                        .unwrap();

                    tracing::trace!("Compute trees");
                    let new_children = hat.net_mut().compute_trees();

                    tracing::trace!("Compute routes");
                    hat.pubsub_tree_change(&mut tables.data, &new_children);
                    hat.queries_tree_change(&mut tables.data, &new_children);
                    hat.token_tree_change(&mut tables.data, &new_children);
                    tables.data.disable_all_routes();
                    drop(wtables);
                }
            }
        });
        Self { _task: task, tx }
    }
}

pub(crate) struct Hat {
    region: Region,
    router_subs: HashSet<Arc<Resource>>,
    router_tokens: HashSet<Arc<Resource>>,
    router_qabls: HashSet<Arc<Resource>>,
    #[allow(dead_code)] // FIXME(regions)
    next_interest_id: InterestId,
    #[allow(dead_code)] // FIXME(regions)
    router_local_interests: HashMap<InterestId, InterestState>,
    router_pending_current_interests: HashMap<InterestId, PendingCurrentInterest>,
    /// Interests declared by nodes in this router's subregions.
    ///
    /// Interest mode can only be one of:
    /// - [`zenoh_protocol::network::interest::InterestMode::Future`]
    /// - [`zenoh_protocol::network::interest::InterestMode::CurrentFuture`]
    #[allow(dead_code)] // FIXME(regions)
    router_remote_interests: HashMap<(ZenohIdProto, InterestId), RemoteInterest>,
    task_controller: TaskController,
    routers_net: Option<Network>, // TODO(regions): remove Option?
    routers_trees_worker: TreesComputationWorker,
}

impl Debug for Hat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.region)
    }
}

impl Hat {
    pub(crate) fn new(region: Region) -> Self {
        // FIXME(regions): peer failover brokering is scrapped
        Self {
            region,
            router_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            router_tokens: HashSet::new(),
            routers_net: None,
            routers_trees_worker: TreesComputationWorker::new(region),
            router_local_interests: HashMap::new(),
            router_pending_current_interests: HashMap::new(),
            router_remote_interests: HashMap::new(),
            next_interest_id: 1,
            task_controller: TaskController::default(),
        }
    }

    pub(crate) fn net(&self) -> &Network {
        self.routers_net.as_ref().unwrap()
    }

    pub(crate) fn net_mut(&mut self) -> &mut Network {
        self.routers_net.as_mut().unwrap()
    }

    pub(self) fn res_hat<'r>(&self, res: &'r Resource) -> &'r HatContext {
        res.context().hats[self.region].ctx.downcast_ref().unwrap()
    }

    pub(self) fn res_hat_mut<'r>(&self, res: &'r mut Arc<Resource>) -> &'r mut HatContext {
        get_mut_unchecked(res).context_mut().hats[self.region]
            .ctx
            .downcast_mut()
            .unwrap()
    }

    #[allow(dead_code)] // FIXME(regions)
    pub(self) fn hat_remote<'r>(&self, remote: &'r Remote) -> &'r HatRemote {
        remote.as_any().downcast_ref().unwrap()
    }

    pub(self) fn face_hat<'f>(&self, face_state: &'f FaceState) -> &'f HatFace {
        face_state.hats[self.region].downcast_ref().unwrap()
    }

    pub(self) fn face_hat_mut<'f>(&self, face_state: &'f mut Arc<FaceState>) -> &'f mut HatFace {
        get_mut_unchecked(face_state).hats[self.region]
            .downcast_mut()
            .unwrap()
    }

    pub(crate) fn face<'t>(
        &self,
        tables: &'t TablesData,
        zid: &ZenohIdProto,
    ) -> Option<&'t Arc<FaceState>> {
        tables.faces.values().find(|face| face.zid == *zid)
    }

    #[allow(dead_code)] // FIXME(regions)
    /// Identifies the gateway of this hat's region (if any).
    pub(crate) fn region_gateway(&self) -> Option<NodeId> {
        // TODO(regions2): elect the primary gateway
        let net = self.net();
        let mut gwys = net.graph.node_indices().filter(|idx| {
            tracing::trace!(
                zid = %net.graph[*idx].zid.short(),
                is_gwy = net.graph[*idx].is_gateway,
                "Gateway candidate node"
            );
            net.graph[*idx].is_gateway
        });

        let gwy = gwys.next()?;

        if gwys.next().is_some() {
            tracing::error!(
                bound = ?self.region,
                total = gwys.count() + 2,
                selected = ?net.graph[gwy].zid,
                "Multiple gateways found in router subregion. \
                Only one gateway per subregion is supported. \
                Selecting arbitrary gateway"
            );
        }

        Some(gwy.index() as NodeId)
    }

    /// Find the next hop in the point-to-point route ending at `dst_nid`.
    ///
    /// This works by exploiting the following property of linkstate routing:
    /// [`crate::net::protocol::network::Tree::directions`] contains routes
    /// up the spanning tree root.
    fn point_to_point_hop(&self, tables: &TablesData, dst_nid: NodeId) -> Option<Arc<FaceState>> {
        let net = self.net();
        let tree = net.trees.get(dst_nid as usize)?;
        let next_hop_node_id = tree.directions.get(dst_nid as usize)?.as_ref()?;
        let next_hop = net
            .graph
            .node_weight(*next_hop_node_id)
            .map(|node| &node.zid)?;
        self.face(tables, next_hop).cloned()
    }

    /// Sends a network message to the router identified by `dst_node_id`.
    #[allow(dead_code)] // FIXME(regions)
    pub(crate) fn send_point_to_point(
        &self,
        ctx: BaseContext,
        dst_node_id: NodeId,
        mut send_message: impl FnMut(&Arc<FaceState>),
    ) {
        let Some(next_hop) = self.point_to_point_hop(ctx.tables, dst_node_id) else {
            tracing::error!("Unable to find next-hop face in point-to-point route");
            return;
        };
        send_message(&next_hop);
    }

    /// Sends a declare message to the router identified by `dst_node_id`.
    ///
    /// See also: [`Self::point_to_point_hop`].
    #[allow(dead_code)] // FIXME(regions)
    pub(crate) fn send_declare_point_to_point(
        &self,
        ctx: BaseContext,
        dst_node_id: NodeId,
        mut send_message: impl FnMut(&mut SendDeclare, &Arc<FaceState>),
    ) {
        let Some(next_hop) = self.point_to_point_hop(ctx.tables, dst_node_id) else {
            tracing::error!("Unable to find next-hop face in point-to-point route");
            return;
        };
        send_message(ctx.send_declare, &next_hop)
    }

    fn schedule_compute_trees(&self, tables_ref: Arc<TablesLock>) {
        tracing::trace!("Schedule trees computation");
        if let Err(err) = self.routers_trees_worker.tx.try_send(tables_ref) {
            tracing::trace!(%err, "Failed to schedule routing tree computation");
        }
    }

    fn get_router(&self, face: &FaceState, nodeid: NodeId) -> Option<ZenohIdProto> {
        match self
            .routers_net
            .as_ref()
            .unwrap()
            .get_link(self.face_hat(face).link_id)
        {
            Some(link) => match link.get_zid(&(nodeid as u64)) {
                Some(router) => Some(*router),
                None => {
                    tracing::error!(
                        "Received router declaration with unknown routing context id {}",
                        nodeid
                    );
                    None
                }
            },
            None => {
                tracing::error!(
                    "Could not find corresponding link in routers network for {}",
                    face
                );
                None
            }
        }
    }
}

impl HatBaseTrait for Hat {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()> {
        let config_guard = runtime.config().lock();
        let config = &config_guard;
        let whatami = config.mode();
        let gossip = unwrap_or_default!(config.scouting().gossip().enabled());
        let gossip_multihop = unwrap_or_default!(config.scouting().gossip().multihop());
        let gossip_target = *unwrap_or_default!(config.scouting().gossip().target().get(whatami));
        if gossip_target.matches(WhatAmI::Client) {
            bail!("\"client\" is not allowed as gossip target")
        }
        let autoconnect = if gossip {
            AutoConnect::gossip(config, whatami, runtime.zid().into())
        } else {
            AutoConnect::disabled()
        };

        let router_link_weights = config
            .routing()
            .router()
            .linkstate()
            .transport_weights()
            .clone();
        drop(config_guard);

        self.routers_net = Some(Network::new(
            ROUTERS_NET_NAME.to_string(),
            tables.zid,
            runtime.clone(),
            true,
            gossip,
            gossip_multihop,
            gossip_target,
            autoconnect,
            link_weights_from_config(router_link_weights, ROUTERS_NET_NAME)?,
            &self.region,
        ));
        Ok(())
    }

    fn new_face(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatFace::new())
    }

    fn new_resource(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatContext::new())
    }

    fn new_remote(&self, face: &Arc<FaceState>, nid: NodeId) -> Option<Remote> {
        self.get_router(face, nid).map(|zid| Remote(Box::new(zid)))
    }

    fn new_local_face(&mut self, _ctx: BaseContext, _tables_ref: &Arc<TablesLock>) -> ZResult<()> {
        bail!("Local sessions should not be bound to router hats");
    }

    #[tracing::instrument(level = "trace", skip_all, fields(src = %ctx.src_face, rgn = %self.region))]
    fn new_transport_unicast_face(
        &mut self,
        ctx: BaseContext,
        transport: &TransportUnicast,
        _other_hats: RegionMap<&dyn HatTrait>,
    ) -> ZResult<()> {
        debug_assert!(self.owns(ctx.src_face));

        let link_id = self.net_mut().add_link(transport.clone());

        self.face_hat_mut(ctx.src_face).link_id = link_id;
        self.schedule_compute_trees(ctx.tables_lock.clone());

        Ok(())
    }

    fn close_face(&mut self, ctx: BaseContext) {
        debug_assert!(self.owns(ctx.src_face));

        self.schedule_compute_trees(ctx.tables_lock.clone());
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn handle_oam(
        &mut self,
        mut ctx: BaseContext,
        oam: &mut Oam,
        zid: &ZenohIdProto,
        whatami: WhatAmI,
        other_hats: RegionMap<&mut dyn HatTrait>,
    ) -> ZResult<()> {
        if oam.id == OAM_LINKSTATE {
            debug_assert_implies!(
                !ctx.src_face.whatami.is_router(),
                ctx.src_face.remote_bound.is_south()
            );

            if let ZExtBody::ZBuf(buf) = mem::take(&mut oam.body) {
                use zenoh_buffers::reader::HasReader;
                use zenoh_codec::RCodec;
                let codec = Zenoh080Routing::new();
                let mut reader = buf.reader();
                let Ok(list): Result<LinkStateList, _> = codec.read(&mut reader) else {
                    bail!("failed to decode link state");
                };

                tracing::trace!(
                    id = %"OAM_LINKSTATE",
                    wai = %whatami.short(),
                    rgn = %self.region(),
                    linkstate = ?list
                );

                let removed_nodes = self
                    .net_mut()
                    .link_states(list.link_states, *zid)
                    .removed_nodes;

                let removed_subscriptions = removed_nodes
                    .iter()
                    .flat_map(|(_, node)| self.unregister_node_subscriptions(node))
                    .collect::<HashSet<_>>();

                let removed_queryables = removed_nodes
                    .iter()
                    .flat_map(|(_, node)| self.unregister_node_queryables(node))
                    .collect::<HashSet<_>>();

                let removed_tokens = removed_nodes
                    .iter()
                    .flat_map(|(_, node)| self.unregister_node_tokens(node))
                    .collect::<HashSet<_>>();

                // FIXME(regions): refactor?

                let region = self.region();

                let mut hats = other_hats
                    .into_iter()
                    .chain(iter::once((self.region, self as &mut dyn HatTrait)))
                    .collect::<RegionMap<_>>();

                for mut res in removed_subscriptions {
                    dispatcher::pubsub::disable_matches_data_routes(ctx.tables, &mut res);

                    let mut remaining = hats
                        .values_mut()
                        .filter(|hat| hat.remote_subscriptions_of(&res).is_some())
                        .collect_vec();

                    if remaining.is_empty() {
                        for hat in hats.values_mut() {
                            hat.unpropagate_subscription(ctx.reborrow(), res.clone());
                        }
                        Resource::clean(&mut res);
                    } else if let [last_owner] = &mut *remaining {
                        last_owner
                            .unpropagate_last_non_owned_subscription(ctx.reborrow(), res.clone())
                    }
                }

                for mut res in removed_queryables {
                    dispatcher::queries::disable_matches_query_routes(ctx.tables, &mut res);

                    let remaining = hats
                        .iter()
                        .filter_map(|(rgn, hat)| {
                            hat.remote_queryables_of(&res).map(|info| (*rgn, info))
                        })
                        .collect_vec();

                    match &*remaining {
                        [] => {
                            for hat in hats.values_mut() {
                                hat.unpropagate_queryable(ctx.reborrow(), res.clone());
                            }
                            Resource::clean(&mut res);
                        }
                        [(last_owner, _)] => hats[last_owner]
                            .unpropagate_last_non_owned_queryable(ctx.reborrow(), res.clone()),
                        _ => {
                            for hat in hats.values_mut() {
                                let other_info = remaining
                                    .iter()
                                    .filter_map(|(region, info)| {
                                        (region != &hat.region()).then_some(*info)
                                    })
                                    .reduce(merge_qabl_infos);

                                hat.propagate_queryable(ctx.reborrow(), res.clone(), other_info);
                            }
                        }
                    }
                }

                for mut res in removed_tokens {
                    let mut remaining = hats
                        .values_mut()
                        .filter(|hat| hat.remote_tokens_of(&res))
                        .collect_vec();

                    if remaining.is_empty() {
                        for hat in hats.values_mut() {
                            hat.unpropagate_token(ctx.reborrow(), res.clone());
                        }
                        Resource::clean(&mut res);
                    } else if let [last_owner] = &mut *remaining {
                        last_owner.unpropagate_last_non_owned_token(ctx.reborrow(), res.clone())
                    }
                }

                hats[region]
                    .as_any()
                    .downcast_ref::<Self>()
                    .unwrap()
                    .schedule_compute_trees(ctx.tables_lock.clone());
            }
        }

        Ok(())
    }

    #[inline]
    fn map_routing_context(&self, _tables: &TablesData, face: &FaceState, nid: NodeId) -> NodeId {
        if self.owns(face) {
            self.net()
                .get_local_context(nid, self.face_hat(face).link_id)
        } else {
            DEFAULT_NODE_ID
        }
    }

    #[inline]
    fn ingress_filter(&self, _tables: &TablesData, _face: &FaceState, _expr: &RoutingExpr) -> bool {
        // FIXME(regions): ensure that there is a south-bound peer that can
        // handle duplicated messages through gossip from peers with multiple
        // connections to the same router gateway.
        true
    }

    #[inline]
    fn egress_filter(
        &self,
        _tables: &TablesData,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        _expr: &RoutingExpr,
    ) -> bool {
        src_face.id != out_face.id
            && out_face.mcast_group.is_none()
            && src_face.mcast_group.is_none()
    }

    fn info(&self, kind: WhatAmI) -> String {
        match kind {
            WhatAmI::Router => self
                .routers_net
                .as_ref()
                .map(|net| net.dot())
                .unwrap_or_else(|| "graph {}".to_string()),
            _ => "graph {}".to_string(),
        }
    }

    fn update_from_config(
        &mut self,
        tables_ref: &Arc<TablesLock>,
        runtime: &Runtime,
    ) -> ZResult<()> {
        let config = runtime.config().lock();
        let router_link_weights = link_weights_from_config(
            config
                .routing()
                .router()
                .linkstate()
                .transport_weights()
                .clone(),
            ROUTERS_NET_NAME,
        )?;
        drop(config);
        if let Some(net) = self.routers_net.as_mut() {
            if net.update_link_weights(router_link_weights) {
                self.schedule_compute_trees(tables_ref.clone());
            }
        }
        Ok(())
    }

    fn links_info(&self) -> HashMap<ZenohIdProto, crate::net::protocol::linkstate::LinkInfo> {
        if let Some(net) = &self.routers_net {
            net.links_info()
        } else {
            HashMap::new()
        }
    }

    fn route_successor(&self, src: ZenohIdProto, dst: ZenohIdProto) -> Option<ZenohIdProto> {
        self.net().route_successor(src, dst)
    }

    fn route_successors(&self) -> Vec<SuccessorEntry> {
        self.net().route_successors()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn whatami(&self) -> WhatAmI {
        WhatAmI::Router
    }

    fn region(&self) -> Region {
        self.region
    }
}

struct HatContext {
    router_subs: HashSet<ZenohIdProto>,
    router_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    router_tokens: HashSet<ZenohIdProto>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            router_tokens: HashSet::new(),
        }
    }
}

struct HatFace {
    link_id: LinkId,
}

impl HatFace {
    fn new() -> Self {
        Self {
            link_id: LinkId::default(),
        }
    }
}

impl HatTrait for Hat {}

#[allow(dead_code)] // FIXME(regions)
type HatRemote = ZenohIdProto;
