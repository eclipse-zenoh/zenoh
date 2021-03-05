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
use uhlc::HLC;

use super::protocol::core::{
    whatami, CongestionControl, PeerId, Reliability, SubInfo, SubMode, ZInt,
};
use super::protocol::io::RBuf;
use super::protocol::proto::{DataInfo, RoutingContext};

use super::face::FaceState;
use super::network::Network;
use super::resource::{Context, PullCaches, Resource, Route};
use super::router::Tables;

#[inline]
async fn send_sourced_subscription_to_net_childs(
    tables: &Tables,
    net: &Network,
    childs: &[NodeIndex],
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    sub_info: &SubInfo,
    routing_context: Option<RoutingContext>,
) {
    for child in childs {
        if net.graph.contains_node(*child) {
            match tables.get_face(&net.graph[*child].pid).cloned() {
                Some(mut someface) => {
                    if src_face.is_none() || someface.id != src_face.unwrap().id {
                        let reskey = Resource::decl_key(res, &mut someface).await;

                        log::debug!(
                            "Send subscription {} on face {} {}",
                            res.name(),
                            someface.id,
                            someface.pid,
                        );

                        someface
                            .primitives
                            .decl_subscriber(&reskey, sub_info, routing_context)
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

async fn propagate_simple_subscription(
    tables: &mut Tables,
    res: &Arc<Resource>,
    sub_info: &SubInfo,
    src_face: &mut Arc<FaceState>,
) {
    for dst_face in &mut tables.faces.values_mut() {
        if src_face.id != dst_face.id
            && !dst_face.local_subs.contains(res)
            && match tables.whatami {
                whatami::ROUTER => dst_face.whatami == whatami::CLIENT,
                whatami::PEER => dst_face.whatami == whatami::CLIENT,
                _ => (src_face.whatami == whatami::CLIENT || dst_face.whatami == whatami::CLIENT),
            }
        {
            unsafe {
                Arc::get_mut_unchecked(dst_face)
                    .local_subs
                    .push(res.clone());
                let reskey = Resource::decl_key(res, dst_face).await;
                dst_face
                    .primitives
                    .decl_subscriber(&reskey, sub_info, None)
                    .await;
            }
        }
    }
}

async fn propagate_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    sub_info: &SubInfo,
    src_face: Option<&Arc<FaceState>>,
    source: &PeerId,
    net_type: whatami::Type,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            if net.trees.len() > tree_sid.index() {
                send_sourced_subscription_to_net_childs(
                    tables,
                    net,
                    &net.trees[tree_sid.index()].childs,
                    res,
                    src_face,
                    sub_info,
                    Some(tree_sid.index() as ZInt),
                )
                .await;
            } else {
                log::trace!("Tree for node {} not yet ready", source);
            }
        }
        None => log::error!(
            "Error propagating sub {}: cannot get index of {}!",
            res.name(),
            source
        ),
    }
}

async unsafe fn register_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubInfo,
    router: PeerId,
) {
    if !res.router_subs.contains(&router) {
        // Register router subscription
        {
            let res_mut = Arc::get_mut_unchecked(res);
            log::debug!(
                "Register router subscription {} (router: {})",
                res_mut.name(),
                router
            );
            res_mut.router_subs.insert(router.clone());
            tables.router_subs.insert(res.clone());
        }

        // Propagate subscription to routers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &router, whatami::ROUTER)
            .await;

        // Propagate subscription to peers
        if face.whatami != whatami::PEER {
            register_peer_subscription(tables, face, res, sub_info, tables.pid.clone()).await
        }
    }

    // Propagate subscription to clients
    propagate_simple_subscription(tables, res, sub_info, face).await;
}

pub async fn declare_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    sub_info: &SubInfo,
    router: PeerId,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(tables, &mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);
            register_router_subscription(tables, face, &mut res, sub_info, router).await;

            compute_matches_data_routes(tables, &mut res);
        },
        None => log::error!("Declare router subscription for unknown rid {}!", prefixid),
    }
}

