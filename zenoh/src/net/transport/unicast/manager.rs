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
use super::super::TransportManager;
use super::authenticator::*;
use super::protocol::core::{PeerId, WhatAmI, ZInt};
use super::transport::{TransportUnicastConfig, TransportUnicastInner};
use super::*;
use crate::net::link::*;
use async_std::prelude::*;
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex};
use async_std::task;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::properties::config::*;
use zenoh_util::{zasynclock, zerror, zlock, zparse};

pub struct TransportManagerConfigUnicast {
    pub lease: Duration,
    pub keep_alive: Duration,
    pub open_timeout: Duration,
    pub open_pending: usize,
    pub max_sessions: usize,
    pub max_links: usize,
    pub is_qos: bool,
    #[cfg(feature = "zero-copy")]
    pub is_shm: bool,
    pub peer_authenticator: HashSet<PeerAuthenticator>,
    pub link_authenticator: HashSet<LinkAuthenticator>,
}

impl Default for TransportManagerConfigUnicast {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl TransportManagerConfigUnicast {
    pub fn builder() -> TransportManagerConfigBuilderUnicast {
        TransportManagerConfigBuilderUnicast::default()
    }
}

pub struct TransportManagerConfigBuilderUnicast {
    pub(super) lease: Duration,
    pub(super) keep_alive: Duration,
    pub(super) open_timeout: Duration,
    pub(super) open_pending: usize,
    pub(super) max_sessions: usize,
    pub(super) max_links: usize,
    pub(super) is_qos: bool,
    #[cfg(feature = "zero-copy")]
    pub(super) is_shm: bool,
    pub(super) peer_authenticator: HashSet<PeerAuthenticator>,
    pub(super) link_authenticator: HashSet<LinkAuthenticator>,
}

impl Default for TransportManagerConfigBuilderUnicast {
    fn default() -> TransportManagerConfigBuilderUnicast {
        TransportManagerConfigBuilderUnicast {
            lease: Duration::from_millis(zparse!(ZN_LINK_LEASE_DEFAULT).unwrap()),
            keep_alive: Duration::from_millis(zparse!(ZN_LINK_KEEP_ALIVE_DEFAULT).unwrap()),
            open_timeout: Duration::from_millis(zparse!(ZN_OPEN_TIMEOUT_DEFAULT).unwrap()),
            open_pending: zparse!(ZN_OPEN_INCOMING_PENDING_DEFAULT).unwrap(),
            max_sessions: zparse!(ZN_MAX_SESSIONS_DEFAULT).unwrap(),
            max_links: zparse!(ZN_MAX_LINKS_DEFAULT).unwrap(),
            is_qos: zparse!(ZN_QOS_DEFAULT).unwrap(),
            #[cfg(feature = "zero-copy")]
            is_shm: zparse!(ZN_SHM_DEFAULT).unwrap(),
            peer_authenticator: HashSet::new(),
            link_authenticator: HashSet::new(),
        }
    }
}

impl TransportManagerConfigBuilderUnicast {
    pub fn lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    pub fn keep_alive(mut self, keep_alive: Duration) -> Self {
        self.keep_alive = keep_alive;
        self
    }

    pub fn open_timeout(mut self, open_timeout: Duration) -> Self {
        self.open_timeout = open_timeout;
        self
    }

    pub fn open_pending(mut self, open_pending: usize) -> Self {
        self.open_pending = open_pending;
        self
    }

    pub fn max_sessions(mut self, max_sessions: usize) -> Self {
        self.max_sessions = max_sessions;
        self
    }

    pub fn max_links(mut self, max_links: usize) -> Self {
        self.max_links = max_links;
        self
    }

    pub fn peer_authenticator(mut self, peer_authenticator: HashSet<PeerAuthenticator>) -> Self {
        self.peer_authenticator = peer_authenticator;
        self
    }

    pub fn link_authenticator(mut self, link_authenticator: HashSet<LinkAuthenticator>) -> Self {
        self.link_authenticator = link_authenticator;
        self
    }

    pub fn qos(mut self, is_qos: bool) -> Self {
        self.is_qos = is_qos;
        self
    }

    #[cfg(feature = "zero-copy")]
    pub fn shm(mut self, is_shm: bool) -> Self {
        self.is_shm = is_shm;
        self
    }

