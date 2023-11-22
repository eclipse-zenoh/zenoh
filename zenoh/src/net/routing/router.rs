use crate::net::primitives::DeMux;
use crate::net::primitives::DummyPrimitives;
use crate::net::primitives::McastMux;
use crate::net::primitives::Mux;
use crate::net::primitives::Primitives;

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
use super::hat;
use super::runtime::Runtime;
use std::any::Any;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::time::Duration;
use uhlc::HLC;
use zenoh_link::Link;
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher, ZenohId};
use zenoh_protocol::network::{NetworkBody, NetworkMessage};
use zenoh_transport::{
    TransportMulticast, TransportPeer, TransportPeerEventHandler, TransportUnicast,
};
// use zenoh_collections::Timer;
use zenoh_result::ZResult;

pub struct Router {
    // whatami: WhatAmI,
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
            // whatami,
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables::new(
                    zid,
                    whatami,
                    hlc,
                    drop_future_timestamp,
                    router_peers_failover_brokering,
                    queries_default_timeout,
                )),
                ctrl_lock: Mutex::new(hat::new_hat(whatami)),
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
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        ctrl_lock.init(
            &mut tables,
            runtime,
            router_full_linkstate,
            peer_full_linkstate,
            router_peers_failover_brokering,
            gossip,
            gossip_multihop,
            autoconnect,
        )
    }

    pub fn new_primitives(&self, primitives: Arc<dyn Primitives + Send + Sync>) -> Arc<Face> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);

        let zid = tables.zid;
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let newface = tables
            .faces
            .entry(fid)
            .or_insert_with(|| {
                FaceState::new(
                    fid,
                    zid,
                    WhatAmI::Client,
                    #[cfg(feature = "stats")]
                    None,
                    primitives.clone(),
                    None,
                    ctrl_lock.new_face(),
                )
            })
            .clone();
        log::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };
        ctrl_lock
            .new_local_face(&mut tables, &self.tables, &mut face)
            .unwrap();
        drop(tables);
        drop(ctrl_lock);
        Arc::new(face)
    }

    pub fn new_transport_unicast(
        &self,
        transport: TransportUnicast,
    ) -> ZResult<Arc<LinkStateInterceptor>> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);

        let whatami = transport.get_whatami()?;
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let zid = transport.get_zid()?;
        #[cfg(feature = "stats")]
        let stats = transport.get_stats()?;
        let newface = tables
            .faces
            .entry(fid)
            .or_insert_with(|| {
                FaceState::new(
                    fid,
                    zid,
                    whatami,
                    #[cfg(feature = "stats")]
                    Some(stats),
                    Arc::new(Mux::new(transport.clone())),
                    None,
                    ctrl_lock.new_face(),
                )
            })
            .clone();
        log::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };

        ctrl_lock.new_transport_unicast_face(&mut tables, &self.tables, &mut face, &transport)?;

        Ok(Arc::new(LinkStateInterceptor::new(
            transport.clone(),
            self.tables.clone(),
            face,
        )))
    }

    pub fn new_transport_multicast(&self, transport: TransportMulticast) -> ZResult<()> {
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let hat_face = tables.hat_code.new_face();
        tables.mcast_groups.push(FaceState::new(
            fid,
            ZenohId::from_str("1").unwrap(),
            WhatAmI::Peer,
            #[cfg(feature = "stats")]
            None,
            Arc::new(McastMux::new(transport.clone())),
            Some(transport),
            hat_face,
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
            Some(transport),
            tables.hat_code.new_face(),
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
                let ctrl_lock = zlock!(self.tables.ctrl_lock);
                let mut tables = zwrite!(self.tables.tables);
                ctrl_lock.handle_oam(&mut tables, &self.tables, oam, &self.transport)
            }
            _ => self.demux.handle_message(msg),
        }
    }

    fn new_link(&self, _link: Link) {}

    fn del_link(&self, _link: Link) {}

    fn closing(&self) {
        self.demux.closing();
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let _ = ctrl_lock.closing(&mut tables, &self.tables, &self.transport);
    }

    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}
