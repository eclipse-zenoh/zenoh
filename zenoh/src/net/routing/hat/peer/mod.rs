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
    mem::{self},
    sync::{atomic::AtomicU32, Arc},
};

use petgraph::graph::NodeIndex;
use zenoh_config::{unwrap_or_default, ModeDependent};
use zenoh_protocol::{
    common::ZExtBody,
    core::{Bound, Region, WhatAmI, ZenohIdProto},
    network::{
        declare::{self, queryable::ext::QueryableInfoType, QueryableId, SubscriberId, TokenId},
        interest::{InterestId, InterestOptions},
        oam::id::OAM_LINKSTATE,
        Declare, DeclareBody, DeclareFinal, Oam,
    },
};
use zenoh_result::ZResult;
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::unicast::TransportUnicast;

use super::{
    super::dispatcher::{
        face::FaceState,
        tables::{NodeId, Resource, TablesData, TablesLock},
    },
    HatBaseTrait, HatTrait,
};
use crate::net::{
    codec::Zenoh080Routing,
    protocol::{
        gossip::Gossip,
        linkstate::{LinkState, LinkStateList},
        network::Network,
    },
    routing::{
        dispatcher::{
            face::InterestState, interests::RemoteInterest, queries::LocalQueryables,
            region::RegionMap,
        },
        gateway::{FaceContext, LocalSubscribers, DEFAULT_NODE_ID},
        hat::{DispatcherContext, Remote},
        RoutingContext,
    },
    runtime::Runtime,
};

mod interests;
mod pubsub;
mod queries;
mod token;

use crate::net::common::AutoConnect;

pub(crate) enum Hat {
    /// Uninitialized value.
    Uninit(Region),
    /// Used for north-bound hats w/o gossip or w/ non-multihop gossip.
    Gossip { gossip: Option<Gossip> },
    /// Used for north-bound hats with multihop gossip or south-bound hats (i.e. peer region
    /// gateways).
    ///
    /// Note that [`Gossip`] interoperates w/ [`Network`]: both implementations use the same
    /// linkstate underlying protocol. Inter-region filtering is possible as [`Gossip`] propagates
    /// its own linkstate info, even though this information is not reflected in
    /// [`crate::net::protocol::gossip::Node`] and is only stored in
    /// [`crate::net::protocol::network::Node`].
    Network {
        region: Region,
        network: Option<Network>,
    },
}

impl Debug for Hat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.region())
    }
}

impl Hat {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new(region: Region) -> Self {
        Self::Uninit(region)
    }

    pub(self) fn face_hat<'f>(&self, face_state: &'f Arc<FaceState>) -> &'f HatFace {
        face_state.hats[self.region()].downcast_ref().unwrap()
    }

    pub(self) fn face_hat_mut<'f>(&self, face_state: &'f mut Arc<FaceState>) -> &'f mut HatFace {
        get_mut_unchecked(face_state).hats[self.region()]
            .downcast_mut()
            .unwrap()
    }

    pub(self) fn hat_remote<'r>(&self, remote: &'r Remote) -> &'r HatRemote {
        remote.as_any().downcast_ref().unwrap()
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

    pub(crate) fn multicast_groups<'t>(
        &self,
        tables: &'t TablesData,
    ) -> impl Iterator<Item = &'t Arc<FaceState>> {
        tables.hats[self.region()].mcast_groups.iter()
    }

    pub(crate) fn net(&self) -> Option<Net<'_>> {
        match self {
            Self::Uninit(..) => unreachable!(),
            Self::Gossip { gossip } => gossip.as_ref().map(Net::Gossip),
            Self::Network { network, .. } => Some(Net::Network(network.as_ref().unwrap())),
        }
    }

    pub(crate) fn net_mut(&mut self) -> Option<NetMut<'_>> {
        match self {
            Self::Uninit(..) => unreachable!(),
            Self::Gossip { gossip } => gossip.as_mut().map(NetMut::Gossip),
            Self::Network { network, .. } => Some(NetMut::Network(network.as_mut().unwrap())),
        }
    }
}