    pub async fn from_config(
        mut self,
        properties: &ConfigProperties,
    ) -> ZResult<TransportManagerConfigBuilderUnicast> {
        if let Some(v) = properties.get(&ZN_LINK_LEASE_KEY) {
            self = self.lease(Duration::from_millis(zparse!(v)?));
        }
        if let Some(v) = properties.get(&ZN_LINK_KEEP_ALIVE_KEY) {
            self = self.keep_alive(Duration::from_millis(zparse!(v)?));
        }
        if let Some(v) = properties.get(&ZN_OPEN_TIMEOUT_KEY) {
            self = self.open_timeout(Duration::from_millis(zparse!(v)?));
        }
        if let Some(v) = properties.get(&ZN_OPEN_INCOMING_PENDING_KEY) {
            self = self.open_pending(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_MAX_SESSIONS_KEY) {
            self = self.max_sessions(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_MAX_LINKS_KEY) {
            self = self.max_links(zparse!(v)?);
        }
        if let Some(v) = properties.get(&ZN_QOS_KEY) {
            self = self.qos(zparse!(v)?);
        }
        #[cfg(feature = "zero-copy")]
        if let Some(v) = properties.get(&ZN_SHM_KEY) {
            self = self.shm(zparse!(v)?);
        }

        self = self.peer_authenticator(PeerAuthenticator::from_config(properties).await?);
        self = self.link_authenticator(LinkAuthenticator::from_config(properties).await?);

        Ok(self)
    }

    #[allow(unused_mut)]
    pub fn build(mut self) -> TransportManagerConfigUnicast {
        #[cfg(feature = "zero-copy")]
        if self.is_shm
            && !self
                .peer_authenticator
                .iter()
                .any(|a| a.id() == PeerAuthenticatorId::Shm)
        {
            self.peer_authenticator
                .insert(SharedMemoryAuthenticator::new().into());
        }
        TransportManagerConfigUnicast {
            lease: self.lease,
            keep_alive: self.keep_alive,
            open_timeout: self.open_timeout,
            open_pending: self.open_pending,
            max_sessions: self.max_sessions,
            max_links: self.max_links,
            peer_authenticator: self.peer_authenticator,
            link_authenticator: self.link_authenticator,
            is_qos: self.is_qos,
            #[cfg(feature = "zero-copy")]
            is_shm: self.is_shm,
        }
    }
}

pub struct TransportManagerStateUnicast {
    // Outgoing and incoming opened (i.e. established) transports
    pub(super) opened: AsyncArc<AsyncMutex<HashMap<PeerId, Opened>>>,
    // Incoming uninitialized transports
    pub(super) incoming: AsyncArc<AsyncMutex<HashMap<LinkUnicast, Option<Vec<u8>>>>>,
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManagerUnicast>>>,
    // Established transports
    pub(super) transports: Arc<Mutex<HashMap<PeerId, Arc<TransportUnicastInner>>>>,
}

impl Default for TransportManagerStateUnicast {
    fn default() -> TransportManagerStateUnicast {
        TransportManagerStateUnicast {
            opened: AsyncArc::new(AsyncMutex::new(HashMap::new())),
            incoming: AsyncArc::new(AsyncMutex::new(HashMap::new())),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub(super) struct Opened {
    pub(super) whatami: WhatAmI,
    pub(super) sn_resolution: ZInt,
    pub(super) initial_sn: ZInt,
}

impl TransportManager {
    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    fn new_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<LinkManagerUnicast> {
        let mut w_guard = zlock!(self.state.unicast.protocols);
        match w_guard.get(protocol) {
            Some(lm) => Ok(lm.clone()),
            None => {
                let lm = LinkManagerBuilderUnicast::make(self.clone(), protocol)?;
                w_guard.insert(protocol.clone(), lm.clone());
                Ok(lm)
            }
        }
    }

    fn get_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<LinkManagerUnicast> {
        match zlock!(self.state.unicast.protocols).get(protocol) {
            Some(manager) => Ok(manager.clone()),
            None => zerror!(ZErrorKind::InvalidLocator {
                descr: format!(
                    "Can not get the link manager for protocol ({}) because it has not been found",
                    protocol
                )
            }),
        }
    }

    fn del_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zlock!(self.state.unicast.protocols).remove(protocol) {
            Some(_) => Ok(()),
            None => zerror!(ZErrorKind::InvalidLocator {
                descr: format!("Can not delete the link manager for protocol ({}) because it has not been found.", protocol)
            })
        }
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener_unicast(&self, mut endpoint: EndPoint) -> ZResult<Locator> {
        let manager = self.new_link_manager_unicast(&endpoint.locator.address.get_proto())?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self
            .config
            .endpoint
            .get(&endpoint.locator.address.get_proto())
        {
            let config = match endpoint.config.as_ref() {
                Some(ec) => {
                    let mut config = config.clone();
                    for (k, v) in ec.iter() {
                        config.insert(k.clone(), v.clone());
                    }
                    config
                }
                None => config.clone(),
            };
            endpoint.config = Some(Arc::new(config));
        };
        manager.new_listener(endpoint).await
    }

    pub async fn del_listener_unicast(&self, endpoint: &EndPoint) -> ZResult<()> {
        let lm = self.get_link_manager_unicast(&endpoint.locator.address.get_proto())?;
        lm.del_listener(endpoint).await?;
        if lm.get_listeners().is_empty() {
            self.del_link_manager_unicast(&endpoint.locator.address.get_proto())?;
        }
        Ok(())
    }

    pub fn get_listeners_unicast(&self) -> Vec<EndPoint> {
        let mut vec: Vec<EndPoint> = vec![];
        for p in zlock!(self.state.unicast.protocols).values() {
            vec.extend_from_slice(&p.get_listeners());
        }
        vec
    }

    pub fn get_locators_unicast(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zlock!(self.state.unicast.protocols).values() {
            vec.extend_from_slice(&p.get_locators());
        }
        vec
    }

    /*************************************/
    /*             TRANSPORT             */
    /*************************************/
    pub(super) fn init_transport_unicast(
        &self,
        config: TransportConfigUnicast,
    ) -> ZResult<TransportUnicast> {
        let mut guard = zlock!(self.state.unicast.transports);

        // First verify if the transport already exists
        if let Some(transport) = guard.get(&config.peer) {
            if transport.whatami != config.whatami {
                let e = format!(
                    "Transport with peer {} already exist. Invalid whatami: {}. Execpted: {}.",
                    config.peer, config.whatami, transport.whatami
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            if transport.sn_resolution != config.sn_resolution {
                let e = format!(
                    "Transport with peer {} already exist. Invalid sn resolution: {}. Execpted: {}.",
                    config.peer, config.sn_resolution, transport.sn_resolution
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            if transport.is_shm != config.is_shm {
                let e = format!(
                    "Transport with peer {} already exist. Invalid is_shm: {}. Execpted: {}.",
                    config.peer, config.is_shm, transport.is_shm
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            return Ok(transport.into());
        }

        // Then verify that we haven't reached the transport number limit
        if guard.len() >= self.config.unicast.max_sessions {
            let e = format!(
                "Max transports reached ({}). Denying new transport with peer: {}",
                self.config.unicast.max_sessions, config.peer
            );
            log::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        // Create the transport transport
        let stc = TransportUnicastConfig {
            manager: self.clone(),
            pid: config.peer,
            whatami: config.whatami,
            sn_resolution: config.sn_resolution,
            initial_sn_tx: config.initial_sn_tx,
            initial_sn_rx: config.initial_sn_rx,
            is_shm: config.is_shm,
            is_qos: config.is_qos,
        };
        let a_st = Arc::new(TransportUnicastInner::new(stc));

        // Create a weak reference to the transport transport
        let transport: TransportUnicast = (&a_st).into();
        // Add the transport transport to the list of active transports
        guard.insert(config.peer, a_st);

        log::debug!(
            "New transport opened with {}: whatami {}, sn resolution {}, initial sn tx {:?}, initial sn rx {:?}, shm: {}, qos: {}",
            config.peer,
            config.whatami,
            config.sn_resolution,
            config.initial_sn_tx,
            config.initial_sn_rx,
            config.is_shm,
            config.is_qos
        );

        Ok(transport)
    }

    pub async fn open_transport_unicast(
        &self,
        mut endpoint: EndPoint,
    ) -> ZResult<TransportUnicast> {
        if endpoint.locator.address.is_multicast() {
            return zerror!(ZErrorKind::InvalidLocator {
                descr: format!(
                    "Can not open a unicast transport with a multicast endpoint: {}.",
                    endpoint
                )
            });
        }

        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self.new_link_manager_unicast(&endpoint.locator.address.get_proto())?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self
            .config
            .endpoint
            .get(&endpoint.locator.address.get_proto())
        {
            let config = match endpoint.config.as_ref() {
                Some(ec) => {
                    let mut config = config.clone();
                    for (k, v) in ec.iter() {
                        config.insert(k.clone(), v.clone());
                    }
                    config
                }
                None => config.clone(),
            };
            endpoint.config = Some(Arc::new(config));
        };

        // Create a new link associated by calling the Link Manager
        let link = manager.new_link(endpoint).await?;
        // Open the link
        super::establishment::open_link(self, &link).await
    }

    pub fn get_transport_unicast(&self, peer: &PeerId) -> Option<TransportUnicast> {
        zlock!(self.state.unicast.transports)
            .get(peer)
            .map(|t| t.into())
    }

    pub fn get_transports_unicast(&self) -> Vec<TransportUnicast> {
        zlock!(self.state.unicast.transports)
            .values()
            .map(|t| t.into())
            .collect()
    }

    pub(super) async fn del_transport_unicast(&self, peer: &PeerId) -> ZResult<()> {
        let _ = zlock!(self.state.unicast.transports)
            .remove(peer)
            .ok_or_else(|| {
                let e = format!("Can not delete the transport of peer: {}", peer);
                log::trace!("{}", e);
                zerror2!(ZErrorKind::Other { descr: e })
            })?;

        for pa in self.config.unicast.peer_authenticator.iter() {
            pa.handle_close(peer).await;
        }
        Ok(())
    }

    pub(crate) async fn handle_new_link_unicast(&self, link: LinkUnicast) {
        let mut guard = zasynclock!(self.state.unicast.incoming);
        if guard.len() >= self.config.unicast.open_pending {
            // We reached the limit of concurrent incoming transport, this means two things:
            // - the values configured for ZN_OPEN_INCOMING_PENDING and ZN_OPEN_TIMEOUT
            //   are too small for the scenario zenoh is deployed in;
            // - there is a tentative of DoS attack.
            // In both cases, let's close the link straight away with no additional notification
            log::trace!("Closing link for preventing potential DoS: {}", link);
            let _ = link.close().await;
            return;
        }

        // A new link is available
        log::trace!("New link waiting... {}", link);
        guard.insert(link.clone(), None);
        drop(guard);

        let mut peer_id: Option<PeerId> = None;
        for la in self.config.unicast.link_authenticator.iter() {
            let res = la.handle_new_link(&link).await;
            match res {
                Ok(pid) => {
                    // Check that all the peer authenticators, eventually return the same PeerId
                    if let Some(pid1) = peer_id.as_ref() {
                        if let Some(pid2) = pid.as_ref() {
                            if pid1 != pid2 {
                                log::debug!("Ambigous PeerID identification for link: {}", link);
                                let _ = link.close().await;
                                zasynclock!(self.state.unicast.incoming).remove(&link);
                                return;
                            }
                        }
                    } else {
                        peer_id = pid;
                    }
                }
                Err(e) => {
                    log::debug!("{}", e);
                    return;
                }
            }
        }

        // Spawn a task to accept the link
        let c_incoming = self.state.unicast.incoming.clone();
        let c_manager = self.clone();
        task::spawn(async move {
            let auth_link = AuthenticatedPeerLink {
                src: link.get_src(),
                dst: link.get_dst(),
                peer_id,
            };

            let res = super::establishment::accept_link(&c_manager, &link, &auth_link)
                .timeout(c_manager.config.unicast.open_timeout)
                .await;
            match res {
                Ok(res) => {
                    if let Err(e) = res {
                        log::debug!("{}", e);
                    }
                }
                Err(e) => {
                    log::debug!("{}", e);
                    let _ = link.close().await;
                }
            }
            zasynclock!(c_incoming).remove(&link);
        });
    }
}
