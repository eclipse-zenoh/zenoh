//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::Arc;
use petgraph::graph::NodeIndex;
use std::collections::{HashMap, HashSet};

use zenoh_protocol::core::{whatami, PeerId, QueryConsolidation, QueryTarget, ResKey, ZInt};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::{DataInfo, RoutingContext};

use crate::routing::face::FaceState;
use crate::routing::network::Network;
use crate::routing::resource::{Context, Resource, Route};
use crate::routing::router::Tables;

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: ZInt,
}

async fn propagate_simple_queryable(
    tables: &mut Tables,
    src_face: &mut Arc<FaceState>,
    res: &Arc<Resource>,
) {
    for dst_face in &mut tables.faces.values_mut() {
        if src_face.id != dst_face.id
            && !dst_face.local_qabls.contains(res)
            && match tables.whatami {
                whatami::ROUTER => dst_face.whatami == whatami::CLIENT,
                whatami::PEER => dst_face.whatami == whatami::CLIENT,
                _ => (src_face.whatami == whatami::CLIENT || dst_face.whatami == whatami::CLIENT),
            }
        {
            unsafe {
                Arc::get_mut_unchecked(dst_face)
                    .local_qabls
                    .push(res.clone());
                let reskey = Resource::decl_key(res, dst_face).await;
                dst_face.primitives.queryable(&reskey, None).await;
            }
        }
    }
}

async unsafe fn register_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: PeerId,
) {
    if !res.router_qabls.contains(&router) {
        // Register router queryable
        {
            let res_mut = Arc::get_mut_unchecked(res);
            log::debug!(
                "Register router queryable {} (router: {})",
                res_mut.name(),
                router
            );
            res_mut.router_qabls.insert(router.clone());
            tables.router_qabls.insert(res.clone());
        }

        // Propagate queryable to routers
        let net = tables.routers_net.as_ref().unwrap();
        match net.get_idx(&router) {
            Some(tree_sid) => {
                if net.trees.len() > tree_sid.index() {
                    for child in &net.trees[tree_sid.index()].childs {
                        match tables.get_face(&net.graph[*child].pid).cloned() {
                            Some(mut someface) => {
                                if someface.id != face.id {
                                    let reskey = Resource::decl_key(res, &mut someface).await;

                                    log::debug!(
                                        "Send router queryable {} on face {} {}",
                                        res.name(),
                                        someface.id,
                                        someface.pid,
                                    );

                                    someface
                                        .primitives
                                        .queryable(&reskey, Some(tree_sid.index() as ZInt))
                                        .await;
                                }
                            }
                            None => {
                                log::trace!("Unable to find face for pid {}", net.graph[*child].pid)
                            }
                        }
                    }
                } else {
                    log::trace!("Tree for router {} not yet ready", router);
                }
            }
            None => log::error!(
                "Error propagating qabl {}: cannot get index of {}!",
                res.name(),
                router
            ),
        }

        // Propagate queryable to peers
        if face.whatami != whatami::PEER {
            register_peer_queryable(tables, face, res, tables.pid.clone()).await
        }
    }

    // Propagate queryable to clients
    propagate_simple_queryable(tables, face, res).await;
}

pub async fn declare_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    router: PeerId,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(&mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);
            register_router_queryable(tables, face, &mut res, router).await;

            compute_matches_query_routes(tables, &mut res);
        },
        None => log::error!("Declare router queryable for unknown rid {}!", prefixid),
    }
}

async unsafe fn register_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: PeerId,
) {
    if !res.peer_qabls.contains(&peer) {
        // Register peer queryable
        {
            let res_mut = Arc::get_mut_unchecked(res);
            log::debug!(
                "Register peer queryable {} (peer: {})",
                res_mut.name(),
                peer
            );
            res_mut.peer_qabls.insert(peer.clone());
            tables.peer_qabls.insert(res.clone());
        }

        // Propagate queryable to peers
        let net = tables.peers_net.as_ref().unwrap();
        match net.get_idx(&peer) {
            Some(tree_sid) => {
                if net.trees.len() > tree_sid.index() {
                    for child in &net.trees[tree_sid.index()].childs {
                        match tables.get_face(&net.graph[*child].pid).cloned() {
                            Some(mut someface) => {
                                if someface.id != face.id {
                                    let reskey = Resource::decl_key(res, &mut someface).await;

                                    log::debug!(
                                        "Send peer queryable {} on face {} {}",
                                        res.name(),
                                        someface.id,
                                        someface.pid,
                                    );

                                    someface
                                        .primitives
                                        .queryable(&reskey, Some(tree_sid.index() as ZInt))
                                        .await;
                                }
                            }
                            None => {
                                log::trace!("Unable to find face for pid {}", net.graph[*child].pid)
                            }
                        }
                    }
                } else {
                    log::trace!("Tree for peer {} not yet ready", peer);
                }
            }
            None => log::error!(
                "Error propagating qabl {}: cannot get index of {}!",
                res.name(),
                peer,
            ),
        }
    }
}

