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
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use zenoh_util::sync::get_mut_unchecked;

use super::protocol::core::{whatami, PeerId, QueryConsolidation, QueryTarget, ResKey, ZInt};
use super::protocol::io::RBuf;
use super::protocol::proto::{DataInfo, RoutingContext};

use super::face::FaceState;
use super::network::Network;
use super::resource::{elect_router, Resource, Route, SessionContext};
use super::router::Tables;

pub(crate) struct Query {
    src_face: Arc<FaceState>,
    src_qid: ZInt,
}

#[inline]
async fn send_sourced_queryable_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: Option<RoutingContext>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].pid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let reskey = Resource::decl_key(res, &mut someface).await;

                        log::debug!("Send queryable {} on {}", res.name(), someface);

                        someface
                            .primitives
                            .decl_queryable(&reskey, routing_context)
                            .await;
                    }
                }
                None => {
                    log::trace!("Unable to find face for pid {}", net.graph[*child].pid)
                }
            }
        }
    }
}

async fn propagate_simple_queryable(
    tables: &mut Tables,
    res: &Arc<Resource>,
    src_face: &mut Arc<FaceState>,
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
            get_mut_unchecked(dst_face).local_qabls.push(res.clone());
            let reskey = Resource::decl_key(res, dst_face).await;
            dst_face.primitives.decl_queryable(&reskey, None).await;
        }
    }
}

async fn propagate_sourced_queryable(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &PeerId,
    net_type: whatami::Type,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_queryable_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    Some(tree_sid.index() as ZInt),
                )
                .await;
            } else {
                log::trace!("Tree for node {} not yet ready", source);
            }
        }
        None => log::error!(
            "Error propagating qabl {}: cannot get index of {}!",
            res.name(),
            source
        ),
    }
}

async fn register_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    router: PeerId,
) {
    if !res.context().router_qabls.contains(&router) {
        // Register router queryable
        {
            log::debug!(
                "Register router queryable {} (router: {})",
                res.name(),
                router
            );
            get_mut_unchecked(res)
                .context_mut()
                .router_qabls
                .insert(router.clone());
            tables.router_qabls.insert(res.clone());
        }

        // Propagate queryable to routers
        propagate_sourced_queryable(tables, res, Some(face), &router, whatami::ROUTER).await;

        // Propagate queryable to peers
        if face.whatami != whatami::PEER {
            register_peer_queryable(tables, face, res, tables.pid.clone()).await
        }
    }

    // Propagate queryable to clients
    propagate_simple_queryable(tables, res, face).await;
}

pub async fn declare_router_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    router: PeerId,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);
            register_router_queryable(tables, face, &mut res, router).await;

            compute_matches_query_routes(tables, &mut res);
        }
        None => log::error!("Declare router queryable for unknown rid {}!", prefixid),
    }
}

async fn register_peer_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    peer: PeerId,
) {
    if !res.context().peer_qabls.contains(&peer) {
        // Register peer queryable
        {
            log::debug!("Register peer queryable {} (peer: {})", res.name(), peer);
            get_mut_unchecked(res)
                .context_mut()
                .peer_qabls
                .insert(peer.clone());
            tables.peer_qabls.insert(res.clone());
        }

        // Propagate queryable to peers
        propagate_sourced_queryable(tables, res, Some(face), &peer, whatami::PEER).await;
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
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);
            register_peer_queryable(tables, face, &mut res, peer).await;

            if tables.whatami == whatami::ROUTER {
                register_router_queryable(tables, face, &mut res, tables.pid.clone()).await;
            }

            compute_matches_query_routes(tables, &mut res);
        }
        None => log::error!("Declare router queryable for unknown rid {}!", prefixid),
    }
}