async unsafe fn register_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubInfo,
    peer: PeerId,
) {
    if !res.peer_subs.contains(&peer) {
        // Register peer subscription
        {
            let res_mut = Arc::get_mut_unchecked(res);
            log::debug!(
                "Register peer subscription {} (peer: {})",
                res_mut.name(),
                peer
            );
            res_mut.peer_subs.insert(peer.clone());
            tables.peer_subs.insert(res.clone());
        }

        // Propagate subscription to peers
        propagate_sourced_subscription(tables, res, sub_info, Some(face), &peer, whatami::PEER)
            .await;
    }
}

pub async fn declare_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    sub_info: &SubInfo,
    peer: PeerId,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(tables, &mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);
            register_peer_subscription(tables, face, &mut res, sub_info, peer).await;

            if tables.whatami == whatami::ROUTER {
                let mut propa_sub_info = sub_info.clone();
                propa_sub_info.mode = SubMode::Push;
                register_router_subscription(
                    tables,
                    face,
                    &mut res,
                    &propa_sub_info,
                    tables.pid.clone(),
                )
                .await;
            }

            compute_matches_data_routes(tables, &mut res);
        },
        None => log::error!("Declare router subscription for unknown rid {}!", prefixid),
    }
}

async unsafe fn register_client_subscription(
    _tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
    sub_info: &SubInfo,
) {
    // Register subscription
    {
        let res = Arc::get_mut_unchecked(res);
        log::debug!("Register subscription {} for face {}", res.name(), face.id);
        match res.contexts.get_mut(&face.id) {
            Some(mut ctx) => match &ctx.subs {
                Some(info) => {
                    if SubMode::Pull == info.mode {
                        Arc::get_mut_unchecked(&mut ctx).subs = Some(sub_info.clone());
                    }
                }
                None => {
                    Arc::get_mut_unchecked(&mut ctx).subs = Some(sub_info.clone());
                }
            },
            None => {
                res.contexts.insert(
                    face.id,
                    Arc::new(Context {
                        face: face.clone(),
                        local_rid: None,
                        remote_rid: None,
                        subs: Some(sub_info.clone()),
                        qabl: false,
                        last_values: HashMap::new(),
                    }),
                );
            }
        }
    }
    Arc::get_mut_unchecked(face).remote_subs.push(res.clone());
}

pub async fn declare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    sub_info: &SubInfo,
) {
    match tables.get_mapping(&face, &prefixid).cloned() {
        Some(mut prefix) => unsafe {
            let mut res = Resource::make_resource(tables, &mut prefix, suffix);
            Resource::match_resource(&tables, &mut res);

            register_client_subscription(tables, face, &mut res, sub_info).await;
            match tables.whatami {
                whatami::ROUTER => {
                    let mut propa_sub_info = sub_info.clone();
                    propa_sub_info.mode = SubMode::Push;
                    register_router_subscription(
                        tables,
                        face,
                        &mut res,
                        &propa_sub_info,
                        tables.pid.clone(),
                    )
                    .await;
                }
                whatami::PEER => {
                    let mut propa_sub_info = sub_info.clone();
                    propa_sub_info.mode = SubMode::Push;
                    register_peer_subscription(
                        tables,
                        face,
                        &mut res,
                        &propa_sub_info,
                        tables.pid.clone(),
                    )
                    .await;
                }
                _ => {
                    propagate_simple_subscription(tables, &res, sub_info, face).await;
                }
            }

            compute_matches_data_routes(tables, &mut res);
        },
        None => log::error!("Declare subscription for unknown rid {}!", prefixid),
    }
}

