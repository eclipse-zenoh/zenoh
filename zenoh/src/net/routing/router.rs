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
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast, TransportPeer};

pub use super::dispatcher::{pubsub::*, queries::*, resource::*};
use super::{
    dispatcher::{
        face::Face,
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
        gateway::Bound,
        tables::{self, Tables},
    },
};

pub struct Router {
    // whatami: WhatAmI,
    pub tables: Arc<TablesLock>,
}

impl Router {
    pub fn new(zid: ZenohIdProto, hlc: Option<Arc<HLC>>, config: &Config) -> ZResult<Self> {
        let hats = [(Bound::North, config.mode().unwrap_or_default())];

        // TODO(regions): add gateway config and use it here

        tracing::trace!(?hats, "new router");

        Ok(Router {
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables {
                    data: TablesData::new(
                        zid,
                        hlc,
                        config,
                        hats.iter()
                            .copied()
                            .map(|(b, wai)| (b, tables::HatTablesData::new(wai)))
                            .collect(),
                    )?,
                    hats: hats
                        .iter()
                        .copied()
                        .map(|(b, wai)| (b, hat::new_hat(wai, config, b)))
                        .collect(),
                }),
                ctrl_lock: Mutex::new(()),
                queries_lock: RwLock::new(()),
            }),
        })
    }

    pub fn init_hats(&mut self, runtime: Runtime) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;
        tables.data.runtime = Some(Runtime::downgrade(&runtime));

        for (_, hat) in tables.hats.iter_mut() {
            hat.init(&mut tables.data, runtime.clone())?
        }

        Ok(())
    }

    pub(crate) fn new_primitives(
        &self,
        primitives: Arc<dyn EPrimitives + Send + Sync>,
        bound: Bound,
    ) -> Arc<Face> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let zid = tables.data.zid;
        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let newface = tables.data.hats[bound]
            .faces
            .entry(fid)
            .or_insert_with(|| {
                Arc::new(
                    FaceStateBuilder::new(
                        fid,
                        zid,
                        bound,
                        primitives.clone(),
                        tables.hats.map(|hat| hat.new_face()),
                    )
                    .whatami(WhatAmI::Client)
                    .local(true)
                    .build(),
                )
            })
            .clone();
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };
        let mut declares = vec![];
        tables.hats[bound]
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

    pub fn new_transport_unicast(
        &self,
        transport: TransportUnicast,
        bound: Bound,
    ) -> ZResult<Arc<DeMux>> {
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

        let newface = tables.data.hats[bound]
            .faces
            .entry(fid)
            .or_insert_with(|| {
                let builder = FaceStateBuilder::new(
                    fid,
                    zid,
                    bound,
                    mux.clone(),
                    tables.hats.map(|hat| hat.new_face()),
                )
                .whatami(whatami)
                .ingress_interceptors(ingress.clone());

                #[cfg(feature = "stats")]
                let builder = builder.stats(stats);

                Arc::new(builder.build())
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
        tables.hats[bound].new_transport_unicast_face(
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

    pub fn new_transport_multicast(
        &self,
        transport: TransportMulticast,
        bound: Bound,
    ) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let mux = Arc::new(McastMux::new(transport.clone(), InterceptorsChain::empty()));
        let face = Arc::new(
            FaceStateBuilder::new(
                fid,
                ZenohIdProto::from_str("1").unwrap(),
                bound,
                mux.clone(),
                tables.hats.map(|hat| hat.new_face()),
            )
            .multicast_groups(transport)
            .build(),
        );
        face.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );
        let _ = mux.face.set(Face {
            state: face.clone(),
            tables: self.tables.clone(),
        });
        tables.data.hats[bound].mcast_groups.push(face);

        tables.data.hats[bound].disable_all_routes();
        Ok(())
    }

    pub fn new_peer_multicast(
        &self,
        transport: TransportMulticast,
        peer: TransportPeer,
        bound: Bound,
    ) -> ZResult<Arc<DeMux>> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let fid = tables.data.face_counter;
        tables.data.face_counter += 1;
        let interceptor = Arc::new(ArcSwap::new(InterceptorsChain::empty().into()));

        #[cfg(feature = "stats")]
        let stats = transport.get_stats()?;

        let face_state_builder = FaceStateBuilder::new(
            fid,
            peer.zid,
            bound,
            Arc::new(DummyPrimitives),
            tables.hats.map(|hat| hat.new_face()),
        )
        .multicast_groups(transport)
        .ingress_interceptors(interceptor.clone())
        .whatami(WhatAmI::Client);

        #[cfg(feature = "stats")]
        let face_state_builder = face_state_builder.stats(stats);
        let face_state = Arc::new(face_state_builder.build());

        face_state.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );
        tables.data.hats[bound].mcast_faces.push(face_state.clone());

        tables.data.hats[bound].disable_all_routes();
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
