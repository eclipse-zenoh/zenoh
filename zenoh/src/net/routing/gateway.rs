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

use arc_swap::ArcSwapOption;
use uhlc::HLC;
use zenoh_config::{
    gateway::{GatewayPresetConf, GatewaySouthConf},
    ExpandedConfig,
};
use zenoh_protocol::core::{Bound, Region, WhatAmI, ZenohIdProto};
use zenoh_result::ZResult;
use zenoh_transport::{multicast::TransportMulticast, unicast::TransportUnicast, TransportPeer};

pub use super::dispatcher::{pubsub::*, resource::*};
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
    routing::{
        dispatcher::{
            face::FaceStateBuilder,
            region::RegionMap,
            tables::{self, Tables},
        },
        hat::{DispatcherContext, HatTrait},
    },
};

pub struct GatewayBuilder<'c> {
    config: &'c ExpandedConfig,
    hlc: Option<Arc<HLC>>,
    #[cfg(feature = "stats")]
    stats: Option<zenoh_stats::StatsRegistry>,
    #[cfg(test)]
    subregions: Option<Vec<Region>>,
    #[cfg(test)]
    disable_async_tree_computation: bool,
}

impl<'conf> GatewayBuilder<'conf> {
    pub fn new(config: &'conf ExpandedConfig) -> GatewayBuilder<'conf> {
        Self {
            config,
            hlc: None,
            #[cfg(feature = "stats")]
            stats: None,
            #[cfg(test)]
            subregions: None,
            #[cfg(test)]
            disable_async_tree_computation: false,
        }
    }

    pub fn hlc(mut self, hlc: Arc<HLC>) -> Self {
        self.hlc = Some(hlc);
        self
    }

    #[cfg(test)]
    pub fn subregions(mut self, subregions: Vec<Region>) -> Self {
        self.subregions.replace(subregions);
        self
    }

    #[cfg(feature = "stats")]
    pub fn stats(mut self, stats: zenoh_stats::StatsRegistry) -> Self {
        self.stats = Some(stats);
        self
    }

    #[cfg(test)]
    pub fn disable_async_tree_computation(mut self, value: bool) -> Self {
        self.disable_async_tree_computation = value;
        self
    }

    pub fn build(self) -> ZResult<Gateway> {
        let mode = self.config.mode();

        let mut regions = vec![Region::North];

        let mut set_regions_with_config = || {
            match self.config.gateway.south.clone().unwrap_or_default() {
                GatewaySouthConf::Preset(GatewayPresetConf::Auto) => match mode {
                    WhatAmI::Router => {
                        for mode in [WhatAmI::Client, WhatAmI::Peer] {
                            regions.push(Region::default_south(mode));
                        }
                    }
                    WhatAmI::Peer => {
                        regions.push(Region::default_south(WhatAmI::Client));
                    }
                    WhatAmI::Client => {}
                },
                GatewaySouthConf::Custom(subregions) => {
                    for (id, _) in subregions.iter().enumerate() {
                        // NOTE(regions): we create three hats per subregion.
                        // If memory usage is an issue, we should create then lazily.
                        for mode in [WhatAmI::Client, WhatAmI::Peer, WhatAmI::Router] {
                            regions.push(Region::South { id, mode });
                        }
                    }
                }
            }

            regions.push(Region::Local);
        };

        #[cfg(not(test))]
        set_regions_with_config();

        #[cfg(test)]
        if let Some(subregions) = self.subregions {
            regions.extend_from_slice(&subregions);
        } else {
            set_regions_with_config()
        }

        let zid = ZenohIdProto::from(self.config.id());

        #[cfg(feature = "stats")]
        let stats = self
            .stats
            .unwrap_or_else(|| zenoh_stats::StatsRegistry::new(zid, mode, &*crate::LONG_VERSION));

        tracing::debug!(?regions);

        let hats = regions
            .iter()
            .copied()
            .map(|region| -> (Region, Box<dyn HatTrait + Send + Sync>) {
                (
                    region,
                    match (region.bound(), region.mode().unwrap_or(mode)) {
                        (Bound::North, WhatAmI::Client) => Box::new(hat::client::Hat::new(region)),
                        (Bound::South, WhatAmI::Client) => Box::new(hat::broker::Hat::new(region)),
                        (_, WhatAmI::Peer) => Box::new(hat::peer::Hat::new(region)),
                        (_, WhatAmI::Router) => {
                            #[cfg(test)]
                            {
                                let mut hat = hat::router::Hat::new(region);
                                hat.set_disable_async_tree_computation(
                                    self.disable_async_tree_computation,
                                );
                                Box::new(hat)
                            }

                            #[cfg(not(test))]
                            Box::new(hat::router::Hat::new(region))
                        }
                    },
                )
            })
            .collect::<RegionMap<_>>();

        let data = TablesData::new(
            zid,
            self.hlc,
            self.config,
            regions
                .iter()
                .copied()
                .map(|b| (b, tables::HatTablesData::new()))
                .collect(),
            #[cfg(feature = "stats")]
            stats,
        )?;

        Ok(Gateway {
            tables: Arc::new(TablesLock {
                tables: RwLock::new(Tables { data, hats }),
                ctrl_lock: Mutex::new(()),
                queries_lock: RwLock::new(()),
            }),
        })
    }
}

pub struct Gateway {
    // whatami: WhatAmI,
    pub tables: Arc<TablesLock>,
}

impl Gateway {
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