async fn register_client_queryable(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    // Register queryable
    {
        let res = get_mut_unchecked(res);
        log::debug!("Register queryable {} for {}", res.name(), face);
        match res.session_ctxs.get_mut(&face.id) {
            Some(mut ctx) => get_mut_unchecked(&mut ctx).qabl = true,
            None => {
                res.session_ctxs.insert(
                    face.id,
                    Arc::new(SessionContext {
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
    get_mut_unchecked(face).remote_qabls.push(res.clone());
}

pub async fn declare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => {
            let mut res = Resource::make_resource(tables, &mut prefix, suffix);
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
                    propagate_simple_queryable(tables, &res, face).await;
                }
            }

            compute_matches_query_routes(tables, &mut res);
        }
        None => log::error!("Declare queryable for unknown rid {}!", prefixid),
    }
}

#[inline]
async fn send_forget_sourced_queryable_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    routing_context: Option<RoutingContext>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].pid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let reskey = Resource::decl_key(res, &mut someface).await;

                        log::debug!("Send forget queryable {} on {}", res.name(), someface);

                        someface
                            .primitives
                            .forget_queryable(&reskey, routing_context)
                            .await;
                    }
                }
                None => {
                    log::trace!("Unable to find face for pid {}", net.graph[*child].pid)
                }
            }
        }
    }
}

async fn propagate_forget_simple_queryable(tables: &mut Tables, res: &mut Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face.local_qabls.contains(res) {
            let reskey = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_queryable(&reskey, None).await;

            get_mut_unchecked(face)
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
            send_forget_sourced_queryable_to_net_childs(
                tables,
                net,
                &net.trees[tree_sid.index()].childs,
                res,
                src_face,
                Some(tree_sid.index() as ZInt),
            )
            .await;
        }
        None => log::error!(
            "Error propagating qabl {}: cannot get index of {}!",
            res.name(),
            source
        ),
    }
}

async fn unregister_router_queryable(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    router: &PeerId,
) {
    log::debug!(
        "Unregister router queryable {} (router: {})",
        res.name(),
        router
    );
    get_mut_unchecked(res)
        .context_mut()
        .router_qabls
        .retain(|qabl| qabl != router);

    if res.context().router_qabls.is_empty() {
        tables.router_qabls.retain(|qabl| !Arc::ptr_eq(qabl, &res));

        undeclare_peer_queryable(tables, None, res, &tables.pid.clone()).await;
        propagate_forget_simple_queryable(tables, res).await;
    }
}

async fn undeclare_router_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &PeerId,
) {
    if res.context().router_qabls.contains(router) {
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
            Some(mut res) => {
                undeclare_router_queryable(tables, Some(face), &mut res, router).await;
                Resource::clean(&mut res)
            }
            None => log::error!("Undeclare unknown router queryable!"),
        },
        None => log::error!("Undeclare router queryable with unknown prefix!"),
    }
}

async fn unregister_peer_queryable(tables: &mut Tables, res: &mut Arc<Resource>, peer: &PeerId) {
    log::debug!("Unregister peer queryable {} (peer: {})", res.name(), peer);
    get_mut_unchecked(res)
        .context_mut()
        .peer_qabls
        .retain(|qabl| qabl != peer);

    if res.context().peer_qabls.is_empty() {
        tables.peer_qabls.retain(|qabl| !Arc::ptr_eq(qabl, &res));
    }
}

async fn undeclare_peer_queryable(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &PeerId,
) {
    if res.context().peer_qabls.contains(&peer) {
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
            Some(mut res) => {
                undeclare_peer_queryable(tables, Some(face), &mut res, peer).await;

                if tables.whatami == whatami::ROUTER
                    && !res.session_ctxs.values().any(|ctx| ctx.qabl)
                    && !tables.peer_qabls.iter().any(|res| {
                        res.context()
                            .peer_qabls
                            .iter()
                            .any(|peer| peer != &tables.pid)
                    })
                {
                    undeclare_router_queryable(tables, None, &mut res, &tables.pid.clone()).await;
                }

                Resource::clean(&mut res)
            }
            None => log::error!("Undeclare unknown peer queryable!"),
        },
        None => log::error!("Undeclare peer queryable with unknown prefix!"),
    }
}