impl HatBaseTrait for Hat {
    fn init(&mut self, tables: &mut TablesData, runtime: Runtime) -> ZResult<()> {
        let config_guard = runtime.config().lock();
        let config = &config_guard;
        let whatami = config.mode();
        let is_gossip_enabled = unwrap_or_default!(config.scouting().gossip().enabled());
        let is_gossip_multihop_enabled = unwrap_or_default!(config.scouting().gossip().multihop());
        let gossip_target = *unwrap_or_default!(config.scouting().gossip().target().get(whatami));
        if gossip_target.matches(WhatAmI::Client) {
            bail!("\"client\" is not allowed as gossip target")
        }
        let autoconnect = if is_gossip_enabled {
            AutoConnect::gossip(config, whatami, runtime.zid().into())
        } else {
            AutoConnect::disabled()
        };
        let wait_declares = unwrap_or_default!(config.open().return_conditions().declares());
        drop(config_guard);

        let Self::Uninit(region) = *self else {
            unreachable!()
        };

        const NAME: &str = "[Gossip]";

        match region.bound() {
            Bound::North => {
                if is_gossip_enabled {
                    if is_gossip_multihop_enabled {
                        *self = Self::Network {
                            region,
                            network: Some(Network::new(
                                NAME.to_string(),
                                tables.zid,
                                runtime,
                                false,
                                is_gossip_enabled,
                                is_gossip_multihop_enabled,
                                gossip_target,
                                autoconnect,
                                HashMap::new(),
                                Bound::North,
                            )),
                        };
                    } else {
                        *self = Self::Gossip {
                            gossip: Some(Gossip::new(
                                NAME.to_string(),
                                tables.zid,
                                runtime,
                                gossip_target,
                                autoconnect,
                                wait_declares,
                            )),
                        };
                    }
                } else {
                    tracing::warn!(
                        "Gossip is disabled: cannot de-duplicate data traversing region gateways"
                    );
                    *self = Self::Gossip { gossip: None };
                }
            }
            Bound::South => {
                *self = Self::Network {
                    region,
                    network: Some(Network::new(
                        NAME.to_string(),
                        tables.zid,
                        runtime,
                        true,
                        is_gossip_enabled,
                        is_gossip_multihop_enabled,
                        gossip_target,
                        autoconnect,
                        HashMap::new(),
                        Bound::South,
                    )),
                };
            }
        }

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
        bail!("Local sessions should not be bound to peer hats");
    }

    #[tracing::instrument(level = "debug", skip(ctx, transport, other_hats), fields(src = %ctx.src_face), ret)]
    fn new_transport_unicast_face(
        &mut self,
        mut ctx: DispatcherContext,
        transport: &TransportUnicast,
        other_hats: RegionMap<&dyn HatTrait>,
    ) -> ZResult<()> {
        debug_assert!(self.owns(ctx.src_face));

        if let Some(net) = self.net_mut() {
            net.add_link(transport.clone(), ctx.src_face.remote_bound);
        }

        // NOTE(regions): we only send/recv initial interests between peers that are mutually north-bound,
        // otherwise we are in PULL mode. In particular, the `open.return_conditions.declares` configuration
        // option doesn't apply to region gateways.
        let do_initial_interest =
            ctx.src_face.region.bound().is_north() && ctx.src_face.remote_bound.is_north();

        tracing::debug!(do_initial_interest);

        if do_initial_interest {
            let face_id = ctx.src_face.id;
            get_mut_unchecked(ctx.src_face).local_interests.insert(
                INITIAL_INTEREST_ID,
                InterestState::new(face_id, InterestOptions::ALL, None, false),
            );
        }

        self.repropagate_interests(ctx.reborrow(), &other_hats);
        self.repropagate_subscribers(ctx.reborrow(), &other_hats);
        self.repropagate_queryables(ctx.reborrow(), &other_hats);
        self.repropagate_tokens(ctx.reborrow(), &other_hats);
        self.disable_all_routes(ctx.tables);

        if do_initial_interest {
            tracing::debug!(dst = %ctx.src_face);
            (ctx.send_declare)(
                &ctx.src_face.primitives,
                RoutingContext::new(Declare {
                    interest_id: Some(INITIAL_INTEREST_ID),
                    ext_qos: declare::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: declare::ext::NodeIdType::default(),
                    body: DeclareBody::DeclareFinal(DeclareFinal),
                }),
            );
        }

        Ok(())
    }

