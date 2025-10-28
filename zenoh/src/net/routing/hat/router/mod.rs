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
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::Hasher,
    mem,
    sync::{atomic::AtomicU32, Arc},
};

use token::{token_linkstate_change, token_remove_node, undeclare_simple_token};
use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI};
use zenoh_protocol::{
    common::ZExtBody,
    core::ZenohIdProto,
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId, TokenId},
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
    pubsub::{pubsub_linkstate_change, pubsub_remove_node, undeclare_simple_subscription},
    queries::{queries_linkstate_change, queries_remove_node, undeclare_simple_queryable},
};
use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, Tables, TablesLock},
    },
    HatBaseTrait, HatTrait, SendDeclare,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::{
        linkstate::{link_weights_from_config, LinkEdgeWeight, LinkStateList},
        network::{shared_nodes, Network},
        PEERS_NET_NAME, ROUTERS_NET_NAME,
    },
    routing::{
        dispatcher::{face::Face, interests::RemoteInterest},
        hat::TREES_COMPUTATION_DELAY_MS,
        router::{LocalQueryables, LocalSubscribers},
    },
    runtime::Runtime,
};

mod interests;
mod pubsub;
mod queries;
mod token;

// Interest id used for Pushed declarations without declare final to other routers/linkstate peers
const INITIAL_INTEREST_ID: u32 = 0;

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

use crate::net::{common::AutoConnect, protocol::network::SuccessorEntry};

struct TreesComputationWorker {
    _task: TerminatableTask,
    tx: flume::Sender<Arc<TablesLock>>,
}

impl TreesComputationWorker {
    fn new(net_type: WhatAmI) -> Self {
        let (tx, rx) = flume::bounded::<Arc<TablesLock>>(1);
        let task = TerminatableTask::spawn_abortable(zenoh_runtime::ZRuntime::Net, async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(
                    *TREES_COMPUTATION_DELAY_MS,
                ))
                .await;
                if let Ok(tables_ref) = rx.recv_async().await {
                    let mut tables = zwrite!(tables_ref.tables);

                    tracing::trace!("Compute trees");
                    let new_children = match net_type {
                        WhatAmI::Router => hat_mut!(tables)
                            .routers_net
                            .as_mut()
                            .unwrap()
                            .compute_trees(),
                        _ => hat_mut!(tables)
                            .linkstatepeers_net
                            .as_mut()
                            .unwrap()
                            .compute_trees(),
                    };

                    tracing::trace!("Compute routes");
                    pubsub::pubsub_tree_change(&mut tables, &new_children, net_type);
                    queries::queries_tree_change(&mut tables, &new_children, net_type);
                    token::token_tree_change(&mut tables, &new_children, net_type);
                    tables.disable_all_routes();
                    drop(tables);
                }
            }
        });
        Self { _task: task, tx }
    }
}

struct HatTables {
    router_subs: HashSet<Arc<Resource>>,
    linkstatepeer_subs: HashSet<Arc<Resource>>,
    router_tokens: HashSet<Arc<Resource>>,
    linkstatepeer_tokens: HashSet<Arc<Resource>>,
    router_qabls: HashSet<Arc<Resource>>,
    linkstatepeer_qabls: HashSet<Arc<Resource>>,
    routers_net: Option<Network>,
    linkstatepeers_net: Option<Network>,
    shared_nodes: Vec<ZenohIdProto>,
    routers_trees_worker: TreesComputationWorker,
    linkstatepeers_trees_worker: TreesComputationWorker,
    router_peers_failover_brokering: bool,
}

impl HatTables {
    fn new(router_peers_failover_brokering: bool) -> Self {
        Self {
            router_subs: HashSet::new(),
            linkstatepeer_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            linkstatepeer_qabls: HashSet::new(),
            router_tokens: HashSet::new(),
            linkstatepeer_tokens: HashSet::new(),
            routers_net: None,
            linkstatepeers_net: None,
            shared_nodes: vec![],
            routers_trees_worker: TreesComputationWorker::new(WhatAmI::Router),
            linkstatepeers_trees_worker: TreesComputationWorker::new(WhatAmI::Peer),
            router_peers_failover_brokering,
        }
    }

