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
    network::{declare::queryable::ext::QueryableInfoType, oam::id::OAM_LINKSTATE, Oam},
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TerminatableTask;
use zenoh_transport::unicast::TransportUnicast;

use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, TablesData, TablesLock},
    },
    HatBaseTrait, HatTrait,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::{
        linkstate::{link_weights_from_config, LinkStateList},
        network::{LinkId, Network},
        ROUTERS_NET_NAME,
    },
    routing::{
        dispatcher::{queries::merge_qabl_infos, region::RegionMap},
        gateway::DEFAULT_NODE_ID,
        hat::{DispatcherContext, Remote, TREES_COMPUTATION_DELAY_MS},
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

                    tables.hats[region]
                        .as_any_mut()
                        .downcast_mut::<Hat>()
                        .unwrap()
                        .do_compute_trees(&mut tables.data);
                }
            }
        });
        Self { _task: task, tx }
    }
}

pub(crate) struct Hat {
    region: Region,
    router_subs: HashSet<Arc<Resource>>,
    /// All tokens declared by routers in the network.
    router_tokens: HashSet<Arc<Resource>>,
    router_qabls: HashSet<Arc<Resource>>,
    routers_net: Option<Network>,
    routers_trees_worker: TreesComputationWorker,
    #[cfg(test)]
    disable_async_tree_computation: bool,
}

impl Debug for Hat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.region)
    }
}