    fn close_face(&mut self, ctx: DispatcherContext) {
        debug_assert!(self.owns(ctx.src_face));

        let mut face_clone = ctx.src_face.clone();
        let face = get_mut_unchecked(&mut face_clone);
        let hat_face = match face.hats[self.region()].downcast_mut::<HatFace>() {
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

        if let Some(net) = self.net_mut() {
            net.remove_link(&face.zid);
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn handle_oam(
        &mut self,
        ctx: DispatcherContext,
        oam: &mut Oam,
        _other_hats: RegionMap<&mut dyn HatTrait>,
    ) -> ZResult<()> {
        if oam.id == OAM_LINKSTATE {
            debug_assert_implies!(
                !ctx.src_face.whatami.is_peer(),
                ctx.src_face.remote_bound.is_south()
            );

            if let ZExtBody::ZBuf(buf) = mem::take(&mut oam.body) {
                if let Some(net) = self.net_mut() {
                    use zenoh_buffers::reader::HasReader;
                    use zenoh_codec::RCodec;
                    let codec = Zenoh080Routing::new();
                    let mut reader = buf.reader();
                    let Ok(list): Result<LinkStateList, _> = codec.read(&mut reader) else {
                        bail!("failed to decode link state");
                    };

                    tracing::trace!(linkstate = ?list);

                    net.link_states(list.link_states, ctx.src_face.zid, ctx.src_face.whatami);
                }
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

    fn info(&self) -> String {
        self.net()
            .as_ref()
            .map(|net| net.dot())
            .unwrap_or_else(|| "graph {}".to_string())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mode(&self) -> WhatAmI {
        WhatAmI::Peer
    }

    fn region(&self) -> Region {
        match self {
            Self::Uninit(..) => unreachable!(),
            Self::Gossip { .. } => Region::North,
            Self::Network { region, .. } => *region,
        }
    }

    fn remote_node_id_to_zid(&self, src: &FaceState, node_id: NodeId) -> Option<ZenohIdProto> {
        debug_assert_eq!(node_id, DEFAULT_NODE_ID);

        Some(src.zid)
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn gateways_of(&self, _tables: &TablesData, zid: &ZenohIdProto) -> Option<Vec<ZenohIdProto>> {
        debug_assert!(self.region().bound().is_south());

        let Self::Network {
            network: Some(net), ..
        } = self
        else {
            bug!("Invalid peer hat");
            return None;
        };

        let links = net
            .graph
            .node_weights()
            .find(|n| &n.zid == zid)
            .map(|n| &n.links)?;

        let gwys = net
            .graph
            .node_weights()
            .filter_map(|n| (n.is_gateway && links.contains_key(&n.zid)).then_some(n.zid))
            .collect();

        Some(gwys)
    }

    #[tracing::instrument(level = "trace", skip(_tables), ret)]
    fn gateways(&self, _tables: &TablesData) -> Option<Vec<ZenohIdProto>> {
        debug_assert!(self.region().bound().is_south());

        let Self::Network {
            network: Some(net), ..
        } = self
        else {
            bug!("Invalid peer hat");
            return None;
        };

        let gwys = net
            .graph
            .node_weights()
            .filter_map(|n| n.is_gateway.then_some(n.zid))
            .collect();

        Some(gwys)
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
    local_subs: LocalSubscribers,
    remote_subs: HashMap<SubscriberId, Arc<Resource>>,
    local_tokens: HashMap<Arc<Resource>, TokenId>,
    remote_tokens: HashMap<TokenId, Arc<Resource>>,
    local_qabls: LocalQueryables,
    remote_qabls: HashMap<QueryableId, (Arc<Resource>, QueryableInfoType)>,
}

impl HatFace {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1), // In p2p, id 0 is erserved for initial interest
            remote_interests: HashMap::new(),
            local_subs: LocalSubscribers::new(),
            remote_subs: HashMap::new(),
            local_tokens: HashMap::new(),
            remote_tokens: HashMap::new(),
            local_qabls: LocalQueryables::new(),
            remote_qabls: HashMap::new(),
        }
    }
}

impl HatTrait for Hat {}

/// In p2p, at connection, while no interest is sent on the network, peers act as if they received
/// an interest `CurrentFuture` with id `0` and send back a `DeclareFinal` with interest id `0`.
/// This 'ghost' interest is registered locally to allow tracking if the `DeclareFinal` has been
/// received or not (finalized).
pub(crate) const INITIAL_INTEREST_ID: u32 = 0;

#[inline]
fn initial_interest(face: &FaceState) -> Option<&InterestState> {
    face.local_interests.get(&INITIAL_INTEREST_ID)
}

type HatRemote = Arc<FaceState>;

pub(crate) enum Net<'a> {
    Gossip(&'a Gossip),
    Network(&'a Network),
}

impl Net<'_> {
    pub(crate) fn dot(&self) -> String {
        match self {
            Self::Gossip(n) => n.dot(),
            Self::Network(n) => n.dot(),
        }
    }
}

pub(crate) enum NetMut<'a> {
    Gossip(&'a mut Gossip),
    Network(&'a mut Network),
}

impl NetMut<'_> {
    pub(crate) fn add_link(self, transport: TransportUnicast, remote_bound: Bound) -> usize {
        match self {
            Self::Gossip(n) => n.add_link(transport, remote_bound),
            Self::Network(n) => n.add_link(transport),
        }
    }

    pub(crate) fn remove_link(self, zid: &ZenohIdProto) -> Vec<(NodeIndex, ZenohIdProto)> {
        match self {
            Self::Gossip(n) => n.remove_link(zid),
            Self::Network(n) => n.remove_link(zid),
        }
    }

    pub(crate) fn link_states(
        self,
        link_states: Vec<LinkState>,
        src: ZenohIdProto,
        src_whatami: WhatAmI,
    ) {
        match self {
            Self::Gossip(n) => {
                n.link_states(link_states, src, src_whatami);
            }
            Self::Network(n) => {
                n.link_states(link_states, src);
            }
        }
    }
}
