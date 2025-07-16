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

use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, TablesData, TablesLock},
    },
    HatBaseTrait, HatTrait, SendDeclare,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::{
        linkstate::{link_weights_from_config, LinkStateList},
        network::Network,
        ROUTERS_NET_NAME,
    },
    routing::{
        dispatcher::{face::Face, interests::RemoteInterest},
        hat::TREES_COMPUTATION_DELAY_MS,
    },
    runtime::Runtime,
};

mod interests;
mod pubsub;
mod queries;
mod token;

macro_rules! hat_mut {
    ($t:expr) => {
        $t.hat
            .as_any_mut()
            .downcast_mut::<crate::net::routing::hat::router::Hat>()
            .unwrap()
    };
}
use hat_mut;

macro_rules! res_hat {
    ($r:expr) => {
        $r.context()
            .hat
            .downcast_ref::<crate::net::routing::hat::router::HatContext>()
            .unwrap()
    };
}
use res_hat;

macro_rules! res_hat_mut {
    ($r:expr) => {
        get_mut_unchecked($r)
            .context_mut()
            .hat
            .downcast_mut::<crate::net::routing::hat::router::HatContext>()
            .unwrap()
    };
}
use res_hat_mut;

macro_rules! face_hat {
    ($f:expr) => {
        $f.hat
            .downcast_ref::<crate::net::routing::hat::router::HatFace>()
            .unwrap()
    };
}
use face_hat;

macro_rules! face_hat_mut {
    ($f:expr) => {
        get_mut_unchecked($f)
            .hat
            .downcast_mut::<crate::net::routing::hat::router::HatFace>()
            .unwrap()
    };
}
use face_hat_mut;

use crate::net::{common::AutoConnect, protocol::network::SuccessorEntry};

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
                    let mut wtables = zwrite!(tables_ref.tables);
                    let tables = &mut *wtables;
                    let hat = hat_mut!(tables);

                    tracing::trace!("Compute trees");
                    let new_children = hat.routers_net.as_mut().unwrap().compute_trees();

                    tracing::trace!("Compute routes");
                    hat.pubsub_tree_change(&mut tables.data, &new_children);
                    hat.queries_tree_change(&mut tables.data, &new_children);
                    hat.token_tree_change(&mut tables.data, &new_children);
                    tables.data.disable_all_routes();
                    drop(wtables);
                }
            }
        });
        Self { _task: task, tx }
    }
}

pub(crate) struct Hat {
    router_subs: HashSet<Arc<Resource>>,
    router_tokens: HashSet<Arc<Resource>>,
    router_qabls: HashSet<Arc<Resource>>,
    routers_net: Option<Network>, // TODO(fuzzypixelz): remove Option?
    routers_trees_worker: TreesComputationWorker,
}

impl Hat {
    pub(crate) fn new() -> Self {
        // FIXME(fuzzypixelz): peer failover brokering is currently scrapped
        Self {
            router_subs: HashSet::new(),
            router_qabls: HashSet::new(),
            router_tokens: HashSet::new(),
            routers_net: None,
            routers_trees_worker: TreesComputationWorker::new(),
        }
    }

    fn schedule_compute_trees(&mut self, tables_ref: Arc<TablesLock>) {
        tracing::trace!("Schedule trees computation");
        let _ = self.routers_trees_worker.tx.try_send(tables_ref);
    }

    fn get_router(&self, face: &Arc<FaceState>, nodeid: NodeId) -> Option<ZenohIdProto> {
        match self
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

    #[inline]
    pub(super) fn push_declaration_profile(&self, face: &FaceState) -> bool {
        // REVIEW(fuzzypixelz): this is from commit 1d8c0c0, but I believe it should be `face.whatami != WhatAmI::Router
        face.whatami == WhatAmI::Peer
    }
}

impl HatBaseTrait for Hat {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()> {
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

        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        let router_link_weights = config
            .routing()
            .router()
            .linkstate()
            .transport_weights()
            .clone();
        drop(config_guard);

        self.routers_net = Some(Network::new(
            ROUTERS_NET_NAME.to_string(),
            tables.zid,
            runtime.clone(),
            true,
            router_peers_failover_brokering,
            gossip,
            gossip_multihop,
            gossip_target,
            autoconnect,
            link_weights_from_config(router_link_weights, ROUTERS_NET_NAME)?,
        ));
        Ok(())
    }

    fn new_face(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatFace::new())
    }

