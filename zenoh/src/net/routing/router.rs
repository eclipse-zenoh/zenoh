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
use super::interceptor::EgressInterceptor;
use super::interceptor::InterceptorsChain;
use super::runtime::Runtime;
use crate::net::codec::Zenoh080Routing;
use crate::net::protocol::linkstate::LinkStateList;
use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::time::Duration;
use tokio::task::JoinHandle;
use uhlc::HLC;
use zenoh_config::Config;
use zenoh_protocol::core::{WhatAmI, ZenohId};
use zenoh_transport::multicast::TransportMulticast;
use zenoh_transport::unicast::TransportUnicast;
use zenoh_transport::TransportPeer;
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

    #[allow(clippy::too_many_arguments)]
    pub fn init_link_state(&mut self, runtime: Runtime) {
        let ctrl_lock = zlock!(self.tables.ctrl_lock);
        let mut tables = zwrite!(self.tables.tables);
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
            Arc::new(InterceptorsChain::from(
                ingress.into_iter().flatten().collect::<Vec<_>>(),
            )),
            InterceptorsChain::from(egress.into_iter().flatten().collect::<Vec<_>>()),
        );
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
        log::debug!("New {}", newface);

        pubsub_new_face(self, &mut newface);
        queries_new_face(self, &mut newface);

        Arc::downgrade(&newface)
    }

    pub fn open_face(
        &mut self,
        zid: ZenohId,
        whatami: WhatAmI,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Weak<FaceState> {
        let fid = self.face_counter;
        self.face_counter += 1;
        let mut newface = self
            .faces
            .entry(fid)
            .or_insert_with(|| {
                FaceState::new(
                    fid,
                    zid,
                    whatami,
                    #[cfg(feature = "stats")]
                    None,
                    primitives.clone(),
                    0,
                    None,
                )
            })
            .clone();
        log::debug!("New {}", newface);

        pubsub_new_face(self, &mut newface);
        queries_new_face(self, &mut newface);

        Arc::downgrade(&newface)
    }

    fn compute_routes(&mut self, res: &mut Arc<Resource>) {
        compute_data_routes(self, res);
        compute_query_routes(self, res);
    }

    pub(crate) fn compute_matches_routes(&mut self, res: &mut Arc<Resource>) {
        if res.context.is_some() {
            self.compute_routes(res);

            let resclone = res.clone();
            for match_ in &mut get_mut_unchecked(res).context_mut().matches {
                let match_ = &mut match_.upgrade().unwrap();
                if !Arc::ptr_eq(match_, &resclone) && match_.context.is_some() {
                    self.compute_routes(match_);
                }
            }
        }
    }

    pub(crate) fn schedule_compute_trees(
        &mut self,
        tables_ref: Arc<TablesLock>,
        net_type: WhatAmI,
    ) {
        log::trace!("Schedule computations");
        if (net_type == WhatAmI::Router && self.routers_trees_task.is_none())
            || (net_type == WhatAmI::Peer && self.peers_trees_task.is_none())
        {
            let task = Some(zenoh_runtime::ZRuntime::Application.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(*TREES_COMPUTATION_DELAY))
                    .await;
                let mut tables = zwrite!(tables_ref.tables);

                log::trace!("Compute trees");
                let new_childs = match net_type {
                    WhatAmI::Router => tables.routers_net.as_mut().unwrap().compute_trees(),
                    _ => tables.peers_net.as_mut().unwrap().compute_trees(),
                };

                log::trace!("Compute routes");
                pubsub_tree_change(&mut tables, &new_childs, net_type);
                queries_tree_change(&mut tables, &new_childs, net_type);

                log::trace!("Computations completed");
                match net_type {
                    WhatAmI::Router => tables.routers_trees_task = None,
                    _ => tables.peers_trees_task = None,
                };
            }));
            match net_type {
                WhatAmI::Router => self.routers_trees_task = task,
                _ => self.peers_trees_task = task,
            };
        }
    }
}

