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
    sync::{atomic::Ordering, Arc, Mutex, RwLock},
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
    interceptor::InterceptorsChain,
    runtime::Runtime,
};
use crate::net::primitives::{DeMux, DummyPrimitives, EPrimitives, McastMux, Mux};

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
        let mut face = tables
            .faces
            .entry(fid)
            .or_insert_with(|| Face {
                tables: self.tables.clone(),
                state: FaceState::new(
                    fid,
                    zid,
                    WhatAmI::Client,
                    #[cfg(feature = "stats")]
                    None,
                    primitives.clone(),
                    None,
                    None,
                    None,
                    ctrl_lock.new_face(),
                    true,
                ),
            })
            .clone();
        tracing::debug!("New {}", face);

        let mut declares = vec![];
        ctrl_lock
            .new_local_face(&mut tables, &self.tables, &mut face, &mut |p, m, r| {
                declares.push((p.clone(), m, r))
            })
            .unwrap();
        drop(tables);
        drop(ctrl_lock);
        for (p, mut m, r) in declares {
            p.intercept_declare(&mut m, r.as_ref());
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

        let ingress = ArcSwap::new(InterceptorsChain::empty().into());
        let egress = ArcSwap::new(InterceptorsChain::empty().into());
        let mux = Arc::new(Mux::new(transport.clone()));
        let mut face = tables
            .faces
            .entry(fid)
            .or_insert_with(|| Face {
                tables: self.tables.clone(),
                state: FaceState::new(
                    fid,
                    zid,
                    whatami,
                    #[cfg(feature = "stats")]
                    Some(stats),
                    mux.clone(),
                    None,
                    Some(ingress),
                    Some(egress),
                    ctrl_lock.new_face(),
                    false,
                ),
            })
            .clone();
        face.state.set_interceptors_from_factories(
            &tables.interceptors,
            tables.next_interceptor_version.load(Ordering::SeqCst),
        );
        tracing::debug!("New {}", face);

        let mut declares = vec![];
        ctrl_lock.new_transport_unicast_face(
            &mut tables,
            &self.tables,
            &mut face,
            &transport,
            &mut |p, m, r| declares.push((p.clone(), m, r)),
        )?;
        drop(tables);
        drop(ctrl_lock);
        for (p, mut m, r) in declares {
            p.intercept_declare(&mut m, r.as_ref());
        }

        Ok(Arc::new(DeMux::new(face, Some(transport))))
    }

    pub fn new_transport_multicast(&self, transport: TransportMulticast) -> ZResult<()> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let mux = Arc::new(McastMux::new(transport.clone()));
        let egress = ArcSwap::new(Arc::new(InterceptorsChain::empty()));
        let face = FaceState::new(
            fid,
            ZenohIdProto::from_str("1").unwrap(),
            WhatAmI::Peer,
            #[cfg(feature = "stats")]
            None,
            mux.clone(),
            Some(transport),
            None,
            Some(egress),
            ctrl_lock.new_face(),
            false,
        );
        face.set_interceptors_from_factories(
            &tables.interceptors,
            tables.next_interceptor_version.load(Ordering::SeqCst),
        );
        let face = Face {
            state: face.clone(),
            tables: self.tables.clone(),
        };
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
        let ingress = ArcSwap::new(InterceptorsChain::empty().into());
        let egress = ArcSwap::new(InterceptorsChain::empty().into());
        let face_state = FaceState::new(
            fid,
            peer.zid,
            WhatAmI::Client, // Quick hack
            #[cfg(feature = "stats")]
            Some(transport.get_stats().unwrap()),
            Arc::new(DummyPrimitives),
            Some(transport),
            Some(ingress),
            Some(egress),
            ctrl_lock.new_face(),
            false,
        );
        face_state.set_interceptors_from_factories(
            &tables.interceptors,
            tables.next_interceptor_version.load(Ordering::SeqCst),
        );
        tables.mcast_faces.push(face_state.clone());

        tables.disable_all_routes();
        Ok(Arc::new(DeMux::new(
            Face {
                tables: self.tables.clone(),
                state: face_state,
            },
            None,
        )))
    }
}
