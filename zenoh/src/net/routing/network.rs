//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use petgraph::graph::NodeIndex;
use petgraph::visit::{VisitMap, Visitable};
use std::convert::TryInto;
use vec_map::VecMap;

use super::protocol::core::{whatami, PeerId, ZInt};
use super::protocol::link::Locator;
use super::protocol::proto::{LinkState, ZenohMessage};
use super::protocol::session::Session;

use super::runtime::orchestrator::SessionOrchestrator;

pub(crate) struct Node {
    pub(crate) pid: PeerId,
    pub(crate) whatami: whatami::Type,
    pub(crate) locators: Option<Vec<Locator>>,
    pub(crate) sn: ZInt,
    pub(crate) links: Vec<PeerId>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.pid)
    }
}

pub(crate) struct Link {
    pub(crate) session: Session,
    mappings: VecMap<PeerId>,
    local_mappings: VecMap<ZInt>,
}

impl Link {
    fn new(session: Session) -> Self {
        Link {
            session,
            mappings: VecMap::new(),
            local_mappings: VecMap::new(),
        }
    }

    #[inline]
    pub(crate) fn set_pid_mapping(&mut self, psid: ZInt, pid: PeerId) {
        self.mappings.insert(psid.try_into().unwrap(), pid);
    }

    #[inline]
    pub(crate) fn get_pid(&self, psid: &ZInt) -> Option<&PeerId> {
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

#[derive(Clone)]
pub(crate) struct Tree {
    pub(crate) parent: Option<NodeIndex>,
    pub(crate) childs: Vec<NodeIndex>,
    pub(crate) directions: Vec<Option<NodeIndex>>,
}

pub(crate) struct Network {
    pub(crate) name: String,
    pub(crate) peers_autoconnect: bool,
    pub(crate) idx: NodeIndex,
    pub(crate) links: Vec<Link>,
    pub(crate) trees: Vec<Tree>,
    pub(crate) graph: petgraph::stable_graph::StableUnGraph<Node, f64>,
    pub(crate) orchestrator: SessionOrchestrator,
}

impl Network {
    pub(crate) async fn new(
        name: String,
        pid: PeerId,
        orchestrator: SessionOrchestrator,
        peers_autoconnect: bool,
    ) -> Self {
        let mut graph = petgraph::stable_graph::StableGraph::default();
        log::debug!("{} Add node (self) {}", name, pid);
        let idx = graph.add_node(Node {
            pid,
            whatami: orchestrator.whatami,
            locators: None,
            sn: 1,
            links: vec![],
        });
        Network {
            name,
            peers_autoconnect,
            idx,
            links: vec![],
            trees: vec![Tree {
                parent: None,
                childs: vec![],
                directions: vec![None],
            }],
            graph,
            orchestrator,
        }
    }

    pub(crate) fn dot(&self) -> String {
        std::format!(
            "{:?}",
            petgraph::dot::Dot::with_config(&self.graph, &[petgraph::dot::Config::EdgeNoLabel])
        )
    }

    #[inline]
    pub(crate) fn get_idx(&self, pid: &PeerId) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|idx| self.graph[*idx].pid == *pid)
    }

    #[inline]
    pub(crate) fn get_link(&self, id: usize) -> &Link {
        &self.links[id]
    }

    #[inline]
    pub(crate) fn get_link_from_pid(&self, pid: &PeerId) -> Option<&Link> {
        self.links
            .iter()
            .find(|link| link.session.get_pid().unwrap() == *pid)
    }

    #[inline]
    pub(crate) fn get_local_context(&self, context: ZInt, link_id: usize) -> usize {
        (*self.get_link(link_id).get_local_psid(&context).unwrap())
            .try_into()
            .unwrap()
    }

    fn add_node(&mut self, node: Node) -> NodeIndex {
        let pid = node.pid.clone();
        let idx = self.graph.add_node(node);
        for link in &mut self.links {
            if let Some((psid, _)) = link.mappings.iter().find(|(_, p)| **p == pid) {
                link.local_mappings.insert(psid, idx.index() as ZInt);
            }
        }
        idx
    }

