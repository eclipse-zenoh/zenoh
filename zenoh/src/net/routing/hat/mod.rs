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
use self::{
    network::Network,
    pubsub::{compute_data_routes_, undeclare_client_subscription},
    queries::{compute_query_routes_, undeclare_client_queryable},
};
use super::dispatcher::{
    face::FaceState,
    tables::{Resource, RoutingContext, Tables, TablesLock},
};
use crate::{
    hat, hat_mut,
    net::{
        codec::Zenoh080Routing,
        protocol::linkstate::LinkStateList,
        routing::hat::{
            network::shared_nodes,
            pubsub::{pubsub_linkstate_change, pubsub_remove_node},
            queries::{queries_linkstate_change, queries_remove_node},
        },
    },
    runtime::Runtime,
};
use async_std::task::JoinHandle;
use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::Hasher,
    sync::Arc,
};
use zenoh_config::{WhatAmI, WhatAmIMatcher, ZenohId};
use zenoh_protocol::{
    common::ZExtBody,
    network::{declare::queryable::ext::QueryableInfo, oam::id::OAM_LINKSTATE, Oam},
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::{Mux, TransportUnicast};

pub mod network;
pub mod pubsub;
pub mod queries;

zconfigurable! {
    static ref TREES_COMPUTATION_DELAY: u64 = 100;
}

pub struct HatTables {
    router_subs: HashSet<Arc<Resource>>,
    peer_subs: HashSet<Arc<Resource>>,
    router_qabls: HashSet<Arc<Resource>>,
    peer_qabls: HashSet<Arc<Resource>>,
    routers_net: Option<Network>,
    peers_net: Option<Network>,
    shared_nodes: Vec<ZenohId>,
    routers_trees_task: Option<JoinHandle<()>>,
    peers_trees_task: Option<JoinHandle<()>>,
    router_peers_failover_brokering: bool,
}

impl HatTables {
    pub fn new(router_peers_failover_brokering: bool) -> Self {
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
                .map(|net| HatTables::failover_brokering_to(net.get_links(peer1), peer2))
                .unwrap_or(false)
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>, net_type: WhatAmI) {
        log::trace!("Schedule computations");
        if (net_type == WhatAmI::Router && self.routers_trees_task.is_none())
            || (net_type == WhatAmI::Peer && self.peers_trees_task.is_none())
        {
            let task = Some(async_std::task::spawn(async move {
                async_std::task::sleep(std::time::Duration::from_millis(*TREES_COMPUTATION_DELAY))
                    .await;
                let mut tables = zwrite!(tables_ref.tables);

                log::trace!("Compute trees");
                let new_childs = match net_type {
                    WhatAmI::Router => hat_mut!(tables)
                        .routers_net
                        .as_mut()
                        .unwrap()
                        .compute_trees(),
                    _ => hat_mut!(tables).peers_net.as_mut().unwrap().compute_trees(),
                };

                log::trace!("Compute routes");
                pubsub::pubsub_tree_change(&mut tables, &new_childs, net_type);
                queries::queries_tree_change(&mut tables, &new_childs, net_type);

                log::trace!("Computations completed");
                match net_type {
                    WhatAmI::Router => hat_mut!(tables).routers_trees_task = None,
                    _ => hat_mut!(tables).peers_trees_task = None,
                };
            }));
            match net_type {
                WhatAmI::Router => self.routers_trees_task = task,
                _ => self.peers_trees_task = task,
            };
        }
    }
}

pub(crate) struct HatContext {
    router_subs: HashSet<ZenohId>,
    peer_subs: HashSet<ZenohId>,
    router_qabls: HashMap<ZenohId, QueryableInfo>,
    peer_qabls: HashMap<ZenohId, QueryableInfo>,
}

impl HatContext {
    pub(crate) fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            peer_qabls: HashMap::new(),
        }
    }
}

pub(crate) struct HatFace {
    local_subs: HashSet<Arc<Resource>>,
    remote_subs: HashSet<Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, QueryableInfo>,
    remote_qabls: HashSet<Arc<Resource>>,
}

impl HatFace {
    pub(crate) fn new() -> Self {
        Self {
            local_subs: HashSet::new(),
            remote_subs: HashSet::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashSet::new(),
        }
    }
}

