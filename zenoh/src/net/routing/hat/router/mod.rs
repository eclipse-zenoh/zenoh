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
use std::{
    any::Any,
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::Hasher,
    sync::{atomic::AtomicU32, Arc},
    time::Duration,
};

use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI, WhatAmIMatcher, ZenohId};
use zenoh_protocol::{
    common::ZExtBody,
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId},
        interest::InterestId,
        oam::id::OAM_LINKSTATE,
        Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TerminatableTask;
use zenoh_transport::unicast::TransportUnicast;

use self::{
    network::{shared_nodes, Network},
    pubsub::{
        pubsub_linkstate_change, pubsub_new_face, pubsub_remove_node, undeclare_client_subscription,
    },
    queries::{
        queries_linkstate_change, queries_new_face, queries_remove_node, undeclare_client_queryable,
    },
};
use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, Tables, TablesLock},
    },
    HatBaseTrait, HatTrait,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::linkstate::LinkStateList,
    routing::{
        dispatcher::face::Face,
        hat::TREES_COMPUTATION_DELAY_MS,
        router::{compute_data_routes, compute_query_routes, RoutesIndexes},
    },
    runtime::Runtime,
};

mod network;
mod pubsub;
mod queries;

macro_rules! hat {
    ($t:expr) => {
        $t.hat.downcast_ref::<HatTables>().unwrap()
    };
}
use hat;

macro_rules! hat_mut {
    ($t:expr) => {
        $t.hat.downcast_mut::<HatTables>().unwrap()
    };
}
use hat_mut;

macro_rules! res_hat {
    ($r:expr) => {
        $r.context().hat.downcast_ref::<HatContext>().unwrap()
    };
}
use res_hat;

macro_rules! res_hat_mut {
    ($r:expr) => {
        get_mut_unchecked($r)
            .context_mut()
            .hat
            .downcast_mut::<HatContext>()
            .unwrap()
    };
}
use res_hat_mut;

macro_rules! face_hat {
    ($f:expr) => {
        $f.hat.downcast_ref::<HatFace>().unwrap()
    };
}
use face_hat;

macro_rules! face_hat_mut {
    ($f:expr) => {
        get_mut_unchecked($f).hat.downcast_mut::<HatFace>().unwrap()
    };
}
use face_hat_mut;

struct HatTables {
    router_subs: HashSet<Arc<Resource>>,
    peer_subs: HashSet<Arc<Resource>>,
    router_qabls: HashSet<Arc<Resource>>,
    peer_qabls: HashSet<Arc<Resource>>,
    routers_net: Option<Network>,
    peers_net: Option<Network>,
    shared_nodes: Vec<ZenohId>,
    routers_trees_task: Option<TerminatableTask>,
    peers_trees_task: Option<TerminatableTask>,
    router_peers_failover_brokering: bool,
}

impl Drop for HatTables {
    fn drop(&mut self) {
        if self.peers_trees_task.is_some() {
            let task = self.peers_trees_task.take().unwrap();
            task.terminate(Duration::from_secs(10));
        }
        if self.routers_trees_task.is_some() {
            let task = self.routers_trees_task.take().unwrap();
            task.terminate(Duration::from_secs(10));
        }
    }
}

impl HatTables {
    fn new(router_peers_failover_brokering: bool) -> Self {
        Self {
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            peer_qabls: HashSet::new(),
            routers_net: None,
            peers_net: None,
            shared_nodes: vec![],
            routers_trees_task: None,
            peers_trees_task: None,
            router_peers_failover_brokering,
        }
    }

    #[inline]
    fn get_net(&self, net_type: WhatAmI) -> Option<&Network> {
        match net_type {
            WhatAmI::Router => self.routers_net.as_ref(),
            WhatAmI::Peer => self.peers_net.as_ref(),
            _ => None,
        }
    }

