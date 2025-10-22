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
    mem,
    sync::{atomic::AtomicU32, Arc},
};

use zenoh_config::{unwrap_or_default, ModeDependent, WhatAmI};
use zenoh_protocol::{
    common::ZExtBody,
    core::ZenohIdProto,
    network::{
        declare::{
            ext::{NodeIdType, QoSType},
            queryable::ext::QueryableInfoType,
            QueryableId, SubscriberId, TokenId,
        },
        interest::{InterestId, InterestOptions},
        oam::id::{OAM_IS_GATEWAY, OAM_LINKSTATE},
        Declare, DeclareBody, DeclareFinal, Oam,
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
    HatBaseTrait, HatTrait, SendDeclare,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::{gossip::Gossip, linkstate::LinkStateList},
    routing::{
        dispatcher::{
            face::{FaceId, InterestState},
            gateway::Bound,
            interests::RemoteInterest,
        },
        hat::{BaseContext, InterestProfile},
        router::FaceContext,
        RoutingContext,
    },
    runtime::Runtime,
};

mod interests;
mod pubsub;
mod queries;
mod token;

use crate::net::common::AutoConnect;

pub(crate) struct Hat {
    bound: Bound,
    gossip: Option<Gossip>,
}

impl Hat {
    pub(crate) fn new(bound: Bound) -> Self {
        Self {
            bound,
            gossip: None,
        }
    }

    pub(self) fn face_hat<'f>(&self, face_state: &'f Arc<FaceState>) -> &'f HatFace {
        face_state.hats[self.bound].downcast_ref().unwrap()
    }

    pub(self) fn face_hat_mut<'f>(&self, face_state: &'f mut Arc<FaceState>) -> &'f mut HatFace {
        get_mut_unchecked(face_state).hats[self.bound]
            .downcast_mut()
            .unwrap()
    }

    pub(crate) fn faces<'t>(&self, tables: &'t TablesData) -> &'t HashMap<usize, Arc<FaceState>> {
        &tables.faces
    }

    pub(crate) fn faces_mut<'t>(
        &self,
        tables: &'t mut TablesData,
    ) -> &'t mut HashMap<usize, Arc<FaceState>> {
        &mut tables.faces
    }

    pub(crate) fn mcast_groups<'t>(
        &self,
        tables: &'t TablesData,
    ) -> impl Iterator<Item = &'t Arc<FaceState>> {
        tables.hats[self.bound].mcast_groups.iter()
    }

    pub(crate) fn face<'t>(
        &self,
        tables: &'t TablesData,
        zid: &ZenohIdProto,
    ) -> Option<&'t Arc<FaceState>> {
        tables.faces.values().find(|face| face.zid == *zid)
    }

    /// Returns `true` if `face` belongs to this [`Hat`].
    pub(crate) fn owns(&self, face: &FaceState) -> bool {
        // TODO(regions): move this method to a Hat trait
        self.bound == face.bound
    }

    /// Returns an iterator over the [`FaceContext`]s this hat [`Self::owns`].
    pub(crate) fn owned_face_contexts<'a>(
        &'a self,
        res: &'a Resource,
    ) -> impl Iterator<Item = (&'a FaceId, &'a Arc<FaceContext>)> {
        // TODO(regions): move this method to a Hat trait
        res.face_ctxs
            .iter()
            .filter(move |(_, ctx)| self.owns(&ctx.face))
    }

    pub(crate) fn owned_faces<'hat, 'tbl>(
        &'hat self,
        tables: &'tbl TablesData,
    ) -> impl Iterator<Item = &'tbl Arc<FaceState>> + 'hat
    where
        'tbl: 'hat,
    {
        tables.faces.values().filter(|face| self.owns(face))
    }
}

impl HatBaseTrait for Hat {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()> {
        let config_guard = runtime.config().lock();
        let config = &config_guard.0;
        let whatami = tables.hats[self.bound].whatami;
        let gossip = unwrap_or_default!(config.scouting().gossip().enabled());
        let gossip_multihop = unwrap_or_default!(config.scouting().gossip().multihop());
        let gossip_target = *unwrap_or_default!(config.scouting().gossip().target().get(whatami));
        if gossip_target.matches(WhatAmI::Client) {
            bail!("\"client\" is not allowed as gossip target")
        }
        let autoconnect = if gossip {
            AutoConnect::gossip(config, whatami, runtime.zid().into())
        } else {
            AutoConnect::disabled()
        };
        let wait_declares = unwrap_or_default!(config.open().return_conditions().declares());
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        drop(config_guard);

