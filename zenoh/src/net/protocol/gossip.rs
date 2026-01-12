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
use std::{collections::HashMap, convert::TryInto};

use petgraph::graph::NodeIndex;
use vec_map::VecMap;
use zenoh_buffers::{
    writer::{DidntWrite, HasWriter},
    ZBuf,
};
use zenoh_codec::WCodec;
use zenoh_link::Locator;
use zenoh_protocol::{
    common::ZExtBody,
    core::{WhatAmI, WhatAmIMatcher, ZenohIdProto},
    network::{oam, oam::id::OAM_LINKSTATE, NetworkBody, NetworkMessage, Oam},
};
use zenoh_transport::unicast::TransportUnicast;

use crate::net::{
    codec::Zenoh080Routing,
    common::AutoConnect,
    protocol::{
        linkstate::{LinkState, LinkStateList},
        network::Network,
    },
    routing::dispatcher::region::Region,
    runtime::{Runtime, WeakRuntime},
};

pub(crate) enum Gossip {
    Gossip(GossipNet),
    Network(Network),
}

impl Gossip {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: String,
        zid: ZenohIdProto,
        runtime: Runtime,
        router_peers_failover_brokering: bool,
        gossip: bool,
        gossip_multihop: bool,
        gossip_target: WhatAmIMatcher,
        autoconnect: AutoConnect,
        wait_declares: bool,
        region: &Region,
    ) -> Self {
        if gossip_multihop {
            Self::Network(Network::new(
                name,
                zid,
                runtime,
                false,
                gossip,
                gossip_multihop,
                gossip_target,
                autoconnect,
                HashMap::new(),
                region,
            ))
        } else {
            Self::Gossip(GossipNet::new(
                name,
                zid,
                runtime,
                router_peers_failover_brokering,
                gossip,
                gossip_target,
                autoconnect,
                wait_declares,
            ))
        }
    }
    pub(crate) fn dot(&self) -> String {
        match self {
            Self::Gossip(n) => n.dot(),
            Self::Network(n) => n.dot(),
        }
    }
    pub(crate) fn add_link(&mut self, transport: TransportUnicast) -> usize {
        match self {
            Self::Gossip(n) => n.add_link(transport),
            Self::Network(n) => n.add_link(transport),
        }
    }
    pub(crate) fn remove_link(&mut self, zid: &ZenohIdProto) -> Vec<(NodeIndex, ZenohIdProto)> {
        match self {
            Self::Gossip(n) => n.remove_link(zid),
            Self::Network(n) => n.remove_link(zid),
        }
    }
    pub(crate) fn link_states(
        &mut self,
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

#[derive(Clone)]
struct Details {
    zid: bool,
    locators: bool,
    links: bool,
}

#[derive(Clone)]
struct Node {
    zid: ZenohIdProto,
    whatami: Option<WhatAmI>,
    locators: Option<Vec<Locator>>,
    sn: u64,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.zid)
    }
}

struct Link {
    transport: TransportUnicast,
    zid: ZenohIdProto,
    mappings: VecMap<ZenohIdProto>,
    local_mappings: VecMap<u64>,
}

impl Link {
    fn new(transport: TransportUnicast) -> Self {
        let zid = transport.get_zid().unwrap();
        Link {
            transport,
            zid,
            mappings: VecMap::new(),
            local_mappings: VecMap::new(),
        }
    }

    #[inline]
    fn set_zid_mapping(&mut self, psid: u64, zid: ZenohIdProto) {
        self.mappings.insert(psid.try_into().unwrap(), zid);
    }

    #[inline]
    fn get_zid(&self, psid: &u64) -> Option<&ZenohIdProto> {
        self.mappings.get((*psid).try_into().unwrap())
    }

    #[inline]
    fn set_local_psid_mapping(&mut self, psid: u64, local_psid: u64) {
        self.local_mappings
            .insert(psid.try_into().unwrap(), local_psid);
    }
}