    fn new_resource(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatContext::new())
    }

    fn new_local_face(
        &mut self,
        _tables: &mut TablesData,
        _tables_ref: &Arc<TablesLock>,
        _face: &mut Face,
        _send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        // Nothing to do
        Ok(())
    }

    fn new_transport_unicast_face(
        &mut self,
        _tables: &mut TablesData,
        tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        transport: &TransportUnicast,
        _send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        let link_id = if face.state.whatami == WhatAmI::Router {
            self.routers_net
                .as_mut()
                .unwrap()
                .add_link(transport.clone())
        } else {
            0
        };

        face_hat_mut!(&mut face.state).link_id = link_id;

        if face.state.whatami == WhatAmI::Router {
            self.schedule_compute_trees(tables_ref.clone())
        }
        Ok(())
    }

    fn close_face(
        &mut self,
        tables: &mut TablesData,
        tables_ref: &Arc<TablesLock>,
        face: &mut Arc<FaceState>,
        send_declare: &mut SendDeclare,
    ) {
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
            self.undeclare_simple_subscription(tables, &mut face_clone, &mut res, send_declare);

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
            self.undeclare_simple_queryable(tables, &mut face_clone, &mut res, send_declare);

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
            self.undeclare_simple_token(tables, &mut face_clone, &mut res, send_declare);
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
        tables.faces.remove(&face.id);

        if face.whatami == WhatAmI::Router {
            for (_, removed_node) in self.routers_net.as_mut().unwrap().remove_link(&face.zid) {
                self.pubsub_remove_node(tables, &removed_node.zid, send_declare);
                self.queries_remove_node(tables, &removed_node.zid, send_declare);
                self.token_remove_node(tables, &removed_node.zid, send_declare);
            }

            self.schedule_compute_trees(tables_ref.clone());
        }
    }

    fn handle_oam(
        &mut self,
        tables: &mut TablesData,
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
                            for (_, removed_node) in self
                                .routers_net
                                .as_mut()
                                .unwrap()
                                .link_states(list.link_states, zid)
                                .removed_nodes
                            {
                                self.pubsub_remove_node(tables, &removed_node.zid, send_declare);
                                self.queries_remove_node(tables, &removed_node.zid, send_declare);
                                self.token_remove_node(tables, &removed_node.zid, send_declare);
                            }

                            self.schedule_compute_trees(tables_ref.clone());
                        }
                        _ => tracing::error!(
                            "ERROR: OAM(Linkstate) received from non router node in router bound."
                        ),
                    };
                }
            }
        }

        Ok(())
    }

    #[inline]
    fn map_routing_context(
        &self,
        _tables: &TablesData,
        face: &FaceState,
        routing_context: NodeId,
    ) -> NodeId {
        if face.whatami == WhatAmI::Router {
            self.routers_net
                .as_ref()
                .unwrap()
                .get_local_context(routing_context, face_hat!(face).link_id)
        } else {
            0
        }
    }

    #[inline]
    fn ingress_filter(
        &self,
        _tables: &TablesData,
        _face: &FaceState,
        _expr: &mut RoutingExpr,
    ) -> bool {
        // FIXME(fuzzypixelz): ensure that there is a south-bound peer that can
        // handle duplicated messages through gossip from peers with multiple
        // connections to the same router gateway.
        true
    }

    #[inline]
    fn egress_filter(
        &self,
        _tables: &TablesData,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        _expr: &mut RoutingExpr,
    ) -> bool {
        src_face.id != out_face.id
            && (out_face.mcast_group.is_none() || src_face.mcast_group.is_none())
    }

    fn info(&self, kind: WhatAmI) -> String {
        match kind {
            WhatAmI::Router => self
                .routers_net
                .as_ref()
                .map(|net| net.dot())
                .unwrap_or_else(|| "graph {}".to_string()),
            _ => "graph {}".to_string(),
        }
    }

    fn update_from_config(
        &mut self,
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
        drop(config);
        if let Some(net) = self.routers_net.as_mut() {
            if net.update_link_weights(router_link_weights) {
                self.schedule_compute_trees(tables_ref.clone());
            }
        }
        Ok(())
    }

    fn links_info(&self) -> HashMap<ZenohIdProto, crate::net::protocol::linkstate::LinkInfo> {
        if let Some(net) = &self.routers_net {
            net.links_info()
        } else {
            HashMap::new()
        }
    }

    fn route_successor(&self, src: ZenohIdProto, dst: ZenohIdProto) -> Option<ZenohIdProto> {
        self.routers_net.as_ref()?.route_successor(src, dst)
    }

    fn route_successors(&self) -> Vec<SuccessorEntry> {
        self.routers_net
            .as_ref()
            .map(|net| net.route_successors())
            .unwrap_or_default()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct HatContext {
    router_subs: HashSet<ZenohIdProto>,
    router_qabls: HashMap<ZenohIdProto, QueryableInfoType>,
    router_tokens: HashSet<ZenohIdProto>,
}

impl HatContext {
    fn new() -> Self {
        Self {
            router_subs: HashSet::new(),
            router_qabls: HashMap::new(),
            router_tokens: HashSet::new(),
        }
    }
}

struct HatFace {
    link_id: usize,
    next_id: AtomicU32, // @TODO: manage rollover and uniqueness
    remote_interests: HashMap<InterestId, RemoteInterest>,
    local_subs: HashMap<Arc<Resource>, SubscriberId>,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, (QueryableId, QueryableInfoType)>,
    remote_qabls: HashMap<QueryableId, Arc<Resource>>,
    local_tokens: HashMap<Arc<Resource>, TokenId>,
    remote_tokens: HashMap<TokenId, Arc<Resource>>,
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

impl HatTrait for Hat {}