    #[inline]
    fn full_net(&self, net_type: WhatAmI) -> bool {
        match net_type {
            WhatAmI::Router => self
                .routers_net
                .as_ref()
                .map(|net| net.full_linkstate)
                .unwrap_or(false),
            WhatAmI::Peer => self
                .peers_net
                .as_ref()
                .map(|net| net.full_linkstate)
                .unwrap_or(false),
            _ => false,
        }
    }

    #[inline]
    fn get_router_links(&self, peer: ZenohId) -> impl Iterator<Item = &ZenohId> + '_ {
        self.peers_net
            .as_ref()
            .unwrap()
            .get_links(peer)
            .iter()
            .filter(move |nid| {
                if let Some(node) = self.routers_net.as_ref().unwrap().get_node(nid) {
                    node.whatami.unwrap_or(WhatAmI::Router) == WhatAmI::Router
                } else {
                    false
                }
            })
    }

    #[inline]
    fn elect_router<'a>(
        &'a self,
        self_zid: &'a ZenohId,
        key_expr: &str,
        mut routers: impl Iterator<Item = &'a ZenohId>,
    ) -> &'a ZenohId {
        match routers.next() {
            None => self_zid,
            Some(router) => {
                let hash = |r: &ZenohId| {
                    let mut hasher = DefaultHasher::new();
                    for b in key_expr.as_bytes() {
                        hasher.write_u8(*b);
                    }
                    for b in &r.to_le_bytes()[..r.size()] {
                        hasher.write_u8(*b);
                    }
                    hasher.finish()
                };
                let mut res = router;
                let mut h = None;
                for router2 in routers {
                    let h2 = hash(router2);
                    if h2 > *h.get_or_insert_with(|| hash(res)) {
                        res = router2;
                        h = Some(h2);
                    }
                }
                res
            }
        }
    }

    #[inline]
    fn failover_brokering_to(source_links: &[ZenohId], dest: ZenohId) -> bool {
        // if source_links is empty then gossip is probably disabled in source peer
        !source_links.is_empty() && !source_links.contains(&dest)
    }

    #[inline]
    fn failover_brokering(&self, peer1: ZenohId, peer2: ZenohId) -> bool {
        self.router_peers_failover_brokering
            && self
                .peers_net
                .as_ref()
                .map(|net| {
                    let links = net.get_links(peer1);
                    HatTables::failover_brokering_to(links, peer2)
                })
                .unwrap_or(false)
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>, net_type: WhatAmI) {
        if (net_type == WhatAmI::Router && self.routers_trees_task.is_none())
            || (net_type == WhatAmI::Peer && self.peers_trees_task.is_none())
        {
            let task = TerminatableTask::spawn(
                zenoh_runtime::ZRuntime::Net,
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        *TREES_COMPUTATION_DELAY_MS,
                    ))
                    .await;
                    let mut tables = zwrite!(tables_ref.tables);

                    tracing::trace!("Compute trees");
                    let new_childs = match net_type {
                        WhatAmI::Router => hat_mut!(tables)
                            .routers_net
                            .as_mut()
                            .unwrap()
                            .compute_trees(),
                        _ => hat_mut!(tables).peers_net.as_mut().unwrap().compute_trees(),
                    };

                    tracing::trace!("Compute routes");
                    pubsub::pubsub_tree_change(&mut tables, &new_childs, net_type);
                    queries::queries_tree_change(&mut tables, &new_childs, net_type);

                    tracing::trace!("Computations completed");
                    match net_type {
                        WhatAmI::Router => hat_mut!(tables).routers_trees_task = None,
                        _ => hat_mut!(tables).peers_trees_task = None,
                    };
                },
                TerminatableTask::create_cancellation_token(),
            );
            match net_type {
                WhatAmI::Router => self.routers_trees_task = Some(task),
                _ => self.peers_trees_task = Some(task),
            };
        }
    }
}

pub(crate) struct HatCode {}