pub async fn declare_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    peer: PeerId,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(&mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);
            register_peer_queryable(tables, face, &mut res, peer).await;

            if tables.whatami == whatami::ROUTER {
                register_router_queryable(tables, face, &mut res, tables.pid.clone()).await;
            }

            compute_matches_query_routes(tables, &mut res);
        },
        None => log::error!("Declare router queryable for unknown rid {}!", prefixid),
    }
}

async unsafe fn register_client_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    // Register queryable
    {
        let res = Arc::get_mut_unchecked(res);
        log::debug!("Register queryable {} for face {}", res.name(), face.id);
        match res.contexts.get_mut(&face.id) {
            Some(mut ctx) => Arc::get_mut_unchecked(&mut ctx).qabl = true,
            None => {
                res.contexts.insert(
                    face.id,
                    Arc::new(Context {
                        face: face.clone(),
                        local_rid: None,
                        remote_rid: None,
                        subs: None,
                        qabl: true,
                        last_values: HashMap::new(),
                    }),
                );
            }
        }
    }
    Arc::get_mut_unchecked(face).remote_qabls.push(res.clone());
}

pub async fn declare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(&mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);

            register_client_queryable(tables, face, &mut res).await;
            match tables.whatami {
                whatami::ROUTER => {
                    register_router_queryable(tables, face, &mut res, tables.pid.clone()).await;
                }
                whatami::PEER => {
                    register_peer_queryable(tables, face, &mut res, tables.pid.clone()).await;
                }
                _ => {
                    propagate_simple_queryable(tables, face, &res).await;
                }
            }

            compute_matches_query_routes(tables, &mut res);
        },
        None => log::error!("Declare queryable for unknown rid {}!", prefixid),
    }
}

async unsafe fn propagate_forget_simple_queryable(tables: &mut Tables, res: &mut Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face.local_qabls.contains(res) {
            let reskey = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_queryable(&reskey, None).await;

            Arc::get_mut_unchecked(face)
                .local_qabls
                .retain(|qabl| qabl != res);
        }
    }
}

async fn propagate_forget_sourced_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &PeerId,
    net_type: whatami::Type,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            for child in &net.trees[tree_sid.index()].childs {
                match tables.get_face(&net.graph[*child].pid).cloned() {
                    Some(mut someface) => {
                        if src_face.is_none() || someface.id != src_face.unwrap().id {
                            let reskey = Resource::decl_key(res, &mut someface).await;

                            log::debug!(
                                "Send forget {} queryable {} on face {} {}",
                                match net_type {
                                    whatami::ROUTER => "router",
                                    _ => "peer",
                                },
                                res.name(),
                                someface.id,
                                someface.pid,
                            );

                            someface
                                .primitives
                                .forget_queryable(&reskey, Some(tree_sid.index() as ZInt))
                                .await;
                        }
                    }
                    None => {
                        log::trace!("Unable to find face for pid {}", net.graph[*child].pid)
                    }
                }
            }
        }
        None => log::error!(
            "Error propagating qabl {}: cannot get index of {}!",
            res.name(),
            source
        ),
    }
}

async unsafe fn unregister_router_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    router: &PeerId,
) {
    log::debug!(
        "Unregister router queryable {} (router: {})",
        res.name(),
        router
    );
    Arc::get_mut_unchecked(res)
        .router_qabls
        .retain(|qabl| qabl != router);

    if res.router_qabls.is_empty() {
        tables.router_qabls.retain(|qabl| !Arc::ptr_eq(qabl, &res));

        undeclare_peer_queryable(tables, None, res, &tables.pid.clone()).await;
        propagate_forget_simple_queryable(tables, res).await;
    }
}

async unsafe fn undeclare_router_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &PeerId,
) {
    if res.router_qabls.contains(router) {
        unregister_router_queryable(tables, res, router).await;
        propagate_forget_sourced_queryable(tables, res, face, router, whatami::ROUTER).await;
    }
}