    pub(crate) fn new_session(&self, primitives: Arc<dyn EPrimitives + Send + Sync>) -> Arc<Face> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let newface = Arc::new(
            FaceStateBuilder::new(
                tables.data.new_face_id(),
                tables.data.zid,
                Region::Local,
                Bound::North,
                primitives.clone(),
                tables.hats.map_ref(|hat| hat.new_face()),
            )
            .whatami(WhatAmI::Client)
            .local(true)
            .build(),
        );
        tables.data.faces.insert(newface.id, newface.clone());
        tracing::debug!("New {}", newface);

        let mut face = Face {
            tables: self.tables.clone(),
            state: newface,
        };
        tables.hats[face.state.region]
            .new_local_face(
                DispatcherContext {
                    tables_lock: &face.tables,
                    tables: &mut tables.data,
                    src_face: &mut face.state,
                    send_declare: &mut |_, _| {
                        unreachable!("no declarations should be pushed to new session faces")
                    },
                },
                &self.tables,
            )
            .unwrap();
        drop(wtables);
        drop(ctrl_lock);

        Arc::new(face)
    }

    pub fn new_transport_unicast(
        &self,
        transport: TransportUnicast,
        region: Region,
        remote_bound: Bound,
    ) -> ZResult<Arc<DeMux>> {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let whatami = transport.get_whatami()?;
        let fid = tables.data.new_face_id();
        let zid = transport.get_zid()?;
        let this_zid = tables.data.zid;

        let ingress = Arc::new(ArcSwapOption::new(InterceptorsChain::empty().into()));
        let mux = Arc::new(Mux::new(transport.clone(), InterceptorsChain::empty()));

        let newface = tables
            .data
            .faces
            .entry(fid)
            .or_insert_with(|| {
                let builder = FaceStateBuilder::new(
                    fid,
                    zid,
                    region,
                    remote_bound,
                    mux.clone(),
                    tables.hats.map_ref(|hat| hat.new_face()),
                )
                .whatami(whatami)
                .ingress_interceptors(ingress.clone());

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
        let (owner_hat, other_hats) = tables
            .hats
            .partition_mut(&region)
            .expect("computed transport region should have a corresponding hat");
        let ctx = DispatcherContext {
            tables_lock: &face.tables,
            tables: &mut tables.data,
            src_face: &mut face.state,
            send_declare: &mut |p, m| declares.push((p.clone(), m)),
        };
        owner_hat.new_transport_unicast_face(
            ctx,
            &transport,
            other_hats.map(|hat| &**hat as &dyn HatTrait),
        )?;
        drop(wtables);
        drop(ctrl_lock);
        for (p, m) in declares {
            m.with_mut(|m| p.send_declare(m));
        }

        Ok(Arc::new(DeMux::new(
            face,
            Some(transport),
            ingress,
            this_zid,
        )))
    }

    pub fn new_transport_multicast(
        &self,
        transport: TransportMulticast,
        region: Region,
    ) -> ZResult<()> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let fid = tables.data.new_face_id();
        let mux = Arc::new(McastMux::new(transport.clone(), InterceptorsChain::empty()));

        #[cfg(feature = "stats")]
        let stats = transport.get_stats().ok();

        let builder = FaceStateBuilder::new(
            fid,
            ZenohIdProto::from_str("1").unwrap(),
            region,
            Bound::default(), // HACK(regions): this is a placeholder
            mux.clone(),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .multicast_group(transport);

        #[cfg(feature = "stats")]
        let builder = {
            if let Some(stats) = stats {
                builder.stats(stats)
            } else {
                builder
            }
        };

        let face = Arc::new(builder.build());

        face.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );
        let _ = mux.face.set(Face {
            state: face.clone(),
            tables: self.tables.clone(),
        });

        tables.data.hats[region].mcast_groups.push(face);

        tables.hats[region].disable_all_routes(&mut tables.data);

        Ok(())
    }

    pub fn new_peer_multicast(
        &self,
        transport: TransportMulticast,
        peer: TransportPeer,
        region: Region,
        remote_bound: Bound,
    ) -> ZResult<Arc<DeMux>> {
        let _ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut wtables = zwrite!(self.tables.tables);
        let tables = &mut *wtables;

        let fid = tables.data.new_face_id();
        let interceptor = Arc::new(ArcSwapOption::new(InterceptorsChain::empty().into()));
        let this_zid = tables.data.zid;

        #[cfg(feature = "stats")]
        let stats = transport.get_stats().ok();

        let builder = FaceStateBuilder::new(
            fid,
            peer.zid,
            region,
            remote_bound,
            Arc::new(DummyPrimitives),
            tables.hats.map_ref(|hat| hat.new_face()),
        )
        .multicast_group(transport)
        .ingress_interceptors(interceptor.clone());

        #[cfg(feature = "stats")]
        let builder = {
            if let Some(stats) = stats {
                builder.stats(stats)
            } else {
                builder
            }
        };

        let face = Arc::new(builder.build());

        face.set_interceptors_from_factories(
            &tables.data.interceptors,
            tables.data.next_interceptor_version.load(Ordering::SeqCst),
        );

        tables.data.hats[region].mcast_faces.push(face.clone());

        tables.hats[region].disable_all_routes(&mut tables.data);

        Ok(Arc::new(DeMux::new(
            Face {
                tables: self.tables.clone(),
                state: face,
            },
            None,
            interceptor,
            this_zid,
        )))
    }
}