impl HatBaseTrait for HatCode {
    fn init(&self, tables: &mut Tables, runtime: Runtime) {
        let config = runtime.config().lock();
        let whatami = tables.whatami;
        let gossip = unwrap_or_default!(config.scouting().gossip().enabled());
        let gossip_multihop = unwrap_or_default!(config.scouting().gossip().multihop());
        let autoconnect = if gossip {
            *unwrap_or_default!(config.scouting().gossip().autoconnect().get(whatami))
        } else {
            WhatAmIMatcher::empty()
        };

        let router_full_linkstate = whatami == WhatAmI::Router;
        let peer_full_linkstate = whatami != WhatAmI::Client
            && unwrap_or_default!(config.routing().peer().mode()) == *"linkstate";
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        drop(config);

        if router_full_linkstate | gossip {
            hat_mut!(tables).routers_net = Some(Network::new(
                "[Routers network]".to_string(),
                tables.zid,
                runtime.clone(),
                router_full_linkstate,
                router_peers_failover_brokering,
                gossip,
                gossip_multihop,
                autoconnect,
            ));
        }
        if peer_full_linkstate | gossip {
            hat_mut!(tables).peers_net = Some(Network::new(
                "[Peers network]".to_string(),
                tables.zid,
                runtime,
                peer_full_linkstate,
                router_peers_failover_brokering,
                gossip,
                gossip_multihop,
                autoconnect,
            ));
        }
        if router_full_linkstate && peer_full_linkstate {
            hat_mut!(tables).shared_nodes = shared_nodes(
                hat!(tables).routers_net.as_ref().unwrap(),
                hat!(tables).peers_net.as_ref().unwrap(),
            );
        }
    }

    fn new_tables(&self, router_peers_failover_brokering: bool) -> Box<dyn Any + Send + Sync> {
        Box::new(HatTables::new(router_peers_failover_brokering))
    }