    async fn make_link_state(&self, idx: NodeIndex, details: bool) -> LinkState {
        let links = self.graph[idx]
            .links
            .iter()
            .filter_map(|pid| {
                if let Some(idx2) = self.get_idx(pid) {
                    Some(idx2.index().try_into().unwrap())
                } else {
                    log::error!(
                        "{} Internal error building link state: cannot get index of {}",
                        self.name,
                        pid
                    );
                    None
                }
            })
            .collect();
        LinkState {
            psid: idx.index().try_into().unwrap(),
            sn: self.graph[idx].sn,
            pid: if details {
                Some(self.graph[idx].pid.clone())
            } else {
                None
            },
            whatami: None,
            locators: if idx == self.idx {
                Some(self.orchestrator.manager().await.get_locators().await)
            } else {
                self.graph[idx].locators.clone()
            },
            links,
        }
    }

    async fn make_msg(&self, idxs: Vec<(NodeIndex, bool)>) -> ZenohMessage {
        let mut list = vec![];
        for (idx, details) in idxs {
            list.push(self.make_link_state(idx, details).await);
        }
        ZenohMessage::make_link_state_list(list, None)
    }

    async fn send_on_link(&self, idxs: Vec<(NodeIndex, bool)>, session: &Session) {
        let msg = self.make_msg(idxs).await;
        log::trace!(
            "{} Send to {} {:?}",
            self.name,
            session.get_pid().unwrap(),
            msg
        );
        if let Err(e) = session.handle_message(msg).await {
            log::error!("{} Error sending LinkStateList: {}", self.name, e);
        }
    }

    async fn send_on_links<P>(&self, idxs: Vec<(NodeIndex, bool)>, mut predicate: P)
    where
        P: FnMut(&Link) -> bool,
    {
        let msg = self.make_msg(idxs).await;
        for link in &self.links {
            if predicate(link) {
                log::trace!(
                    "{} Send to {} {:?}",
                    self.name,
                    link.session.get_pid().unwrap(),
                    msg
                );
                if let Err(e) = link.session.handle_message(msg.clone()).await {
                    log::error!("{} Error sending LinkStateList: {}", self.name, e);
                }
            }
        }
    }

    fn update_edge(&mut self, idx1: NodeIndex, idx2: NodeIndex) {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::default();
        if self.graph[idx1].pid.as_slice() > self.graph[idx2].pid.as_slice() {
            hasher.write(self.graph[idx2].pid.as_slice());
            hasher.write(self.graph[idx1].pid.as_slice());
        } else {
            hasher.write(self.graph[idx1].pid.as_slice());
            hasher.write(self.graph[idx2].pid.as_slice());
        }
        let weight = 100.0 + ((hasher.finish() as u32) as f64) / std::u32::MAX as f64;
        self.graph.update_edge(idx1, idx2, weight);
    }

