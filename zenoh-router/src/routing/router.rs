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
use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_trait::async_trait;
use petgraph::graph::NodeIndex;
use petgraph::visit::{VisitMap, Visitable};
use std::collections::HashMap;
use std::convert::TryInto;
use uhlc::HLC;

use zenoh_protocol::core::{whatami, PeerId, ZInt};
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{LinkState, ZenohBody, ZenohMessage};
use zenoh_protocol::session::{
    DeMux, Mux, Primitives, Session, SessionEventHandler, SessionHandler,
};

use zenoh_util::core::ZResult;

use crate::routing::broker::{Broker, Tables};
use crate::routing::face::Face;
use crate::runtime::orchestrator::SessionOrchestrator;

pub struct Router {
    whatami: whatami::Type,
    pub broker: Broker,
    pub routers_net: Option<Arc<RwLock<Network>>>,
    pub peers_net: Option<Arc<RwLock<Network>>>,
}

impl Router {
    pub fn new(whatami: whatami::Type, hlc: Option<HLC>) -> Self {
        Router {
            whatami,
            broker: Broker::new(whatami, hlc),
            routers_net: None,
            peers_net: None,
        }
    }

    pub async fn init_link_state(&mut self, pid: PeerId, orchestrator: SessionOrchestrator) {
        if orchestrator.whatami == whatami::ROUTER {
            self.routers_net = Some(Arc::new(RwLock::new(
                Network::new(
                    "[Routers network]".to_string(),
                    pid.clone(),
                    orchestrator.clone(),
                )
                .await,
            )));
        }
        self.peers_net = Some(Arc::new(RwLock::new(
            Network::new("[Peers network]".to_string(), pid, orchestrator).await,
        )));
    }

    pub async fn new_primitives(
        &self,
        primitives: Arc<dyn Primitives + Send + Sync>,
    ) -> Arc<dyn Primitives + Send + Sync> {
        self.broker.new_primitives(primitives).await
    }
}

#[async_trait]
impl SessionHandler for Router {
    async fn new_session(
        &self,
        session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let whatami = session.get_whatami()?;
        if whatami != whatami::CLIENT && self.peers_net.is_some() {
            let network = match self.whatami {
                whatami::ROUTER => match whatami {
                    whatami::ROUTER => self.routers_net.as_ref().unwrap().clone(),
                    _ => self.peers_net.as_ref().unwrap().clone(),
                },
                _ => self.peers_net.as_ref().unwrap().clone(),
            };
            let handler = Arc::new(LinkStateInterceptor::new(
                session.clone(),
                network.clone(),
                DeMux::new(Face {
                    tables: self.broker.tables.clone(),
                    state: Tables::open_face(
                        &self.broker.tables,
                        whatami,
                        Arc::new(Mux::new(Arc::new(session.clone()))),
                    )
                    .await
                    .upgrade()
                    .unwrap(),
                }),
            ));
            network.write().await.add_link(session.clone()).await;
            Ok(handler)
        } else {
            Ok(Arc::new(DeMux::new(Face {
                tables: self.broker.tables.clone(),
                state: Tables::open_face(
                    &self.broker.tables,
                    whatami,
                    Arc::new(Mux::new(Arc::new(session.clone()))),
                )
                .await
                .upgrade()
                .unwrap(),
            })))
        }
    }
}

struct Node {
    pid: PeerId,
    whatami: whatami::Type,
    locators: Option<Vec<Locator>>,
    sn: ZInt,
    links: Vec<PeerId>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.pid)
    }
}

struct Link {
    session: Session,
    mappings: HashMap<ZInt, PeerId>,
}

impl Link {
    fn new(session: Session) -> Self {
        Link {
            session,
            mappings: HashMap::new(),
        }
    }
}

pub struct Network {
    name: String,
    idx: NodeIndex,
    links: Vec<Link>,
    graph: petgraph::stable_graph::StableUnGraph<Node, f32>,
    orchestrator: SessionOrchestrator,
}

