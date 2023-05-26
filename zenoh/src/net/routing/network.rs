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
use super::runtime::Runtime;
use petgraph::graph::NodeIndex;
use petgraph::visit::{IntoNodeReferences, VisitMap, Visitable};
use std::convert::TryInto;
use vec_map::VecMap;
use zenoh_config::whatami::WhatAmIMatcher;
use zenoh_link::Locator;
use zenoh_protocol::{
    core::{WhatAmI, ZInt, ZenohId},
    zenoh::{LinkState, ZenohMessage},
};
use zenoh_transport::TransportUnicast;

#[derive(Clone)]
struct Details {
    zid: bool,
    locators: bool,
    links: bool,
}

#[derive(Clone)]
pub(crate) struct Node {
    pub(crate) zid: ZenohId,
    pub(crate) whatami: Option<WhatAmI>,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) sn: ZInt,
    pub(crate) links: Vec<ZenohId>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.zid)
    }
}

pub(crate) struct Link {
    pub(crate) transport: TransportUnicast,
    zid: ZenohId,
    mappings: VecMap<ZenohId>,
    local_mappings: VecMap<ZInt>,
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
    pub(crate) fn set_zid_mapping(&mut self, psid: ZInt, zid: ZenohId) {
        self.mappings.insert(psid.try_into().unwrap(), zid);
    }

    #[inline]
    pub(crate) fn get_zid(&self, psid: &ZInt) -> Option<&ZenohId> {
        self.mappings.get((*psid).try_into().unwrap())
    }

    #[inline]
    pub(crate) fn set_local_psid_mapping(&mut self, psid: ZInt, local_psid: ZInt) {
        self.local_mappings
            .insert(psid.try_into().unwrap(), local_psid);
    }

    #[inline]
    pub(crate) fn get_local_psid(&self, psid: &ZInt) -> Option<&ZInt> {
        self.local_mappings.get((*psid).try_into().unwrap())
    }
}

pub(crate) struct Changes {
    pub(crate) updated_nodes: Vec<(NodeIndex, Node)>,
    pub(crate) removed_nodes: Vec<(NodeIndex, Node)>,
}

#[derive(Clone)]
pub(crate) struct Tree {
    pub(crate) parent: Option<NodeIndex>,
    pub(crate) childs: Vec<NodeIndex>,
    pub(crate) directions: Vec<Option<NodeIndex>>,
}

pub(crate) struct Network {
    pub(crate) name: String,
    pub(crate) full_linkstate: bool,
    pub(crate) router_peers_failover_brokering: bool,
    pub(crate) gossip: bool,
    pub(crate) gossip_multihop: bool,
    pub(crate) autoconnect: WhatAmIMatcher,
    pub(crate) idx: NodeIndex,
    pub(crate) links: VecMap<Link>,
    pub(crate) trees: Vec<Tree>,
    pub(crate) distances: Vec<f64>,
    pub(crate) graph: petgraph::stable_graph::StableUnGraph<Node, f64>,
    pub(crate) runtime: Runtime,
}

