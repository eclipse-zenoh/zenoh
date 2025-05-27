//
// Copyright (c) 2025 ZettaScale Technology
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
    collections::{HashMap, HashSet},
    convert::TryInto,
};

use itertools::Itertools;
use petgraph::{
    graph::NodeIndex,
    visit::{EdgeRef, IntoNodeReferences, VisitMap, Visitable},
};
use rand::Rng;
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

use super::linkstate::LinkInfo;
use crate::net::{
    codec::Zenoh080Routing,
    common::AutoConnect,
    protocol::linkstate::{LinkEdgeWeight, LinkState, LinkStateList, LocalLinkState},
    routing::dispatcher::tables::NodeId,
    runtime::Runtime,
};

#[derive(Clone, Default)]
struct Details {
    zid: bool,
    locators: bool,
    links: bool,
}

#[derive(Clone)]
pub(crate) struct Node {
    pub(crate) zid: ZenohIdProto,
    pub(crate) whatami: Option<WhatAmI>,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) sn: u64,
    pub(crate) links: HashMap<ZenohIdProto, LinkEdgeWeight>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.zid)
    }
}

pub(crate) struct Link {
    pub(crate) transport: TransportUnicast,
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
    pub(crate) fn set_zid_mapping(&mut self, psid: u64, zid: ZenohIdProto) {
        self.mappings.insert(psid.try_into().unwrap(), zid);
    }

    #[inline]
    pub(crate) fn get_zid(&self, psid: &u64) -> Option<&ZenohIdProto> {
        self.mappings.get((*psid).try_into().unwrap())
    }

    #[inline]
    pub(crate) fn set_local_psid_mapping(&mut self, psid: u64, local_psid: u64) {
        self.local_mappings
            .insert(psid.try_into().unwrap(), local_psid);
    }

    #[inline]
    pub(crate) fn get_local_psid(&self, psid: &u64) -> Option<&u64> {
        self.local_mappings.get((*psid).try_into().unwrap())
    }
}

#[derive(Default)]
pub(crate) struct Changes {
    pub(crate) updated_nodes: Vec<(NodeIndex, Node)>,
    pub(crate) removed_nodes: Vec<(NodeIndex, Node)>,
}

#[derive(Clone)]
pub(crate) struct Tree {
    pub(crate) parent: Option<NodeIndex>,
    pub(crate) children: Vec<NodeIndex>,
    pub(crate) directions: Vec<Option<NodeIndex>>,
}

pub(crate) struct Network {
    pub(crate) name: String,
    pub(crate) full_linkstate: bool,
    pub(crate) router_peers_failover_brokering: bool,
    pub(crate) gossip: bool,
    pub(crate) gossip_multihop: bool,
    pub(crate) gossip_target: WhatAmIMatcher,
    pub(crate) autoconnect: AutoConnect,
    pub(crate) idx: NodeIndex,
    pub(crate) links: VecMap<Link>,
    pub(crate) trees: Vec<Tree>,
    pub(crate) distances: Vec<f64>,
    pub(crate) graph: petgraph::stable_graph::StableUnGraph<Node, f64>,
    pub(crate) runtime: Runtime,
    pub(crate) link_weights: HashMap<ZenohIdProto, LinkEdgeWeight>,
}