impl Network {
    async fn new(name: String, pid: PeerId, orchestrator: SessionOrchestrator) -> Self {
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
            idx,
            links: vec![],
            graph,
            orchestrator,
        }
    }

    pub fn dot(&self) -> String {
        std::format!(
            "{:?}",
            petgraph::dot::Dot::with_config(&self.graph, &[petgraph::dot::Config::EdgeNoLabel])
        )
    }

    fn get_idx(&self, pid: &PeerId) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|idx| self.graph[*idx].pid == *pid)
    }

    fn get_link(&self, pid: &PeerId) -> Option<&Link> {
        self.links
            .iter()
            .find(|link| link.session.get_pid().unwrap() == *pid)
    }

    fn get_link_mut(&mut self, pid: &PeerId) -> Option<&mut Link> {
        self.links
            .iter_mut()
            .find(|link| link.session.get_pid().unwrap() == *pid)
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
                Some(self.orchestrator.manager.get_locators().await)
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

    async fn link_states(&mut self, link_states: Vec<LinkState>, src: PeerId) {
        log::trace!("{} Received from {} raw: {:?}", self.name, src, link_states);

        let src_link = match self.get_link_mut(&src) {
            Some(link) => link,
            None => {
                log::error!(
                    "{} Received LinkStateList from unknown link {}",
                    self.name,
                    src
                );
                return;
            }
        };

        // register psid<->pid mappings & apply mapping to nodes
        let link_states = link_states
            .into_iter()
            .filter_map(|link_state| {
                if let Some(pid) = link_state.pid {
                    src_link.mappings.insert(link_state.psid, pid.clone());
                    Some((
                        pid,
                        link_state.whatami.or(Some(whatami::ROUTER)).unwrap(),
                        link_state.locators,
                        link_state.sn,
                        link_state.links,
                    ))
                } else {
                    match src_link.mappings.get(&link_state.psid) {
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
        let src_link = self.get_link(&src).unwrap();
        let link_states = link_states
            .into_iter()
            .map(|(pid, wai, locs, sn, links)| {
                let links: Vec<PeerId> = links
                    .iter()
                    .filter_map(|l| {
                        if let Some(pid) = src_link.mappings.get(&l) {
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
        let link_states = link_states
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
                        let idx = self.graph.add_node(node);
                        Some((links, idx, true))
                    }
                },
            )
            .collect::<Vec<(Vec<PeerId>, NodeIndex, bool)>>();

        // Add/remove edges from graph
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
                        self.graph.update_edge(*idx1, idx2, 1.0);
                    }
                } else {
                    log::error!(
                        "{} Internal error handling link state: cannot get index of {}",
                        self.name,
                        &link
                    );
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

        let removed = self.remove_detached_nodes();
        let link_states = link_states
            .into_iter()
            .filter(|ls| !removed.contains(&ls.1))
            .collect::<Vec<(Vec<PeerId>, NodeIndex, bool)>>();

        if self.orchestrator.whatami == whatami::PEER {
            // Connect discovered peers
            for (_, idx, _) in &link_states {
                let node = &self.graph[*idx];
                if node.whatami == whatami::PEER || node.whatami == whatami::ROUTER {
                    if let Some(locators) = &node.locators {
                        let orchestrator = self.orchestrator.clone();
                        let pid = node.pid.clone();
                        let locators = locators.clone();
                        async_std::task::spawn(async move {
                            // Workaround for concurrent session estaglishment problem
                            async_std::task::sleep(std::time::Duration::from_millis(
                                rand::random::<u64>() % 500,
                            ))
                            .await;
                            // /Workaround
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
    }

    async fn add_link(&mut self, session: Session) {
        self.links.push(Link::new(session.clone()));

        let pid = session.get_pid().unwrap();
        let whatami = session.get_whatami().unwrap();
        let (idx, new) = match self.get_idx(&pid) {
            Some(idx) => (idx, false),
            None => {
                log::debug!("{} Add node (link) {}", self.name, pid);
                (
                    self.graph.add_node(Node {
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
            self.graph.update_edge(self.idx, idx, 1.0);
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
    }

    async fn remove_link(&mut self, session: &Session) {
        let pid = session.get_pid().unwrap();
        log::trace!("{} remove_link {}", self.name, pid);
        self.links
            .retain(|link| link.session.get_pid().unwrap() != pid);
        self.graph[self.idx].links.retain(|link| *link != pid);

        let idx = self.get_idx(&pid).unwrap();
        if let Some(edge) = self.graph.find_edge_undirected(self.idx, idx) {
            self.graph.remove_edge(edge.0);
        }
        self.remove_detached_nodes();

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
                locators: Some(self.orchestrator.manager.get_locators().await),
                links,
            }],
            None,
        );

        for link in &self.links {
            if let Err(e) = link.session.handle_message(msg.clone()).await {
                log::error!("{} Error sending LinkStateList: {}", self.name, e);
            }
        }
    }

    fn remove_detached_nodes(&mut self) -> Vec<NodeIndex> {
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
        self.graph.retain_nodes(|graph, idx| {
            let pid = &graph[idx].pid;
            let retain = visit_map.is_visited(&idx);
            if !retain {
                log::debug!("Remove node {}", pid);
                removed.push(idx);
            }
            retain
        });
        removed
    }
}

pub struct LinkStateInterceptor {
    session: Session,
    network: Arc<RwLock<Network>>,
    demux: DeMux<Face>,
}

impl LinkStateInterceptor {
    fn new(session: Session, network: Arc<RwLock<Network>>, demux: DeMux<Face>) -> Self {
        LinkStateInterceptor {
            session,
            network,
            demux,
        }
    }
}

#[async_trait]
impl SessionEventHandler for LinkStateInterceptor {
    async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()> {
        match msg.body {
            ZenohBody::LinkStateList(list) => {
                let pid = self.session.get_pid().unwrap();
                self.network
                    .write()
                    .await
                    .link_states(list.link_states, pid)
                    .await;
                Ok(())
            }
            _ => self.demux.handle_message(msg).await,
        }
    }

    async fn new_link(&self, _link: zenoh_protocol::link::Link) {}

    async fn del_link(&self, _link: zenoh_protocol::link::Link) {}

    async fn closing(&self) {
        self.demux.closing().await;
        self.network.write().await.remove_link(&self.session).await;
    }

    async fn closed(&self) {}
}