pub(crate) fn close_face(tables: &TablesLock, face: &mut Arc<FaceState>) {
    let ctrl_lock = zlock!(tables.ctrl_lock);
    let mut wtables = zwrite!(tables.tables);
    let mut face_clone = face.clone();
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
    for mut res in face
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
                        .valid_data_routes = false;
                    subs_matches.push(match_);
                }
            }
            get_mut_unchecked(&mut res).context_mut().valid_data_routes = false;
            subs_matches.push(res);
        }
    }

    let mut qabls_matches = vec![];
    for mut res in face
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
                        .valid_query_routes = false;
                    qabls_matches.push(match_);
                }
            }
            get_mut_unchecked(&mut res).context_mut().valid_query_routes = false;
            qabls_matches.push(res);
        }
    }
    drop(wtables);

    let mut matches_data_routes = vec![];
    let mut matches_query_routes = vec![];
    let rtables = zread!(tables.tables);
    for _match in subs_matches.drain(..) {
        matches_data_routes.push((_match.clone(), compute_data_routes_(&rtables, &_match)));
    }
    for _match in qabls_matches.drain(..) {
        matches_query_routes.push((_match.clone(), compute_query_routes_(&rtables, &_match)));
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
    drop(ctrl_lock);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn init(
    tables: &Arc<TablesLock>,
    runtime: Runtime,
    router_full_linkstate: bool,
    peer_full_linkstate: bool,
    router_peers_failover_brokering: bool,
    gossip: bool,
    gossip_multihop: bool,
    autoconnect: WhatAmIMatcher,
) {
    let mut tables = zwrite!(tables.tables);
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

pub(crate) fn new_transport_unicast(
    tables_ref: &Arc<TablesLock>,
    transport: TransportUnicast,
) -> ZResult<Arc<FaceState>> {
    let ctrl_lock = zlock!(tables_ref.ctrl_lock);
    let mut tables = zwrite!(tables_ref.tables);
    let whatami = transport.get_whatami()?;

    let link_id = match (tables.whatami, whatami) {
        (WhatAmI::Router, WhatAmI::Router) => hat_mut!(tables)
            .routers_net
            .as_mut()
            .unwrap()
            .add_link(transport.clone()),
        (WhatAmI::Router, WhatAmI::Peer)
        | (WhatAmI::Peer, WhatAmI::Router)
        | (WhatAmI::Peer, WhatAmI::Peer) => {
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

    let face = tables
        .open_net_face(
            transport.get_zid()?,
            transport.get_whatami()?,
            #[cfg(feature = "stats")]
            transport.get_stats()?,
            Arc::new(Mux::new(transport)),
            link_id,
        )
        .upgrade()
        .unwrap();

    match (tables.whatami, whatami) {
        (WhatAmI::Router, WhatAmI::Router) => {
            hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
        }
        (WhatAmI::Router, WhatAmI::Peer)
        | (WhatAmI::Peer, WhatAmI::Router)
        | (WhatAmI::Peer, WhatAmI::Peer) => {
            if hat_mut!(tables).full_net(WhatAmI::Peer) {
                hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
            }
        }
        _ => (),
    }
    drop(tables);
    drop(ctrl_lock);
    Ok(face)
}

pub(crate) fn handle_oam(
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

                let ctrl_lock = zlock!(tables_ref.ctrl_lock);
                let mut tables = zwrite!(tables_ref.tables);
                let whatami = transport.get_whatami()?;
                match (tables.whatami, whatami) {
                    (WhatAmI::Router, WhatAmI::Router) => {
                        for (_, removed_node) in hat_mut!(tables)
                            .routers_net
                            .as_mut()
                            .unwrap()
                            .link_states(list.link_states, zid)
                            .removed_nodes
                        {
                            pubsub_remove_node(&mut tables, &removed_node.zid, WhatAmI::Router);
                            queries_remove_node(&mut tables, &removed_node.zid, WhatAmI::Router);
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
                    (WhatAmI::Router, WhatAmI::Peer)
                    | (WhatAmI::Peer, WhatAmI::Router)
                    | (WhatAmI::Peer, WhatAmI::Peer) => {
                        if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                            let changes = net.link_states(list.link_states, zid);
                            if hat!(tables).full_net(WhatAmI::Peer) {
                                for (_, removed_node) in changes.removed_nodes {
                                    pubsub_remove_node(
                                        &mut tables,
                                        &removed_node.zid,
                                        WhatAmI::Peer,
                                    );
                                    queries_remove_node(
                                        &mut tables,
                                        &removed_node.zid,
                                        WhatAmI::Peer,
                                    );
                                }

                                if tables.whatami == WhatAmI::Router {
                                    hat_mut!(tables).shared_nodes = shared_nodes(
                                        hat!(tables).routers_net.as_ref().unwrap(),
                                        hat!(tables).peers_net.as_ref().unwrap(),
                                    );
                                }

                                hat_mut!(tables)
                                    .schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                            } else {
                                for (_, updated_node) in changes.updated_nodes {
                                    pubsub_linkstate_change(
                                        &mut tables,
                                        &updated_node.zid,
                                        &updated_node.links,
                                    );
                                    queries_linkstate_change(
                                        &mut tables,
                                        &updated_node.zid,
                                        &updated_node.links,
                                    );
                                }
                            }
                        }
                    }
                    _ => (),
                };
                drop(tables);
                drop(ctrl_lock);
            }
        }
    }

    Ok(())
}

pub(crate) fn closing(tables_ref: &Arc<TablesLock>, transport: &TransportUnicast) -> ZResult<()> {
    match (transport.get_zid(), transport.get_whatami()) {
        (Ok(zid), Ok(whatami)) => {
            let ctrl_lock = zlock!(tables_ref.ctrl_lock);
            let mut tables = zwrite!(tables_ref.tables);
            match (tables.whatami, whatami) {
                (WhatAmI::Router, WhatAmI::Router) => {
                    for (_, removed_node) in hat_mut!(tables)
                        .routers_net
                        .as_mut()
                        .unwrap()
                        .remove_link(&zid)
                    {
                        pubsub_remove_node(&mut tables, &removed_node.zid, WhatAmI::Router);
                        queries_remove_node(&mut tables, &removed_node.zid, WhatAmI::Router);
                    }

                    if hat!(tables).full_net(WhatAmI::Peer) {
                        hat_mut!(tables).shared_nodes = shared_nodes(
                            hat!(tables).routers_net.as_ref().unwrap(),
                            hat!(tables).peers_net.as_ref().unwrap(),
                        );
                    }

                    hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
                }
                (WhatAmI::Router, WhatAmI::Peer)
                | (WhatAmI::Peer, WhatAmI::Router)
                | (WhatAmI::Peer, WhatAmI::Peer) => {
                    if hat!(tables).full_net(WhatAmI::Peer) {
                        for (_, removed_node) in hat_mut!(tables)
                            .peers_net
                            .as_mut()
                            .unwrap()
                            .remove_link(&zid)
                        {
                            pubsub_remove_node(&mut tables, &removed_node.zid, WhatAmI::Peer);
                            queries_remove_node(&mut tables, &removed_node.zid, WhatAmI::Peer);
                        }

                        if tables.whatami == WhatAmI::Router {
                            hat_mut!(tables).shared_nodes = shared_nodes(
                                hat!(tables).routers_net.as_ref().unwrap(),
                                hat!(tables).peers_net.as_ref().unwrap(),
                            );
                        }

                        hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                    } else if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                        net.remove_link(&zid);
                    }
                }
                _ => (),
            };
            drop(tables);
            drop(ctrl_lock);
        }
        (_, _) => log::error!("Closed transport in session closing!"),
    }
    Ok(())
}

pub(crate) fn map_routing_context(
    tables: &Tables,
    face: &FaceState,
    routing_context: RoutingContext,
) -> RoutingContext {
    match tables.whatami {
        WhatAmI::Router => match face.whatami {
            WhatAmI::Router => hat!(tables)
                .routers_net
                .as_ref()
                .unwrap()
                .get_local_context(routing_context, face.link_id),
            WhatAmI::Peer => {
                if hat!(tables).full_net(WhatAmI::Peer) {
                    hat!(tables)
                        .peers_net
                        .as_ref()
                        .unwrap()
                        .get_local_context(routing_context, face.link_id)
                } else {
                    0
                }
            }
            _ => 0,
        },
        WhatAmI::Peer => {
            if hat!(tables).full_net(WhatAmI::Peer) {
                hat!(tables)
                    .peers_net
                    .as_ref()
                    .unwrap()
                    .get_local_context(routing_context, face.link_id)
            } else {
                0
            }
        }
        _ => 0,
    }
}

#[macro_export]
macro_rules! hat {
    ($t:expr) => {
        $t.hat.downcast_ref::<HatTables>().unwrap()
    };
}

#[macro_export]
macro_rules! hat_mut {
    ($t:expr) => {
        $t.hat.downcast_mut::<HatTables>().unwrap()
    };
}

#[macro_export]
macro_rules! res_hat {
    ($r:expr) => {
        $r.context().hat.downcast_ref::<HatContext>().unwrap()
    };
}

#[macro_export]
macro_rules! res_hat_mut {
    ($r:expr) => {
        get_mut_unchecked($r)
            .context_mut()
            .hat
            .downcast_mut::<HatContext>()
            .unwrap()
    };
}

#[macro_export]
macro_rules! face_hat {
    ($f:expr) => {
        $f.hat.downcast_ref::<HatFace>().unwrap()
    };
}

#[macro_export]
macro_rules! face_hat_mut {
    ($f:expr) => {
        get_mut_unchecked($f).hat.downcast_mut::<HatFace>().unwrap()
    };
}

fn get_router(tables: &Tables, face: &Arc<FaceState>, nodeid: RoutingContext) -> Option<ZenohId> {
    match hat!(tables)
        .routers_net
        .as_ref()
        .unwrap()
        .get_link(face.link_id)
    {
        Some(link) => match link.get_zid(&(nodeid as u64)) {
            Some(router) => Some(*router),
            None => {
                log::error!(
                    "Received router declaration with unknown routing context id {}",
                    nodeid
                );
                None
            }
        },
        None => {
            log::error!(
                "Could not find corresponding link in routers network for {}",
                face
            );
            None
        }
    }
}

fn get_peer(tables: &Tables, face: &Arc<FaceState>, nodeid: RoutingContext) -> Option<ZenohId> {
    match hat!(tables)
        .peers_net
        .as_ref()
        .unwrap()
        .get_link(face.link_id)
    {
        Some(link) => match link.get_zid(&(nodeid as u64)) {
            Some(router) => Some(*router),
            None => {
                log::error!(
                    "Received peer declaration with unknown routing context id {}",
                    nodeid
                );
                None
            }
        },
        None => {
            log::error!(
                "Could not find corresponding link in peers network for {}",
                face
            );
            None
        }
    }
}