#[inline]
async fn send_forget_sourced_subscription_to_net_childs(
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

                        log::debug!(
                            "Send forget subscription {} on face {} {}",
                            res.name(),
                            someface.id,
                            someface.pid,
                        );

                        someface
                            .primitives
                            .forget_subscriber(&reskey, routing_context)
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

async unsafe fn propagate_forget_simple_subscription(tables: &mut Tables, res: &Arc<Resource>) {
    for face in tables.faces.values_mut() {
        if face.local_subs.contains(res) {
            let reskey = Resource::get_best_key(res, "", face.id);
            face.primitives.forget_subscriber(&reskey, None).await;

            Arc::get_mut_unchecked(face)
                .local_subs
                .retain(|sub| sub != res);
        }
    }
}

async fn propagate_forget_sourced_subscription(
    tables: &Tables,
    res: &Arc<Resource>,
    src_face: Option<&Arc<FaceState>>,
    source: &PeerId,
    net_type: whatami::Type,
) {
    let net = tables.get_net(net_type).unwrap();
    match net.get_idx(source) {
        Some(tree_sid) => {
            send_forget_sourced_subscription_to_net_childs(
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
            "Error propagating sub {}: cannot get index of {}!",
            res.name(),
            source
        ),
    }
}

async unsafe fn unregister_router_subscription(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    router: &PeerId,
) {
    log::debug!(
        "Unregister router subscription {} (router: {})",
        res.name(),
        router
    );
    Arc::get_mut_unchecked(res)
        .router_subs
        .retain(|sub| sub != router);

    if res.router_subs.is_empty() {
        tables.router_subs.retain(|sub| !Arc::ptr_eq(sub, &res));

        undeclare_peer_subscription(tables, None, res, &tables.pid.clone()).await;
        propagate_forget_simple_subscription(tables, res).await;
    }
}

async unsafe fn undeclare_router_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    router: &PeerId,
) {
    if res.router_subs.contains(router) {
        unregister_router_subscription(tables, res, router).await;
        propagate_forget_sourced_subscription(tables, res, face, router, whatami::ROUTER).await;
    }
}

pub async fn forget_router_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    router: &PeerId,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                undeclare_router_subscription(tables, Some(face), &mut res, router).await;
                Resource::clean(&mut res)
            },
            None => log::error!("Undeclare unknown router subscription!"),
        },
        None => log::error!("Undeclare router subscription with unknown prefix!"),
    }
}

async unsafe fn unregister_peer_subscription(
    tables: &mut Tables,
    res: &mut Arc<Resource>,
    peer: &PeerId,
) {
    log::debug!(
        "Unregister peer subscription {} (peer: {})",
        res.name(),
        peer
    );
    Arc::get_mut_unchecked(res)
        .peer_subs
        .retain(|sub| sub != peer);

    if res.peer_subs.is_empty() {
        tables.peer_subs.retain(|sub| !Arc::ptr_eq(sub, &res));
    }
}

async unsafe fn undeclare_peer_subscription(
    tables: &mut Tables,
    face: Option<&Arc<FaceState>>,
    res: &mut Arc<Resource>,
    peer: &PeerId,
) {
    if res.peer_subs.contains(&peer) {
        unregister_peer_subscription(tables, res, peer).await;
        propagate_forget_sourced_subscription(tables, res, face, peer, whatami::PEER).await;
    }
}

pub async fn forget_peer_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
    peer: &PeerId,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                undeclare_peer_subscription(tables, Some(face), &mut res, peer).await;

                if tables.whatami == whatami::ROUTER
                    && !res.contexts.values().any(|ctx| ctx.subs.is_some())
                    && !tables
                        .peer_subs
                        .iter()
                        .any(|res| res.peer_subs.iter().any(|peer| peer != &tables.pid))
                {
                    undeclare_router_subscription(tables, None, &mut res, &tables.pid.clone())
                        .await;
                }

                Resource::clean(&mut res)
            },
            None => log::error!("Undeclare unknown peer subscription!"),
        },
        None => log::error!("Undeclare peer subscription with unknown prefix!"),
    }
}

