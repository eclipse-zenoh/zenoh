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
    network::Network, pubsub::undeclare_client_subscription, queries::undeclare_client_queryable,
};
use super::dispatcher::{
    face::FaceState,
    queries::compute_query_routes_,
    tables::{compute_data_routes_, Resource, TablesLock},
};
use async_std::task::JoinHandle;
use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::Hasher,
    sync::Arc,
};
use zenoh_config::{WhatAmI, ZenohId};
use zenoh_protocol::network::declare::queryable::ext::QueryableInfo;
use zenoh_sync::get_mut_unchecked;

pub mod network;
pub mod pubsub;
pub mod queries;

zconfigurable! {
    static ref TREES_COMPUTATION_DELAY: u64 = 100;
}

pub struct HatTables {
    pub(crate) router_subs: HashSet<Arc<Resource>>,
    pub(crate) peer_subs: HashSet<Arc<Resource>>,
    pub(crate) router_qabls: HashSet<Arc<Resource>>,
    pub(crate) peer_qabls: HashSet<Arc<Resource>>,
    pub(crate) routers_net: Option<Network>,
    pub(crate) peers_net: Option<Network>,
    pub(crate) shared_nodes: Vec<ZenohId>,
    pub(crate) routers_trees_task: Option<JoinHandle<()>>,
    pub(crate) peers_trees_task: Option<JoinHandle<()>>,
    pub(crate) router_peers_failover_brokering: bool,
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
    pub(crate) fn get_net(&self, net_type: WhatAmI) -> Option<&Network> {
        match net_type {
            WhatAmI::Router => self.routers_net.as_ref(),
            WhatAmI::Peer => self.peers_net.as_ref(),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn full_net(&self, net_type: WhatAmI) -> bool {
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
    pub(crate) fn get_router_links(&self, peer: ZenohId) -> impl Iterator<Item = &ZenohId> + '_ {
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
    pub(crate) fn elect_router<'a>(
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
    pub(crate) fn failover_brokering_to(source_links: &[ZenohId], dest: ZenohId) -> bool {
        // if source_links is empty then gossip is probably disabled in source peer
        !source_links.is_empty() && !source_links.contains(&dest)
    }

    #[inline]
    pub(crate) fn failover_brokering(&self, peer1: ZenohId, peer2: ZenohId) -> bool {
        self.router_peers_failover_brokering
            && self
                .peers_net
                .as_ref()
                .map(|net| HatTables::failover_brokering_to(net.get_links(peer1), peer2))
                .unwrap_or(false)
    }

    pub(crate) fn schedule_compute_trees(
        &mut self,
        tables_ref: Arc<TablesLock>,
        net_type: WhatAmI,
    ) {
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
                    WhatAmI::Router => tables.hat.routers_net.as_mut().unwrap().compute_trees(),
                    _ => tables.hat.peers_net.as_mut().unwrap().compute_trees(),
                };

                log::trace!("Compute routes");
                pubsub::pubsub_tree_change(&mut tables, &new_childs, net_type);
                queries::queries_tree_change(&mut tables, &new_childs, net_type);

                log::trace!("Computations completed");
                match net_type {
                    WhatAmI::Router => tables.hat.routers_trees_task = None,
                    _ => tables.hat.peers_trees_task = None,
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
    pub fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            peer_qabls: HashMap::new(),
        }
    }
}

pub(crate) struct HatFace {
    pub(crate) local_subs: HashSet<Arc<Resource>>,
    pub(crate) remote_subs: HashSet<Arc<Resource>>,
    pub(crate) local_qabls: HashMap<Arc<Resource>, QueryableInfo>,
    pub(crate) remote_qabls: HashSet<Arc<Resource>>,
}

impl HatFace {
    pub fn new() -> Self {
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
    for mut res in face.hat.remote_subs.drain() {
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
    for mut res in face.hat.remote_qabls.drain() {
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
