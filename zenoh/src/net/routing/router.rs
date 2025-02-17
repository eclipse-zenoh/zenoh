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
use std::{
    str::FromStr,
    sync::{Arc, Mutex, RwLock},
};

use arc_swap::ArcSwap;
use uhlc::HLC;
use zenoh_config::Config;
use zenoh_protocol::core::{WhatAmI, ZenohIdProto};
// use zenoh_collections::Timer;
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast, TransportPeer};

pub(crate) use super::dispatcher::token::*;
pub use super::dispatcher::{pubsub::*, queries::*, resource::*};
use super::{
    dispatcher::{
        face::{Face, FaceState},
        tables::{Tables, TablesLock},
    },
    hat,
    interceptor::{EgressInterceptor, InterceptorsChain},
    runtime::Runtime,
};
use crate::net::{
    primitives::{DeMux, DummyPrimitives, EPrimitives, McastMux, Mux},
    routing::interceptor::IngressInterceptor,
};

pub struct Router {
    // whatami: WhatAmI,
    pub tables: Arc<TablesLock>,
}

impl Router {
    pub fn new(
        zid: ZenohIdProto,
        whatami: WhatAmI,
        hlc: Option<Arc<HLC>>,
        config: &Config,
    ) -> ZResult<Self> {
        Ok(Router {
            // whatami,
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables::new(zid, whatami, hlc, config)?),
                ctrl_lock: Mutex::new(hat::new_hat(whatami, config)),
                queries_lock: RwLock::new(()),
            }),
        })
    }

    pub fn init_link_state(&mut self, runtime: Runtime) -> ZResult<()> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        tables.runtime = Some(Runtime::downgrade(&runtime));
        ctrl_lock.init(&mut tables, runtime)
    }

    pub(crate) fn new_primitives(
        &self,
        primitives: Arc<dyn EPrimitives + Send + Sync>,
    ) -> Arc<Face> {
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
                    None,
                    ctrl_lock.new_face(),
                )
            })
            .clone();
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };
        let mut declares = vec![];
        ctrl_lock
            .new_local_face(&mut tables, &self.tables, &mut face, &mut |p, m| {
                declares.push((p.clone(), m))
            })
            .unwrap();
        drop(tables);
        drop(ctrl_lock);
        for (p, m) in declares {
            p.send_declare(m);
        }
        Arc::new(face)
    }

    pub fn new_transport_unicast(&self, transport: TransportUnicast) -> ZResult<Arc<DeMux>> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);

        let whatami = transport.get_whatami()?;
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let zid = transport.get_zid()?;
        #[cfg(feature = "stats")]
        let stats = transport.get_stats()?;
        let (ingress, egress): (Vec<_>, Vec<_>) = tables
            .interceptors
            .iter()
            .map(|itor| itor.new_transport_unicast(&transport))
            .unzip();
        let (ingress, egress) = (
            InterceptorsChain::from(ingress.into_iter().flatten().collect::<Vec<_>>()),
            InterceptorsChain::from(egress.into_iter().flatten().collect::<Vec<_>>()),
        );
        let ingress = Arc::new(ArcSwap::new(ingress.into()));
        let mux = Arc::new(Mux::new(transport.clone(), egress));
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
                    mux.clone(),
                    None,
                    Some(ingress.clone()),
                    ctrl_lock.new_face(),
                )
            })
            .clone();
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };

        let _ = mux.face.set(Face::downgrade(&face));

        let mut declares = vec![];
        ctrl_lock.new_transport_unicast_face(
            &mut tables,
            &self.tables,
            &mut face,
            &transport,
            &mut |p, m| declares.push((p.clone(), m)),
        )?;
        drop(tables);
        drop(ctrl_lock);
        for (p, m) in declares {
            p.send_declare(m);
        }

        Ok(Arc::new(DeMux::new(face, Some(transport), ingress)))
    }

    pub fn new_transport_multicast(&self, transport: TransportMulticast) -> ZResult<()> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let interceptor = InterceptorsChain::from(
            tables
                .interceptors
                .iter()
                .filter_map(|itor| itor.new_transport_multicast(&transport))
                .collect::<Vec<EgressInterceptor>>(),
        );
        let mux = Arc::new(McastMux::new(transport.clone(), interceptor.into()));
        let face = FaceState::new(
            fid,
            ZenohIdProto::from_str("1").unwrap(),
            WhatAmI::Peer,
            #[cfg(feature = "stats")]
            None,
            mux.clone(),
            Some(transport),
            None,
            ctrl_lock.new_face(),
        );
        let _ = mux.face.set(Face {
            state: face.clone(),
            tables: self.tables.clone(),
        });
        tables.mcast_groups.push(face);

        tables.disable_all_routes();
        Ok(())
    }

    pub fn new_peer_multicast(
        &self,
        transport: TransportMulticast,
        peer: TransportPeer,
    ) -> ZResult<Arc<DeMux>> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let interceptor = InterceptorsChain::from(
            tables
                .interceptors
                .iter()
                .filter_map(|itor| itor.new_peer_multicast(&transport))
                .collect::<Vec<IngressInterceptor>>(),
        );
        let interceptor = Arc::new(ArcSwap::new(interceptor.into()));
        let face_state = FaceState::new(
            fid,
            peer.zid,
            WhatAmI::Client, // Quick hack
            #[cfg(feature = "stats")]
            Some(transport.get_stats().unwrap()),
            Arc::new(DummyPrimitives),
            Some(transport),
            Some(interceptor.clone()),
            ctrl_lock.new_face(),
        );
        tables.mcast_faces.push(face_state.clone());

        tables.disable_all_routes();
        Ok(Arc::new(DeMux::new(
            Face {
                tables: self.tables.clone(),
                state: face_state,
            },
            None,
            interceptor,
        )))
    }
}