pub(crate) async unsafe fn undeclare_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    res: &mut Arc<Resource>,
) {
    log::debug!(
        "Unregister client subscription {} for face {}",
        res.name(),
        face.id
    );
    if let Some(mut ctx) = Arc::get_mut_unchecked(res).contexts.get_mut(&face.id) {
        Arc::get_mut_unchecked(&mut ctx).subs = None;
    }
    Arc::get_mut_unchecked(face)
        .remote_subs
        .retain(|x| !Arc::ptr_eq(&x, &res));

    match tables.whatami {
        whatami::ROUTER => {
            if !res.contexts.values().any(|ctx| ctx.subs.is_some())
                && !tables
                    .peer_subs
                    .iter()
                    .any(|res| res.peer_subs.iter().any(|peer| *peer != tables.pid))
            {
                undeclare_router_subscription(tables, None, res, &tables.pid.clone()).await;
            }
        }
        whatami::PEER => {
            if !res.contexts.values().any(|ctx| ctx.subs.is_some())
                && !tables
                    .peer_subs
                    .iter()
                    .any(|res| res.peer_subs.iter().any(|peer| *peer != tables.pid))
            {
                undeclare_peer_subscription(tables, None, res, &tables.pid.clone()).await;
            }
        }
        _ => {
            if !res.contexts.values().any(|ctx| ctx.subs.is_some()) {
                propagate_forget_simple_subscription(tables, res).await;
            }
        }
    }

    let mut client_subs: Vec<Arc<FaceState>> = res
        .contexts
        .values()
        .filter_map(|ctx| {
            if ctx.subs.is_some() {
                Some(ctx.face.clone())
            } else {
                None
            }
        })
        .collect();
    if client_subs.len() == 1
        && !tables
            .router_subs
            .iter()
            .any(|res| res.peer_subs.iter().any(|peer| *peer != tables.pid))
        && !tables
            .peer_subs
            .iter()
            .any(|res| res.peer_subs.iter().any(|peer| *peer != tables.pid))
    {
        let face = &mut client_subs[0];
        if face.local_subs.contains(&res) {
            let reskey = Resource::get_best_key(&res, "", face.id);
            face.primitives.forget_subscriber(&reskey, None).await;

            Arc::get_mut_unchecked(face)
                .local_subs
                .retain(|sub| sub != &(*res));
        }
    }

    Resource::clean(res)
}

pub async fn forget_client_subscription(
    tables: &mut Tables,
    face: &mut Arc<FaceState>,
    prefixid: ZInt,
    suffix: &str,
) {
    match tables.get_mapping(&face, &prefixid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                undeclare_client_subscription(tables, face, &mut res).await;
            },
            None => log::error!("Undeclare unknown subscription!"),
        },
        None => log::error!("Undeclare subscription with unknown prefix!"),
    }
}

pub(crate) async fn pubsub_new_client_face(tables: &mut Tables, face: &mut Arc<FaceState>) {
    let sub_info = SubInfo {
        reliability: Reliability::Reliable, // TODO
        mode: SubMode::Push,
        period: None,
    };
    for sub in &tables.router_subs {
        unsafe {
            Arc::get_mut_unchecked(face).local_subs.push(sub.clone());
            let reskey = Resource::decl_key(&sub, face).await;
            face.primitives
                .decl_subscriber(&reskey, &sub_info, None)
                .await;
        }
    }
}

pub(crate) async fn pubsub_remove_node(
    tables: &mut Tables,
    node: &PeerId,
    net_type: whatami::Type,
) {
    match net_type {
        whatami::ROUTER => unsafe {
            for mut res in tables
                .router_subs
                .iter()
                .filter(|res| res.router_subs.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_router_subscription(tables, &mut res, node).await;
                Resource::clean(&mut res)
            }
        },
        whatami::PEER => unsafe {
            for mut res in tables
                .peer_subs
                .iter()
                .filter(|res| res.peer_subs.contains(node))
                .cloned()
                .collect::<Vec<Arc<Resource>>>()
            {
                unregister_peer_subscription(tables, &mut res, node).await;
                Resource::clean(&mut res)
            }
        },
        _ => (),
    }
}