impl Network {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: String,
        zid: ZenohIdProto,
        runtime: Runtime,
        full_linkstate: bool,
        router_peers_failover_brokering: bool,
        gossip: bool,
        gossip_multihop: bool,
        gossip_target: WhatAmIMatcher,
        autoconnect: AutoConnect,
        link_weights: HashMap<ZenohIdProto, LinkEdgeWeight>,
    ) -> Self {
        let mut graph = petgraph::stable_graph::StableGraph::default();
        tracing::debug!("{} Add node (self) {}", name, zid);
        let idx = graph.add_node(Node {
            zid,
            whatami: Some(runtime.whatami()),
            locators: None,
            sn: 1,
            links: HashMap::new(),
        });

        Network {
            name,
            full_linkstate,
            router_peers_failover_brokering,
            gossip,
            gossip_multihop,
            gossip_target,
            autoconnect,
            idx,
            links: VecMap::new(),
            trees: vec![Tree {
                parent: None,
                children: vec![],
                directions: vec![None],
            }],
            distances: vec![0.0],
            graph,
            runtime,
            link_weights,
        }
    }

    pub(crate) fn update_link_weights(
        &mut self,
        link_weights: HashMap<ZenohIdProto, LinkEdgeWeight>,
    ) -> bool {
        let mut dests_to_update = Vec::new();
        for (dst_zid, weight) in &mut self.graph[self.idx].links {
            let old_weight = self.link_weights.get(dst_zid);
            let new_weight = link_weights.get(dst_zid);
            if old_weight == new_weight {
                continue;
            }
            *weight = new_weight.copied().unwrap_or_default();
            dests_to_update.push(*dst_zid);
        }

        self.link_weights = link_weights;
        tracing::info!(
            "{} Update link weights to {:?}",
            &self.name,
            &self.link_weights
        );

        if dests_to_update.is_empty()
            || !(self.full_linkstate || self.router_peers_failover_brokering)
        {
            return false;
        }

        for d in dests_to_update {
            if let Some(dest_idx) = self.get_idx(&d) {
                if self.full_linkstate
                    && self.graph[dest_idx]
                        .links
                        .contains_key(&self.graph[self.idx].zid)
                {
                    tracing::trace!(
                        "Update edge (link_weight) {} {}",
                        self.graph[self.idx].zid,
                        d
                    );
                    self.update_edge(self.idx, dest_idx);
                }
            }
        }

        self.graph[self.idx].sn += 1;
        self.send_on_links(
            vec![(
                self.idx,
                Details {
                    zid: false,
                    links: true,
                    ..Default::default()
                },
            )],
            |_link| true,
        );
        true
    }

    pub(crate) fn dot(&self) -> String {
        std::format!("{:?}", petgraph::dot::Dot::new(&self.graph))
    }

    #[inline]
    pub(crate) fn get_node(&self, zid: &ZenohIdProto) -> Option<&Node> {
        self.graph.node_weights().find(|weight| weight.zid == *zid)
    }

    #[inline]
    pub(crate) fn get_idx(&self, zid: &ZenohIdProto) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|idx| self.graph[*idx].zid == *zid)
    }

    #[inline]
    pub(crate) fn get_link(&self, id: usize) -> Option<&Link> {
        self.links.get(id)
    }

    #[inline]
    pub(crate) fn get_link_from_zid(&self, zid: &ZenohIdProto) -> Option<&Link> {
        self.links.values().find(|link| link.zid == *zid)
    }

    #[inline]
    pub(crate) fn get_local_context(&self, context: NodeId, link_id: usize) -> NodeId {
        match self.get_link(link_id) {
            Some(link) => match link.get_local_psid(&(context as u64)) {
                Some(psid) => (*psid).try_into().unwrap_or(0),
                None => {
                    tracing::error!(
                        "Cannot find local psid for context {} on link {}",
                        context,
                        link_id
                    );
                    0
                }
            },
            None => {
                tracing::error!("Cannot find link {}", link_id);
                0
            }
        }
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

    fn make_link_state(&self, idx: NodeIndex, details: &Details) -> LinkState {
        let mut weights = vec![];
        let mut links = vec![];
        let mut has_non_default_weight = false;
        if details.links {
            for (dest, weight) in &self.graph[idx].links {
                match self.get_idx(dest) {
                    Some(idx2) => {
                        links.push(idx2.index().try_into().unwrap());
                        weights.push(weight.as_raw());
                        has_non_default_weight = has_non_default_weight || weight.is_set();
                    }
                    None => {
                        tracing::error!(
                            "{} Internal error building link state: cannot get index of {}",
                            self.name,
                            dest
                        );
                        continue;
                    }
                }
            }
        }
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
            link_weights: has_non_default_weight.then_some(weights),
        }
    }

    fn make_msg(&self, idxs: &Vec<(NodeIndex, Details)>) -> Result<NetworkMessage, DidntWrite> {
        let mut link_states = vec![];
        for (idx, details) in idxs {
            link_states.push(self.make_link_state(*idx, details));
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

    fn send_on_link(&self, mut idxs: Vec<(NodeIndex, Details)>, transport: &TransportUnicast) {
        for idx in &mut idxs {
            idx.1.locators = self.propagate_locators(idx.0, transport);
        }
        if let Ok(mut msg) = self.make_msg(&idxs) {
            tracing::trace!("{} Send to {:?} {:?}", self.name, transport.get_zid(), msg);
            if let Err(e) = transport.schedule(msg.as_mut()) {
                tracing::debug!("{} Error sending LinkStateList: {}", self.name, e);
            }
        } else {
            tracing::error!("Failed to encode Linkstate message");
        }
    }

    fn send_on_links<P>(&self, mut idxs: Vec<(NodeIndex, Details)>, mut parameters: P)
    where
        P: FnMut(&Link) -> bool,
    {
        for link in self.links.values() {
            for idx in &mut idxs {
                idx.1.locators = self.propagate_locators(idx.0, &link.transport);
            }
            if let Ok(msg) = self.make_msg(&idxs) {
                if parameters(link) {
                    tracing::trace!("{} Send to {} {:?}", self.name, link.zid, msg);
                    if let Err(e) = link.transport.schedule(msg.clone().as_mut()) {
                        tracing::debug!("{} Error sending LinkStateList: {}", self.name, e);
                    }
                }
            } else {
                tracing::error!("Failed to encode Linkstate message");
            }
        }
    }

    // Indicates if locators should be included when propagating Linkstate message
    // from the given node.
    // Returns true if gossip is enabled and if multihop gossip is enabled or
    // the node is one of self neighbours.
    fn propagate_locators(&self, idx: NodeIndex, target: &TransportUnicast) -> bool {
        let target_whatami = target.get_whatami().unwrap_or_default();
        self.gossip
            && self.gossip_target.matches(target_whatami)
            && (self.gossip_multihop
                || idx == self.idx
                || self.links.values().any(|link| {
                    self.graph
                        .node_weight(idx)
                        .map(|node| link.zid == node.zid)
                        .unwrap_or(true)
                }))
    }

    fn get_default_link_weight_to(&self, zid: &ZenohIdProto) -> LinkEdgeWeight {
        self.link_weights.get(zid).copied().unwrap_or_default()
    }

    fn update_edge(&mut self, idx1: NodeIndex, idx2: NodeIndex) {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::default();
        let zid1 = self.graph[idx1].zid;
        let zid2 = self.graph[idx2].zid;
        if zid1 > zid2 {
            hasher.write(&self.graph[idx2].zid.to_le_bytes());
            hasher.write(&self.graph[idx1].zid.to_le_bytes());
        } else {
            hasher.write(&self.graph[idx1].zid.to_le_bytes());
            hasher.write(&self.graph[idx2].zid.to_le_bytes());
        }
        let w1 = self.graph[idx1]
            .links
            .iter()
            .find(|(dest, _weight)| *dest == &zid2)
            .and_then(|(_dest, weight)| weight.is_set().then_some(weight.value()));
        let w2 = self.graph[idx2]
            .links
            .iter()
            .find(|(dest, _weight)| *dest == &zid1)
            .and_then(|(_dest, weight)| weight.is_set().then_some(weight.value()));
        let w = match (w1, w2) {
            (None, None) => LinkEdgeWeight::default().value(),
            (None, Some(w2)) => w2,
            (Some(w1), None) => w1,
            (Some(w1), Some(w2)) => w1.max(w2),
        };
        let weight =
            w as f64 * (1.0 + 0.01 * (((hasher.finish() as u32) as f64) / u32::MAX as f64));
        self.graph.update_edge(idx1, idx2, weight);
    }

    // Converts psid to zid based on src
    fn convert_to_local_link_states(
        &mut self,
        link_states: Vec<LinkState>,
        src: ZenohIdProto,
    ) -> Vec<LocalLinkState> {
        let links = &mut self.links;
        let graph = &self.graph;

        // get src link to recover psid<->zid mappings
        let src_link = match links.values_mut().find(|link| link.zid == src) {
            Some(link) => link,
            None => {
                tracing::error!(
                    "{} Received LinkStateList from unknown link {}",
                    self.name,
                    src
                );
                return Vec::default();
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
                        link_state.link_weights,
                    ))
                } else {
                    match src_link.get_zid(&link_state.psid) {
                        Some(zid) => Some((
                            *zid,
                            link_state.whatami.unwrap_or(WhatAmI::Router),
                            link_state.locators,
                            link_state.sn,
                            link_state.links,
                            link_state.link_weights,
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

        link_states
            .into_iter()
            .map(|(zid, whatami, locators, sn, links, weights)| {
                let mut edges = HashMap::with_capacity(links.len());
                for i in 0..links.len() {
                    match src_link.get_zid(&links[i]) {
                        Some(zid) => {
                            edges.insert(
                                *zid,
                                weights
                                    .as_ref()
                                    .map(|w| LinkEdgeWeight::from_raw(w[i]))
                                    .unwrap_or_default(),
                            );
                        }
                        None => {
                            tracing::error!(
                                "{} Received LinkState from {} with unknown link mapping {}",
                                self.name,
                                src,
                                links[i]
                            );
                            continue;
                        }
                    }
                }
                LocalLinkState {
                    sn,
                    zid,
                    whatami,
                    locators,
                    links: edges,
                }
            })
            .collect::<Vec<_>>()
    }

    fn process_linkstates_peer_to_peer(&mut self, link_states: Vec<LocalLinkState>) -> Changes {
        let mut changes = Changes::default();

        for ls in link_states.into_iter() {
            let idx = match self.get_idx(&ls.zid) {
                None => {
                    let idx = self.add_node(Node {
                        zid: ls.zid,
                        whatami: Some(ls.whatami),
                        locators: ls.locators.clone(),
                        sn: ls.sn,
                        links: ls.links,
                    });
                    changes.updated_nodes.push((idx, self.graph[idx].clone()));
                    if ls.locators.is_none() {
                        continue;
                    }
                    idx
                }
                Some(idx) => {
                    let node = &mut self.graph[idx];
                    if node.sn >= ls.sn {
                        continue;
                    }
                    node.sn = ls.sn;
                    node.links.clone_from(&ls.links);
                    changes.updated_nodes.push((idx, node.clone()));
                    if ls.locators.is_none() || node.locators == ls.locators {
                        continue;
                    }
                    node.locators.clone_from(&ls.locators);
                    idx
                }
            };

            if self.gossip {
                if self.gossip_multihop || self.links.values().any(|link| link.zid == ls.zid) {
                    self.send_on_links(
                        vec![(
                            idx,
                            Details {
                                zid: true,
                                links: false,
                                ..Default::default()
                            },
                        )],
                        |link| link.zid != ls.zid,
                    );
                }

                if self.autoconnect.should_autoconnect(ls.zid, ls.whatami) {
                    // SAFETY if we reached this point locators are not None
                    self.connect_discovered_peer(ls.zid, ls.locators.unwrap());
                }
            }
        }
        changes
    }

    fn connect_discovered_peer(&self, zid: ZenohIdProto, locators: Vec<Locator>) {
        let runtime = self.runtime.clone();
        self.runtime.spawn(async move {
            if runtime
                .manager()
                .get_transport_unicast(&zid)
                .await
                .is_none()
            {
                // random backoff
                let sleep_time =
                    std::time::Duration::from_millis(rand::thread_rng().gen_range(0..100));
                tokio::time::sleep(sleep_time).await;
                runtime.connect_peer(&zid, &locators).await;
            }
        });
    }

    fn propagate_link_states(
        &self,
        src: ZenohIdProto,
        new_nodes: Vec<NodeIndex>,
        updated_nodes: HashSet<NodeIndex>,
    ) {
        // Propagate link states
        // Note: we need to send all states at once for each face
        // to avoid premature node deletion on the other side
        let new_nodes = new_nodes
            .iter()
            .map(|idx| {
                (
                    *idx,
                    Details {
                        zid: true,
                        links: true,
                        ..Default::default()
                    },
                )
            })
            .collect_vec();

        for link in self.links.values() {
            let mut out = new_nodes.clone();
            if link.zid != src {
                for idx in &updated_nodes {
                    if link.zid != self.graph[*idx].zid {
                        out.push((
                            *idx,
                            Details {
                                zid: false,
                                links: true,
                                ..Default::default()
                            },
                        ));
                    }
                }
            }
            if !out.is_empty() {
                self.send_on_link(out, &link.transport);
            }
        }
    }

    pub(crate) fn link_states(
        &mut self,
        link_states: Vec<LinkState>,
        src: ZenohIdProto,
    ) -> Changes {
        tracing::trace!("{} Received from {} raw: {:?}", self.name, src, link_states);

        let link_states = self.convert_to_local_link_states(link_states, src);

        for link_state in &link_states {
            tracing::trace!(
                "{} Received from {} mapped: {:?}",
                self.name,
                src,
                link_state
            );
        }

        if !self.full_linkstate {
            return self.process_linkstates_peer_to_peer(link_states);
        }

        let mut new_nodes = vec![];
        let mut updated_nodes = HashSet::new();
        // Add nodes to graph & filter out up to date states
        for ls in link_states {
            let idx1 = match self.get_idx(&ls.zid) {
                Some(idx) => {
                    let node = &mut self.graph[idx];
                    let oldsn = node.sn;
                    if oldsn < ls.sn {
                        node.sn = ls.sn;
                        node.links.clone_from(&ls.links);
                        if ls.locators.is_some() {
                            node.locators = ls.locators;
                        }
                        if oldsn == 0 {
                            new_nodes.push(idx);
                        } else {
                            updated_nodes.insert(idx);
                        }
                        idx
                    } else {
                        // outdated link state - ignore
                        continue;
                    }
                }
                None => {
                    let node = Node {
                        zid: ls.zid,
                        whatami: Some(ls.whatami),
                        locators: ls.locators,
                        sn: ls.sn,
                        links: ls.links.clone(),
                    };
                    tracing::debug!("{} Add node (state) {}", self.name, ls.zid);
                    let idx = self.add_node(node);
                    new_nodes.push(idx);
                    idx
                }
            };
            // add, update and remove edges
            for dest in ls.links.keys() {
                if let Some(idx2) = self.get_idx(dest) {
                    if self.graph[idx2].links.contains_key(&self.graph[idx1].zid) {
                        self.update_edge(idx1, idx2);
                        tracing::trace!(
                            "{} Update edge (state) {} {}",
                            self.name,
                            self.graph[idx1].zid,
                            self.graph[idx2].zid
                        );
                    }
                } else {
                    let node = Node {
                        zid: *dest,
                        whatami: None,
                        locators: None,
                        sn: 0,
                        links: HashMap::new(),
                    };
                    tracing::debug!("{} Add node (reintroduced) {}", self.name, dest.clone());
                    let idx = self.add_node(node);
                    new_nodes.push(idx);
                }
            }

            let mut edges = vec![];
            let mut neighbors = self.graph.neighbors_undirected(idx1).detach();
            while let Some(edge) = neighbors.next(&self.graph) {
                edges.push(edge);
            }
            for (eidx, idx2) in edges {
                if !ls.links.contains_key(&self.graph[idx2].zid) {
                    tracing::trace!(
                        "{} Remove edge (state) {} {}",
                        self.name,
                        self.graph[idx1].zid,
                        self.graph[idx2].zid
                    );
                    self.graph.remove_edge(eidx);
                }
            }
        }

        let removed = self.remove_detached_nodes();
        new_nodes.retain(|idx| !removed.iter().any(|(r_idx, _)| r_idx == idx));
        updated_nodes.retain(|idx| !removed.iter().any(|(r_idx, _)| r_idx == idx));

        if self.autoconnect.is_enabled() {
            // Connect discovered peers
            for idx in new_nodes.iter().chain(updated_nodes.iter()) {
                let node = &self.graph[*idx];
                if let Some(whatami) = node.whatami {
                    if self.autoconnect.should_autoconnect(node.zid, whatami) {
                        if let Some(locators) = &node.locators {
                            self.connect_discovered_peer(node.zid, locators.clone());
                        }
                    }
                }
            }
        }

        self.propagate_link_states(src, new_nodes, updated_nodes);

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
                    tracing::debug!("{} Add node (link) {}", self.name, zid);
                    (
                        self.add_node(Node {
                            zid,
                            whatami: Some(whatami),
                            locators: None,
                            sn: 0,
                            links: HashMap::new(),
                        }),
                        true,
                    )
                }
            };

            let link_weight = self.get_default_link_weight_to(&zid);
            self.graph[self.idx].links.insert(zid, link_weight);
            self.graph[self.idx].sn += 1;

            if self.full_linkstate
                && self.graph[idx]
                    .links
                    .contains_key(&self.graph[self.idx].zid)
            {
                self.update_edge(self.idx, idx);
                tracing::trace!(
                    "{} Update edge (link) {} {}",
                    &self.name,
                    self.graph[self.idx].zid,
                    zid
                );
            }

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
                                        links: false,
                                        ..Default::default()
                                    },
                                ),
                                (
                                    self.idx,
                                    Details {
                                        zid: false,
                                        links: true,
                                        ..Default::default()
                                    },
                                ),
                            ]
                        } else {
                            vec![(
                                self.idx,
                                Details {
                                    zid: false,
                                    links: true,
                                    ..Default::default()
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
                self.full_linkstate
                    || self.gossip_multihop
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
                        links: self.full_linkstate
                            || (self.router_peers_failover_brokering
                                && idx == self.idx
                                && whatami == WhatAmI::Router),
                        ..Default::default()
                    },
                )
            })
            .collect();
        self.send_on_link(idxs, &transport);
        free_index
    }

    pub(crate) fn remove_link(&mut self, zid: &ZenohIdProto) -> Vec<(NodeIndex, Node)> {
        tracing::trace!("{} remove_link {}", self.name, zid);
        self.links.retain(|_, link| link.zid != *zid);
        self.graph[self.idx].links.retain(|dest, _| dest != zid);

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
                        links: true,
                        ..Default::default()
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
                            links: true,
                            ..Default::default()
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
                for succzid in self.graph[node].links.keys() {
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
                tracing::debug!("Remove node {}", &self.graph[idx].zid);
                removed.push((idx, self.graph.remove_node(idx).unwrap()));
            }
        }
        removed
    }

    pub(crate) fn compute_trees(&mut self) -> Vec<Vec<NodeIndex>> {
        let indexes = self.graph.node_indices().collect::<Vec<NodeIndex>>();
        let max_idx = indexes.iter().max().unwrap();

        let old_children: Vec<Vec<NodeIndex>> =
            self.trees.iter().map(|t| t.children.clone()).collect();

        self.trees.clear();
        self.trees.resize_with(max_idx.index() + 1, || Tree {
            parent: None,
            children: vec![],
            directions: vec![],
        });

        for tree_root_idx in &indexes {
            let paths = petgraph::algo::bellman_ford(&self.graph, *tree_root_idx).unwrap();

            if tree_root_idx.index() == 0 {
                self.distances = paths.distances;
            }

            if tracing::enabled!(tracing::Level::DEBUG) {
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
                tracing::debug!("Tree {} {:?}", self.graph[*tree_root_idx].zid, ps);
            }

            self.trees[tree_root_idx.index()].parent = paths.predecessors[self.idx.index()];

            for idx in &indexes {
                if let Some(parent_idx) = paths.predecessors[idx.index()] {
                    if parent_idx == self.idx {
                        self.trees[tree_root_idx.index()].children.push(*idx);
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

        let mut new_children = Vec::with_capacity(self.trees.len());
        new_children.resize(self.trees.len(), vec![]);

        for i in 0..new_children.len() {
            new_children[i] = if i < old_children.len() {
                self.trees[i]
                    .children
                    .iter()
                    .filter(|idx| !old_children[i].contains(idx))
                    .cloned()
                    .collect()
            } else {
                self.trees[i].children.clone()
            };
        }

        new_children
    }

    #[inline]
    pub(crate) fn get_links(
        &self,
        node: ZenohIdProto,
    ) -> Option<&HashMap<ZenohIdProto, LinkEdgeWeight>> {
        self.get_node(&node).map(|node| &node.links)
    }

    pub(crate) fn links_info(&self) -> HashMap<ZenohIdProto, LinkInfo> {
        let mut out = HashMap::new();
        for e in self
            .graph
            .edges_directed(self.idx, petgraph::Direction::Outgoing)
        {
            let actual_weight: u16 = *e.weight() as u16;
            let dst_weight = self.graph[e.target()]
                .links
                .get(&self.graph[self.idx].zid)
                .copied()
                .unwrap_or_default()
                .into();
            let src_weight = self.graph[self.idx]
                .links
                .get(&self.graph[e.target()].zid)
                .copied()
                .unwrap_or_default()
                .into();
            out.insert(
                self.graph[e.target()].zid,
                LinkInfo {
                    src_weight,
                    dst_weight,
                    actual_weight,
                },
            );
        }
        out
    }

    fn successor_entry(&self, src: NodeIndex, dst: NodeIndex) -> Option<SuccessorEntry> {
        let succ = self.trees[src.index()].directions[dst.index()]?;
        Some(SuccessorEntry {
            source: self.graph[src].zid,
            destination: self.graph[dst].zid,
            successor: self.graph[succ].zid,
        })
    }

    pub(crate) fn route_successor(
        &self,
        src: ZenohIdProto,
        dst: ZenohIdProto,
    ) -> Option<ZenohIdProto> {
        let (mut src_idx, mut dst_idx) = (None, None);
        for (idx, node) in self.graph.node_references() {
            if node.zid == src {
                src_idx = Some(idx);
                if dst_idx.is_some() {
                    break;
                }
            }
            if node.zid == dst {
                dst_idx = Some(idx);
                if src_idx.is_some() {
                    break;
                }
            }
        }
        Some(self.successor_entry(src_idx?, dst_idx?)?.successor)
    }

    pub(crate) fn route_successors(&self) -> Vec<SuccessorEntry> {
        self.graph
            .node_indices()
            .cartesian_product(self.graph.node_indices())
            .filter_map(|(src, dst)| self.successor_entry(src, dst))
            .collect()
    }
}

#[inline]
pub(crate) fn shared_nodes(net1: &Network, net2: &Network) -> Vec<ZenohIdProto> {
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

pub(crate) struct SuccessorEntry {
    pub(crate) source: ZenohIdProto,
    pub(crate) destination: ZenohIdProto,
    pub(crate) successor: ZenohIdProto,
}