impl Hat {
    pub(crate) fn new(region: Region) -> Self {
        Self {
            region,
            router_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            router_tokens: HashSet::new(),
            routers_net: None,
            routers_trees_worker: TreesComputationWorker::new(region),
            #[cfg(test)]
            disable_async_tree_computation: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_disable_async_tree_computation(&mut self, value: bool) {
        self.disable_async_tree_computation = value;
    }

    pub(crate) fn net(&self) -> &Network {
        self.routers_net
            .as_ref()
            .expect("router Network should be initialized")
    }

    pub(crate) fn net_mut(&mut self) -> &mut Network {
        self.routers_net
            .as_mut()
            .expect("router Network should be initialized")
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

    fn compute_trees_async(&self, tables_ref: Arc<TablesLock>) {
        tracing::trace!("Schedule trees computation");
        if let Err(err) = self.routers_trees_worker.tx.try_send(tables_ref) {
            tracing::trace!(%err, "Failed to schedule routing tree computation");
        }
    }

    fn compute_trees(&mut self, ctx: DispatcherContext) {
        #[cfg(test)]
        {
            if self.disable_async_tree_computation {
                self.do_compute_trees(ctx.tables);
            } else {
                self.compute_trees_async(ctx.tables_lock.clone());
            }
        }

        #[cfg(not(test))]
        self.compute_trees_async(ctx.tables_lock.clone());
    }

    fn do_compute_trees(&mut self, tables: &mut TablesData) {
        tracing::trace!("Compute trees");
        let new_children = self.net_mut().compute_trees();
        tracing::trace!("Compute routes");
        self.pubsub_tree_change(tables, &new_children);
        self.queries_tree_change(tables, &new_children);
        self.token_tree_change(tables, &new_children);
        self.disable_all_routes(tables);
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
            self.region().bound(),
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

    fn new_local_face(
        &mut self,
        _ctx: DispatcherContext,
        _tables_ref: &Arc<TablesLock>,
    ) -> ZResult<()> {
        bail!("Local sessions should not be bound to router hats");
    }

    #[tracing::instrument(level = "debug", skip(ctx, transport, _other_hats), fields(src = %ctx.src_face), ret)]
    fn new_transport_unicast_face(
        &mut self,
        ctx: DispatcherContext,
        transport: &TransportUnicast,
        _other_hats: RegionMap<&dyn HatTrait>,
    ) -> ZResult<()> {
        debug_assert!(self.owns(ctx.src_face));

        let link_id = self.net_mut().add_link(transport.clone());
        self.face_hat_mut(ctx.src_face).link_id = link_id;

        self.compute_trees(ctx);

        Ok(())
    }

    fn close_face(&mut self, ctx: DispatcherContext) {
        debug_assert!(self.owns(ctx.src_face));

        self.compute_trees(ctx);
    }

    #[tracing::instrument(level = "debug", skip(ctx, other_hats), ret)]
    fn handle_oam(
        &mut self,
        mut ctx: DispatcherContext,
        oam: &mut Oam,
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

                tracing::trace!(linkstate = ?list);

                let removed_nodes = self
                    .net_mut()
                    .link_states(list.link_states, ctx.src_face.zid)
                    .removed_nodes;

                let removed_subscribers = removed_nodes
                    .iter()
                    .flat_map(|(_, node)| self.unregister_node_subscribers(node))
                    .collect::<HashSet<_>>();

                let removed_queryables = removed_nodes
                    .iter()
                    .flat_map(|(_, node)| self.unregister_node_queryables(node))
                    .collect::<HashSet<_>>();

                let removed_tokens = removed_nodes
                    .iter()
                    .flat_map(|(_, node)| self.unregister_node_tokens(node))
                    .collect::<HashSet<_>>();

                let region = self.region();

                let mut hats = other_hats
                    .into_iter()
                    .chain(iter::once((self.region, self as &mut dyn HatTrait)))
                    .collect::<RegionMap<_>>();

                for mut res in removed_subscribers {
                    hats[region].disable_data_routes(&mut res);

                    let mut remaining = hats
                        .values_mut()
                        .filter(|hat| hat.remote_subscribers_of(ctx.tables, &res).is_some())
                        .collect_vec();

                    if remaining.is_empty() {
                        for hat in hats.values_mut() {
                            hat.unpropagate_subscriber(ctx.reborrow(), res.clone());
                        }
                        Resource::clean(&mut res);
                    } else if let [last_owner] = &mut *remaining {
                        last_owner
                            .unpropagate_last_non_owned_subscriber(ctx.reborrow(), res.clone())
                    }
                }

                for mut res in removed_queryables {
                    hats[region].disable_query_routes(&mut res);

                    let remaining = hats
                        .iter()
                        .filter_map(|(rgn, hat)| {
                            hat.remote_queryables_of(ctx.tables, &res)
                                .map(|info| (rgn, info))
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
                        .filter(|hat| hat.remote_tokens_of(ctx.tables, &res))
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
                    .as_any_mut()
                    .downcast_mut::<Self>()
                    .unwrap()
                    .compute_trees(ctx);
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

    fn info(&self) -> String {
        self.net().dot()
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
                self.compute_trees_async(tables_ref.clone());
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

    fn mode(&self) -> WhatAmI {
        WhatAmI::Router
    }

    fn region(&self) -> Region {
        self.region
    }

    fn remote_node_id_to_zid(&self, src: &FaceState, node_id: NodeId) -> Option<ZenohIdProto> {
        self.get_router(src, node_id)
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn gateways_of(&self, _tables: &TablesData, zid: &ZenohIdProto) -> Option<Vec<ZenohIdProto>> {
        debug_assert!(self.region().bound().is_south());

        let node = self.net().graph.node_weights().find(|n| &n.zid == zid)?;

        let gwys = self
            .net()
            .graph
            .node_weights()
            .filter_map(|n| (n.is_gateway && node.links.contains_key(&n.zid)).then_some(n.zid))
            .collect_vec();

        Some(gwys)
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn gateways(&self, _tables: &TablesData) -> Option<Vec<ZenohIdProto>> {
        debug_assert!(self.region().bound().is_south());

        let gwys = self
            .net()
            .graph
            .node_weights()
            .filter_map(|n| n.is_gateway.then_some(n.zid))
            .collect();

        Some(gwys)
    }
}

struct HatContext {
    router_subs: HashSet<ZenohIdProto>,
    router_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    /// Routers that have declared a token on the given resource.
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
