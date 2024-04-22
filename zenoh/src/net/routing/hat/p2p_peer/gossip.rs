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
use crate::net::codec::Zenoh080Routing;
use crate::net::protocol::linkstate::{LinkState, LinkStateList};
use crate::net::runtime::Runtime;
use crate::runtime::WeakRuntime;
use petgraph::graph::NodeIndex;
use std::convert::TryInto;
use vec_map::VecMap;
use zenoh_buffers::writer::{DidntWrite, HasWriter};
use zenoh_buffers::ZBuf;
use zenoh_codec::WCodec;
use zenoh_link::Locator;
use zenoh_protocol::common::ZExtBody;
use zenoh_protocol::core::{WhatAmI, WhatAmIMatcher, ZenohId};
use zenoh_protocol::network::oam::id::OAM_LINKSTATE;
use zenoh_protocol::network::{oam, NetworkBody, NetworkMessage, Oam};
use zenoh_transport::unicast::TransportUnicast;

#[derive(Clone)]
struct Details {
    zid: bool,
    locators: bool,
    links: bool,
}

#[derive(Clone)]
pub(super) struct Node {
    pub(super) zid: ZenohId,
    pub(super) whatami: Option<WhatAmI>,
    pub(super) locators: Option<Vec<Locator>>,
    pub(super) sn: u64,
    pub(super) links: Vec<ZenohId>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.zid)
    }
}

pub(super) struct Link {
    pub(super) transport: TransportUnicast,
    zid: ZenohId,
    mappings: VecMap<ZenohId>,
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
    pub(super) fn set_zid_mapping(&mut self, psid: u64, zid: ZenohId) {
        self.mappings.insert(psid.try_into().unwrap(), zid);
    }

    #[inline]
    pub(super) fn get_zid(&self, psid: &u64) -> Option<&ZenohId> {
        self.mappings.get((*psid).try_into().unwrap())
    }

    #[inline]
    pub(super) fn set_local_psid_mapping(&mut self, psid: u64, local_psid: u64) {
        self.local_mappings
            .insert(psid.try_into().unwrap(), local_psid);
    }
}

pub(super) struct Network {
    pub(super) name: String,
    pub(super) router_peers_failover_brokering: bool,
    pub(super) gossip: bool,
    pub(super) gossip_multihop: bool,
    pub(super) autoconnect: WhatAmIMatcher,
    pub(super) idx: NodeIndex,
    pub(super) links: VecMap<Link>,
    pub(super) graph: petgraph::stable_graph::StableUnGraph<Node, f64>,
    pub(super) runtime: WeakRuntime,
}

impl Network {
    pub(super) fn new(
        name: String,
        zid: ZenohId,
        runtime: Runtime,
        router_peers_failover_brokering: bool,
        gossip: bool,
        gossip_multihop: bool,
        autoconnect: WhatAmIMatcher,
    ) -> Self {
        let mut graph = petgraph::stable_graph::StableGraph::default();
        tracing::debug!("{} Add node (self) {}", name, zid);
        let idx = graph.add_node(Node {
            zid,
            whatami: Some(runtime.whatami()),
            locators: None,
            sn: 1,
            links: vec![],
        });
        Network {
            name,
            router_peers_failover_brokering,
            gossip,
            gossip_multihop,
            autoconnect,
            idx,
            links: VecMap::new(),
            graph,
            runtime: Runtime::downgrade(&runtime),
        }
    }

    //noinspection ALL
    // pub(super) fn dot(&self) -> String {
    //     std::format!(
    //         "{:?}",
    //         petgraph::dot::Dot::with_config(&self.graph, &[petgraph::dot::Config::EdgeNoLabel])
    //     )
    // }

