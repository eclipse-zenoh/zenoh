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
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicU32, Arc},
    time::Duration,
};

use token::{token_remove_node, undeclare_client_token};
use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI, WhatAmIMatcher};
use zenoh_protocol::{
    common::ZExtBody,
    core::ZenohIdProto,
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId},
        interest::{InterestId, InterestOptions},
        oam::id::OAM_LINKSTATE,
        Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TerminatableTask;
use zenoh_transport::unicast::TransportUnicast;

use self::{
    network::Network,
    pubsub::{pubsub_remove_node, undeclare_client_subscription},
    queries::{queries_remove_node, undeclare_client_queryable},
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
    protocol::linkstate::LinkStateList,
    routing::{
        dispatcher::face::Face,
        hat::TREES_COMPUTATION_DELAY_MS,
        router::{compute_data_routes, compute_query_routes, RoutesIndexes},
    },
    runtime::Runtime,
};

mod interests;
mod network;
mod pubsub;
mod queries;
mod token;

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
    peer_subs: HashSet<Arc<Resource>>,
    peer_tokens: HashSet<Arc<Resource>>,
    peer_qabls: HashSet<Arc<Resource>>,
    peers_net: Option<Network>,
    peers_trees_task: Option<TerminatableTask>,
}

impl Drop for HatTables {
    fn drop(&mut self) {
        if self.peers_trees_task.is_some() {
            let task = self.peers_trees_task.take().unwrap();
            task.terminate(Duration::from_secs(10));
        }
    }
}

impl HatTables {
    fn new() -> Self {
        Self {
            peer_subs: HashSet::new(),
            peer_tokens: HashSet::new(),
            peer_qabls: HashSet::new(),
            peers_net: None,
            peers_trees_task: None,
        }
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>) {
        if self.peers_trees_task.is_none() {
            let task = TerminatableTask::spawn(
                zenoh_runtime::ZRuntime::Net,
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        *TREES_COMPUTATION_DELAY_MS,
                    ))
                    .await;
                    let mut tables = zwrite!(tables_ref.tables);

                    tracing::trace!("Compute trees");
                    let new_children = hat_mut!(tables).peers_net.as_mut().unwrap().compute_trees();

                    tracing::trace!("Compute routes");
                    pubsub::pubsub_tree_change(&mut tables, &new_children);
                    queries::queries_tree_change(&mut tables, &new_children);
                    token::token_tree_change(&mut tables, &new_children);

                    tracing::trace!("Computations completed");
                    hat_mut!(tables).peers_trees_task = None;
                },
                TerminatableTask::create_cancellation_token(),
            );
            self.peers_trees_task = Some(task);
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

        let peer_full_linkstate = whatami != WhatAmI::Client
            && unwrap_or_default!(config.routing().peer().mode()) == *"linkstate";
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        drop(config);

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

    fn new_tables(&self, _router_peers_failover_brokering: bool) -> Box<dyn Any + Send + Sync> {
        Box::new(HatTables::new())
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
        let link_id = if face.state.whatami != WhatAmI::Client {
            if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                net.add_link(transport.clone())
            } else {
                0
            }
        } else {
            0
        };

        face_hat_mut!(&mut face.state).link_id = link_id;

        if face.state.whatami != WhatAmI::Client {
            hat_mut!(tables).schedule_compute_trees(tables_ref.clone());
        }
        Ok(())
    }

    fn close_face(
        &self,
        tables: &TablesLock,
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
            undeclare_client_subscription(&mut wtables, &mut face_clone, &mut res, send_declare);

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
        for (_, mut res) in hat_face.remote_qabls.drain() {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_client_queryable(&mut wtables, &mut face_clone, &mut res, send_declare);

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
            undeclare_client_token(&mut wtables, &mut face_clone, &mut res, send_declare);
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
        send_declare: &mut SendDeclare,
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
                    if whatami != WhatAmI::Client {
                        if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                            let changes = net.link_states(list.link_states, zid);

                            for (_, removed_node) in changes.removed_nodes {
                                pubsub_remove_node(tables, &removed_node.zid, send_declare);
                                queries_remove_node(tables, &removed_node.zid, send_declare);
                                token_remove_node(tables, &removed_node.zid, send_declare);
                            }

                            hat_mut!(tables).schedule_compute_trees(tables_ref.clone());
                        }
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
        hat!(tables)
            .peers_net
            .as_ref()
            .unwrap()
            .get_local_context(routing_context, face_hat!(face).link_id)
    }

    fn closing(
        &self,
        tables: &mut Tables,
        tables_ref: &Arc<TablesLock>,
        transport: &TransportUnicast,
        send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        match (transport.get_zid(), transport.get_whatami()) {
            (Ok(zid), Ok(whatami)) => {
                if whatami != WhatAmI::Client {
                    for (_, removed_node) in hat_mut!(tables)
                        .peers_net
                        .as_mut()
                        .unwrap()
                        .remove_link(&zid)
                    {
                        pubsub_remove_node(tables, &removed_node.zid, send_declare);
                        queries_remove_node(tables, &removed_node.zid, send_declare);
                        token_remove_node(tables, &removed_node.zid, send_declare);
                    }

                    hat_mut!(tables).schedule_compute_trees(tables_ref.clone());
                };
            }
            (_, _) => tracing::error!("Closed transport in session closing!"),
        }
        Ok(())
    }

    #[inline]
    fn ingress_filter(&self, _tables: &Tables, _face: &FaceState, _expr: &mut RoutingExpr) -> bool {
        true
    }

    #[inline]
    fn egress_filter(
        &self,
        _tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        _expr: &mut RoutingExpr,
    ) -> bool {
        src_face.id != out_face.id
            && match (src_face.mcast_group.as_ref(), out_face.mcast_group.as_ref()) {
                (Some(l), Some(r)) => l != r,
                _ => true,
            }
    }

    fn info(&self, tables: &Tables, kind: WhatAmI) -> String {
        match kind {
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
    peer_subs: HashSet<ZenohIdProto>,
    peer_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    peer_tokens: HashSet<ZenohIdProto>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            peer_subs: HashSet::new(),
            peer_qabls: HashMap::new(),
            peer_tokens: HashSet::new(),
        }
    }
}

struct HatFace {
    link_id: usize,
    next_id: AtomicU32, // @TODO: manage rollover and uniqueness
    remote_interests: HashMap<InterestId, (Option<Arc<Resource>>, InterestOptions)>,
    local_subs: HashMap<Arc<Resource>, SubscriberId>,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    local_tokens: HashMap<Arc<Resource>, SubscriberId>,
    remote_tokens: HashMap<SubscriberId, Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, (QueryableId, QueryableInfoType)>,
    remote_qabls: HashMap<QueryableId, Arc<Resource>>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            link_id: 0,
            next_id: AtomicU32::new(0),
            remote_interests: HashMap::new(),
            local_subs: HashMap::new(),
            remote_subs: HashMap::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashMap::new(),
            local_tokens: HashMap::new(),
            remote_tokens: HashMap::new(),
        }
    }
}

fn get_peer(tables: &Tables, face: &Arc<FaceState>, nodeid: NodeId) -> Option<ZenohIdProto> {
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
    let indexes = hat!(tables)
        .peers_net
        .as_ref()
        .unwrap()
        .graph
        .node_indices()
        .map(|i| i.index() as NodeId)
        .collect::<Vec<NodeId>>();
    RoutesIndexes {
        routers: indexes.clone(),
        peers: indexes,
        clients: vec![0],
    }
}
