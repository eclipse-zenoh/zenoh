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
    mem,
    sync::{atomic::AtomicU32, Arc},
};

use token::{token_remove_node, undeclare_simple_token};
use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI};
use zenoh_protocol::{
    common::ZExtBody,
    core::ZenohIdProto,
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
    pubsub::{pubsub_remove_node, undeclare_simple_subscription},
    queries::{queries_remove_node, undeclare_simple_queryable},
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
        linkstate::{link_weights_from_config, LinkStateList},
        network::Network,
        PEERS_NET_NAME,
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

use crate::net::common::AutoConnect;

struct TreesComputationWorker {
    _task: TerminatableTask,
    tx: flume::Sender<Arc<TablesLock>>,
}

impl TreesComputationWorker {
    fn new() -> Self {
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
                    let new_children = hat_mut!(tables)
                        .linkstatepeers_net
                        .as_mut()
                        .unwrap()
                        .compute_trees();

                    tracing::trace!("Compute routes");
                    pubsub::pubsub_tree_change(&mut tables, &new_children);
                    queries::queries_tree_change(&mut tables, &new_children);
                    token::token_tree_change(&mut tables, &new_children);
                    tables.disable_all_routes();
                    drop(tables);
                }
            }
        });
        Self { _task: task, tx }
    }
}

struct HatTables {
    linkstatepeer_subs: HashSet<Arc<Resource>>,
    linkstatepeer_tokens: HashSet<Arc<Resource>>,
    linkstatepeer_qabls: HashSet<Arc<Resource>>,
    linkstatepeers_net: Option<Network>,
    linkstatepeers_trees_worker: TreesComputationWorker,
}

impl HatTables {
    fn new() -> Self {
        Self {
            linkstatepeer_subs: HashSet::new(),
            linkstatepeer_tokens: HashSet::new(),
            linkstatepeer_qabls: HashSet::new(),
            linkstatepeers_net: None,
            linkstatepeers_trees_worker: TreesComputationWorker::new(),
        }
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>) {
        tracing::trace!("Schedule trees computation");
        let _ = self.linkstatepeers_trees_worker.tx.try_send(tables_ref);
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

        let peer_full_linkstate =
            unwrap_or_default!(config.routing().peer().mode()) == *"linkstate";
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());

        let peer_link_weights = config
            .routing()
            .peer()
            .linkstate()
            .transport_weights()
            .clone();
        drop(config_guard);

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
        Ok(())
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
            if let Some(net) = hat_mut!(tables).linkstatepeers_net.as_mut() {
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

        if face.whatami != WhatAmI::Client {
            for (_, removed_node) in hat_mut!(wtables)
                .linkstatepeers_net
                .as_mut()
                .unwrap()
                .remove_link(&face.zid)
            {
                pubsub_remove_node(&mut wtables, &removed_node.zid, send_declare);
                queries_remove_node(&mut wtables, &removed_node.zid, send_declare);
                token_remove_node(&mut wtables, &removed_node.zid, send_declare);
            }

            hat_mut!(wtables).schedule_compute_trees(tables_ref.clone());
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
                    if whatami != WhatAmI::Client {
                        if let Some(net) = hat_mut!(tables).linkstatepeers_net.as_mut() {
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
        if face.whatami != WhatAmI::Client {
            hat!(tables)
                .linkstatepeers_net
                .as_ref()
                .unwrap()
                .get_local_context(routing_context, face_hat!(face).link_id)
        } else {
            0
        }
    }

    #[inline]
    fn ingress_filter(&self, _tables: &Tables, _face: &FaceState, _expr: &RoutingExpr) -> bool {
        true
    }

    #[inline]
    fn egress_filter(
        &self,
        _tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        _expr: &RoutingExpr,
    ) -> bool {
        src_face.id != out_face.id
            && (out_face.mcast_group.is_none() || src_face.mcast_group.is_none())
    }

    fn info(&self, tables: &Tables, kind: WhatAmI) -> String {
        match kind {
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
        let peer_link_weights = runtime
            .config()
            .lock()
            .0
            .routing()
            .peer()
            .linkstate()
            .transport_weights()
            .clone();
        let peer_link_weights = link_weights_from_config(peer_link_weights, PEERS_NET_NAME)?;
        if let Some(net) = hat_mut!(tables).linkstatepeers_net.as_mut() {
            if net.update_link_weights(peer_link_weights) {
                hat_mut!(tables).schedule_compute_trees(tables_ref.clone());
            }
        }
        Ok(())
    }

    fn links_info(
        &self,
        tables: &Tables,
    ) -> HashMap<ZenohIdProto, crate::net::protocol::linkstate::LinkInfo> {
        match &hat!(tables).linkstatepeers_net {
            Some(net) => net.links_info(),
            None => HashMap::new(),
        }
    }
}

struct HatContext {
    linkstatepeer_subs: HashSet<ZenohIdProto>,
    linkstatepeer_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    linkstatepeer_tokens: HashSet<ZenohIdProto>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            linkstatepeer_subs: HashSet::new(),
            linkstatepeer_qabls: HashMap::new(),
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
    local_tokens: HashMap<Arc<Resource>, SubscriberId>,
    remote_tokens: HashMap<SubscriberId, Arc<Resource>>,
    local_qabls: LocalQueryables,
    remote_qabls: HashMap<QueryableId, (Arc<Resource>, QueryableInfoType)>,
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
pub(super) fn push_declaration_profile(face: &FaceState) -> bool {
    face.whatami != WhatAmI::Client
}