pub(crate) async fn pubsub_tree_change(
    tables: &mut Tables,
    new_childs: &[Vec<NodeIndex>],
    net_type: whatami::Type,
) {
    // propagate subs to new childs
    for (tree_sid, tree_childs) in new_childs.iter().enumerate() {
        if !tree_childs.is_empty() {
            let net = tables.get_net(net_type).unwrap();
            let tree_idx = NodeIndex::new(tree_sid);
            if net.graph.contains_node(tree_idx) {
                let tree_id = net.graph[tree_idx].pid.clone();

                let subs_res = match net_type {
                    whatami::ROUTER => &tables.router_subs,
                    _ => &tables.peer_subs,
                };

                for res in subs_res {
                    let subs = match net_type {
                        whatami::ROUTER => &res.router_subs,
                        _ => &res.peer_subs,
                    };
                    for sub in subs {
                        if *sub == tree_id {
                            let sub_info = SubInfo {
                                reliability: Reliability::Reliable, // TODO
                                mode: SubMode::Push,
                                period: None,
                            };
                            send_sourced_subscription_to_net_childs(
                                tables,
                                net,
                                tree_childs,
                                res,
                                None,
                                &sub_info,
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
    unsafe {
        compute_data_routes_from(tables, &mut tables.root_res.clone());
    }
}

#[inline]
fn insert_faces_for_subs(
    route: &mut Route,
    prefix: &Arc<Resource>,
    suffix: &str,
    tables: &Tables,
    net: &Network,
    source: usize,
    subs: &HashSet<PeerId>,
) {
    if net.trees.len() > source {
        for sub in subs {
            if let Some(sub_idx) = net.get_idx(sub) {
                if net.trees[source].directions.len() > sub_idx.index() {
                    if let Some(direction) = net.trees[source].directions[sub_idx.index()] {
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

unsafe fn compute_data_route(
    tables: &Tables,
    prefix: &Arc<Resource>,
    suffix: &str,
    source: Option<usize>,
    source_type: whatami::Type,
) -> Arc<Route> {
    let mut route = HashMap::new();
    let res = Resource::get_resource(prefix, suffix);
    let matches = match res.as_ref() {
        Some(res) => Cow::from(&res.matches),
        None => Cow::from(Resource::get_matches(
            tables,
            &[&prefix.name(), suffix].concat(),
        )),
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
            insert_faces_for_subs(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                router_source,
                &mres.router_subs,
            );

            let net = tables.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                whatami::PEER => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_subs(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                peer_source,
                &mres.peer_subs,
            );
        }

        if tables.whatami == whatami::PEER {
            let net = tables.peers_net.as_ref().unwrap();
            let peer_source = match source_type {
                whatami::ROUTER | whatami::PEER => source.unwrap(),
                _ => net.idx.index(),
            };
            insert_faces_for_subs(
                &mut route,
                prefix,
                suffix,
                tables,
                net,
                peer_source,
                &mres.peer_subs,
            );
        }

        for (sid, context) in &mut mres.contexts {
            if let Some(subinfo) = &context.subs {
                if subinfo.mode == SubMode::Push {
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

fn compute_matching_pulls(
    tables: &Tables,
    prefix: &Arc<Resource>,
    suffix: &str,
) -> Arc<PullCaches> {
    let mut pull_caches = vec![];
    let res = Resource::get_resource(prefix, suffix);
    let matches = match res.as_ref() {
        Some(res) => Cow::from(&res.matches),
        None => Cow::from(Resource::get_matches(
            tables,
            &[&prefix.name(), suffix].concat(),
        )),
    };
    for mres in matches.iter() {
        let mres = mres.upgrade().unwrap();
        for context in mres.contexts.values() {
            if let Some(subinfo) = &context.subs {
                if subinfo.mode == SubMode::Pull {
                    pull_caches.push(context.clone());
                }
            }
        }
    }
    Arc::new(pull_caches)
}

pub(crate) unsafe fn compute_data_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
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
        res_mut.routers_data_routes.clear();
        res_mut
            .routers_data_routes
            .resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

        for idx in &indexes {
            res_mut.routers_data_routes[idx.index()] =
                compute_data_route(tables, res, "", Some(idx.index()), whatami::ROUTER);
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
        res_mut.peers_data_routes.clear();
        res_mut
            .peers_data_routes
            .resize_with(max_idx.index() + 1, || Arc::new(HashMap::new()));

        for idx in &indexes {
            res_mut.peers_data_routes[idx.index()] =
                compute_data_route(tables, res, "", Some(idx.index()), whatami::PEER);
        }
    }
    if tables.whatami == whatami::CLIENT {
        res_mut.client_data_route =
            Some(compute_data_route(tables, res, "", None, whatami::CLIENT));
    }
    res_mut.matching_pulls = compute_matching_pulls(tables, res, "");
}

unsafe fn compute_data_routes_from(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_data_routes(tables, res);
    let res = Arc::get_mut_unchecked(res);
    for child in res.childs.values_mut() {
        compute_data_routes_from(tables, child);
    }
}

pub(crate) unsafe fn compute_matches_data_routes(tables: &mut Tables, res: &mut Arc<Resource>) {
    compute_data_routes(tables, res);

    let resclone = res.clone();
    for match_ in &mut Arc::get_mut_unchecked(res).matches {
        if !Arc::ptr_eq(&match_.upgrade().unwrap(), &resclone) {
            compute_data_routes(tables, &mut match_.upgrade().unwrap());
        }
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
pub async fn route_data(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    rid: u64,
    suffix: &str,
    congestion_control: CongestionControl,
    info: Option<DataInfo>,
    payload: RBuf,
    routing_context: Option<RoutingContext>,
) {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => unsafe {
            log::trace!("Route data for res {}{}", prefix.name(), suffix,);

            let res = Resource::get_resource(prefix, suffix);

            let route = match tables.whatami {
                whatami::ROUTER => match face.whatami {
                    whatami::ROUTER => {
                        let routers_net = tables.routers_net.as_ref().unwrap();
                        let local_context =
                            routers_net.get_local_context(routing_context.unwrap(), face.link_id);
                        match &res {
                            Some(res) => res.routers_data_routes[local_context].clone(),
                            None => compute_data_route(
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
                        let local_context =
                            peers_net.get_local_context(routing_context.unwrap(), face.link_id);
                        match &res {
                            Some(res) => res.peers_data_routes[local_context].clone(),
                            None => compute_data_route(
                                tables,
                                prefix,
                                suffix,
                                Some(local_context),
                                whatami::PEER,
                            ),
                        }
                    }
                    _ => match &res {
                        Some(res) => res.routers_data_routes[0].clone(),
                        None => compute_data_route(tables, prefix, suffix, None, whatami::CLIENT),
                    },
                },
                whatami::PEER => match face.whatami {
                    whatami::ROUTER | whatami::PEER => {
                        let peers_net = tables.peers_net.as_ref().unwrap();
                        let local_context =
                            peers_net.get_local_context(routing_context.unwrap(), face.link_id);
                        match &res {
                            Some(res) => res.peers_data_routes[local_context].clone(),
                            None => compute_data_route(
                                tables,
                                prefix,
                                suffix,
                                Some(local_context),
                                whatami::PEER,
                            ),
                        }
                    }
                    _ => match &res {
                        Some(res) => res.peers_data_routes[0].clone(),
                        None => compute_data_route(tables, prefix, suffix, None, whatami::CLIENT),
                    },
                },
                _ => match &res {
                    Some(res) => match &res.client_data_route {
                        Some(route) => route.clone(),
                        None => compute_data_route(tables, prefix, suffix, None, whatami::CLIENT),
                    },
                    None => compute_data_route(tables, prefix, suffix, None, whatami::CLIENT),
                },
            };

            let matching_pulls = match &res {
                Some(res) => res.matching_pulls.clone(),
                None => compute_matching_pulls(tables, prefix, suffix),
            };

            if !(route.is_empty() && matching_pulls.is_empty()) {
                // if an HLC was configured (via Config.add_timestamp),
                // check DataInfo and add a timestamp if there isn't
                let data_info = match &tables.hlc {
                    Some(hlc) => match treat_timestamp(hlc, info).await {
                        Ok(info) => info,
                        Err(e) => {
                            log::error!(
                                "Error treating timestamp for received Data ({}): drop it!",
                                e
                            );
                            return;
                        }
                    },
                    None => info,
                };

                if route.len() == 1 && matching_pulls.len() == 0 {
                    let (outface, reskey, context) = route.values().next().unwrap();
                    if face.id != outface.id {
                        outface
                            .primitives
                            .send_data(
                                &reskey,
                                payload,
                                Reliability::Reliable, // TODO: Need to check the active subscriptions to determine the right reliability value
                                congestion_control,
                                data_info,
                                *context,
                            )
                            .await
                    }
                } else {
                    for (outface, reskey, context) in route.values() {
                        if face.id != outface.id {
                            outface
                                .primitives
                                .send_data(
                                    &reskey,
                                    payload.clone(),
                                    Reliability::Reliable, // TODO: Need to check the active subscriptions to determine the right reliability value
                                    congestion_control,
                                    data_info.clone(),
                                    *context,
                                )
                                .await
                        }
                    }

                    for context in matching_pulls.iter() {
                        Arc::get_mut_unchecked(&mut context.clone())
                            .last_values
                            .insert(
                                [&prefix.name(), suffix].concat(),
                                (data_info.clone(), payload.clone()),
                            );
                    }
                }
            }
        },
        None => {
            log::error!("Route data with unknown rid {}!", rid);
        }
    }
}

async fn treat_timestamp(hlc: &HLC, info: Option<DataInfo>) -> Result<Option<DataInfo>, String> {
    if let Some(mut data_info) = info {
        if let Some(ref ts) = data_info.timestamp {
            // Timestamp is present; update HLC with it (possibly raising error if delta exceed)
            hlc.update_with_timestamp(ts).await?;
            Ok(Some(data_info))
        } else {
            // Timestamp not present; add one
            data_info.timestamp = Some(hlc.new_timestamp().await);
            log::trace!("Adding timestamp to DataInfo: {:?}", data_info.timestamp);
            Ok(Some(data_info))
        }
    } else {
        // No DataInfo; add one with a Timestamp
        Ok(Some(new_datainfo(hlc.new_timestamp().await)))
    }
}

fn new_datainfo(ts: uhlc::Timestamp) -> DataInfo {
    DataInfo {
        source_id: None,
        source_sn: None,
        first_router_id: None,
        first_router_sn: None,
        timestamp: Some(ts),
        kind: None,
        encoding: None,
    }
}

pub async fn pull_data(
    tables: &mut Tables,
    face: &Arc<FaceState>,
    _is_final: bool,
    rid: ZInt,
    suffix: &str,
    _pull_id: ZInt,
    _max_samples: &Option<ZInt>,
) {
    match tables.get_mapping(&face, &rid) {
        Some(prefix) => match Resource::get_resource(prefix, suffix) {
            Some(mut res) => unsafe {
                let res = Arc::get_mut_unchecked(&mut res);
                match res.contexts.get_mut(&face.id) {
                    Some(mut ctx) => match &ctx.subs {
                        Some(subinfo) => {
                            for (name, (info, data)) in &ctx.last_values {
                                let reskey =
                                    Resource::get_best_key(&tables.root_res, name, face.id);
                                face.primitives
                                    .send_data(
                                        &reskey,
                                        data.clone(),
                                        subinfo.reliability,
                                        CongestionControl::Drop, // TODO: Default value for the time being
                                        info.clone(),
                                        None,
                                    )
                                    .await;
                            }
                            Arc::get_mut_unchecked(&mut ctx).last_values.clear();
                        }
                        None => {
                            log::error!(
                                "Pull data for unknown subscription {} (no info)!",
                                [&prefix.name(), suffix].concat()
                            );
                        }
                    },
                    None => {
                        log::error!(
                            "Pull data for unknown subscription {} (no context)!",
                            [&prefix.name(), suffix].concat()
                        );
                    }
                }
            },
            None => {
                log::error!(
                    "Pull data for unknown subscription {} (no resource)!",
                    [&prefix.name(), suffix].concat()
                );
            }
        },
        None => {
            log::error!("Pull data with unknown rid {}!", rid);
        }
    };
}