pub(crate) struct GossipNet {
    name: String,
    router_peers_failover_brokering: bool,
    gossip: bool,
    gossip_target: WhatAmIMatcher,
    autoconnect: AutoConnect,
    wait_declares: bool,
    idx: NodeIndex,
    links: VecMap<Link>,
    graph: petgraph::stable_graph::StableUnGraph<Node, f64>,
    runtime: WeakRuntime,
}

impl GossipNet {
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: String,
        zid: ZenohIdProto,
        runtime: Runtime,
        router_peers_failover_brokering: bool,
        gossip: bool,
        gossip_target: WhatAmIMatcher,
        autoconnect: AutoConnect,
        wait_declares: bool,
    ) -> Self {
        let mut graph = petgraph::stable_graph::StableGraph::default();
        tracing::debug!("{} Add node (self) {}", name, zid);
        let idx = graph.add_node(Node {
            zid,
            whatami: Some(runtime.whatami()),
            locators: None,
            sn: 1,
        });
        GossipNet {
            name,
            router_peers_failover_brokering,
            gossip,
            gossip_target,
            autoconnect,
            wait_declares,
            idx,
            links: VecMap::new(),
            graph,
            runtime: Runtime::downgrade(&runtime),
        }
    }

    fn dot(&self) -> String {
        std::format!("{:?}", petgraph::dot::Dot::new(&self.graph))
    }

    #[inline]
    fn get_idx(&self, zid: &ZenohIdProto) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|idx| self.graph[*idx].zid == *zid)
    }

    fn add_node(&mut self, node: Node) -> NodeIndex {
        let zid = node.zid;
        let idx = self.graph.add_node(node);
        for link in self.links.values_mut() {
            if let Some((psid, _)) = link.mappings.iter().find(|(_, p)| **p == zid) {
                link.local_mappings.insert(psid, idx.index() as u64);
            }
        }
        idx
    }

    fn make_link_state(&self, idx: NodeIndex, details: Details) -> LinkState {
        let links = if details.links && idx == self.idx {
            self.links
                .iter()
                .filter_map(|(_, link)| {
                    if let Some(idx2) = self.get_idx(&link.zid) {
                        Some(idx2.index().try_into().unwrap())
                    } else {
                        tracing::error!(
                            "{} Internal error building link state: cannot get index of {}",
                            self.name,
                            link.zid,
                        );
                        None
                    }
                })
                .collect()
        } else {
            vec![]
        };
        LinkState {
            psid: idx.index().try_into().unwrap(),
            sn: self.graph[idx].sn,
            zid: if details.zid {
                Some(self.graph[idx].zid)
            } else {
                None
            },
            whatami: self.graph[idx].whatami,
            locators: if details.locators {
                if idx == self.idx {
                    Some(self.runtime.upgrade().unwrap().get_locators())
                } else {
                    self.graph[idx].locators.clone()
                }
            } else {
                None
            },
            links,
            link_weights: None,
            is_gateway: false, // TODO(regions): should this be aligned with the South ext?
        }
    }

    fn make_msg(&self, idxs: Vec<(NodeIndex, Details)>) -> Result<NetworkMessage, DidntWrite> {
        let mut link_states = vec![];
        for (idx, details) in idxs {
            link_states.push(self.make_link_state(idx, details));
        }
        let codec = Zenoh080Routing::new();
        let mut buf = ZBuf::empty();
        codec.write(&mut buf.writer(), &LinkStateList { link_states })?;
        Ok(NetworkBody::OAM(Oam {
            id: OAM_LINKSTATE,
            body: ZExtBody::ZBuf(buf),
            ext_qos: oam::ext::QoSType::OAM,
            ext_tstamp: None,
        })
        .into())
    }

    fn send_on_link(&self, idxs: Vec<(NodeIndex, Details)>, transport: &TransportUnicast) {
        if transport
            .get_whatami()
            .is_ok_and(|w| self.gossip_target.matches(w))
        {
            if let Ok(mut msg) = self.make_msg(idxs) {
                tracing::trace!("{} Send to {:?} {:?}", self.name, transport.get_zid(), msg);
                if let Err(e) = transport.schedule(msg.as_mut()) {
                    tracing::debug!("{} Error sending LinkStateList: {}", self.name, e);
                }
            } else {
                tracing::error!("Failed to encode Linkstate message");
            }
        }
    }

    fn send_on_links<P>(&self, idxs: Vec<(NodeIndex, Details)>, mut parameters: P)
    where
        P: FnMut(&Link) -> bool,
    {
        if let Ok(msg) = self.make_msg(idxs) {
            for link in self.links.values() {
                if link
                    .transport
                    .get_whatami()
                    .is_ok_and(|w| self.gossip_target.matches(w))
                    && parameters(link)
                {
                    tracing::trace!("{} Send to {} {:?}", self.name, link.zid, msg);
                    if let Err(e) = link.transport.schedule(msg.clone().as_mut()) {
                        tracing::debug!("{} Error sending LinkStateList: {}", self.name, e);
                    }
                }
            }
        } else {
            tracing::error!("Failed to encode Linkstate message");
        }
    }

    fn link_states(
        &mut self,
        link_states: Vec<LinkState>,
        src: ZenohIdProto,
        src_whatami: WhatAmI,
    ) {
        tracing::trace!("{} Received from {} raw: {:?}", self.name, src, link_states);
        let strong_runtime = self.runtime.upgrade().unwrap();

        let graph = &self.graph;
        let links = &mut self.links;

        let src_link = match links.values_mut().find(|link| link.zid == src) {
            Some(link) => link,
            None => {
                tracing::error!(
                    "{} Received LinkStateList from unknown link {}",
                    self.name,
                    src
                );
                return;
            }
        };

        // register psid<->zid mappings & apply mapping to nodes
        let link_states = link_states
            .into_iter()
            .filter_map(|link_state| {
                if let Some(zid) = link_state.zid {
                    src_link.set_zid_mapping(link_state.psid, zid);
                    if let Some(idx) = graph.node_indices().find(|idx| graph[*idx].zid == zid) {
                        src_link.set_local_psid_mapping(link_state.psid, idx.index() as u64);
                    }
                    Some((
                        zid,
                        link_state.whatami.unwrap_or(WhatAmI::Router),
                        link_state.locators,
                        link_state.sn,
                        link_state.links,
                    ))
                } else {
                    match src_link.get_zid(&link_state.psid) {
                        Some(zid) => Some((
                            *zid,
                            link_state.whatami.unwrap_or(WhatAmI::Router),
                            link_state.locators,
                            link_state.sn,
                            link_state.links,
                        )),
                        None => {
                            tracing::error!(
                                "Received LinkState from {} with unknown node mapping {}",
                                src,
                                link_state.psid
                            );
                            None
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        for link_state in &link_states {
            tracing::trace!(
                "{} Received from {} mapped: {:?}",
                self.name,
                src,
                link_state
            );
        }

        for (zid, whatami, locators, sn, _links) in link_states.into_iter() {
            if zid == src {
                let idx = match self.get_idx(&zid) {
                    None => {
                        let idx = self.add_node(Node {
                            zid,
                            whatami: Some(whatami),
                            locators: locators.clone(),
                            sn,
                        });
                        locators.is_some().then_some(idx)
                    }
                    Some(idx) => {
                        let node = &mut self.graph[idx];
                        let oldsn = node.sn;
                        (oldsn < sn)
                            .then(|| {
                                node.sn = sn;
                                (node.locators != locators && locators.is_some()).then(|| {
                                    node.locators.clone_from(&locators);
                                    idx
                                })
                            })
                            .flatten()
                    }
                };
                if self.gossip {
                    if let Some(idx) = idx {
                        self.send_on_links(
                            vec![(
                                idx,
                                Details {
                                    zid: true,
                                    locators: true,
                                    links: false,
                                },
                            )],
                            |link| link.zid != zid,
                        );
                    }
                }
            }

            if self.gossip && self.autoconnect.should_autoconnect(zid, whatami) {
                // Connect discovered peers
                zenoh_runtime::ZRuntime::Net.block_in_place(
                    strong_runtime
                        .start_conditions()
                        .add_peer_connector_zid(zid),
                );
                if let Some(locators) = locators {
                    let runtime = strong_runtime.clone();
                    let wait_declares = self.wait_declares;
                    strong_runtime.spawn(async move {
                        if runtime
                            .manager()
                            .get_transport_unicast(&zid)
                            .await
                            .is_none()
                            && runtime.connect_peer(&zid, &locators).await
                            && ((!wait_declares) || whatami != WhatAmI::Peer)
                        {
                            runtime
                                .start_conditions()
                                .terminate_peer_connector_zid(zid)
                                .await;
                        }
                    });
                }
            }
        }
        if (!self.wait_declares) || src_whatami != WhatAmI::Peer {
            zenoh_runtime::ZRuntime::Net.block_in_place(
                strong_runtime
                    .start_conditions()
                    .terminate_peer_connector_zid(src),
            );
        }
    }

    fn add_link(&mut self, transport: TransportUnicast) -> usize {
        let free_index = {
            let mut i = 0;
            while self.links.contains_key(i) {
                i += 1;
            }
            i
        };
        self.links.insert(free_index, Link::new(transport.clone()));

        let zid = transport.get_zid().unwrap();
        let whatami = transport.get_whatami().unwrap();

        if self.router_peers_failover_brokering {
            let (idx, new) = match self.get_idx(&zid) {
                Some(idx) => (idx, false),
                None => {
                    tracing::debug!("{} Add node (link) {}", self.name, zid);
                    (
                        self.add_node(Node {
                            zid,
                            whatami: Some(whatami),
                            locators: None,
                            sn: 0,
                        }),
                        true,
                    )
                }
            };
            self.graph[self.idx].sn += 1;

            // Send updated self linkstate on all existing links except new one
            self.links
                .values()
                .filter(|link| {
                    link.zid != zid
                        && link.transport.get_whatami().unwrap_or(WhatAmI::Peer) == WhatAmI::Router
                })
                .for_each(|link| {
                    self.send_on_link(
                        if new {
                            vec![
                                (
                                    idx,
                                    Details {
                                        zid: true,
                                        locators: false,
                                        links: false,
                                    },
                                ),
                                (
                                    self.idx,
                                    Details {
                                        zid: false,
                                        locators: true,
                                        links: true,
                                    },
                                ),
                            ]
                        } else {
                            vec![(
                                self.idx,
                                Details {
                                    zid: false,
                                    locators: true,
                                    links: true,
                                },
                            )]
                        },
                        &link.transport,
                    )
                });
        }

        // Send all nodes linkstate on new link
        let idxs = self
            .graph
            .node_indices()
            .map(|idx| {
                (
                    idx,
                    Details {
                        zid: true,
                        locators: true,
                        links: (self.router_peers_failover_brokering
                            && idx == self.idx
                            && whatami == WhatAmI::Router),
                    },
                )
            })
            .collect();
        self.send_on_link(idxs, &transport);
        free_index
    }

    fn remove_link(&mut self, zid: &ZenohIdProto) -> Vec<(NodeIndex, ZenohIdProto)> {
        tracing::trace!("{} remove_link {}", self.name, zid);
        self.links.retain(|_, link| link.zid != *zid);

        if let Some(idx) = self.get_idx(zid) {
            self.graph.remove_node(idx);
        }
        if self.router_peers_failover_brokering {
            self.send_on_links(
                vec![(
                    self.idx,
                    Details {
                        zid: false,
                        locators: self.gossip,
                        links: true,
                    },
                )],
                |link| {
                    link.zid != *zid
                        && link.transport.get_whatami().unwrap_or(WhatAmI::Peer) == WhatAmI::Router
                },
            );
        }
        vec![]
    }
}
