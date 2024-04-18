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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use crate::{
    net::routing::{
        dispatcher::face::Face,
        router::{compute_data_routes, compute_query_routes, RoutesIndexes},
    },
    runtime::Runtime,
};

use self::{
    pubsub::{pubsub_new_face, undeclare_client_subscription},
    queries::{queries_new_face, undeclare_client_queryable},
};
use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, Tables, TablesLock},
    },
    HatBaseTrait, HatTrait,
};
use std::{
    any::Any,
    collections::HashMap,
    sync::{atomic::AtomicU32, Arc},
};
use zenoh_config::WhatAmI;
use zenoh_protocol::network::Oam;
use zenoh_protocol::network::{
    declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId},
    interest::InterestId,
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::unicast::TransportUnicast;

mod pubsub;
mod queries;

macro_rules! face_hat {
    ($f:expr) => {
        $f.hat.downcast_ref::<HatFace>().unwrap()
    };
}
use face_hat;

macro_rules! face_hat_mut {
    ($f:expr) => {
        get_mut_unchecked($f).hat.downcast_mut::<HatFace>().unwrap()
    };
}
use face_hat_mut;

struct HatTables {}

impl HatTables {
    fn new() -> Self {
        Self {}
    }
}

pub(crate) struct HatCode {}

impl HatBaseTrait for HatCode {
    fn init(&self, _tables: &mut Tables, _runtime: Runtime) {}

    fn new_tables(&self, _router_peers_failover_brokering: bool) -> Box<dyn Any + Send + Sync> {
        Box::new(HatTables::new())
    }

    fn new_face(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatFace::new())
    }

    fn new_resource(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatContext::new())
    }

    fn new_local_face(
        &self,
        tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        face: &mut Face,
    ) -> ZResult<()> {
        pubsub_new_face(tables, &mut face.state);
        queries_new_face(tables, &mut face.state);
        Ok(())
    }

    fn new_transport_unicast_face(
        &self,
        tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        face: &mut Face,
        _transport: &TransportUnicast,
    ) -> ZResult<()> {
        pubsub_new_face(tables, &mut face.state);
        queries_new_face(tables, &mut face.state);
        Ok(())
    }

    fn close_face(&self, tables: &TablesLock, face: &mut Arc<FaceState>) {
        let mut wtables = zwrite!(tables.tables);
        let mut face_clone = face.clone();
        let face = get_mut_unchecked(face);
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
        for (_id, mut res) in face
            .hat
            .downcast_mut::<HatFace>()
            .unwrap()
            .remote_subs
            .drain()
        {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_client_subscription(&mut wtables, &mut face_clone, &mut res);

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
        for (_id, mut res) in face
            .hat
            .downcast_mut::<HatFace>()
            .unwrap()
            .remote_qabls
            .drain()
        {
            get_mut_unchecked(&mut res).session_ctxs.remove(&face.id);
            undeclare_client_queryable(&mut wtables, &mut face_clone, &mut res);

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
        drop(wtables);

        let mut matches_data_routes = vec![];
        let mut matches_query_routes = vec![];
        let rtables = zread!(tables.tables);
        for _match in subs_matches.drain(..) {
            let mut expr = RoutingExpr::new(&_match, "");
            matches_data_routes.push((_match.clone(), compute_data_routes(&rtables, &mut expr)));
        }
        for _match in qabls_matches.drain(..) {
            matches_query_routes.push((_match.clone(), compute_query_routes(&rtables, &_match)));
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
    }

    fn handle_oam(
        &self,
        _tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        _oam: Oam,
        _transport: &TransportUnicast,
    ) -> ZResult<()> {
        Ok(())
    }

    #[inline]
    fn map_routing_context(
        &self,
        _tables: &Tables,
        _face: &FaceState,
        _routing_context: NodeId,
    ) -> NodeId {
        0
    }

    fn closing(
        &self,
        _tables: &mut Tables,
        _tables_ref: &Arc<TablesLock>,
        _transport: &TransportUnicast,
    ) -> ZResult<()> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    #[inline]
    fn ingress_filter(&self, _tables: &Tables, _face: &FaceState, _expr: &mut RoutingExpr) -> bool {
        true
    }

    #[inline]
    fn egress_filter(
        &self,
        _tables: &Tables,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        _expr: &mut RoutingExpr,
    ) -> bool {
        src_face.id != out_face.id
            && match (src_face.mcast_group.as_ref(), out_face.mcast_group.as_ref()) {
                (Some(l), Some(r)) => l != r,
                _ => true,
            }
    }

    fn info(&self, _tables: &Tables, _kind: WhatAmI) -> String {
        "graph {}".to_string()
    }
}

struct HatContext {}

impl HatContext {
    fn new() -> Self {
        Self {}
    }
}

struct HatFace {
    next_id: AtomicU32, // @TODO: manage rollover and uniqueness
    remote_sub_interests: HashMap<InterestId, Option<Arc<Resource>>>,
    local_subs: HashMap<Arc<Resource>, SubscriberId>,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, (QueryableId, QueryableInfoType)>,
    remote_qabls: HashMap<QueryableId, Arc<Resource>>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(0),
            remote_sub_interests: HashMap::new(),
            local_subs: HashMap::new(),
            remote_subs: HashMap::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashMap::new(),
        }
    }
}

impl HatTrait for HatCode {}

#[inline]
fn get_routes_entries() -> RoutesIndexes {
    RoutesIndexes {
        routers: vec![0],
        peers: vec![0],
        clients: vec![0],
    }
}
