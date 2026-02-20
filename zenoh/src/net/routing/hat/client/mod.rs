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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::{
    any::Any,
    collections::HashMap,
    fmt::Debug,
    sync::{atomic::AtomicU32, Arc},
};

use zenoh_config::WhatAmI;
use zenoh_protocol::{
    core::{Region, ZenohIdProto},
    network::{
        declare::{queryable::ext::QueryableInfoType, QueryableId, SubscriberId, TokenId},
        interest::InterestId,
        Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::unicast::TransportUnicast;

use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, RoutingExpr, TablesData, TablesLock},
    },
    HatBaseTrait, HatTrait,
};
use crate::net::{
    routing::{
        dispatcher::{interests::RemoteInterest, region::RegionMap},
        hat::{DispatcherContext, Remote},
        router::FaceContext,
    },
    runtime::Runtime,
};

mod interests;
mod pubsub;
mod queries;
mod token;

pub(crate) struct Hat {
    region: Region,
}

impl Debug for Hat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.region)
    }
}

impl Hat {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new(region: Region) -> Self {
        debug_assert!(region.bound().is_north());

        Self { region }
    }

    pub(self) fn face_hat<'f>(&self, face_state: &'f Arc<FaceState>) -> &'f HatFace {
        face_state.hats[self.region].downcast_ref().unwrap()
    }

    pub(self) fn face_hat_mut<'f>(&self, face_state: &'f mut Arc<FaceState>) -> &'f mut HatFace {
        get_mut_unchecked(face_state).hats[self.region]
            .downcast_mut()
            .unwrap()
    }

    /// Returns an iterator over the [`FaceContext`]s this hat [`Self::owns`].
    pub(crate) fn owned_face_contexts<'r>(
        &'r self,
        res: &'r Resource,
    ) -> impl Iterator<Item = &'r Arc<FaceContext>> {
        res.face_ctxs
            .values()
            .filter(move |ctx| self.owns(&ctx.face))
    }

    pub(crate) fn owned_faces<'h, 't>(
        &'h self,
        tables: &'t TablesData,
    ) -> impl Iterator<Item = &'t Arc<FaceState>> + 'h
    where
        't: 'h,
    {
        tables.faces.values().filter(|face| self.owns(face))
    }

    pub(crate) fn owned_faces_mut<'h, 't>(
        &'h self,
        tables: &'t mut TablesData,
    ) -> impl Iterator<Item = &'t mut Arc<FaceState>> + 'h
    where
        't: 'h,
    {
        tables.faces.values_mut().filter(|face| self.owns(face))
    }
}

impl HatBaseTrait for Hat {
    fn init(&mut self, _tables: &mut TablesData, _runtime: Runtime) -> ZResult<()> {
        Ok(())
    }

    fn new_face(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatFace::new())
    }

    fn new_resource(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatContext::new())
    }

    fn new_remote(&self, face: &Arc<FaceState>, _nid: NodeId) -> Option<Remote> {
        Some(Remote(Box::new(face.clone())))
    }

    fn new_local_face(
        &mut self,
        _ctx: DispatcherContext,
        _tables_ref: &Arc<TablesLock>,
    ) -> ZResult<()> {
        bail!("Local sessions should not be bound to client hats");
    }

    #[tracing::instrument(level = "trace", skip_all, fields(src = %ctx.src_face, rgn = %self.region))]
    fn new_transport_unicast_face(
        &mut self,
        mut ctx: DispatcherContext,
        _transport: &TransportUnicast,
        other_hats: RegionMap<&dyn HatTrait>,
    ) -> ZResult<()> {
        debug_assert!(self.owns(ctx.src_face));
        debug_assert!(ctx.src_face.remote_bound.is_south());
        debug_assert!(ctx.src_face.region.bound().is_north());
        debug_assert_eq!(self.owned_faces(ctx.tables).count(), 1);

        self.interests_new_face(ctx.reborrow(), &other_hats);
        self.pubsub_new_face(ctx.reborrow(), &other_hats);
        self.queries_new_face(ctx.reborrow(), &other_hats);
        self.tokens_new_face(ctx.reborrow(), &other_hats);
        ctx.tables.disable_all_routes();
        Ok(())
    }

    fn close_face(&mut self, ctx: DispatcherContext) {
        debug_assert!(self.owns(ctx.src_face));

        let mut face_clone = ctx.src_face.clone();
        let face = get_mut_unchecked(&mut face_clone);
        let hat_face = match face.hats[self.region].downcast_mut::<HatFace>() {
            Some(hate_face) => hate_face,
            None => {
                tracing::error!("Error downcasting face hat in close_face!");
                return;
            }
        };

        hat_face.remote_interests.clear();
        hat_face.local_subs.clear();
        hat_face.local_qabls.clear();
        hat_face.local_tokens.clear();
    }

    fn handle_oam(
        &mut self,
        _ctx: DispatcherContext,
        _oam: &mut Oam,
        _zid: &ZenohIdProto,
        _whatami: WhatAmI,
        _other_hats: RegionMap<&mut dyn HatTrait>,
    ) -> ZResult<()> {
        Ok(())
    }

    #[inline]
    fn map_routing_context(
        &self,
        _tables: &TablesData,
        _face: &FaceState,
        _routing_context: NodeId,
    ) -> NodeId {
        0
    }

    #[inline]
    fn ingress_filter(&self, _tables: &TablesData, _face: &FaceState, _expr: &RoutingExpr) -> bool {
        true
    }

    #[inline]
    fn egress_filter(
        &self,
        _tables: &TablesData,
        src_face: &FaceState,
        out_face: &Arc<FaceState>,
        _expr: &RoutingExpr,
    ) -> bool {
        // REVIEW(regions): Does this make sense given that only peers do multicast?
        src_face.id != out_face.id
            && out_face.mcast_group.is_none()
            && src_face.mcast_group.is_none()
    }

    fn info(&self, _kind: WhatAmI) -> String {
        "graph {}".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn whatami(&self) -> WhatAmI {
        WhatAmI::Client
    }

    fn region(&self) -> Region {
        self.region
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
    remote_interests: HashMap<InterestId, RemoteInterest>,
    local_subs: HashMap<Arc<Resource>, SubscriberId>,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, (QueryableId, QueryableInfoType)>,
    remote_qabls: HashMap<QueryableId, (Arc<Resource>, QueryableInfoType)>,
    local_tokens: HashMap<Arc<Resource>, TokenId>,
    remote_tokens: HashMap<TokenId, Arc<Resource>>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1), // REVIEW(regions): changed form 0 to 1 to simplify testing
            remote_interests: HashMap::new(),
            local_subs: HashMap::new(),
            remote_subs: HashMap::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashMap::new(),
            local_tokens: HashMap::new(),
            remote_tokens: HashMap::new(),
        }
    }
}

impl HatTrait for Hat {}

#[allow(dead_code)] // FIXME(regions)
type HatRemote = Arc<FaceState>;