pub async fn forget_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    router: &PeerId,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                undeclare_router_queryable(tables, Some(face), &mut res, router).await;
                Resource::clean(&mut res)
            },
            None => log::error!("Undeclare unknown router queryable!"),
        },
        None => log::error!("Undeclare router queryable with unknown prefix!"),
    }
}

async unsafe fn unregister_peer_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    peer: &PeerId,
) {
    log::debug!("Unregister peer queryable {} (peer: {})", res.name(), peer);
    Arc::get_mut_unchecked(res)
        .peer_qabls
        .retain(|qabl| qabl != peer);

    if res.peer_qabls.is_empty() {
        tables.peer_qabls.retain(|qabl| !Arc::ptr_eq(qabl, &res));
    }
}

async unsafe fn undeclare_peer_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &PeerId,
) {
    if res.peer_qabls.contains(&peer) {
        unregister_peer_queryable(tables, res, peer).await;
        propagate_forget_sourced_queryable(tables, res, face, peer, whatami::PEER).await;
    }
}

pub async fn forget_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    peer: &PeerId,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                undeclare_peer_queryable(tables, Some(face), &mut res, peer).await;

                if tables.whatami == whatami::ROUTER
                    && !res.contexts.values().any(|ctx| ctx.qabl)
                    && !tables
                        .peer_qabls
                        .iter()
                        .any(|res| res.peer_qabls.iter().any(|peer| peer != &tables.pid))
                {
                    undeclare_router_queryable(tables, None, &mut res, &tables.pid.clone()).await;
                }

                Resource::clean(&mut res)
            },
            None => log::error!("Undeclare unknown peer queryable!"),
        },
        None => log::error!("Undeclare peer queryable with unknown prefix!"),
    }
}

pub(crate) async unsafe fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!(
        "Unregister client queryable {} for face {}",
        res.name(),
        face.id
    );
    if let Some(mut ctx) = Arc::get_mut_unchecked(res).contexts.get_mut(&face.id) {
        Arc::get_mut_unchecked(&mut ctx).qabl = false;
    }
    Arc::get_mut_unchecked(face)
        .remote_qabls
        .retain(|x| !Arc::ptr_eq(&x, &res));

    match tables.whatami {
        whatami::ROUTER => {
            if !res.contexts.values().any(|ctx| ctx.qabl)
                && !tables
                    .peer_qabls
                    .iter()
                    .any(|res| res.peer_qabls.iter().any(|peer| *peer != tables.pid))
            {
                undeclare_router_queryable(tables, None, res, &tables.pid.clone()).await;
            }
        }
        whatami::PEER => {
            if !res.contexts.values().any(|ctx| ctx.qabl)
                && !tables
                    .peer_qabls
                    .iter()
                    .any(|res| res.peer_qabls.iter().any(|peer| *peer != tables.pid))
            {
                undeclare_peer_queryable(tables, None, res, &tables.pid.clone()).await;
            }
        }
        _ => {
            if !res.contexts.values().any(|ctx| ctx.qabl) {
                propagate_forget_simple_queryable(tables, res).await;
            }
        }
    }

    let mut client_qabls: Vec<Arc<FaceState>> = res
        .contexts
        .values()
        .filter_map(|ctx| {
            if ctx.qabl {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect();
    if client_qabls.len() == 1
        && !tables
            .router_qabls
            .iter()
            .any(|res| res.peer_qabls.iter().any(|peer| *peer != tables.pid))
        && !tables
            .peer_qabls
            .iter()
            .any(|res| res.peer_qabls.iter().any(|peer| *peer != tables.pid))
    {
        let face = &mut client_qabls[0];
        if face.local_qabls.contains(&res) {
            let reskey = Resource::get_best_key(&res, "", face.id);
            face.primitives.forget_queryable(&reskey, None).await;

            Arc::get_mut_unchecked(face)
                .local_qabls
                .retain(|qabl| qabl != &(*res));
        }
    }

    Resource::clean(res)
}

pub async fn forget_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                undeclare_client_queryable(tables, face, &mut res).await;
            },
            None => log::error!("Undeclare unknown queryable!"),
        },
        None => log::error!("Undeclare queryable with unknown prefix!"),
    }
}