    fn new_face(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatFace::new())
    }

    fn new_resource(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatContext::new())
    }

    fn new_local_face(
        &self,
        tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        face: &mut Face,
    ) -> ZResult<()> {
        pubsub_new_face(tables, &mut face.state);
        queries_new_face(tables, &mut face.state);
        Ok(())
    }

    fn new_transport_unicast_face(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        transport: &TransportUnicast,
    ) -> ZResult<()> {
        let link_id = match face.state.whatami {
            WhatAmI::Router => hat_mut!(tables)
                .routers_net
                .as_mut()
                .unwrap()
                .add_link(transport.clone()),
            WhatAmI::Peer => {
                if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                    net.add_link(transport.clone())
                } else {
                    0
                }
            }
            _ => 0,
        };

        if hat!(tables).full_net(WhatAmI::Router) && hat!(tables).full_net(WhatAmI::Peer) {
            hat_mut!(tables).shared_nodes = shared_nodes(
                hat!(tables).routers_net.as_ref().unwrap(),
                hat!(tables).peers_net.as_ref().unwrap(),
            );
        }

        face_hat_mut!(&mut face.state).link_id = link_id;
        pubsub_new_face(tables, &mut face.state);
        queries_new_face(tables, &mut face.state);

        match face.state.whatami {
            WhatAmI::Router => {
                hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
            }
            WhatAmI::Peer => {
                if hat_mut!(tables).full_net(WhatAmI::Peer) {
                    hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                }
            }
            _ => (),
        }
        Ok(())
    }

    fn close_face(&self, tables: &TablesLock, face: &mut Arc<FaceState>) {
        let mut wtables = zwrite!(tables.tables);
        let mut face_clone = face.clone();

        face_hat_mut!(face).remote_sub_interests.clear();
        face_hat_mut!(face).local_subs.clear();
        face_hat_mut!(face).remote_qabl_interests.clear();
        face_hat_mut!(face).local_qabls.clear();

        let face = get_mut_unchecked(face);
        for res in face.remote_mappings.values_mut() {
            get_mut_unchecked(res).session_ctxs.remove(&face.id);
            Resource::clean(res);
        }
        face.remote_mappings.clear();
        for res in face.local_mappings.values_mut() {
            get_mut_unchecked(res).session_ctxs.remove(&face.id);
            Resource::clean(res);
        }
        face.local_mappings.clear();

        let mut subs_matches = vec![];
        for (_id, mut res) in face
            .hat
            .downcast_mut::<HatFace>()
            .unwrap()
            .remote_subs
            .drain()
        {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_client_subscription(&mut wtables, &mut face_clone, &mut res);

            if res.context.is_some() {
                for match_ in &res.context().matches {
                    let mut match_ = match_.upgrade().unwrap();
                    if !Arc::ptr_eq(&match_, &res) {
                        get_mut_unchecked(&mut match_)
                            .context_mut()
                            .disable_data_routes();
                        subs_matches.push(match_);
                    }
                }
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .disable_data_routes();
                subs_matches.push(res);
            }
        }

        let mut qabls_matches = vec![];
        for (_, mut res) in face
            .hat
            .downcast_mut::<HatFace>()
            .unwrap()
            .remote_qabls
            .drain()
        {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_client_queryable(&mut wtables, &mut face_clone, &mut res);

            if res.context.is_some() {
                for match_ in &res.context().matches {
                    let mut match_ = match_.upgrade().unwrap();
                    if !Arc::ptr_eq(&match_, &res) {
                        get_mut_unchecked(&mut match_)
                            .context_mut()
                            .disable_query_routes();
                        qabls_matches.push(match_);
                    }
                }
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .disable_query_routes();
                qabls_matches.push(res);
            }
        }
        drop(wtables);

        let mut matches_data_routes = vec![];
        let mut matches_query_routes = vec![];
        let rtables = zread!(tables.tables);
        for _match in subs_matches.drain(..) {
            let mut expr = RoutingExpr::new(&_match, "");
            matches_data_routes.push((_match.clone(), compute_data_routes(&rtables, &mut expr)));
        }
        for _match in qabls_matches.drain(..) {
            matches_query_routes.push((_match.clone(), compute_query_routes(&rtables, &_match)));
        }
        drop(rtables);

        let mut wtables = zwrite!(tables.tables);
        for (mut res, data_routes) in matches_data_routes {
            get_mut_unchecked(&mut res)
                .context_mut()
                .update_data_routes(data_routes);
            Resource::clean(&mut res);
        }
        for (mut res, query_routes) in matches_query_routes {
            get_mut_unchecked(&mut res)
                .context_mut()
                .update_query_routes(query_routes);
            Resource::clean(&mut res);
        }
        wtables.faces.remove(&face.id);
        drop(wtables);
    }

    fn handle_oam(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        oam: Oam,
        transport: &TransportUnicast,
    ) -> ZResult<()> {
        if oam.id == OAM_LINKSTATE {
            if let ZExtBody::ZBuf(buf) = oam.body {
                if let Ok(zid) = transport.get_zid() {
                    use zenoh_buffers::reader::HasReader;
                    use zenoh_codec::RCodec;
                    let codec = Zenoh080Routing::new();
                    let mut reader = buf.reader();
                    let list: LinkStateList = codec.read(&mut reader).unwrap();

                    let whatami = transport.get_whatami()?;
                    match whatami {
                        WhatAmI::Router => {
                            for (_, removed_node) in hat_mut!(tables)
                                .routers_net
                                .as_mut()
                                .unwrap()
                                .link_states(list.link_states, zid)
                                .removed_nodes
                            {
                                pubsub_remove_node(tables, &removed_node.zid, WhatAmI::Router);
                                queries_remove_node(tables, &removed_node.zid, WhatAmI::Router);
                            }

                            if hat!(tables).full_net(WhatAmI::Peer) {
                                hat_mut!(tables).shared_nodes = shared_nodes(
                                    hat!(tables).routers_net.as_ref().unwrap(),
                                    hat!(tables).peers_net.as_ref().unwrap(),
                                );
                            }

                            hat_mut!(tables)
                                .schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
                        }
                        WhatAmI::Peer => {
                            if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                                let changes = net.link_states(list.link_states, zid);
                                if hat!(tables).full_net(WhatAmI::Peer) {
                                    for (_, removed_node) in changes.removed_nodes {
                                        pubsub_remove_node(
                                            tables,
                                            &removed_node.zid,
                                            WhatAmI::Peer,
                                        );
                                        queries_remove_node(
                                            tables,
                                            &removed_node.zid,
                                            WhatAmI::Peer,
                                        );
                                    }

                                    hat_mut!(tables).shared_nodes = shared_nodes(
                                        hat!(tables).routers_net.as_ref().unwrap(),
                                        hat!(tables).peers_net.as_ref().unwrap(),
                                    );

                                    hat_mut!(tables)
                                        .schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                                } else {
                                    for (_, updated_node) in changes.updated_nodes {
                                        pubsub_linkstate_change(
                                            tables,
                                            &updated_node.zid,
                                            &updated_node.links,
                                        );
                                        queries_linkstate_change(
                                            tables,
                                            &updated_node.zid,
                                            &updated_node.links,
                                        );
                                    }
                                }
                            }
                        }
                        _ => (),
                    };
                }
            }
        }

        Ok(())
    }

    #[inline]
    fn map_routing_context(
        &self,
        tables: &Tables,
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId {
        match face.whatami {
            WhatAmI::Router => hat!(tables)
                .routers_net
                .as_ref()
                .unwrap()
                .get_local_context(routing_context, face_hat!(face).link_id),
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    hat!(tables)
                        .peers_net
                        .as_ref()
                        .unwrap()
                        .get_local_context(routing_context, face_hat!(face).link_id)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn closing(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        transport: &TransportUnicast,
    ) -> ZResult<()> {
        match (transport.get_zid(), transport.get_whatami()) {
            (Ok(zid), Ok(whatami)) => {
                match whatami {
                    WhatAmI::Router => {
                        for (_, removed_node) in hat_mut!(tables)
                            .routers_net
                            .as_mut()
                            .unwrap()
                            .remove_link(&zid)
                        {
                            pubsub_remove_node(tables, &removed_node.zid, WhatAmI::Router);
                            queries_remove_node(tables, &removed_node.zid, WhatAmI::Router);
                        }

                        if hat!(tables).full_net(WhatAmI::Peer) {
                            hat_mut!(tables).shared_nodes = shared_nodes(
                                hat!(tables).routers_net.as_ref().unwrap(),
                                hat!(tables).peers_net.as_ref().unwrap(),
                            );
                        }

                        hat_mut!(tables)
                            .schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
                    }
                    WhatAmI::Peer => {
                        if hat!(tables).full_net(WhatAmI::Peer) {
                            for (_, removed_node) in hat_mut!(tables)
                                .peers_net
                                .as_mut()
                                .unwrap()
                                .remove_link(&zid)
                            {
                                pubsub_remove_node(tables, &removed_node.zid, WhatAmI::Peer);
                                queries_remove_node(tables, &removed_node.zid, WhatAmI::Peer);
                            }

                            hat_mut!(tables).shared_nodes = shared_nodes(
                                hat!(tables).routers_net.as_ref().unwrap(),
                                hat!(tables).peers_net.as_ref().unwrap(),
                            );

                            hat_mut!(tables)
                                .schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                        } else if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                            net.remove_link(&zid);
                        }
                    }
                    _ => (),
                };
            }
            (_, _) => tracing::error!("Closed transport in session closing!"),
        }
        Ok(())
    }

    #[inline]
    fn ingress_filter(&self, tables: &Tables, face: &FaceState, expr: &mut RoutingExpr) -> bool {
        face.whatami != WhatAmI::Peer
            || hat!(tables).peers_net.is_none()
            || tables.zid
                == *hat!(tables).elect_router(
                    &tables.zid,
                    expr.full_expr(),
                    hat!(tables).get_router_links(face.zid),
                )
    }

    #[inline]
    fn egress_filter(
        &self,
        tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &mut RoutingExpr,
    ) -> bool {
        if src_face.id != out_face.id
            && match (src_face.mcast_group.as_ref(), out_face.mcast_group.as_ref()) {
                (Some(l), Some(r)) => l != r,
                _ => true,
            }
        {
            let dst_master = out_face.whatami != WhatAmI::Peer
                || hat!(tables).peers_net.is_none()
                || tables.zid
                    == *hat!(tables).elect_router(
                        &tables.zid,
                        expr.full_expr(),
                        hat!(tables).get_router_links(out_face.zid),
                    );

            return dst_master
                && (src_face.whatami != WhatAmI::Peer
                    || out_face.whatami != WhatAmI::Peer
                    || hat!(tables).full_net(WhatAmI::Peer)
                    || hat!(tables).failover_brokering(src_face.zid, out_face.zid));
        }
        false
    }

    fn info(&self, tables: &Tables, kind: WhatAmI) -> String {
        match kind {
            WhatAmI::Router => hat!(tables)
                .routers_net
                .as_ref()
                .map(|net| net.dot())
                .unwrap_or_else(|| "graph {}".to_string()),
            WhatAmI::Peer => hat!(tables)
                .peers_net
                .as_ref()
                .map(|net| net.dot())
                .unwrap_or_else(|| "graph {}".to_string()),
            _ => "graph {}".to_string(),
        }
    }
}

struct HatContext {
    router_subs: HashSet<ZenohId>,
    peer_subs: HashSet<ZenohId>,
    router_qabls: HashMap<ZenohId, QueryableInfoType>,
    peer_qabls: HashMap<ZenohId, QueryableInfoType>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            peer_qabls: HashMap::new(),
        }
    }
}