    pub(crate) async fn link_states(
        &mut self,
        link_states: Vec<LinkState>,
        src: PeerId,
    ) -> Vec<(NodeIndex, Node)> {
        log::trace!("{} Received from {} raw: {:?}", self.name, src, link_states);

        let graph = &self.graph;
        let links = &mut self.links;

        let src_link = match links
            .iter_mut()
            .find(|link| link.session.get_pid().unwrap() == src)
        {
            Some(link) => link,
            None => {
                log::error!(
                    "{} Received LinkStateList from unknown link {}",
                    self.name,
                    src
                );
                return vec![];
            }
        };

        // register psid<->pid mappings & apply mapping to nodes
        let link_states = link_states
            .into_iter()
            .filter_map(|link_state| {
                if let Some(pid) = link_state.pid {
                    src_link.set_pid_mapping(link_state.psid, pid.clone());
                    if let Some(idx) = graph.node_indices().find(|idx| graph[*idx].pid == pid) {
                        src_link.set_local_psid_mapping(link_state.psid, idx.index() as u64);
                    }
                    Some((
                        pid,
                        link_state.whatami.or(Some(whatami::ROUTER)).unwrap(),
                        link_state.locators,
                        link_state.sn,
                        link_state.links,
                    ))
                } else {
                    match src_link.get_pid(&link_state.psid) {
                        Some(pid) => Some((
                            pid.clone(),
                            link_state.whatami.or(Some(whatami::ROUTER)).unwrap(),
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
            .collect::<Vec<(PeerId, whatami::Type, Option<Vec<Locator>>, ZInt, Vec<ZInt>)>>();

        // apply psid<->pid mapping to links
        let src_link = self.get_link_from_pid(&src).unwrap();
        let link_states = link_states
            .into_iter()
            .map(|(pid, wai, locs, sn, links)| {
                let links: Vec<PeerId> = links
                    .iter()
                    .filter_map(|l| {
                        if let Some(pid) = src_link.get_pid(&l) {
                            Some(pid.clone())
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
                (pid, wai, locs, sn, links)
            })
            .collect::<Vec<(
                PeerId,
                whatami::Type,
                Option<Vec<Locator>>,
                ZInt,
                Vec<PeerId>,
            )>>();

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

        // Add nodes to graph & filter out up to date states
        let mut link_states = link_states
            .into_iter()
            .filter_map(
                |(pid, whatami, locators, sn, links)| match self.get_idx(&pid) {
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
                            pid: pid.clone(),
                            whatami,
                            locators,
                            sn,
                            links: links.clone(),
                        };
                        log::debug!("{} Add node (state) {}", self.name, pid);
                        let idx = self.add_node(node);
                        Some((links, idx, true))
                    }
                },
            )
            .collect::<Vec<(Vec<PeerId>, NodeIndex, bool)>>();

        // Add/remove edges from graph
        let mut reintroduced_nodes = vec![];
        for (links, idx1, _) in &link_states {
            for link in links {
                if let Some(idx2) = self.get_idx(&link) {
                    if self.graph[idx2].links.contains(&self.graph[*idx1].pid) {
                        log::trace!(
                            "{} Update edge (state) {} {}",
                            self.name,
                            self.graph[*idx1].pid,
                            self.graph[idx2].pid
                        );
                        self.update_edge(*idx1, idx2);
                    }
                } else {
                    let node = Node {
                        pid: link.clone(),
                        whatami: 0,
                        locators: None,
                        sn: 0,
                        links: vec![],
                    };
                    log::debug!("{} Add node (reintroduced) {}", self.name, link.clone());
                    let idx = self.add_node(node);
                    reintroduced_nodes.push((vec![], idx, true));
                }
            }
            let mut neighbors = self.graph.neighbors_undirected(*idx1).detach();
            while let Some((eidx, idx2)) = neighbors.next(&self.graph) {
                if !links.contains(&self.graph[idx2].pid) {
                    log::trace!(
                        "{} Remove edge (state) {} {}",
                        self.name,
                        self.graph[*idx1].pid,
                        self.graph[idx2].pid
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
            .collect::<Vec<(Vec<PeerId>, NodeIndex, bool)>>();

        if self.peers_autoconnect && self.orchestrator.whatami == whatami::PEER {
            // Connect discovered peers
            for (_, idx, _) in &link_states {
                let node = &self.graph[*idx];
                if node.whatami == whatami::PEER || node.whatami == whatami::ROUTER {
                    if let Some(locators) = &node.locators {
                        let orchestrator = self.orchestrator.clone();
                        let pid = node.pid.clone();
                        let locators = locators.clone();
                        async_std::task::spawn(async move {
                            // random backoff
                            async_std::task::sleep(std::time::Duration::from_millis(
                                rand::random::<u64>() % 100,
                            ))
                            .await;
                            orchestrator.connect_peer(&pid, &locators).await;
                        });
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
                Vec<(Vec<PeerId>, NodeIndex, bool)>,
                Vec<(Vec<PeerId>, NodeIndex, bool)>,
            ) = link_states.into_iter().partition(|(_, _, new)| *new);
            let new_idxs = new_idxs
                .into_iter()
                .map(|(_, idx1, _new_node)| (idx1, true))
                .collect::<Vec<(NodeIndex, bool)>>();
            for link in &self.links {
                let link_pid = link.session.get_pid().unwrap();
                if link_pid != src {
                    let updated_idxs: Vec<(NodeIndex, bool)> = updated_idxs
                        .clone()
                        .into_iter()
                        .filter_map(|(_, idx1, _)| {
                            if link_pid != self.graph[idx1].pid {
                                Some((idx1, false))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !new_idxs.is_empty() || !updated_idxs.is_empty() {
                        self.send_on_link(
                            [&new_idxs[..], &updated_idxs[..]].concat(),
                            &link.session,
                        )
                        .await;
                    }
                } else if !new_idxs.is_empty() {
                    self.send_on_link(new_idxs.clone(), &link.session).await;
                }
            }
        }
        removed
    }

    pub(crate) async fn add_link(&mut self, session: Session) -> usize {
        self.links.push(Link::new(session.clone()));

        let pid = session.get_pid().unwrap();
        let whatami = session.get_whatami().unwrap();
        let (idx, new) = match self.get_idx(&pid) {
            Some(idx) => (idx, false),
            None => {
                log::debug!("{} Add node (link) {}", self.name, pid);
                (
                    self.add_node(Node {
                        pid: pid.clone(),
                        whatami,
                        locators: None,
                        sn: 0,
                        links: vec![],
                    }),
                    true,
                )
            }
        };
        if self.graph[idx].links.contains(&self.graph[self.idx].pid) {
            log::trace!("Update edge (link) {} {}", self.graph[self.idx].pid, pid);
            self.update_edge(self.idx, idx);
        }
        self.graph[self.idx].links.push(pid.clone());
        self.graph[self.idx].sn += 1;

        if new {
            self.send_on_links(vec![(idx, true), (self.idx, false)], |link| {
                link.session.get_pid().unwrap() != pid
            })
            .await;
        } else {
            self.send_on_links(vec![(self.idx, false)], |link| {
                link.session.get_pid().unwrap() != pid
            })
            .await;
        }

        let idxs = self.graph.node_indices().map(|i| (i, true)).collect();
        self.send_on_link(idxs, &session).await;
        self.links.len() - 1
    }

    pub(crate) async fn remove_link(&mut self, session: &Session) -> Vec<(NodeIndex, Node)> {
        let pid = session.get_pid().unwrap();
        log::trace!("{} remove_link {}", self.name, pid);
        self.links
            .retain(|link| link.session.get_pid().unwrap() != pid);
        self.graph[self.idx].links.retain(|link| *link != pid);

        let idx = self.get_idx(&pid).unwrap();
        if let Some(edge) = self.graph.find_edge_undirected(self.idx, idx) {
            self.graph.remove_edge(edge.0);
        }
        let removed = self.remove_detached_nodes();

        self.graph[self.idx].sn += 1;

        let links = self
            .links
            .iter()
            .map(|link| {
                self.get_idx(&link.session.get_pid().unwrap())
                    .unwrap()
                    .index()
                    .try_into()
                    .unwrap()
            })
            .collect::<Vec<ZInt>>();

        let msg = ZenohMessage::make_link_state_list(
            vec![LinkState {
                psid: self.idx.index().try_into().unwrap(),
                sn: self.graph[self.idx].sn,
                pid: None,
                whatami: None,
                locators: Some(self.orchestrator.manager().await.get_locators().await),
                links,
            }],
            None,
        );

        for link in &self.links {
            if let Err(e) = link.session.handle_message(msg.clone()).await {
                log::error!("{} Error sending LinkStateList: {}", self.name, e);
            }
        }

        removed
    }

    fn remove_detached_nodes(&mut self) -> Vec<(NodeIndex, Node)> {
        let mut dfs_stack = vec![self.idx];
        let mut visit_map = self.graph.visit_map();
        while let Some(node) = dfs_stack.pop() {
            if visit_map.visit(node) {
                for succpid in &self.graph[node].links {
                    if let Some(succ) = self.get_idx(succpid) {
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
                log::debug!("Remove node {}", &self.graph[idx].pid);
                removed.push((idx, self.graph.remove_node(idx).unwrap()));
            }
        }
        removed
    }

    pub(crate) async fn compute_trees(&mut self) -> Vec<Vec<NodeIndex>> {
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
            let path = petgraph::algo::bellman_ford(&self.graph, *tree_root_idx)
                .unwrap()
                .1;

            if log::log_enabled!(log::Level::Debug) {
                let ps: Vec<Option<String>> = path
                    .iter()
                    .enumerate()
                    .map(|(is, o)| {
                        o.map(|ip| {
                            format!(
                                "{} <- {}",
                                self.graph[ip].pid.clone(),
                                self.graph[NodeIndex::new(is)].pid.clone()
                            )
                        })
                    })
                    .collect();
                log::debug!("Tree {} {:?}", self.graph[*tree_root_idx].pid, ps);
            }

            self.trees[tree_root_idx.index()].parent = path[self.idx.index()];

            for idx in &indexes {
                if let Some(parent_idx) = path[idx.index()] {
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
                    while let Some(parent) = path[current.index()] {
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
}