impl Network {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: String,
        zid: ZenohId,
        runtime: Runtime,
        full_linkstate: bool,
        router_peers_failover_brokering: bool,
        gossip: bool,
        gossip_multihop: bool,
        autoconnect: WhatAmIMatcher,
    ) -> Self {
        let mut graph = petgraph::stable_graph::StableGraph::default();
        log::debug!("{} Add node (self) {}", name, zid);
        let idx = graph.add_node(Node {
            zid,
            whatami: Some(runtime.whatami),
            locators: None,
            sn: 1,
            links: vec![],
        });
        Network {
            name,
            full_linkstate,
            router_peers_failover_brokering,
            gossip,
            gossip_multihop,
            autoconnect,
            idx,
            links: VecMap::new(),
            trees: vec![Tree {
                parent: None,
                childs: vec![],
                directions: vec![None],
            }],
            distances: vec![0.0],
            graph,
            runtime,
        }
    }

    //noinspection ALL
    pub(crate) fn dot(&self) -> String {
        std::format!(
            "{:?}",
            petgraph::dot::Dot::with_config(&self.graph, &[petgraph::dot::Config::EdgeNoLabel])
        )
    }

    #[inline]
    pub(crate) fn get_node(&self, zid: &ZenohId) -> Option<&Node> {
        self.graph.node_weights().find(|weight| weight.zid == *zid)
    }

    #[inline]
    pub(crate) fn get_idx(&self, zid: &ZenohId) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|idx| self.graph[*idx].zid == *zid)
    }

    #[inline]
    pub(crate) fn get_link(&self, id: usize) -> Option<&Link> {
        self.links.get(id)
    }

    #[inline]
    pub(crate) fn get_link_from_zid(&self, zid: &ZenohId) -> Option<&Link> {
        self.links.values().find(|link| link.zid == *zid)
    }

    #[inline]
    pub(crate) fn get_local_context(&self, context: Option<ZInt>, link_id: usize) -> usize {
        let context = context.unwrap_or(0);
        match self.get_link(link_id) {
            Some(link) => match link.get_local_psid(&context) {
                Some(psid) => (*psid).try_into().unwrap_or(0),
                None => {
                    log::error!(
                        "Cannot find local psid for context {} on link {}",
                        context,
                        link_id
                    );
                    0
                }
            },
            None => {
                log::error!("Cannot find link {}", link_id);
                0
            }
        }
    }

    fn add_node(&mut self, node: Node) -> NodeIndex {
        let zid = node.zid;
        let idx = self.graph.add_node(node);
        for link in self.links.values_mut() {
            if let Some((psid, _)) = link.mappings.iter().find(|(_, p)| **p == zid) {
                link.local_mappings.insert(psid, idx.index() as ZInt);
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
                        log::error!(
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
                    Some(self.runtime.get_locators())
                } else {
                    self.graph[idx].locators.clone()
                }
            } else {
                None
            },
            links,
        }
    }

    fn make_msg(&self, idxs: Vec<(NodeIndex, Details)>) -> ZenohMessage {
        let mut list = vec![];
        for (idx, details) in idxs {
            list.push(self.make_link_state(idx, details));
        }
        ZenohMessage::make_link_state_list(list, None)
    }

    fn send_on_link(&self, idxs: Vec<(NodeIndex, Details)>, transport: &TransportUnicast) {
        let msg = self.make_msg(idxs);
        log::trace!("{} Send to {:?} {:?}", self.name, transport.get_zid(), msg);
        if let Err(e) = transport.handle_message(msg) {
            log::debug!("{} Error sending LinkStateList: {}", self.name, e);
        }
    }

    fn send_on_links<P>(&self, idxs: Vec<(NodeIndex, Details)>, mut parameters: P)
    where
        P: FnMut(&Link) -> bool,
    {
        let msg = self.make_msg(idxs);
        for link in self.links.values() {
            if parameters(link) {
                log::trace!("{} Send to {} {:?}", self.name, link.zid, msg);
                if let Err(e) = link.transport.handle_message(msg.clone()) {
                    log::debug!("{} Error sending LinkStateList: {}", self.name, e);
                }
            }
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

    fn update_edge(&mut self, idx1: NodeIndex, idx2: NodeIndex) {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::default();
        if self.graph[idx1].zid > self.graph[idx2].zid {
            hasher.write(&self.graph[idx2].zid.to_le_bytes());
            hasher.write(&self.graph[idx1].zid.to_le_bytes());
        } else {
            hasher.write(&self.graph[idx1].zid.to_le_bytes());
            hasher.write(&self.graph[idx2].zid.to_le_bytes());
        }
        let weight = 100.0 + ((hasher.finish() as u32) as f64) / u32::MAX as f64;
        self.graph.update_edge(idx1, idx2, weight);
    }

    pub(crate) fn link_states(&mut self, link_states: Vec<LinkState>, src: ZenohId) -> Changes {
        log::trace!("{} Received from {} raw: {:?}", self.name, src, link_states);

        let graph = &self.graph;
        let links = &mut self.links;

        let src_link = match links.values_mut().find(|link| link.zid == src) {
            Some(link) => link,
            None => {
                log::error!(
                    "{} Received LinkStateList from unknown link {}",
                    self.name,
                    src
                );
                return Changes {
                    updated_nodes: vec![],
                    removed_nodes: vec![],
                };
            }
        };

        // register psid<->zid mappings & apply mapping to nodes
        #[allow(clippy::needless_collect)] // need to release borrow on self
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
                            log::error!(
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
                            log::error!(
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

        // log::trace!(
        //     "{} Received from {} mapped: {:?}",
        //     self.name,
        //     src,
        //     link_states
        // );
        for link_state in &link_states {
            log::trace!(
                "{} Received from {} mapped: {:?}",
                self.name,
                src,
                link_state
            );
        }

        if !self.full_linkstate {
            let mut changes = Changes {
                updated_nodes: vec![],
                removed_nodes: vec![],
            };
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
                        changes.updated_nodes.push((idx, self.graph[idx].clone()));
                        locators.is_some().then_some(idx)
                    }
                    Some(idx) => {
                        let node = &mut self.graph[idx];
                        let oldsn = node.sn;
                        (oldsn < sn)
                            .then(|| {
                                node.sn = sn;
                                node.links = links.clone();
                                changes.updated_nodes.push((idx, node.clone()));
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
                            if self.runtime.manager().get_transport(&zid).is_none()
                                && self.autoconnect.matches(whatami)
                            {
                                if let Some(locators) = locators {
                                    let runtime = self.runtime.clone();
                                    self.runtime.spawn(async move {
                                        // random backoff
                                        async_std::task::sleep(std::time::Duration::from_millis(
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
            return changes;
        }

        // Add nodes to graph & filter out up to date states
        let mut link_states = link_states
            .into_iter()
            .filter_map(
                |(zid, whatami, locators, sn, links)| match self.get_idx(&zid) {
                    Some(idx) => {
                        let node = &mut self.graph[idx];
                        let oldsn = node.sn;
                        if oldsn < sn {
                            node.sn = sn;
                            node.links = links.clone();
                            if locators.is_some() {
                                node.locators = locators;
                            }
                            if oldsn == 0 {
                                Some((links, idx, true))
                            } else {
                                Some((links, idx, false))
                            }
                        } else {
                            None
                        }
                    }
                    None => {
                        let node = Node {
                            zid,
                            whatami: Some(whatami),
                            locators,
                            sn,
                            links: links.clone(),
                        };
                        log::debug!("{} Add node (state) {}", self.name, zid);
                        let idx = self.add_node(node);
                        Some((links, idx, true))
                    }
                },
            )
            .collect::<Vec<(Vec<ZenohId>, NodeIndex, bool)>>();

        // Add/remove edges from graph
        let mut reintroduced_nodes = vec![];
        for (links, idx1, _) in &link_states {
            for link in links {
                if let Some(idx2) = self.get_idx(link) {
                    if self.graph[idx2].links.contains(&self.graph[*idx1].zid) {
                        log::trace!(
                            "{} Update edge (state) {} {}",
                            self.name,
                            self.graph[*idx1].zid,
                            self.graph[idx2].zid
                        );
                        self.update_edge(*idx1, idx2);
                    }
                } else {
                    let node = Node {
                        zid: *link,
                        whatami: None,
                        locators: None,
                        sn: 0,
                        links: vec![],
                    };
                    log::debug!("{} Add node (reintroduced) {}", self.name, link.clone());
                    let idx = self.add_node(node);
                    reintroduced_nodes.push((vec![], idx, true));
                }
            }
            let mut edges = vec![];
            let mut neighbors = self.graph.neighbors_undirected(*idx1).detach();
            while let Some(edge) = neighbors.next(&self.graph) {
                edges.push(edge);
            }
            for (eidx, idx2) in edges {
                if !links.contains(&self.graph[idx2].zid) {
                    log::trace!(
                        "{} Remove edge (state) {} {}",
                        self.name,
                        self.graph[*idx1].zid,
                        self.graph[idx2].zid
                    );
                    self.graph.remove_edge(eidx);
                }
            }
        }
        link_states.extend(reintroduced_nodes);

        let removed = self.remove_detached_nodes();
        let link_states = link_states
            .into_iter()
            .filter(|ls| !removed.iter().any(|(idx, _)| idx == &ls.1))
            .collect::<Vec<(Vec<ZenohId>, NodeIndex, bool)>>();

        if !self.autoconnect.is_empty() {
            // Connect discovered peers
            for (_, idx, _) in &link_states {
                let node = &self.graph[*idx];
                if let Some(whatami) = node.whatami {
                    if self.runtime.manager().get_transport(&node.zid).is_none()
                        && self.autoconnect.matches(whatami)
                    {
                        if let Some(locators) = &node.locators {
                            let runtime = self.runtime.clone();
                            let zid = node.zid;
                            let locators = locators.clone();
                            self.runtime.spawn(async move {
                                // random backoff
                                async_std::task::sleep(std::time::Duration::from_millis(
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

        // Propagate link states
        // Note: we need to send all states at once for each face
        // to avoid premature node deletion on the other side
        #[allow(clippy::type_complexity)]
        if !link_states.is_empty() {
            let (new_idxs, updated_idxs): (
                Vec<(Vec<ZenohId>, NodeIndex, bool)>,
                Vec<(Vec<ZenohId>, NodeIndex, bool)>,
            ) = link_states.into_iter().partition(|(_, _, new)| *new);
            let new_idxs = new_idxs
                .into_iter()
                .map(|(_, idx1, _new_node)| {
                    (
                        idx1,
                        Details {
                            zid: true,
                            locators: self.propagate_locators(idx1),
                            links: true,
                        },
                    )
                })
                .collect::<Vec<(NodeIndex, Details)>>();
            for link in self.links.values() {
                if link.zid != src {
                    let updated_idxs: Vec<(NodeIndex, Details)> = updated_idxs
                        .clone()
                        .into_iter()
                        .filter_map(|(_, idx1, _)| {
                            if link.zid != self.graph[idx1].zid {
                                Some((
                                    idx1,
                                    Details {
                                        zid: false,
                                        locators: self.propagate_locators(idx1),
                                        links: true,
                                    },
                                ))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !new_idxs.is_empty() || !updated_idxs.is_empty() {
                        self.send_on_link(
                            [&new_idxs[..], &updated_idxs[..]].concat(),
                            &link.transport,
                        );
                    }
                } else if !new_idxs.is_empty() {
                    self.send_on_link(new_idxs.clone(), &link.transport);
                }
            }
        }
        Changes {
            updated_nodes: vec![],
            removed_nodes: removed,
        }
    }

    pub(crate) fn add_link(&mut self, transport: TransportUnicast) -> usize {
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

        if self.full_linkstate || self.router_peers_failover_brokering {
            let (idx, new) = match self.get_idx(&zid) {
                Some(idx) => (idx, false),
                None => {
                    log::debug!("{} Add node (link) {}", self.name, zid);
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
            if self.full_linkstate && self.graph[idx].links.contains(&self.graph[self.idx].zid) {
                log::trace!("Update edge (link) {} {}", self.graph[self.idx].zid, zid);
                self.update_edge(self.idx, idx);
            }
            self.graph[self.idx].links.push(zid);
            self.graph[self.idx].sn += 1;

            // Send updated self linkstate on all existing links except new one
            self.links
                .values()
                .filter(|link| {
                    link.zid != zid
                        && (self.full_linkstate
                            || link.transport.get_whatami().unwrap_or(WhatAmI::Peer)
                                == WhatAmI::Router)
                })
                .for_each(|link| {
                    self.send_on_link(
                        if new || (!self.full_linkstate && !self.gossip_multihop) {
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
            .filter_map(|idx| {
                (self.full_linkstate
                    || self.gossip_multihop
                    || self.links.values().any(|link| link.zid == zid)
                    || (self.router_peers_failover_brokering
                        && idx == self.idx
                        && whatami == WhatAmI::Router))
                    .then(|| {
                        (
                            idx,
                            Details {
                                zid: true,
                                locators: self.propagate_locators(idx),
                                links: self.full_linkstate
                                    || (self.router_peers_failover_brokering
                                        && idx == self.idx
                                        && whatami == WhatAmI::Router),
                            },
                        )
                    })
            })
            .collect();
        self.send_on_link(idxs, &transport);
        free_index
    }

    pub(crate) fn remove_link(&mut self, zid: &ZenohId) -> Vec<(NodeIndex, Node)> {
        log::trace!("{} remove_link {}", self.name, zid);
        self.links.retain(|_, link| link.zid != *zid);
        self.graph[self.idx].links.retain(|link| *link != *zid);

        if self.full_linkstate {
            if let Some((edge, _)) = self
                .get_idx(zid)
                .and_then(|idx| self.graph.find_edge_undirected(self.idx, idx))
            {
                self.graph.remove_edge(edge);
            }
            let removed = self.remove_detached_nodes();

            self.graph[self.idx].sn += 1;

            self.send_on_links(
                vec![(
                    self.idx,
                    Details {
                        zid: false,
                        locators: self.gossip,
                        links: true,
                    },
                )],
                |_| true,
            );

            removed
        } else {
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
                            && link.transport.get_whatami().unwrap_or(WhatAmI::Peer)
                                == WhatAmI::Router
                    },
                );
            }
            vec![]
        }
    }

    fn remove_detached_nodes(&mut self) -> Vec<(NodeIndex, Node)> {
        let mut dfs_stack = vec![self.idx];
        let mut visit_map = self.graph.visit_map();
        while let Some(node) = dfs_stack.pop() {
            if visit_map.visit(node) {
                for succzid in &self.graph[node].links {
                    if let Some(succ) = self.get_idx(succzid) {
                        if !visit_map.is_visited(&succ) {
                            dfs_stack.push(succ);
                        }
                    }
                }
            }
        }

        let mut removed = vec![];
        for idx in self.graph.node_indices().collect::<Vec<NodeIndex>>() {
            if !visit_map.is_visited(&idx) {
                log::debug!("Remove node {}", &self.graph[idx].zid);
                removed.push((idx, self.graph.remove_node(idx).unwrap()));
            }
        }
        removed
    }

    pub(crate) fn compute_trees(&mut self) -> Vec<Vec<NodeIndex>> {
        let indexes = self.graph.node_indices().collect::<Vec<NodeIndex>>();
        let max_idx = indexes.iter().max().unwrap();

        let old_childs: Vec<Vec<NodeIndex>> = self.trees.iter().map(|t| t.childs.clone()).collect();

        self.trees.clear();
        self.trees.resize_with(max_idx.index() + 1, || Tree {
            parent: None,
            childs: vec![],
            directions: vec![],
        });

        for tree_root_idx in &indexes {
            let paths = petgraph::algo::bellman_ford(&self.graph, *tree_root_idx).unwrap();

            if tree_root_idx.index() == 0 {
                self.distances = paths.distances;
            }

            if log::log_enabled!(log::Level::Debug) {
                let ps: Vec<Option<String>> = paths
                    .predecessors
                    .iter()
                    .enumerate()
                    .map(|(is, o)| {
                        o.map(|ip| {
                            format!(
                                "{} <- {}",
                                self.graph[ip].zid,
                                self.graph[NodeIndex::new(is)].zid
                            )
                        })
                    })
                    .collect();
                log::debug!("Tree {} {:?}", self.graph[*tree_root_idx].zid, ps);
            }

            self.trees[tree_root_idx.index()].parent = paths.predecessors[self.idx.index()];

            for idx in &indexes {
                if let Some(parent_idx) = paths.predecessors[idx.index()] {
                    if parent_idx == self.idx {
                        self.trees[tree_root_idx.index()].childs.push(*idx);
                    }
                }
            }

            self.trees[tree_root_idx.index()]
                .directions
                .resize_with(max_idx.index() + 1, || None);
            let mut dfs = petgraph::algo::DfsSpace::new(&self.graph);
            for destination in &indexes {
                if self.idx != *destination
                    && petgraph::algo::has_path_connecting(
                        &self.graph,
                        self.idx,
                        *destination,
                        Some(&mut dfs),
                    )
                {
                    let mut direction = None;
                    let mut current = *destination;
                    while let Some(parent) = paths.predecessors[current.index()] {
                        if parent == self.idx {
                            direction = Some(current);
                            break;
                        } else {
                            current = parent;
                        }
                    }

                    self.trees[tree_root_idx.index()].directions[destination.index()] =
                        match direction {
                            Some(direction) => Some(direction),
                            None => self.trees[tree_root_idx.index()].parent,
                        };
                }
            }
        }

        let mut new_childs = Vec::with_capacity(self.trees.len());
        new_childs.resize(self.trees.len(), vec![]);

        for i in 0..new_childs.len() {
            new_childs[i] = if i < old_childs.len() {
                self.trees[i]
                    .childs
                    .iter()
                    .filter(|idx| !old_childs[i].contains(idx))
                    .cloned()
                    .collect()
            } else {
                self.trees[i].childs.clone()
            };
        }

        new_childs
    }

    #[inline]
    pub(super) fn get_links(&self, node: ZenohId) -> &[ZenohId] {
        self.get_node(&node)
            .map(|node| &node.links[..])
            .unwrap_or_default()
    }
}

#[inline]
pub(super) fn shared_nodes(net1: &Network, net2: &Network) -> Vec<ZenohId> {
    net1.graph
        .node_references()
        .filter_map(|(_, node1)| {
            net2.graph
                .node_references()
                .any(|(_, node2)| node1.zid == node2.zid)
                .then_some(node1.zid)
        })
        .collect()
}