pub fn close_face(tables: &TablesLock, face: &Weak<FaceState>) {
    match face.upgrade() {
        Some(mut face) => {
            log::debug!("Close {}", face);
            finalize_pending_queries(tables, &mut face);

            let ctrl_lock = zlock!(tables.ctrl_lock);
            let mut wtables = zwrite!(tables.tables);
            let mut face_clone = face.clone();
            let face = get_mut_unchecked(&mut face);
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
            for mut res in face.remote_subs.drain() {
                get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
                undeclare_client_subscription(&mut wtables, &mut face_clone, &mut res);

                if res.context.is_some() {
                    for match_ in &res.context().matches {
                        let mut match_ = match_.upgrade().unwrap();
                        if !Arc::ptr_eq(&match_, &res) {
                            get_mut_unchecked(&mut match_)
                                .context_mut()
                                .valid_data_routes = false;
                            subs_matches.push(match_);
                        }
                    }
                    get_mut_unchecked(&mut res).context_mut().valid_data_routes = false;
                    subs_matches.push(res);
                }
            }

            let mut qabls_matches = vec![];
            for mut res in face.remote_qabls.drain() {
                get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
                undeclare_client_queryable(&mut wtables, &mut face_clone, &mut res);

                if res.context.is_some() {
                    for match_ in &res.context().matches {
                        let mut match_ = match_.upgrade().unwrap();
                        if !Arc::ptr_eq(&match_, &res) {
                            get_mut_unchecked(&mut match_)
                                .context_mut()
                                .valid_query_routes = false;
                            qabls_matches.push(match_);
                        }
                    }
                    get_mut_unchecked(&mut res).context_mut().valid_query_routes = false;
                    qabls_matches.push(res);
                }
            }
            drop(wtables);

            let mut matches_data_routes = vec![];
            let mut matches_query_routes = vec![];
            let rtables = zread!(tables.tables);
            for _match in subs_matches.drain(..) {
                matches_data_routes.push((_match.clone(), compute_data_routes_(&rtables, &_match)));
            }
            for _match in qabls_matches.drain(..) {
                matches_query_routes
                    .push((_match.clone(), compute_query_routes_(&rtables, &_match)));
            }
            drop(rtables);

            let mut wtables = zwrite!(tables.tables);
            for (mut res, data_routes) in matches_data_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_data_routes(data_routes);
                Resource::clean(&mut res);
            }
            for (mut res, query_routes) in matches_query_routes {
                get_mut_unchecked(&mut res)
                    .context_mut()
                    .update_query_routes(query_routes);
                Resource::clean(&mut res);
            }
            wtables.faces.remove(&face.id);
            drop(wtables);
            drop(ctrl_lock);
        }
        None => log::error!("Face already closed!"),
    }
}

pub struct TablesLock {
    pub tables: RwLock<Tables>,
    pub ctrl_lock: Mutex<()>,
    pub queries_lock: RwLock<()>,
}

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
            tables.routers_net = Some(Network::new(
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
            tables.peers_net = Some(Network::new(
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
            tables.shared_nodes = shared_nodes(
                tables.routers_net.as_ref().unwrap(),
                tables.peers_net.as_ref().unwrap(),
            );
        }
    }

    pub fn new_primitives(&self, primitives: Arc<dyn Primitives + Send + Sync>) -> Arc<Face> {
        Arc::new(Face {
            tables: self.tables.clone(),
            state: newface,
        };

        let _ = mux.face.set(face.clone());

        ctrl_lock.new_transport_unicast_face(&mut tables, &self.tables, &mut face, &transport)?;

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
        let mux = Arc::new(McastMux::new(transport.clone(), interceptor));
        let face = FaceState::new(
            fid,
            ZenohId::from_str("1").unwrap(),
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

        // recompute routes
        let mut root_res = tables.root_res.clone();
        update_data_routes_from(&mut tables, &mut root_res);
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
        let interceptor = Arc::new(InterceptorsChain::from(
            tables
                .interceptors
                .iter()
                .filter_map(|itor| itor.new_peer_multicast(&transport))
                .collect::<Vec<IngressInterceptor>>(),
        ));
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

        // recompute routes
        let mut root_res = tables.root_res.clone();
        update_data_routes_from(&mut tables, &mut root_res);
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