        if gossip {
            self.gossip = Some(Gossip::new(
                "[Gossip]".to_string(),
                tables.zid,
                runtime,
                router_peers_failover_brokering,
                gossip,
                gossip_multihop,
                gossip_target,
                autoconnect,
                wait_declares,
            ));
        }
        Ok(())
    }

    fn new_face(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatFace::new())
    }

    fn new_resource(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(HatContext::new())
    }

    fn new_local_face(
        &mut self,
        mut ctx: BaseContext,
        _tables_ref: &Arc<TablesLock>,
    ) -> ZResult<()> {
        self.interests_new_face(ctx.reborrow());

        let profile = if ctx.src_face.bound.is_north() {
            InterestProfile::Push
        } else {
            InterestProfile::Pull
        };

        self.pubsub_new_face(ctx.reborrow(), profile);
        self.queries_new_face(ctx.reborrow(), profile);
        self.token_new_face(ctx.reborrow(), profile);
        ctx.tables.disable_all_routes();
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all, fields(src = %ctx.src_face, wai = %self.whatami().short(), bnd = %self.bound))]
    fn new_transport_unicast_face(
        &mut self,
        mut ctx: BaseContext,
        _tables_ref: &Arc<TablesLock>,
        transport: &TransportUnicast,
    ) -> ZResult<()> {
        // FIXME(regions): compute proper profile
        let profile = if ctx.src_face.bound.is_north() {
            InterestProfile::Push
        } else {
            InterestProfile::Pull
        };

        if ctx.src_face.whatami != WhatAmI::Client {
            if let Some(net) = self.gossip.as_mut() {
                net.add_link(transport.clone());
            }
        }
        if ctx.src_face.whatami == WhatAmI::Peer {
            let face_id = ctx.src_face.id;
            get_mut_unchecked(ctx.src_face).local_interests.insert(
                INITIAL_INTEREST_ID,
                InterestState::new(face_id, InterestOptions::ALL, None, false),
            );
        }

        self.interests_new_face(ctx.reborrow());
        self.pubsub_new_face(ctx.reborrow(), profile);
        self.queries_new_face(ctx.reborrow(), profile);
        self.token_new_face(ctx.reborrow(), profile);
        ctx.tables.disable_all_routes();

        if ctx.src_face.whatami == WhatAmI::Peer {
            (ctx.send_declare)(
                &ctx.src_face.primitives,
                RoutingContext::new(Declare {
                    interest_id: Some(INITIAL_INTEREST_ID),
                    ext_qos: QoSType::default(),
                    ext_tstamp: None,
                    ext_nodeid: NodeIdType::default(),
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                }),
            );
        }
        Ok(())
    }

    fn close_face(&mut self, mut ctx: BaseContext, _tables_ref: &Arc<TablesLock>) {
        // FIXME(regions): compute proper profile
        let profile = InterestProfile::Push;

        let mut face_clone = ctx.src_face.clone();
        let face = get_mut_unchecked(&mut face_clone);
        let hat_face = match face.hats[self.bound].downcast_mut::<HatFace>() {
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

        for res in face.remote_mappings.values_mut() {
            get_mut_unchecked(res).face_ctxs.remove(&face.id);
            Resource::clean(res);
        }
        face.remote_mappings.clear();
        for res in face.local_mappings.values_mut() {
            get_mut_unchecked(res).face_ctxs.remove(&face.id);
            Resource::clean(res);
        }
        face.local_mappings.clear();

        let mut subs_matches = vec![];
        for (_id, mut res) in hat_face.remote_subs.drain() {
            get_mut_unchecked(&mut res).face_ctxs.remove(&face.id);
            self.undeclare_simple_subscription(ctx.reborrow(), &mut res);

            if res.ctx.is_some() {
                for match_ in &res.context().matches {
                    let mut match_ = match_.upgrade().unwrap();
                    if !Arc::ptr_eq(&match_, &res) {
                        get_mut_unchecked(&mut match_).context_mut().hats[self.bound]
                            .disable_data_routes();
                        subs_matches.push(match_);
                    }
                }
                get_mut_unchecked(&mut res).context_mut().hats[self.bound].disable_data_routes();
                subs_matches.push(res);
            }
        }

        let mut qabls_matches = vec![];
        for (_id, mut res) in hat_face.remote_qabls.drain() {
            get_mut_unchecked(&mut res).face_ctxs.remove(&face.id);
            self.undeclare_simple_queryable(ctx.reborrow(), &mut res, profile);

            if res.ctx.is_some() {
                for match_ in &res.context().matches {
                    let mut match_ = match_.upgrade().unwrap();
                    if !Arc::ptr_eq(&match_, &res) {
                        get_mut_unchecked(&mut match_).context_mut().hats[self.bound]
                            .disable_query_routes();
                        qabls_matches.push(match_);
                    }
                }
                get_mut_unchecked(&mut res).context_mut().hats[self.bound].disable_query_routes();
                qabls_matches.push(res);
            }
        }

        for (_id, mut res) in hat_face.remote_tokens.drain() {
            get_mut_unchecked(&mut res).face_ctxs.remove(&face.id);
            self.undeclare_simple_queryable(ctx.reborrow(), &mut res, profile);
        }

        for mut res in subs_matches {
            get_mut_unchecked(&mut res).context_mut().hats[self.bound].disable_data_routes();
            Resource::clean(&mut res);
        }
        for mut res in qabls_matches {
            get_mut_unchecked(&mut res).context_mut().hats[self.bound].disable_query_routes();
            Resource::clean(&mut res);
        }
        self.faces_mut(ctx.tables).remove(&face.id);

        if face.whatami != WhatAmI::Client {
            if let Some(net) = self.gossip.as_mut() {
                net.remove_link(&face.zid);
            }
        };
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn handle_oam(
        &mut self,
        tables: &mut TablesData,
        _tables_ref: &Arc<TablesLock>,
        oam: &mut Oam,
        zid: &ZenohIdProto,
        whatami: WhatAmI,
        _send_declare: &mut SendDeclare,
    ) -> ZResult<()> {
        if oam.id == OAM_LINKSTATE {
            if let ZExtBody::ZBuf(buf) = mem::take(&mut oam.body) {
                if whatami != WhatAmI::Client {
                    if let Some(net) = self.gossip.as_mut() {
                        use zenoh_buffers::reader::HasReader;
                        use zenoh_codec::RCodec;
                        let codec = Zenoh080Routing::new();
                        let mut reader = buf.reader();
                        let Ok(list): Result<LinkStateList, _> = codec.read(&mut reader) else {
                            bail!("failed to decode link state");
                        };

                        tracing::trace!(id = %"OAM_LINKSTATE", linkstate = ?list);

                        net.link_states(list.link_states, *zid, whatami);
                    }
                };
            }
        } else if oam.id == OAM_IS_GATEWAY {
            let Some(face) = self.face(tables, zid) else {
                bail!("Could not find transport face for ZID {zid}")
            };

            tracing::trace!(id = %"OAM_IS_GATEWAY");

            self.face_hat_mut(&mut face.clone()).is_gateway = true;

            let gwy_count = self
                .faces(tables)
                .iter()
                .filter(|(_, f)| self.face_hat(f).is_gateway)
                .count();

            if gwy_count > 1 {
                tracing::error!(
                    bound = ?self.bound,
                    total = gwy_count,
                    "Multiple gateways found in peer subregion. \
                    Only one gateway per subregion is supported."
                );
            }
        }

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
        src_face.id != out_face.id
            && (out_face.mcast_group.is_none()
                || (src_face.whatami == WhatAmI::Client && src_face.mcast_group.is_none()))
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
        WhatAmI::Peer
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
    local_tokens: HashMap<Arc<Resource>, TokenId>,
    remote_tokens: HashMap<TokenId, Arc<Resource>>,
    local_qabls: HashMap<Arc<Resource>, (QueryableId, QueryableInfoType)>,
    remote_qabls: HashMap<QueryableId, Arc<Resource>>,
    is_gateway: bool,
}

impl HatFace {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1), // In p2p, id 0 is erserved for initial interest
            remote_interests: HashMap::new(),
            local_subs: HashMap::new(),
            remote_subs: HashMap::new(),
            local_tokens: HashMap::new(),
            remote_tokens: HashMap::new(),
            local_qabls: HashMap::new(),
            remote_qabls: HashMap::new(),
            is_gateway: false,
        }
    }
}

impl HatTrait for Hat {}

// In p2p, at connection, while no interest is sent on the network,
// peers act as if they received an interest CurrentFuture with id 0
// and send back a DeclareFinal with interest_id 0.
// This 'ghost' interest is registered locally to allow tracking if
// the DeclareFinal has been received or not (finalized).

pub(crate) const INITIAL_INTEREST_ID: u32 = 0;

#[inline]
fn initial_interest(face: &FaceState) -> Option<&InterestState> {
    face.local_interests.get(&INITIAL_INTEREST_ID)
}

#[inline]
pub(super) fn push_declaration_profile(face: &FaceState) -> bool {
    face.whatami != WhatAmI::Client
}