    #[inline]
    pub(super) fn get_idx(&self, zid: &ZenohId) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|idx| self.graph[*idx].zid == *zid)
    }

    #[inline]
    pub(super) fn get_link_from_zid(&self, zid: &ZenohId) -> Option<&Link> {
        self.links.values().find(|link| link.zid == *zid)
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
        let links = if details.links {
            self.graph[idx]
                .links
                .iter()
                .filter_map(|zid| {
                    if let Some(idx2) = self.get_idx(zid) {
                        Some(idx2.index().try_into().unwrap())
                    } else {
                        tracing::error!(
                            "{} Internal error building link state: cannot get index of {}",
                            self.name,
                            zid
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
            ext_qos: oam::ext::QoSType::oam_default(),
            ext_tstamp: None,
        })
        .into())
    }

    fn send_on_link(&self, idxs: Vec<(NodeIndex, Details)>, transport: &TransportUnicast) {
        if let Ok(msg) = self.make_msg(idxs) {
            tracing::trace!("{} Send to {:?} {:?}", self.name, transport.get_zid(), msg);
            if let Err(e) = transport.schedule(msg) {
                tracing::debug!("{} Error sending LinkStateList: {}", self.name, e);
            }
        } else {
            tracing::error!("Failed to encode Linkstate message");
        }
    }

    fn send_on_links<P>(&self, idxs: Vec<(NodeIndex, Details)>, mut parameters: P)
    where
        P: FnMut(&Link) -> bool,
    {
        if let Ok(msg) = self.make_msg(idxs) {
            for link in self.links.values() {
                if parameters(link) {
                    tracing::trace!("{} Send to {} {:?}", self.name, link.zid, msg);
                    if let Err(e) = link.transport.schedule(msg.clone()) {
                        tracing::debug!("{} Error sending LinkStateList: {}", self.name, e);
                    }
                }
            }
        } else {
            tracing::error!("Failed to encode Linkstate message");
        }
    }

    // Indicates if locators should be included when propagating Linkstate message
    // from the given node.
    // Returns true if gossip is enabled and if multihop gossip is enabled or
    // the node is one of self neighbours.
    fn propagate_locators(&self, idx: NodeIndex) -> bool {
        self.gossip
            && (self.gossip_multihop
                || idx == self.idx
                || self.links.values().any(|link| {
                    self.graph
                        .node_weight(idx)
                        .map(|node| link.zid == node.zid)
                        .unwrap_or(true)
                }))
    }

    pub(super) fn link_states(&mut self, link_states: Vec<LinkState>, src: ZenohId) {
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

        // apply psid<->zid mapping to links
        let src_link = self.get_link_from_zid(&src).unwrap();
        let link_states = link_states
            .into_iter()
            .map(|(zid, wai, locs, sn, links)| {
                let links: Vec<ZenohId> = links
                    .iter()
                    .filter_map(|l| {
                        if let Some(zid) = src_link.get_zid(l) {
                            Some(*zid)
                        } else {
                            tracing::error!(
                                "{} Received LinkState from {} with unknown link mapping {}",
                                self.name,
                                src,
                                l
                            );
                            None
                        }
                    })
                    .collect();
                (zid, wai, locs, sn, links)
            })
            .collect::<Vec<_>>();

        // tracing::trace!(
        //     "{} Received from {} mapped: {:?}",
        //     self.name,
        //     src,
        //     link_states
        // );
        for link_state in &link_states {
            tracing::trace!(
                "{} Received from {} mapped: {:?}",
                self.name,
                src,
                link_state
            );
        }

        for (zid, whatami, locators, sn, links) in link_states.into_iter() {
            let idx = match self.get_idx(&zid) {
                None => {
                    let idx = self.add_node(Node {
                        zid,
                        whatami: Some(whatami),
                        locators: locators.clone(),
                        sn,
                        links,
                    });
                    locators.is_some().then_some(idx)
                }
                Some(idx) => {
                    let node = &mut self.graph[idx];
                    let oldsn = node.sn;
                    (oldsn < sn)
                        .then(|| {
                            node.sn = sn;
                            node.links = links.clone();
                            (node.locators != locators && locators.is_some()).then(|| {
                                node.locators = locators.clone();
                                idx
                            })
                        })
                        .flatten()
                }
            };

            if self.gossip {
                if let Some(idx) = idx {
                    if self.gossip_multihop || self.links.values().any(|link| link.zid == zid) {
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

                    if !self.autoconnect.is_empty() {
                        // Connect discovered peers
                        if zenoh_runtime::ZRuntime::Acceptor
                            .block_in_place(strong_runtime.manager().get_transport_unicast(&zid))
                            .is_none()
                            && self.autoconnect.matches(whatami)
                        {
                            if let Some(locators) = locators {
                                let runtime = strong_runtime.clone();
                                strong_runtime.spawn(async move {
                                    // random backoff
                                    tokio::time::sleep(std::time::Duration::from_millis(
                                        rand::random::<u64>() % 100,
                                    ))
                                    .await;
                                    runtime.connect_peer(&zid, &locators).await;
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    pub(super) fn add_link(&mut self, transport: TransportUnicast) -> usize {
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
                            links: vec![],
                        }),
                        true,
                    )
                }
            };
            self.graph[self.idx].links.push(zid);
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
                        if new || (!self.gossip_multihop) {
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
                                        locators: self.propagate_locators(idx),
                                        links: true,
                                    },
                                ),
                            ]
                        } else {
                            vec![(
                                self.idx,
                                Details {
                                    zid: false,
                                    locators: self.propagate_locators(idx),
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
            .filter(|&idx| {
                self.gossip_multihop
                    || self.links.values().any(|link| link.zid == zid)
                    || (self.router_peers_failover_brokering
                        && idx == self.idx
                        && whatami == WhatAmI::Router)
            })
            .map(|idx| {
                (
                    idx,
                    Details {
                        zid: true,
                        locators: self.propagate_locators(idx),
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

    pub(super) fn remove_link(&mut self, zid: &ZenohId) -> Vec<(NodeIndex, Node)> {
        tracing::trace!("{} remove_link {}", self.name, zid);
        self.links.retain(|_, link| link.zid != *zid);
        self.graph[self.idx].links.retain(|link| *link != *zid);

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