    #[inline]
    fn get_net(&self, net_type: WhatAmI) -> Option<&Network> {
        match net_type {
            WhatAmI::Router => self.routers_net.as_ref(),
            WhatAmI::Peer => self.linkstatepeers_net.as_ref(),
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
                .linkstatepeers_net
                .as_ref()
                .map(|net| net.full_linkstate)
                .unwrap_or(false),
            _ => false,
        }
    }

    #[inline]
    fn get_router_links(&self, peer: ZenohIdProto) -> impl Iterator<Item = &ZenohIdProto> + '_ {
        self.linkstatepeers_net
            .as_ref()
            .unwrap()
            .get_links(peer)
            .map(|h| h.keys())
            .into_iter()
            .flatten()
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
        self_zid: &'a ZenohIdProto,
        key_expr: &str,
        mut routers: impl Iterator<Item = &'a ZenohIdProto>,
    ) -> &'a ZenohIdProto {
        match routers.next() {
            None => self_zid,
            Some(router) => {
                let hash = |r: &ZenohIdProto| {
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
    fn failover_brokering_to(
        source_links: &HashMap<ZenohIdProto, LinkEdgeWeight>,
        dest: &ZenohIdProto,
    ) -> bool {
        // if source_links is empty then gossip is probably disabled in source peer
        !source_links.is_empty() && !source_links.contains_key(dest)
    }

    #[inline]
    fn failover_brokering(&self, peer1: ZenohIdProto, peer2: ZenohIdProto) -> bool {
        self.router_peers_failover_brokering
            && self
                .linkstatepeers_net
                .as_ref()
                .map(|net| {
                    let res = match net.get_links(peer1) {
                        Some(links) => HatTables::failover_brokering_to(links, &peer2),
                        None => false,
                    };
                    tracing::trace!("failover_brokering {} {} : {}", peer1, peer2, res);
                    res
                })
                .unwrap_or(false)
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>, net_type: WhatAmI) {
        tracing::trace!("Schedule trees computation");
        match net_type {
            WhatAmI::Router => {
                let _ = self.routers_trees_worker.tx.try_send(tables_ref);
            }
            WhatAmI::Peer => {
                let _ = self.linkstatepeers_trees_worker.tx.try_send(tables_ref);
            }
            _ => (),
        }
    }
}

pub(crate) struct HatCode {}

impl HatBaseTrait for HatCode {
    fn init(&self, tables: &mut Tables, runtime: Runtime) -> ZResult<()> {
        let config_guard = runtime.config().lock();
        let config = &config_guard.0;
        let whatami = tables.whatami;
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

        let router_full_linkstate = true;
        let peer_full_linkstate =
            unwrap_or_default!(config.routing().peer().mode()) == *"linkstate";
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        let router_link_weights = config
            .routing()
            .router()
            .linkstate()
            .transport_weights()
            .clone();
        let peer_link_weights = config
            .routing()
            .peer()
            .linkstate()
            .transport_weights()
            .clone();
        drop(config_guard);

        if router_full_linkstate | gossip {
            hat_mut!(tables).routers_net = Some(Network::new(
                ROUTERS_NET_NAME.to_string(),
                tables.zid,
                runtime.clone(),
                router_full_linkstate,
                router_peers_failover_brokering,
                gossip,
                gossip_multihop,
                gossip_target,
                autoconnect,
                link_weights_from_config(router_link_weights, ROUTERS_NET_NAME)?,
            ));
        }
        if peer_full_linkstate | gossip {
            hat_mut!(tables).linkstatepeers_net = Some(Network::new(
                PEERS_NET_NAME.to_string(),
                tables.zid,
                runtime,
                peer_full_linkstate,
                router_peers_failover_brokering,
                gossip,
                gossip_multihop,
                gossip_target,
                autoconnect,
                link_weights_from_config(peer_link_weights, PEERS_NET_NAME)?,
            ));
        }
        if router_full_linkstate && peer_full_linkstate {
            hat_mut!(tables).shared_nodes = shared_nodes(
                hat!(tables).routers_net.as_ref().unwrap(),
                hat!(tables).linkstatepeers_net.as_ref().unwrap(),
            );
        }
        Ok(())
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
        _tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        _face: &mut Face,
        _send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        // Nothing to do
        Ok(())
    }

    fn new_transport_unicast_face(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        transport: &TransportUnicast,
        _send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        let link_id = match face.state.whatami {
            WhatAmI::Router => hat_mut!(tables)
                .routers_net
                .as_mut()
                .unwrap()
                .add_link(transport.clone()),
            WhatAmI::Peer => {
                if let Some(net) = hat_mut!(tables).linkstatepeers_net.as_mut() {
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
                hat!(tables).linkstatepeers_net.as_ref().unwrap(),
            );
        }

        face_hat_mut!(&mut face.state).link_id = link_id;

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

    fn close_face(
        &self,
        tables: &TablesLock,
        tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
        let mut wtables = zwrite!(tables.tables);
        let mut face_clone = face.clone();
        let face = get_mut_unchecked(face);
        let hat_face = match face.hat.downcast_mut::<HatFace>() {
            Some(hate_face) => hate_face,
            None => {
                tracing::error!("Error downcasting face hat in close_face!");
                return;
            }
        };

        hat_face.remote_interests.clear();
        hat_face.local_subs.clear();
        hat_face.local_qabls.clear();
        hat_face.local_tokens.clear();

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
        for (_id, mut res) in hat_face.remote_subs.drain() {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_simple_subscription(&mut wtables, &mut face_clone, &mut res, send_declare);

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
        for (_, (mut res, _)) in hat_face.remote_qabls.drain() {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_simple_queryable(&mut wtables, &mut face_clone, &mut res, send_declare);

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

        for (_id, mut res) in hat_face.remote_tokens.drain() {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_simple_token(&mut wtables, &mut face_clone, &mut res, send_declare);
        }

        for mut res in subs_matches {
            get_mut_unchecked(&mut res)
                .context_mut()
                .disable_data_routes();
            Resource::clean(&mut res);
        }
        for mut res in qabls_matches {
            get_mut_unchecked(&mut res)
                .context_mut()
                .disable_query_routes();
            Resource::clean(&mut res);
        }
        wtables.faces.remove(&face.id);

        match face.whatami {
            WhatAmI::Router => {
                for (_, removed_node) in hat_mut!(wtables)
                    .routers_net
                    .as_mut()
                    .unwrap()
                    .remove_link(&face.zid)
                {
                    pubsub_remove_node(
                        &mut wtables,
                        &removed_node.zid,
                        WhatAmI::Router,
                        send_declare,
                    );
                    queries_remove_node(
                        &mut wtables,
                        &removed_node.zid,
                        WhatAmI::Router,
                        send_declare,
                    );
                    token_remove_node(
                        &mut wtables,
                        &removed_node.zid,
                        WhatAmI::Router,
                        send_declare,
                    );
                }

                if hat!(wtables).full_net(WhatAmI::Peer) {
                    hat_mut!(wtables).shared_nodes = shared_nodes(
                        hat!(wtables).routers_net.as_ref().unwrap(),
                        hat!(wtables).linkstatepeers_net.as_ref().unwrap(),
                    );
                }

                hat_mut!(wtables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
            }
            WhatAmI::Peer => {
                if hat!(wtables).full_net(WhatAmI::Peer) {
                    for (_, removed_node) in hat_mut!(wtables)
                        .linkstatepeers_net
                        .as_mut()
                        .unwrap()
                        .remove_link(&face.zid)
                    {
                        pubsub_remove_node(
                            &mut wtables,
                            &removed_node.zid,
                            WhatAmI::Peer,
                            send_declare,
                        );
                        queries_remove_node(
                            &mut wtables,
                            &removed_node.zid,
                            WhatAmI::Peer,
                            send_declare,
                        );
                        token_remove_node(
                            &mut wtables,
                            &removed_node.zid,
                            WhatAmI::Peer,
                            send_declare,
                        );
                    }

                    hat_mut!(wtables).shared_nodes = shared_nodes(
                        hat!(wtables).routers_net.as_ref().unwrap(),
                        hat!(wtables).linkstatepeers_net.as_ref().unwrap(),
                    );

                    hat_mut!(wtables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                } else if let Some(net) = hat_mut!(wtables).linkstatepeers_net.as_mut() {
                    net.remove_link(&face.zid);
                }
            }
            _ => (),
        };
        drop(wtables);
    }

    fn handle_oam(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        oam: &mut Oam,
        transport: &TransportUnicast,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        if oam.id == OAM_LINKSTATE {
            if let ZExtBody::ZBuf(buf) = mem::take(&mut oam.body) {
                if let Ok(zid) = transport.get_zid() {
                    use zenoh_buffers::reader::HasReader;
                    use zenoh_codec::RCodec;
                    let codec = Zenoh080Routing::new();
                    let mut reader = buf.reader();
                    let Ok(list): Result<LinkStateList, _> = codec.read(&mut reader) else {
                        bail!("failed to decode link state");
                    };

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
                                pubsub_remove_node(
                                    tables,
                                    &removed_node.zid,
                                    WhatAmI::Router,
                                    send_declare,
                                );
                                queries_remove_node(
                                    tables,
                                    &removed_node.zid,
                                    WhatAmI::Router,
                                    send_declare,
                                );
                                token_remove_node(
                                    tables,
                                    &removed_node.zid,
                                    WhatAmI::Router,
                                    send_declare,
                                );
                            }

                            if hat!(tables).full_net(WhatAmI::Peer) {
                                hat_mut!(tables).shared_nodes = shared_nodes(
                                    hat!(tables).routers_net.as_ref().unwrap(),
                                    hat!(tables).linkstatepeers_net.as_ref().unwrap(),
                                );
                            }

                            hat_mut!(tables)
                                .schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
                        }
                        WhatAmI::Peer => {
                            if let Some(net) = hat_mut!(tables).linkstatepeers_net.as_mut() {
                                let changes = net.link_states(list.link_states, zid);
                                if hat!(tables).full_net(WhatAmI::Peer) {
                                    for (_, removed_node) in changes.removed_nodes {
                                        pubsub_remove_node(
                                            tables,
                                            &removed_node.zid,
                                            WhatAmI::Peer,
                                            send_declare,
                                        );
                                        queries_remove_node(
                                            tables,
                                            &removed_node.zid,
                                            WhatAmI::Peer,
                                            send_declare,
                                        );
                                        token_remove_node(
                                            tables,
                                            &removed_node.zid,
                                            WhatAmI::Peer,
                                            send_declare,
                                        );
                                    }

                                    hat_mut!(tables).shared_nodes = shared_nodes(
                                        hat!(tables).routers_net.as_ref().unwrap(),
                                        hat!(tables).linkstatepeers_net.as_ref().unwrap(),
                                    );

                                    hat_mut!(tables)
                                        .schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                                } else {
                                    for (_, updated_node) in changes.updated_nodes {
                                        pubsub_linkstate_change(
                                            tables,
                                            &updated_node.zid,
                                            &updated_node.links,
                                            send_declare,
                                        );
                                        queries_linkstate_change(
                                            tables,
                                            &updated_node.zid,
                                            &updated_node.links,
                                            send_declare,
                                        );
                                        token_linkstate_change(
                                            tables,
                                            &updated_node.zid,
                                            &updated_node.links,
                                            send_declare,
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
                        .linkstatepeers_net
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

    #[inline]
    fn ingress_filter(&self, tables: &Tables, face: &FaceState, expr: &RoutingExpr) -> bool {
        face.whatami != WhatAmI::Peer
            || hat!(tables).linkstatepeers_net.is_none()
            || tables.zid
                == *hat!(tables).elect_router(
                    &tables.zid,
                    expr.key_expr().map_or("", |ke| ke),
                    hat!(tables).get_router_links(face.zid),
                )
    }

    #[inline]
    fn egress_filter(
        &self,
        tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        expr: &RoutingExpr,
    ) -> bool {
        if src_face.id != out_face.id
            && (out_face.mcast_group.is_none() || src_face.mcast_group.is_none())
        {
            let dst_master = out_face.whatami != WhatAmI::Peer
                || hat!(tables).linkstatepeers_net.is_none()
                || tables.zid
                    == *hat!(tables).elect_router(
                        &tables.zid,
                        expr.key_expr().map_or("", |ke| ke),
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
                .linkstatepeers_net
                .as_ref()
                .map(|net| net.dot())
                .unwrap_or_else(|| "graph {}".to_string()),
            _ => "graph {}".to_string(),
        }
    }

    fn update_from_config(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        runtime: &Runtime,
    ) -> ZResult<()> {
        let config = runtime.config().lock();
        let router_link_weights = link_weights_from_config(
            config
                .0
                .routing()
                .router()
                .linkstate()
                .transport_weights()
                .clone(),
            ROUTERS_NET_NAME,
        )?;
        let peer_link_weights = link_weights_from_config(
            config
                .0
                .routing()
                .peer()
                .linkstate()
                .transport_weights()
                .clone(),
            PEERS_NET_NAME,
        )?;
        drop(config);
        if let Some(net) = hat_mut!(tables).routers_net.as_mut() {
            if net.update_link_weights(router_link_weights) {
                hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
            }
        }
        if let Some(net) = hat_mut!(tables).linkstatepeers_net.as_mut() {
            if net.update_link_weights(peer_link_weights) {
                hat_mut!(tables).schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
            }
        }
        Ok(())
    }

    fn links_info(
        &self,
        tables: &Tables,
    ) -> HashMap<ZenohIdProto, crate::net::protocol::linkstate::LinkInfo> {
        let mut out = HashMap::new();
        if let Some(net) = &hat!(tables).routers_net {
            out.extend(net.links_info());
        }
        if let Some(net) = &hat!(tables).linkstatepeers_net {
            out.extend(net.links_info());
        }
        out
    }

    fn route_successor(
        &self,
        tables: &Tables,
        src: ZenohIdProto,
        dst: ZenohIdProto,
    ) -> Option<ZenohIdProto> {
        hat!(tables).routers_net.as_ref()?.route_successor(src, dst)
    }

    fn route_successors(&self, tables: &Tables) -> Vec<SuccessorEntry> {
        hat!(tables)
            .routers_net
            .as_ref()
            .map(|net| net.route_successors())
            .unwrap_or_default()
    }
}

struct HatContext {
    router_subs: HashSet<ZenohIdProto>,
    linkstatepeer_subs: HashSet<ZenohIdProto>,
    router_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    linkstatepeer_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    router_tokens: HashSet<ZenohIdProto>,
    linkstatepeer_tokens: HashSet<ZenohIdProto>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            linkstatepeer_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            linkstatepeer_qabls: HashMap::new(),
            router_tokens: HashSet::new(),
            linkstatepeer_tokens: HashSet::new(),
        }
    }
}

struct HatFace {
    link_id: usize,
    next_id: AtomicU32, // @TODO: manage rollover and uniqueness
    remote_interests: HashMap<InterestId, RemoteInterest>,
    local_subs: LocalSubscribers,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    local_qabls: LocalQueryables,
    remote_qabls: HashMap<QueryableId, (Arc<Resource>, QueryableInfoType)>,
    local_tokens: HashMap<Arc<Resource>, TokenId>,
    remote_tokens: HashMap<TokenId, Arc<Resource>>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            link_id: 0,
            next_id: AtomicU32::new(0),
            remote_interests: HashMap::new(),
            local_subs: LocalSubscribers::new(),
            remote_subs: HashMap::new(),
            local_qabls: LocalQueryables::new(),
            remote_qabls: HashMap::new(),
            local_tokens: HashMap::new(),
            remote_tokens: HashMap::new(),
        }
    }
}

fn get_router(tables: &Tables, face: &Arc<FaceState>, nodeid: NodeId) -> Option<ZenohIdProto> {
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

fn get_peer(tables: &Tables, face: &Arc<FaceState>, nodeid: NodeId) -> Option<ZenohIdProto> {
    match hat!(tables)
        .linkstatepeers_net
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
pub(super) fn push_declaration_profile(tables: &Tables, face: &FaceState) -> bool {
    !(face.whatami == WhatAmI::Client
        || (face.whatami == WhatAmI::Peer && !hat!(tables).full_net(WhatAmI::Peer)))
}