pub(crate) async fn queries_new_client_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    for qabl in &tables.router_qabls {
        unsafe {
            Arc::get_mut_unchecked(face).local_qabls.push(qabl.clone());
            let reskey = Resource::decl_key(&qabl, face).await;
            face.primitives.queryable(&reskey, None).await;
        }
    }
}

pub(crate) async fn queries_remove_node(
    tables: &mut Tables,
    node: &PeerId,
    net_type: whatami::Type,
) {
    match net_type {
        whatami::ROUTER => unsafe {
            for mut res in tables
                .router_qabls
                .iter()
                .filter(|res| res.router_qabls.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_router_queryable(tables, &mut res, node).await;
                Resource::clean(&mut res)
            }
        },
        whatami::PEER => unsafe {
            for mut res in tables
                .peer_qabls
                .iter()
                .filter(|res| res.peer_qabls.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_peer_queryable(tables, &mut res, node).await;
                Resource::clean(&mut res)
            }
        },
        _ => (),
    }
}

pub(crate) async fn queries_tree_change(
    tables: &mut Tables,
    new_childs: &[Vec<NodeIndex>],
    net_type: whatami::Type,
) {
    // propagate qabls to new childs
    for (tree_sid, tree_childs) in new_childs.iter().enumerate() {
        if !tree_childs.is_empty() {
            let net = tables.get_net(net_type).unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].pid.clone();

                let qabls_res = match net_type {
                    whatami::ROUTER => &tables.router_qabls,
                    _ => &tables.peer_qabls,
                };

                for res in qabls_res {
                    let qabls = match net_type {
                        whatami::ROUTER => &res.router_qabls,
                        _ => &res.peer_qabls,
                    };
                    for qabl in qabls {
                        if *qabl == tree_id {
                            for child in tree_childs {
                                match tables.get_face(&net.graph[*child].pid).cloned() {
                                    Some(mut face) => {
                                        let reskey = Resource::decl_key(&res, &mut face).await;
                                        log::debug!(
                                            "Send {} queryable {} on face {} {} (new_child)",
                                            net_type,
                                            res.name(),
                                            face.id,
                                            face.pid,
                                        );
                                        face.primitives
                                            .queryable(&reskey, Some(tree_sid as ZInt))
                                            .await;
                                    }
                                    None => {
                                        log::trace!(
                                            "Unable to find face for pid {}",
                                            net.graph[*child].pid
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // recompute routes
    unsafe {
        compute_query_routes_from(tables, &mut tables.root_res.clone());
    }
}

#[inline]
fn insert_faces_for_qabls(
    route: &mut Route,
    prefix: &Arc<Resource>,
    suffix: &str,
    tables: &Tables,
    net: &Network,
    source: usize,
    qabls: &HashSet<PeerId>,
) {
    if net.trees.len() > source {
        for qabl in qabls {
            if let Some(qabl_idx) = net.get_idx(qabl) {
                if net.trees[source].directions.len() > qabl_idx.index() {
                    if let Some(direction) = net.trees[source].directions[qabl_idx.index()] {
                        if net.graph.contains_node(direction) {
                            if let Some(face) = tables.get_face(&net.graph[direction].pid) {
                                route.entry(face.id).or_insert_with(|| {
                                    let reskey = Resource::get_best_key(prefix, suffix, face.id);
                                    (face.clone(), reskey, Some(source as u64))
                                });
                            }
                        }
                    }
                }
            }
        }
    } else {
        log::trace!("Tree for node {} not yet ready", source);
    }
}

unsafe fn compute_query_route(
    tables: &Tables,
    prefix: &Arc<Resource>,
    suffix: &str,
    source: Option<usize>,
    source_type: whatami::Type,
) -> Route {
    let mut route = HashMap::new();
    let resname = [&prefix.name(), suffix].concat();
    let res = Resource::get_resource(prefix, suffix);
    let matches = match res.as_ref() {
        Some(res) => std::borrow::Cow::from(&res.matches),
        None => std::borrow::Cow::from(Resource::get_matches(tables, &resname)),
    };

    for mres in matches.iter() {
        let mut mres = mres.upgrade().unwrap();
        let mres = Arc::get_mut_unchecked(&mut mres);
        if tables.whatami == whatami::ROUTER {
            let net = tables.routers_net.as_ref().unwrap();
            let router_source = match source_type {
                whatami::ROUTER => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_qabls(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                router_source,
                &mres.router_qabls,
            );
            let net = tables.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                whatami::PEER => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_qabls(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                peer_source,
                &mres.peer_qabls,
            );
        }

        if tables.whatami == whatami::PEER {
            let net = tables.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                whatami::ROUTER | whatami::PEER => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_qabls(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                peer_source,
                &mres.peer_qabls,
            );
        }

        for (sid, context) in &mut mres.contexts {
            if context.qabl {
                route.entry(*sid).or_insert_with(|| {
                    let reskey = Resource::get_best_key(prefix, suffix, *sid);
                    (context.face.clone(), reskey, None)
                });
            }
        }
    }
    route
}

unsafe fn compute_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    let mut res_mut = res.clone();
    let res_mut = Arc::get_mut_unchecked(&mut res_mut);
    if tables.whatami == whatami::ROUTER {
        let indexes = tables
            .routers_net
            .as_ref()
            .unwrap()
            .graph
            .node_indices()
            .collect::<Vec<NodeIndex>>();
        let max_idx = indexes.iter().max().unwrap();
        res_mut.routers_query_routes.clear();
        res_mut
            .routers_query_routes
            .resize_with(max_idx.index() + 1, HashMap::new);

        for idx in &indexes {
            res_mut.routers_query_routes[idx.index()] =
                compute_query_route(tables, res, "", Some(idx.index()), whatami::ROUTER);
        }
    }
    if tables.whatami == whatami::ROUTER || tables.whatami == whatami::PEER {
        let indexes = tables
            .peers_net
            .as_ref()
            .unwrap()
            .graph
            .node_indices()
            .collect::<Vec<NodeIndex>>();
        let max_idx = indexes.iter().max().unwrap();
        res_mut.peers_query_routes.clear();
        res_mut
            .peers_query_routes
            .resize_with(max_idx.index() + 1, HashMap::new);

        for idx in &indexes {
            res_mut.peers_query_routes[idx.index()] =
                compute_query_route(tables, res, "", Some(idx.index()), whatami::PEER);
        }
    }
    if tables.whatami == whatami::CLIENT {
        res_mut.client_query_route =
            Some(compute_query_route(tables, res, "", None, whatami::CLIENT));
    }
}

unsafe fn compute_query_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_query_routes(tables, res);
    let res = Arc::get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_query_routes_from(tables, child);
    }
}

pub(crate) unsafe fn compute_matches_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_query_routes(tables, res);

    let resclone = res.clone();
    for match_ in &mut Arc::get_mut_unchecked(res).matches {
        if !Arc::ptr_eq(&match_.upgrade().unwrap(), &resclone) {
            compute_query_routes(tables, &mut match_.upgrade().unwrap());
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn route_query(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    rid: ZInt,
    suffix: &str,
    predicate: &str,
    qid: ZInt,
    target: QueryTarget,
    consolidation: QueryConsolidation,
    routing_context: Option<RoutingContext>,
) {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => unsafe {
            log::debug!(
                "Route query {}:{} for res {}{}",
                face.id,
                qid,
                prefix.name(),
                suffix,
            );

            let route = match tables.whatami {
                whatami::ROUTER => match face.whatami {
                    whatami::ROUTER => {
                        let routers_net = tables.routers_net.as_ref().unwrap();
                        let local_context = routers_net
                            .get_idx(
                                &routers_net
                                    .get_link(&face.pid)
                                    .unwrap()
                                    .mappings
                                    .get(&routing_context.unwrap())
                                    .unwrap(),
                            )
                            .unwrap()
                            .index();
                        match Resource::get_resource(prefix, suffix) {
                            Some(res) => res.routers_query_routes[local_context].clone(),
                            None => compute_query_route(
                                tables,
                                prefix,
                                suffix,
                                Some(local_context),
                                whatami::ROUTER,
                            ),
                        }
                    }
                    whatami::PEER => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context = peers_net
                            .get_idx(
                                &peers_net
                                    .get_link(&face.pid)
                                    .unwrap()
                                    .mappings
                                    .get(&routing_context.unwrap())
                                    .unwrap(),
                            )
                            .unwrap()
                            .index();
                        match Resource::get_resource(prefix, suffix) {
                            Some(res) => res.peers_query_routes[local_context].clone(),
                            None => compute_query_route(
                                tables,
                                prefix,
                                suffix,
                                Some(local_context),
                                whatami::PEER,
                            ),
                        }
                    }
                    _ => match Resource::get_resource(prefix, suffix) {
                        Some(res) => res.routers_query_routes[0].clone(),
                        None => compute_query_route(tables, prefix, suffix, None, whatami::CLIENT),
                    },
                },
                whatami::PEER => match face.whatami {
                    whatami::ROUTER | whatami::PEER => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context = peers_net
                            .get_idx(
                                &peers_net
                                    .get_link(&face.pid)
                                    .unwrap()
                                    .mappings
                                    .get(&routing_context.unwrap())
                                    .unwrap(),
                            )
                            .unwrap()
                            .index();
                        match Resource::get_resource(prefix, suffix) {
                            Some(res) => res.peers_query_routes[local_context].clone(),
                            None => compute_query_route(
                                tables,
                                prefix,
                                suffix,
                                Some(local_context),
                                whatami::PEER,
                            ),
                        }
                    }
                    _ => match Resource::get_resource(prefix, suffix) {
                        Some(res) => res.peers_query_routes[0].clone(),
                        None => compute_query_route(tables, prefix, suffix, None, whatami::CLIENT),
                    },
                },
                _ => match Resource::get_resource(prefix, suffix) {
                    Some(res) => match &res.client_query_route {
                        Some(route) => route.clone(),
                        None => compute_query_route(tables, prefix, suffix, None, whatami::CLIENT),
                    },
                    None => compute_query_route(tables, prefix, suffix, None, whatami::CLIENT),
                },
            };

            match route.len() {
                0 => {
                    log::debug!(
                        "Send final reply {}:{} (no matching queryables)",
                        face.id,
                        qid
                    );
                    face.primitives.clone().reply_final(qid).await
                }
                _ => {
                    let query = Arc::new(Query {
                        src_face: face.clone(),
                        src_qid: qid,
                    });

                    for (_id, (mut outface, reskey, context)) in route {
                        if face.id != outface.id {
                            let outface_mut = Arc::get_mut_unchecked(&mut outface);
                            outface_mut.next_qid += 1;
                            let qid = outface_mut.next_qid;
                            outface_mut.pending_queries.insert(qid, query.clone());

                            outface
                                .primitives
                                .query(
                                    &reskey,
                                    predicate,
                                    qid,
                                    target.clone(),
                                    consolidation.clone(),
                                    context,
                                )
                                .await
                        }
                    }
                }
            }
        },
        None => {
            log::error!("Route query with unknown rid {}! Send final reply.", rid);
            face.primitives.clone().reply_final(qid).await
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn route_reply_data(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    qid: ZInt,
    source_kind: ZInt,
    replier_id: PeerId,
    reskey: ResKey,
    info: Option<DataInfo>,
    payload: RBuf,
) {
    match face.pending_queries.get(&qid) {
        Some(query) => {
            query
                .src_face
                .primitives
                .clone()
                .reply_data(
                    query.src_qid,
                    source_kind,
                    replier_id,
                    reskey,
                    info,
                    payload,
                )
                .await;
        }
        None => log::error!("Route reply for unknown query!"),
    }
}

pub(crate) async fn route_reply_final(_tables: &mut Tables, face: &mut Arc<FaceState>, qid: ZInt) {
    match face.pending_queries.get(&qid) {
        Some(query) => unsafe {
            log::debug!(
                "Received final reply {}:{} from face {}",
                query.src_face.id,
                qid,
                face.id
            );
            if Arc::strong_count(&query) == 1 {
                log::debug!("Propagate final reply {}:{}", query.src_face.id, qid);
                query
                    .src_face
                    .primitives
                    .clone()
                    .reply_final(query.src_qid)
                    .await;
            }
            Arc::get_mut_unchecked(face).pending_queries.remove(&qid);
        },
        None => log::error!("Route reply for unknown query!"),
    }
}

pub(crate) async fn finalize_pending_queries(_tables: &mut Tables, face: &mut Arc<FaceState>) {
    for query in face.pending_queries.values() {
        log::debug!(
            "Finalize reply {}:{} for closing face {}",
            query.src_face.id,
            query.src_qid,
            face.id
        );
        if Arc::strong_count(&query) == 1 {
            log::debug!(
                "Propagate final reply {}:{}",
                query.src_face.id,
                query.src_qid
            );
            query
                .src_face
                .primitives
                .clone()
                .reply_final(query.src_qid)
                .await;
        }
    }
    unsafe {
        Arc::get_mut_unchecked(face).pending_queries.clear();
    }
}