pub(crate) async fn undeclare_client_queryable(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!("Unregister client queryable {} for  {}", res.name(), face);
    if let Some(mut ctx) = get_mut_unchecked(res).session_ctxs.get_mut(&face.id) {
        get_mut_unchecked(&mut ctx).qabl = false;
    }
    get_mut_unchecked(face)
        .remote_qabls
        .retain(|x| !Arc::ptr_eq(&x, &res));

    match tables.whatami {
        whatami::ROUTER => {
            if !res.session_ctxs.values().any(|ctx| ctx.qabl)
                && !tables.peer_qabls.iter().any(|res| {
                    res.context()
                        .peer_qabls
                        .iter()
                        .any(|peer| *peer != tables.pid)
                })
            {
                undeclare_router_queryable(tables, None, res, &tables.pid.clone()).await;
            }
        }
        whatami::PEER => {
            if !res.session_ctxs.values().any(|ctx| ctx.qabl)
                && !tables.peer_qabls.iter().any(|res| {
                    res.context()
                        .peer_qabls
                        .iter()
                        .any(|peer| *peer != tables.pid)
                })
            {
                undeclare_peer_queryable(tables, None, res, &tables.pid.clone()).await;
            }
        }
        _ => {
            if !res.session_ctxs.values().any(|ctx| ctx.qabl) {
                propagate_forget_simple_queryable(tables, res).await;
            }
        }
    }

    let mut client_qabls: Vec<Arc<FaceState>> = res
        .session_ctxs
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
        && !tables.router_qabls.iter().any(|res| {
            res.context()
                .peer_qabls
                .iter()
                .any(|peer| *peer != tables.pid)
        })
        && !tables.peer_qabls.iter().any(|res| {
            res.context()
                .peer_qabls
                .iter()
                .any(|peer| *peer != tables.pid)
        })
    {
        let face = &mut client_qabls[0];
        if face.local_qabls.contains(&res) {
            let reskey = Resource::get_best_key(&res, "", face.id);
            face.primitives.forget_queryable(&reskey, None).await;

            get_mut_unchecked(face)
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
            Some(mut res) => {
                undeclare_client_queryable(tables, face, &mut res).await;
            }
            None => log::error!("Undeclare unknown queryable!"),
        },
        None => log::error!("Undeclare queryable with unknown prefix!"),
    }
}

pub(crate) async fn queries_new_client_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    for qabl in &tables.router_qabls {
        get_mut_unchecked(face).local_qabls.push(qabl.clone());
        let reskey = Resource::decl_key(&qabl, face).await;
        face.primitives.decl_queryable(&reskey, None).await;
    }
}

pub(crate) async fn queries_remove_node(
    tables: &mut Tables,
    node: &PeerId,
    net_type: whatami::Type,
) {
    match net_type {
        whatami::ROUTER => {
            for mut res in tables
                .router_qabls
                .iter()
                .filter(|res| res.context().router_qabls.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_router_queryable(tables, &mut res, node).await;
                Resource::clean(&mut res)
            }
        }
        whatami::PEER => {
            for mut res in tables
                .peer_qabls
                .iter()
                .filter(|res| res.context().peer_qabls.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_peer_queryable(tables, &mut res, node).await;

                if tables.whatami == whatami::ROUTER
                    && !res.session_ctxs.values().any(|ctx| ctx.qabl)
                    && !tables.peer_qabls.iter().any(|res| {
                        res.context()
                            .peer_qabls
                            .iter()
                            .any(|peer| peer != &tables.pid)
                    })
                {
                    undeclare_router_queryable(tables, None, &mut res, &tables.pid.clone()).await;
                }

                Resource::clean(&mut res)
            }
        }
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
                        whatami::ROUTER => &res.context().router_qabls,
                        _ => &res.context().peer_qabls,
                    };
                    for qabl in qabls {
                        if *qabl == tree_id {
                            send_sourced_queryable_to_net_childs(
                                tables,
                                net,
                                tree_childs,
                                res,
                                None,
                                Some(tree_sid as ZInt),
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }

    // recompute routes
    compute_query_routes_from(tables, &mut tables.root_res.clone());
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

fn compute_query_route(
    tables: &Tables,
    prefix: &Arc<Resource>,
    suffix: &str,
    source: Option<usize>,
    source_type: whatami::Type,
) -> Arc<Route> {
    let mut route = HashMap::new();
    let res_name = [&prefix.name(), suffix].concat();
    let res = Resource::get_resource(prefix, suffix);
    let matches = res
        .as_ref()
        .map(|res| res.context.as_ref())
        .flatten()
        .map(|ctx| Cow::from(&ctx.matches))
        .unwrap_or_else(|| Cow::from(Resource::get_matches(tables, &res_name)));

    let master = tables.whatami != whatami::ROUTER
        || *elect_router(&res_name, &tables.shared_nodes) == tables.pid;

    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        if tables.whatami == whatami::ROUTER {
            if master || source_type == whatami::ROUTER {
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
                    &mres.context().router_qabls,
                );
            }

            if master || source_type != whatami::ROUTER {
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
                    &mres.context().peer_qabls,
                );
            }
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
                &mres.context().peer_qabls,
            );
        }

        if tables.whatami != whatami::ROUTER || master || source_type == whatami::ROUTER {
            for (sid, context) in &mres.session_ctxs {
                if context.qabl {
                    route.entry(*sid).or_insert_with(|| {
                        let reskey = Resource::get_best_key(prefix, suffix, *sid);
                        (context.face.clone(), reskey, None)
                    });
                }
            }
        }
    }
    Arc::new(route)
}

