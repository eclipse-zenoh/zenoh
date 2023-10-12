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
use super::dispatcher::face::{Face, FaceState};
pub use super::dispatcher::pubsub::*;
pub use super::dispatcher::queries::*;
pub use super::dispatcher::resource::*;
use super::dispatcher::tables::Tables;
use super::dispatcher::tables::TablesLock;
use super::hat::network::{shared_nodes, Network};
pub use super::hat::pubsub::*;
pub use super::hat::queries::*;
use super::runtime::Runtime;
use crate::net::codec::Zenoh080Routing;
use crate::net::protocol::linkstate::LinkStateList;
use std::any::Any;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::time::Duration;
use uhlc::HLC;
use zenoh_link::Link;
use zenoh_protocol::common::ZExtBody;
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher, ZenohId};
use zenoh_protocol::network::oam::id::OAM_LINKSTATE;
use zenoh_protocol::network::{NetworkBody, NetworkMessage};
use zenoh_transport::{
    DeMux, DummyPrimitives, McastMux, Mux, Primitives, TransportMulticast, TransportPeer,
    TransportPeerEventHandler, TransportUnicast,
};
// use zenoh_collections::Timer;
use zenoh_result::ZResult;

pub struct Router {
    whatami: WhatAmI,
    pub tables: Arc<TablesLock>,
}

