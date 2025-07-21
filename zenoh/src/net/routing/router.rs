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
use cfg_if::cfg_if;
use uhlc::HLC;
use zenoh_config::{unwrap_or_default, Config};
use zenoh_protocol::core::{WhatAmI, ZenohIdProto};
// use zenoh_collections::Timer;
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast, TransportPeer};

pub(crate) use super::dispatcher::token::*;
pub use super::dispatcher::{pubsub::*, queries::*, resource::*};
use super::{
    dispatcher::{
        face::{Face, FaceState},
        tables::{TablesData, TablesLock},
    },
    hat,
    interceptor::InterceptorsChain,
    runtime::Runtime,
};
use crate::net::{
    primitives::{DeMux, DummyPrimitives, EPrimitives, McastMux, Mux},
    routing::dispatcher::{
        face::FaceStateBuilder,
        gateway::{Bound, BoundMap},
        tables::{self, Tables},
    },
};

pub struct Router {
    // whatami: WhatAmI,
    pub tables: Arc<TablesLock>,
}

impl Router {
    pub fn new(zid: ZenohIdProto, hlc: Option<Arc<HLC>>, config: &Config) -> ZResult<Self> {
        let whatami = config.mode().unwrap_or_default();
        let south_whatami = config.gateway.south.mode().unwrap_or_default();

        let eastwest = hat::new_hat(whatami, config, Bound::eastwest0());
        let south = hat::new_hat(south_whatami, config, Bound::south0());
        Ok(Router {
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables {
                    data: TablesData::new(
                        zid,
                        hlc,
                        config,
                        BoundMap::from_iter([
                            (Bound::eastwest0(), tables::HatData::new(whatami)),
                            (Bound::south0(), tables::HatData::new(south_whatami)),
                        ]),
                    )?,
                    hat: BoundMap::from_iter([
                        (Bound::eastwest0(), eastwest),
                        (Bound::south0(), south),
                    ]),
                }),
                ctrl_lock: Mutex::new(()),
                queries_lock: RwLock::new(()),
            }),
        })
    }

    pub fn init_link_state(&mut self, runtime: Runtime) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;
        tables.data.runtime = Some(Runtime::downgrade(&runtime));
        tables.hat.init(&mut tables.data, runtime)
    }

    pub(crate) fn new_primitives(
        &self,
        primitives: Arc<dyn EPrimitives + Send + Sync>,
    ) -> Arc<Face> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let zid = tables.data.zid;
        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let newface = tables
            .data
            .faces
            .entry(fid)
            .or_insert_with(|| {
                FaceStateBuilder::new(
                    fid,
                    zid,
                    // REVIEW(fuzzypixelz): is this correct?
                    Bound::south0(),
                    primitives.clone(),
                    tables.hat.new_face(),
                )
                .whatami(WhatAmI::Client)
                .local(true)
                .build()
            })
            .clone();
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };
        let mut declares = vec![];
        tables
            .hat
            .new_local_face(&mut tables.data, &self.tables, &mut face, &mut |p, m| {
                declares.push((p.clone(), m))
            })
            .unwrap();
        drop(wtables);
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
        }
        Arc::new(face)
    }

    pub fn new_transport_unicast(&self, transport: TransportUnicast) -> ZResult<Arc<DeMux>> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let whatami = transport.get_whatami()?;
        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let zid = transport.get_zid()?;
        #[cfg(feature = "stats")]
        let stats = transport.get_stats()?;

        let ingress = Arc::new(ArcSwap::new(InterceptorsChain::empty().into()));
        let mux = Arc::new(Mux::new(transport.clone(), InterceptorsChain::empty()));

        let newface = tables
            .data
            .faces
            .entry(fid)
            .or_insert_with(|| {
                let builder = FaceStateBuilder::new(fid, self.bound_of(transport), zid, mux.clone(), tables.hat.new_face())
                    .whatami(whatami)
                    .ingress_interceptors(ingress.clone());

                cfg_if! { if #[cfg(feature = "stats")] { builder.stats(stats).build() } else { builder.build() } }
            })
            .clone();
        newface.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };

        let _ = mux.face.set(Face::downgrade(&face));

        let mut declares = vec![];
        tables.hat.new_transport_unicast_face(
            &mut tables.data,
            &self.tables,
            &mut face,
            &transport,
            &mut |p, m| declares.push((p.clone(), m)),
        )?;
        drop(wtables);
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
        }

        Ok(Arc::new(DeMux::new(face, Some(transport), ingress)))
    }

    pub fn new_transport_multicast(&self, transport: TransportMulticast) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let mux = Arc::new(McastMux::new(transport.clone(), InterceptorsChain::empty()));
        let face = FaceStateBuilder::new(
            fid,
            ZenohIdProto::from_str("1").unwrap(),
            mux.clone(),
            tables.hat.new_face(),
        )
        .multicast_groups(transport)
        .build();
        face.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );
        let _ = mux.face.set(Face {
            state: face.clone(),
            tables: self.tables.clone(),
        });
        tables.data.mcast_groups.push(face);

        tables.data.disable_all_routes();
        Ok(())
    }

    pub fn new_peer_multicast(
        &self,
        transport: TransportMulticast,
        peer: TransportPeer,
    ) -> ZResult<Arc<DeMux>> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let interceptor = Arc::new(ArcSwap::new(InterceptorsChain::empty().into()));
        let face_state_builder = FaceStateBuilder::new(
            fid,
            peer.zid,
            Arc::new(DummyPrimitives),
            tables.hat.new_face(),
        )
        .multicast_groups(transport)
        .ingress_interceptors(interceptor.clone())
        .whatami(WhatAmI::Client);
        cfg_if! {
            if #[cfg(feature = "stats")] {
                let stats = transport.get_stats()?;
                let face_state = face_state_builder.stats(stats).build();
            } else {
                let face_state = face_state_builder.build();
            }
        }
        face_state.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );
        tables.data.mcast_faces.push(face_state.clone());

        tables.data.disable_all_routes();
        Ok(Arc::new(DeMux::new(
            Face {
                tables: self.tables.clone(),
                state: face_state,
            },
            None,
            interceptor,
        )))
    }

    fn bound_of(&self, transport: &TransportUnicast) -> ZResult<bool> {
        Ok(matches!(
            (self.whatami, transport.get_whatami()?),
            (WhatAmI::Router, WhatAmI::Client | WhatAmI::Peer) | (WhatAmI::Peer, WhatAmI::Client)
        ))
    }
}