pub(crate) fn compute_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        let mut res_mut = res.clone();
        let res_mut = get_mut_unchecked(&mut res_mut);
        if tables.whatami == whatami::ROUTER {
            let indexes = tables
                .routers_net
                .as_ref()
                .unwrap()
                .graph
                .node_indices()
                .collect::<Vec<NodeIndex>>();
            let max_idx = indexes.iter().max().unwrap();
            let routers_query_routes = &mut res_mut.context_mut().routers_query_routes;
            routers_query_routes.clear();
            routers_query_routes.resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

            for idx in &indexes {
                routers_query_routes[idx.index()] =
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
            let peers_query_routes = &mut res_mut.context_mut().peers_query_routes;
            peers_query_routes.clear();
            peers_query_routes.resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

            for idx in &indexes {
                peers_query_routes[idx.index()] =
                    compute_query_route(tables, res, "", Some(idx.index()), whatami::PEER);
            }
        }
        if tables.whatami == whatami::CLIENT {
            res_mut.context_mut().client_query_route =
                Some(compute_query_route(tables, res, "", None, whatami::CLIENT));
        }
    }
}

fn compute_query_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_query_routes(tables, res);
    let res = get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_query_routes_from(tables, child);
    }
}

pub(crate) fn compute_matches_query_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    if res.context.is_some() {
        compute_query_routes(tables, res);

        let resclone = res.clone();
        for match_ in &mut get_mut_unchecked(res).context_mut().matches {
            if !Arc::ptr_eq(&match_.upgrade().unwrap(), &resclone) {
                compute_query_routes(tables, &mut match_.upgrade().unwrap());
            }
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
        Some(prefix) => {
            log::debug!(
                "Route query {}:{} for res {}{}",
                face,
                qid,
                prefix.name(),
                suffix,
            );

            let route = match tables.whatami {
                whatami::ROUTER => match face.whatami {
                    whatami::ROUTER => {
                        let routers_net = tables.routers_net.as_ref().unwrap();
                        let local_context =
                            routers_net.get_local_context(routing_context.unwrap(), face.link_id);
                        Resource::get_resource(prefix, suffix)
                            .map(|res| res.routers_query_route(local_context))
                            .flatten()
                            .unwrap_or_else(|| {
                                compute_query_route(
                                    tables,
                                    prefix,
                                    suffix,
                                    Some(local_context),
                                    whatami::ROUTER,
                                )
                            })
                    }
                    whatami::PEER => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context =
                            peers_net.get_local_context(routing_context.unwrap(), face.link_id);
                        Resource::get_resource(prefix, suffix)
                            .map(|res| res.peers_query_route(local_context))
                            .flatten()
                            .unwrap_or_else(|| {
                                compute_query_route(
                                    tables,
                                    prefix,
                                    suffix,
                                    Some(local_context),
                                    whatami::PEER,
                                )
                            })
                    }
                    _ => Resource::get_resource(prefix, suffix)
                        .map(|res| res.routers_query_route(0))
                        .flatten()
                        .unwrap_or_else(|| {
                            compute_query_route(tables, prefix, suffix, None, whatami::CLIENT)
                        }),
                },
                whatami::PEER => match face.whatami {
                    whatami::ROUTER | whatami::PEER => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context =
                            peers_net.get_local_context(routing_context.unwrap(), face.link_id);
                        Resource::get_resource(prefix, suffix)
                            .map(|res| res.peers_query_route(local_context))
                            .flatten()
                            .unwrap_or_else(|| {
                                compute_query_route(
                                    tables,
                                    prefix,
                                    suffix,
                                    Some(local_context),
                                    whatami::PEER,
                                )
                            })
                    }
                    _ => Resource::get_resource(prefix, suffix)
                        .map(|res| res.peers_query_route(0))
                        .flatten()
                        .unwrap_or_else(|| {
                            compute_query_route(tables, prefix, suffix, None, whatami::CLIENT)
                        }),
                },
                _ => Resource::get_resource(prefix, suffix)
                    .map(|res| res.client_query_route())
                    .flatten()
                    .unwrap_or_else(|| {
                        compute_query_route(tables, prefix, suffix, None, whatami::CLIENT)
                    }),
            };

            if route.is_empty()
                || (route.len() == 1 && route.iter().next().unwrap().1 .0.id == face.id)
            {
                log::debug!("Send final reply {}:{} (no matching queryables)", face, qid);
                face.primitives.clone().send_reply_final(qid).await
            } else {
                let query = Arc::new(Query {
                    src_face: face.clone(),
                    src_qid: qid,
                });

                for (outface, reskey, context) in route.values() {
                    if face.id != outface.id {
                        let mut outface = outface.clone();
                        let outface_mut = get_mut_unchecked(&mut outface);
                        outface_mut.next_qid += 1;
                        let qid = outface_mut.next_qid;
                        outface_mut.pending_queries.insert(qid, query.clone());

                        log::trace!("Propagate query {}:{} to {}", query.src_face, qid, outface);

                        outface
                            .primitives
                            .send_query(
                                &reskey,
                                predicate,
                                qid,
                                target.clone(),
                                consolidation.clone(),
                                *context,
                            )
                            .await
                    }
                }
            }
        }
        None => {
            log::error!("Route query with unknown rid {}! Send final reply.", rid);
            face.primitives.clone().send_reply_final(qid).await
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn route_send_reply_data(
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
                .send_reply_data(
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

pub(crate) async fn route_send_reply_final(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    qid: ZInt,
) {
    match face.pending_queries.get(&qid) {
        Some(query) => {
            log::debug!(
                "Received final reply {}:{} from {}",
                query.src_face,
                qid,
                face
            );
            if Arc::strong_count(&query) == 1 {
                log::debug!("Propagate final reply {}:{}", query.src_face, qid);
                query
                    .src_face
                    .primitives
                    .clone()
                    .send_reply_final(query.src_qid)
                    .await;
            }
            get_mut_unchecked(face).pending_queries.remove(&qid);
        }
        None => log::error!("Route reply for unknown query!"),
    }
}

pub(crate) async fn finalize_pending_queries(_tables: &mut Tables, face: &mut Arc<FaceState>) {
    for query in face.pending_queries.values() {
        log::debug!(
            "Finalize reply {}:{} for closing {}",
            query.src_face,
            query.src_qid,
            face
        );
        if Arc::strong_count(&query) == 1 {
            log::debug!("Propagate final reply {}:{}", query.src_face, query.src_qid);
            query
                .src_face
                .primitives
                .clone()
                .send_reply_final(query.src_qid)
                .await;
        }
    }
    get_mut_unchecked(face).pending_queries.clear();
}
