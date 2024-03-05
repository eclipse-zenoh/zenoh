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
    pubsub::{pubsub_new_face, pubsub_remove_node, undeclare_client_subscription},
    queries::{queries_new_face, queries_remove_node, undeclare_client_queryable},
};
use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, Tables, TablesLock},
    },
    HatBaseTrait, HatTrait,
};
use crate::{
    net::{
        codec::Zenoh080Routing,
        protocol::linkstate::LinkStateList,
        routing::{
            dispatcher::face::Face,
            hat::TREES_COMPUTATION_DELAY_MS,
            router::{
                compute_data_routes, compute_matching_pulls, compute_query_routes, RoutesIndexes,
            },
        },
    },
    runtime::Runtime,
};
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::task::JoinHandle;
use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI, WhatAmIMatcher, ZenohId};
use zenoh_protocol::{
    common::ZExtBody,
    network::{declare::queryable::ext::QueryableInfo, oam::id::OAM_LINKSTATE, Oam},
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::unicast::TransportUnicast;

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
    peer_subs: HashSet<Arc<Resource>>,
    peer_qabls: HashSet<Arc<Resource>>,
    peers_net: Option<Network>,
    peers_trees_task: Option<JoinHandle<()>>,
}

impl HatTables {
    fn new() -> Self {
        Self {
            peer_subs: HashSet::new(),
            peer_qabls: HashSet::new(),
            peers_net: None,
            peers_trees_task: None,
        }
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>) {
        log::trace!("Schedule computations");
        if self.peers_trees_task.is_none() {
            let task = Some(zenoh_runtime::ZRuntime::Net.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(
                    *TREES_COMPUTATION_DELAY_MS,
                ))
                .await;
                let mut tables = zwrite!(tables_ref.tables);

                log::trace!("Compute trees");
                let new_childs = hat_mut!(tables).peers_net.as_mut().unwrap().compute_trees();

                log::trace!("Compute routes");
                pubsub::pubsub_tree_change(&mut tables, &new_childs);
                queries::queries_tree_change(&mut tables, &new_childs);

                log::trace!("Computations completed");
                hat_mut!(tables).peers_trees_task = None;
            }));
            self.peers_trees_task = task;
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
        pubsub_new_face(tables, &mut face.state);
        queries_new_face(tables, &mut face.state);

        if face.state.whatami != WhatAmI::Client {
            hat_mut!(tables).schedule_compute_trees(tables_ref.clone());
        }
        Ok(())
    }

    fn close_face(&self, tables: &TablesLock, face: &mut Arc<FaceState>) {
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
            matches_data_routes.push((
                _match.clone(),
                compute_data_routes(&rtables, &mut expr),
                compute_matching_pulls(&rtables, &mut expr),
            ));
        }
        for _match in qabls_matches.drain(..) {
            matches_query_routes.push((_match.clone(), compute_query_routes(&rtables, &_match)));
        }
        drop(rtables);

        let mut wtables = zwrite!(tables.tables);
        for (mut res, data_routes, matching_pulls) in matches_data_routes {
            get_mut_unchecked(&mut res)
                .context_mut()
                .update_data_routes(data_routes);
            get_mut_unchecked(&mut res)
                .context_mut()
                .update_matching_pulls(matching_pulls);
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
                    if whatami != WhatAmI::Client {
                        if let Some(net) = hat_mut!(tables).peers_net.as_mut() {
                            let changes = net.link_states(list.link_states, zid);

                            for (_, removed_node) in changes.removed_nodes {
                                pubsub_remove_node(tables, &removed_node.zid);
                                queries_remove_node(tables, &removed_node.zid);
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
                        pubsub_remove_node(tables, &removed_node.zid);
                        queries_remove_node(tables, &removed_node.zid);
                    }

                    hat_mut!(tables).schedule_compute_trees(tables_ref.clone());
                };
            }
            (_, _) => log::error!("Closed transport in session closing!"),
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
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
    router_subs: HashSet<ZenohId>,
    peer_subs: HashSet<ZenohId>,
    peer_qabls: HashMap<ZenohId, QueryableInfo>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            peer_subs: HashSet::new(),
            peer_qabls: HashMap::new(),
        }
    }
}

struct HatFace {
    link_id: usize,
    local_subs: HashSet<Arc<Resource>>,
    remote_subs: HashSet<Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, QueryableInfo>,
    remote_qabls: HashSet<Arc<Resource>>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            link_id: 0,
            local_subs: HashSet::new(),
            remote_subs: HashSet::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashSet::new(),
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