struct HatFace {
    link_id: usize,
    next_id: AtomicU32, // @TODO: manage rollover and uniqueness
    remote_sub_interests: HashMap<InterestId, (Option<Arc<Resource>>, bool)>,
    local_subs: HashMap<Arc<Resource>, SubscriberId>,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    remote_qabl_interests: HashMap<InterestId, Option<Arc<Resource>>>,
    local_qabls: HashMap<Arc<Resource>, (QueryableId, QueryableInfoType)>,
    remote_qabls: HashMap<QueryableId, Arc<Resource>>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            link_id: 0,
            next_id: AtomicU32::new(0),
            remote_sub_interests: HashMap::new(),
            local_subs: HashMap::new(),
            remote_subs: HashMap::new(),
            remote_qabl_interests: HashMap::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashMap::new(),
        }
    }
}

fn get_router(tables: &Tables, face: &Arc<FaceState>, nodeid: NodeId) -> Option<ZenohId> {
    match hat!(tables)
        .routers_net
        .as_ref()
        .unwrap()
        .get_link(face_hat!(face).link_id)
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

fn get_peer(tables: &Tables, face: &Arc<FaceState>, nodeid: NodeId) -> Option<ZenohId> {
    match hat!(tables)
        .peers_net
        .as_ref()
        .unwrap()
        .get_link(face_hat!(face).link_id)
    {
        Some(link) => match link.get_zid(&(nodeid as u64)) {
            Some(router) => Some(*router),
            None => {
                tracing::error!(
                    "Received peer declaration with unknown routing context id {}",
                    nodeid
                );
                None
            }
        },
        None => {
            tracing::error!(
                "Could not find corresponding link in peers network for {}",
                face
            );
            None
        }
    }
}

impl HatTrait for HatCode {}

#[inline]
fn get_routes_entries(tables: &Tables) -> RoutesIndexes {
    let routers_indexes = hat!(tables)
        .routers_net
        .as_ref()
        .unwrap()
        .graph
        .node_indices()
        .map(|i| i.index() as NodeId)
        .collect::<Vec<NodeId>>();
    let peers_indexes = if hat!(tables).full_net(WhatAmI::Peer) {
        hat!(tables)
            .peers_net
            .as_ref()
            .unwrap()
            .graph
            .node_indices()
            .map(|i| i.index() as NodeId)
            .collect::<Vec<NodeId>>()
    } else {
        vec![0]
    };
    RoutesIndexes {
        routers: routers_indexes,
        peers: peers_indexes,
        clients: vec![0],
    }
}