impl Router {
    pub fn new(
        zid: ZenohId,
        whatami: WhatAmI,
        hlc: Option<Arc<HLC>>,
        drop_future_timestamp: bool,
        router_peers_failover_brokering: bool,
        queries_default_timeout: Duration,
    ) -> Self {
        Router {
            whatami,
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables::new(
                    zid,
                    whatami,
                    hlc,
                    drop_future_timestamp,
                    router_peers_failover_brokering,
                    queries_default_timeout,
                )),
                ctrl_lock: Mutex::new(()),
                queries_lock: RwLock::new(()),
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn init_link_state(
        &mut self,
        runtime: Runtime,
        router_full_linkstate: bool,
        peer_full_linkstate: bool,
        router_peers_failover_brokering: bool,
        gossip: bool,
        gossip_multihop: bool,
        autoconnect: WhatAmIMatcher,
    ) {
        let mut tables = zwrite!(self.tables.tables);
        if router_full_linkstate | gossip {
            tables.hat.routers_net = Some(Network::new(
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
            tables.hat.peers_net = Some(Network::new(
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
            tables.hat.shared_nodes = shared_nodes(
                tables.hat.routers_net.as_ref().unwrap(),
                tables.hat.peers_net.as_ref().unwrap(),
            );
        }
    }

    pub fn new_primitives(&self, primitives: Arc<dyn Primitives + Send + Sync>) -> Arc<Face> {
        Arc::new(Face {
            tables: self.tables.clone(),
            state: {
                let ctrl_lock = zlock!(self.tables.ctrl_lock);
                let mut tables = zwrite!(self.tables.tables);
                let zid = tables.zid;
                let face = tables
                    .open_face(zid, WhatAmI::Client, primitives)
                    .upgrade()
                    .unwrap();
                drop(tables);
                drop(ctrl_lock);
                face
            },
        })
    }

    pub fn new_transport_unicast(
        &self,
        transport: TransportUnicast,
    ) -> ZResult<Arc<LinkStateInterceptor>> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let whatami = transport.get_whatami()?;

        let link_id = match (self.whatami, whatami) {
            (WhatAmI::Router, WhatAmI::Router) => tables
                .hat
                .routers_net
                .as_mut()
                .unwrap()
                .add_link(transport.clone()),
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if let Some(net) = tables.hat.peers_net.as_mut() {
                    net.add_link(transport.clone())
                } else {
                    0
                }
            }
            _ => 0,
        };

        if tables.hat.full_net(WhatAmI::Router) && tables.hat.full_net(WhatAmI::Peer) {
            tables.hat.shared_nodes = shared_nodes(
                tables.hat.routers_net.as_ref().unwrap(),
                tables.hat.peers_net.as_ref().unwrap(),
            );
        }

        let handler = Arc::new(LinkStateInterceptor::new(
            transport.clone(),
            self.tables.clone(),
            Face {
                tables: self.tables.clone(),
                state: tables
                    .open_net_face(
                        transport.get_zid().unwrap(),
                        whatami,
                        #[cfg(feature = "stats")]
                        transport.get_stats().unwrap(),
                        Arc::new(Mux::new(transport)),
                        link_id,
                    )
                    .upgrade()
                    .unwrap(),
            },
        ));

        match (self.whatami, whatami) {
            (WhatAmI::Router, WhatAmI::Router) => {
                tables
                    .hat
                    .schedule_compute_trees(self.tables.clone(), WhatAmI::Router);
            }
            (WhatAmI::Router, WhatAmI::Peer)
            | (WhatAmI::Peer, WhatAmI::Router)
            | (WhatAmI::Peer, WhatAmI::Peer) => {
                if tables.hat.full_net(WhatAmI::Peer) {
                    tables
                        .hat
                        .schedule_compute_trees(self.tables.clone(), WhatAmI::Peer);
                }
            }
            _ => (),
        }
        drop(tables);
        drop(ctrl_lock);
        Ok(handler)
    }

    pub fn new_transport_multicast(&self, transport: TransportMulticast) -> ZResult<()> {
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        tables.mcast_groups.push(FaceState::new(
            fid,
            ZenohId::from_str("1").unwrap(),
            WhatAmI::Peer,
            #[cfg(feature = "stats")]
            None,
            Arc::new(McastMux::new(transport.clone())),
            0,
            Some(transport),
        ));

        // recompute routes
        let mut root_res = tables.root_res.clone();
        compute_data_routes_from(&mut tables, &mut root_res);
        Ok(())
    }

    pub fn new_peer_multicast(
        &self,
        transport: TransportMulticast,
        peer: TransportPeer,
    ) -> ZResult<Arc<DeMux<Face>>> {
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let face_state = FaceState::new(
            fid,
            peer.zid,
            WhatAmI::Client, // Quick hack
            #[cfg(feature = "stats")]
            Some(transport.get_stats().unwrap()),
            Arc::new(DummyPrimitives),
            0,
            Some(transport),
        );
        tables.mcast_faces.push(face_state.clone());

        // recompute routes
        let mut root_res = tables.root_res.clone();
        compute_data_routes_from(&mut tables, &mut root_res);
        Ok(Arc::new(DeMux::new(Face {
            tables: self.tables.clone(),
            state: face_state,
        })))
    }
}

pub struct LinkStateInterceptor {
    pub(crate) transport: TransportUnicast,
    pub(crate) tables: Arc<TablesLock>,
    pub(crate) face: Face,
    pub(crate) demux: DeMux<Face>,
}

impl LinkStateInterceptor {
    fn new(transport: TransportUnicast, tables: Arc<TablesLock>, face: Face) -> Self {
        LinkStateInterceptor {
            transport,
            tables,
            face: face.clone(),
            demux: DeMux::new(face),
        }
    }
}

impl TransportPeerEventHandler for LinkStateInterceptor {
    fn handle_message(&self, msg: NetworkMessage) -> ZResult<()> {
        log::trace!("Recv {:?}", msg);
        match msg.body {
            NetworkBody::OAM(oam) => {
                if oam.id == OAM_LINKSTATE {
                    if let ZExtBody::ZBuf(buf) = oam.body {
                        if let Ok(zid) = self.transport.get_zid() {
                            use zenoh_buffers::reader::HasReader;
                            use zenoh_codec::RCodec;
                            let codec = Zenoh080Routing::new();
                            let mut reader = buf.reader();
                            let list: LinkStateList = codec.read(&mut reader).unwrap();

                            let ctrl_lock = zlock!(self.tables.ctrl_lock);
                            let mut tables = zwrite!(self.tables.tables);
                            let whatami = self.transport.get_whatami()?;
                            match (tables.whatami, whatami) {
                                (WhatAmI::Router, WhatAmI::Router) => {
                                    for (_, removed_node) in tables
                                        .hat
                                        .routers_net
                                        .as_mut()
                                        .unwrap()
                                        .link_states(list.link_states, zid)
                                        .removed_nodes
                                    {
                                        pubsub_remove_node(
                                            &mut tables,
                                            &removed_node.zid,
                                            WhatAmI::Router,
                                        );
                                        queries_remove_node(
                                            &mut tables,
                                            &removed_node.zid,
                                            WhatAmI::Router,
                                        );
                                    }

                                    if tables.hat.full_net(WhatAmI::Peer) {
                                        tables.hat.shared_nodes = shared_nodes(
                                            tables.hat.routers_net.as_ref().unwrap(),
                                            tables.hat.peers_net.as_ref().unwrap(),
                                        );
                                    }

                                    tables.hat.schedule_compute_trees(
                                        self.tables.clone(),
                                        WhatAmI::Router,
                                    );
                                }
                                (WhatAmI::Router, WhatAmI::Peer)
                                | (WhatAmI::Peer, WhatAmI::Router)
                                | (WhatAmI::Peer, WhatAmI::Peer) => {
                                    if let Some(net) = tables.hat.peers_net.as_mut() {
                                        let changes = net.link_states(list.link_states, zid);
                                        if tables.hat.full_net(WhatAmI::Peer) {
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
                                                tables.hat.shared_nodes = shared_nodes(
                                                    tables.hat.routers_net.as_ref().unwrap(),
                                                    tables.hat.peers_net.as_ref().unwrap(),
                                                );
                                            }

                                            tables.hat.schedule_compute_trees(
                                                self.tables.clone(),
                                                WhatAmI::Peer,
                                            );
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
            _ => self.demux.handle_message(msg),
        }
    }

    fn new_link(&self, _link: Link) {}

    fn del_link(&self, _link: Link) {}

    fn closing(&self) {
        self.demux.closing();
        let tables_ref = self.tables.clone();
        match (self.transport.get_zid(), self.transport.get_whatami()) {
            (Ok(zid), Ok(whatami)) => {
                let ctrl_lock = zlock!(tables_ref.ctrl_lock);
                let mut tables = zwrite!(tables_ref.tables);
                match (tables.whatami, whatami) {
                    (WhatAmI::Router, WhatAmI::Router) => {
                        for (_, removed_node) in
                            tables.hat.routers_net.as_mut().unwrap().remove_link(&zid)
                        {
                            pubsub_remove_node(&mut tables, &removed_node.zid, WhatAmI::Router);
                            queries_remove_node(&mut tables, &removed_node.zid, WhatAmI::Router);
                        }

                        if tables.hat.full_net(WhatAmI::Peer) {
                            tables.hat.shared_nodes = shared_nodes(
                                tables.hat.routers_net.as_ref().unwrap(),
                                tables.hat.peers_net.as_ref().unwrap(),
                            );
                        }

                        tables
                            .hat
                            .schedule_compute_trees(tables_ref.clone(), WhatAmI::Router);
                    }
                    (WhatAmI::Router, WhatAmI::Peer)
                    | (WhatAmI::Peer, WhatAmI::Router)
                    | (WhatAmI::Peer, WhatAmI::Peer) => {
                        if tables.hat.full_net(WhatAmI::Peer) {
                            for (_, removed_node) in
                                tables.hat.peers_net.as_mut().unwrap().remove_link(&zid)
                            {
                                pubsub_remove_node(&mut tables, &removed_node.zid, WhatAmI::Peer);
                                queries_remove_node(&mut tables, &removed_node.zid, WhatAmI::Peer);
                            }

                            if tables.whatami == WhatAmI::Router {
                                tables.hat.shared_nodes = shared_nodes(
                                    tables.hat.routers_net.as_ref().unwrap(),
                                    tables.hat.peers_net.as_ref().unwrap(),
                                );
                            }

                            tables
                                .hat
                                .schedule_compute_trees(tables_ref.clone(), WhatAmI::Peer);
                        } else if let Some(net) = tables.hat.peers_net.as_mut() {
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
    }

    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}
