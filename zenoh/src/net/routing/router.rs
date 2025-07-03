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
        let hat_code = hat::new_hat(whatami, config);
        Ok(Router {
            // whatami,
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables::new(zid, whatami, hlc, config, hat_code.as_ref())?),
                hat_code,
                ctrl_lock: Mutex::new(()),
                queries_lock: RwLock::new(()),
            }),
        })
    }

    pub fn init_link_state(&mut self, runtime: Runtime) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        tables.runtime = Some(Runtime::downgrade(&runtime));
        self.tables.hat_code.init(&mut tables, runtime)
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
                    self.tables.hat_code.new_face(),
                    true,
                )
            })
            .clone();
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };
        let mut declares = vec![];
        self.tables
            .hat_code
            .new_local_face(&mut tables, &self.tables, &mut face, &mut |p, m| {
                declares.push((p.clone(), m))
            })
            .unwrap();
        drop(tables);
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
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

        let ingress = Arc::new(ArcSwap::new(InterceptorsChain::empty().into()));
        let mux = Arc::new(Mux::new(transport.clone(), InterceptorsChain::empty()));
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
                    self.tables.hat_code.new_face(),
                    false,
                )
            })
            .clone();
        newface.set_interceptors_from_factories(
            &tables.interceptors,
            tables.next_interceptor_version.load(Ordering::SeqCst),
        );
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };

        let _ = mux.face.set(Face::downgrade(&face));

        let mut declares = vec![];
        self.tables.hat_code.new_transport_unicast_face(
            &mut tables,
            &self.tables,
            &mut face,
            &transport,
            &mut |p, m| declares.push((p.clone(), m)),
        )?;
        drop(tables);
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
        }

        Ok(Arc::new(DeMux::new(face, Some(transport), ingress)))
    }

    pub fn new_transport_multicast(&self, transport: TransportMulticast) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let mux = Arc::new(McastMux::new(transport.clone(), InterceptorsChain::empty()));
        let face = FaceState::new(
            fid,
            ZenohIdProto::from_str("1").unwrap(),
            WhatAmI::Peer,
            #[cfg(feature = "stats")]
            None,
            mux.clone(),
            Some(transport),
            None,
            self.tables.hat_code.new_face(),
            false,
        );
        face.set_interceptors_from_factories(
            &tables.interceptors,
            tables.next_interceptor_version.load(Ordering::SeqCst),
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
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
        let fid = tables.face_counter;
        tables.face_counter += 1;
        let interceptor = Arc::new(ArcSwap::new(InterceptorsChain::empty().into()));
        let face_state = FaceState::new(
            fid,
            peer.zid,
            WhatAmI::Client, // Quick hack
            #[cfg(feature = "stats")]
            Some(transport.get_stats().unwrap()),
            Arc::new(DummyPrimitives),
            Some(transport),
            Some(interceptor.clone()),
            self.tables.hat_code.new_face(),
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
            interceptor,
        )))
    }
}
