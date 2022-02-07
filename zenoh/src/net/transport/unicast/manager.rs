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
use super::establishment::authenticator::*;
use super::protocol::core::ZenohId;
use super::transport::{TransportUnicastConfig, TransportUnicastInner};
use super::*;
use crate::config::Config;
use crate::net::link::*;
use crate::net::protocol::message::CloseReason;
use async_std::prelude::*;
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex, RwLock as AsyncRwLock};
use async_std::task;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::properties::config::*;
use zenoh_util::{zasynclock, zerror, zlock, zparse};

/*************************************/
/*         TRANSPORT CONFIG          */
/*************************************/
pub struct TransportManagerConfigUnicast {
    pub lease: Duration,
    pub keep_alive: Duration,
    pub open_timeout: Duration,
    pub open_pending: usize,
    pub max_sessions: usize,
    pub max_links: usize,
    pub is_qos: bool,
    #[cfg(feature = "shared-memory")]
    pub is_shm: bool,
}

pub struct TransportManagerStateUnicast {
    // Incoming uninitialized transports
    pub(super) incoming: AsyncArc<AsyncMutex<usize>>,
    // Active peer authenticators
    pub(super) peer_authenticator: AsyncArc<AsyncRwLock<HashSet<PeerAuthenticator>>>,
    // Active link authenticators
    pub(super) link_authenticator: AsyncArc<AsyncRwLock<HashSet<LinkAuthenticator>>>,
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManagerUnicast>>>,
    // Established transports
    pub(super) transports: Arc<Mutex<HashMap<ZenohId, Arc<TransportUnicastInner>>>>,
}

pub struct TransportManagerParamsUnicast {
    pub config: TransportManagerConfigUnicast,
    pub state: TransportManagerStateUnicast,
}

pub struct TransportManagerBuilderUnicast {
    // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
    //       set the actual keep_alive timeout to one fourth of the lease time.
    //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
    //       check which considers a link as failed when no messages are received in 3.5 times the
    //       target interval.
    pub(super) lease: Duration,
    pub(super) keep_alive: Duration,
    pub(super) open_timeout: Duration,
    pub(super) open_pending: usize,
    pub(super) max_sessions: usize,
    pub(super) max_links: usize,
    pub(super) is_qos: bool,
    #[cfg(feature = "shared-memory")]
    pub(super) is_shm: bool,
    pub(super) peer_authenticator: HashSet<PeerAuthenticator>,
    pub(super) link_authenticator: HashSet<LinkAuthenticator>,
}

impl TransportManagerBuilderUnicast {
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

    #[cfg(feature = "shared-memory")]
    pub fn shm(mut self, is_shm: bool) -> Self {
        self.is_shm = is_shm;
        self
    }

    pub async fn from_config(
        mut self,
        properties: &Config,
    ) -> ZResult<TransportManagerBuilderUnicast> {
        if let Some(v) = properties.transport().link().lease() {
            self = self.lease(Duration::from_millis(*v));
        }
        if let Some(v) = properties.transport().link().keep_alive() {
            self = self.keep_alive(Duration::from_millis(*v));
        }
        if let Some(v) = properties.transport().unicast().open_timeout() {
            self = self.open_timeout(Duration::from_millis(*v));
        }
        if let Some(v) = properties.transport().unicast().open_pending() {
            self = self.open_pending(*v);
        }
        if let Some(v) = properties.transport().unicast().max_sessions() {
            self = self.max_sessions(*v);
        }
        if let Some(v) = properties.transport().unicast().max_links() {
            self = self.max_links(*v);
        }
        if let Some(v) = properties.transport().qos() {
            self = self.qos(*v);
        }
        #[cfg(feature = "shared-memory")]
        if let Some(v) = properties.shared_memory() {
            self = self.shm(*v);
        }
        self = self.peer_authenticator(PeerAuthenticator::from_config(properties).await?);
        self = self.link_authenticator(LinkAuthenticator::from_config(properties).await?);

        Ok(self)
    }

    pub fn build(
        #[allow(unused_mut)] // auth_pubkey and shared-memory features require mut
        mut self,
    ) -> ZResult<TransportManagerParamsUnicast> {
        let config = TransportManagerConfigUnicast {
            lease: self.lease,
            keep_alive: self.keep_alive,
            open_timeout: self.open_timeout,
            open_pending: self.open_pending,
            max_sessions: self.max_sessions,
            max_links: self.max_links,
            is_qos: self.is_qos,
            #[cfg(feature = "shared-memory")]
            is_shm: self.is_shm,
        };

        // Enable pubkey authentication by default to avoid PeerId spoofing
        #[cfg(feature = "auth_pubkey")]
        if !self
            .peer_authenticator
            .iter()
            .any(|a| a.id() == PeerAuthenticatorId::PublicKey)
        {
            self.peer_authenticator
                .insert(PubKeyAuthenticator::make()?.into());
        }

        #[cfg(feature = "shared-memory")]
        if self.is_shm
            && !self
                .peer_authenticator
                .iter()
                .any(|a| a.id() == PeerAuthenticatorId::Shm)
        {
            self.peer_authenticator
                .insert(SharedMemoryAuthenticator::make()?.into());
        }

        let state = TransportManagerStateUnicast {
            incoming: AsyncArc::new(AsyncMutex::new(0)),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
            link_authenticator: AsyncArc::new(AsyncRwLock::new(self.link_authenticator)),
            peer_authenticator: AsyncArc::new(AsyncRwLock::new(self.peer_authenticator)),
        };

        let params = TransportManagerParamsUnicast { config, state };

        Ok(params)
    }
}

impl Default for TransportManagerBuilderUnicast {
    fn default() -> Self {
        Self {
            lease: Duration::from_millis(zparse!(ZN_LINK_LEASE_DEFAULT).unwrap()),
            keep_alive: Duration::from_millis(zparse!(ZN_LINK_KEEP_ALIVE_DEFAULT).unwrap()),
            open_timeout: Duration::from_millis(zparse!(ZN_OPEN_TIMEOUT_DEFAULT).unwrap()),
            open_pending: zparse!(ZN_OPEN_INCOMING_PENDING_DEFAULT).unwrap(),
            max_sessions: zparse!(ZN_MAX_SESSIONS_UNICAST_DEFAULT).unwrap(),
            max_links: zparse!(ZN_MAX_LINKS_DEFAULT).unwrap(),
            is_qos: zparse!(ZN_QOS_DEFAULT).unwrap(),
            #[cfg(feature = "shared-memory")]
            is_shm: zparse!(ZN_SHM_DEFAULT).unwrap(),
            peer_authenticator: HashSet::new(),
            link_authenticator: HashSet::new(),
        }
    }
}

/*************************************/
/*         TRANSPORT MANAGER         */
/*************************************/
impl TransportManager {
    pub fn config_unicast() -> TransportManagerBuilderUnicast {
        TransportManagerBuilderUnicast::default()
    }

    pub async fn close_unicast(&self) {
        log::trace!("TransportManagerUnicast::clear())");

        let mut la_guard = zasyncwrite!(self.state.unicast.link_authenticator);
        let mut pa_guard = zasyncwrite!(self.state.unicast.peer_authenticator);

        for la in la_guard.drain() {
            let _ = la.close().await;
        }

        for pa in pa_guard.drain() {
            let _ = pa.close().await;
        }

        let mut pl_guard = zlock!(self.state.unicast.protocols)
            .drain()
            .map(|(_, v)| v)
            .collect::<Vec<Arc<dyn LinkManagerUnicastTrait>>>();

        for pl in pl_guard.drain(..) {
            for ep in pl.get_listeners().iter() {
                let _ = pl.del_listener(ep).await;
            }
        }

        let mut tu_guard = zlock!(self.state.unicast.transports)
            .drain()
            .map(|(_, v)| v)
            .collect::<Vec<Arc<TransportUnicastInner>>>();
        for tu in tu_guard.drain(..) {
            let _ = tu.close(CloseReason::Generic).await;
        }
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    fn new_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<LinkManagerUnicast> {
        let mut w_guard = zlock!(self.state.unicast.protocols);
        if let Some(lm) = w_guard.get(protocol) {
            Ok(lm.clone())
        } else {
            let lm = LinkManagerBuilderUnicast::make(self.clone(), protocol)?;
            w_guard.insert(protocol.clone(), lm.clone());
            Ok(lm)
        }
    }

    fn get_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<LinkManagerUnicast> {
        match zlock!(self.state.unicast.protocols).get(protocol) {
            Some(manager) => Ok(manager.clone()),
            None => bail!(
                "Can not get the link manager for protocol ({}) because it has not been found",
                protocol
            ),
        }
    }

    fn del_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zlock!(self.state.unicast.protocols).remove(protocol) {
            Some(_) => Ok(()),
            None => bail!(
                "Can not delete the link manager for protocol ({}) because it has not been found.",
                protocol
            ),
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
        match guard.get(&config.peer) {
            Some(transport) => {
                // If it exists, verify that fundamental parameters like are correct.
                // Ignore the non fundamental parameters like initial SN.
                if transport.config.whatami != config.whatami {
                    let e = zerror!(
                        "Transport with peer {} already exist. Invalid whatami: {}. Execpted: {}.",
                        config.peer,
                        config.whatami,
                        transport.config.whatami
                    );
                    log::trace!("{}", e);
                    return Err(e.into());
                }

                if transport.config.sn_bytes != config.sn_bytes {
                    let e = zerror!(
                    "Transport with peer {} already exist. Invalid sn resolution: {}. Execpted: {}.",
                    config.peer, config.sn_bytes, transport.config.sn_bytes
                );
                    log::trace!("{}", e);
                    return Err(e.into());
                }

                #[cfg(feature = "shared-memory")]
                if transport.config.is_shm != config.is_shm {
                    let e = zerror!(
                        "Transport with peer {} already exist. Invalid is_shm: {}. Execpted: {}.",
                        config.peer,
                        config.is_shm,
                        transport.config.is_shm
                    );
                    log::trace!("{}", e);
                    return Err(e.into());
                }

                if transport.config.is_qos != config.is_qos {
                    let e = zerror!(
                        "Transport with peer {} already exist. Invalid is_qos: {}. Execpted: {}.",
                        config.peer,
                        config.is_qos,
                        transport.config.is_qos
                    );
                    log::trace!("{}", e);
                    return Err(e.into());
                }

                Ok(transport.into())
            }
            None => {
                // Then verify that we haven't reached the transport number limit
                if guard.len() >= self.config.unicast.max_sessions {
                    let e = zerror!(
                        "Max transports reached ({}). Denying new transport with peer: {}",
                        self.config.unicast.max_sessions,
                        config.peer
                    );
                    log::trace!("{}", e);
                    return Err(e.into());
                }

                // Create the transport
                let stc = TransportUnicastConfig {
                    manager: self.clone(),
                    zid: config.peer,
                    whatami: config.whatami,
                    sn_bytes: config.sn_bytes,
                    initial_sn_tx: config.initial_sn_tx,
                    is_shm: config.is_shm,
                    is_qos: config.is_qos,
                };
                let a_t = Arc::new(TransportUnicastInner::make(stc)?);

                // Add the transport transport to the list of active transports
                let transport: TransportUnicast = (&a_t).into();
                guard.insert(config.peer, a_t);

                log::debug!(
                    "New transport opened with {}: whatami {}, sn resolution {}, initial sn {:?}, shm: {}, qos: {}",
                    config.peer,
                    config.whatami,
                    config.sn_bytes,
                    config.initial_sn_tx,
                    config.is_shm,
                    config.is_qos
                );

                Ok(transport)
            }
        }
    }

    pub async fn open_transport_unicast(
        &self,
        mut endpoint: EndPoint,
    ) -> ZResult<TransportUnicast> {
        if endpoint.locator.address.is_multicast() {
            bail!(
                "Can not open a unicast transport with a multicast endpoint: {}.",
                endpoint
            )
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
        let mut auth_link = AuthenticatedPeerLink {
            src: link.get_src(),
            dst: link.get_src(),
            zid: None,
        };
        super::establishment::open::open_link(&link, self, &mut auth_link).await
    }

    pub fn get_transport_unicast(&self, peer: &ZenohId) -> Option<TransportUnicast> {
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

    pub(super) async fn del_transport_unicast(&self, peer: &ZenohId) -> ZResult<()> {
        let _ = zlock!(self.state.unicast.transports)
            .remove(peer)
            .ok_or_else(|| {
                let e = zerror!("Can not delete the transport of peer: {}", peer);
                log::trace!("{}", e);
                e
            })?;

        for pa in zasyncread!(self.state.unicast.peer_authenticator).iter() {
            pa.handle_close(peer).await;
        }

        Ok(())
    }

    pub(crate) async fn handle_new_link_unicast(&self, link: LinkUnicast) {
        let mut guard = zasynclock!(self.state.unicast.incoming);
        if *guard >= self.config.unicast.open_pending {
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
        *guard += 1;
        drop(guard);

        let mut zid: Option<ZenohId> = None;
        let peer_link = Link::from(&link);
        for la in zasyncread!(self.state.unicast.link_authenticator).iter() {
            let res = la.handle_new_link(&peer_link).await;
            match res {
                Ok(z) => {
                    // Check that all the peer authenticators, eventually return the same ZenohId
                    if let Some(zid1) = z.as_ref() {
                        if let Some(zid2) = z.as_ref() {
                            if zid1 != zid2 {
                                log::debug!("Ambigous ZenohId identification for link: {}", link);
                                let _ = link.close().await;
                                let mut guard = zasynclock!(self.state.unicast.incoming);
                                *guard -= 1;
                                return;
                            }
                        }
                    } else {
                        zid = z;
                    }
                }
                Err(e) => {
                    log::debug!("{}", e);
                    let mut guard = zasynclock!(self.state.unicast.incoming);
                    *guard -= 1;
                    return;
                }
            }
        }

        // Spawn a task to accept the link
        let c_manager = self.clone();
        task::spawn(async move {
            let mut auth_link = AuthenticatedPeerLink {
                src: link.get_src(),
                dst: link.get_dst(),
                zid,
            };

            let res = super::establishment::accept::accept_link(&link, &c_manager, &mut auth_link)
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
            let mut guard = zasynclock!(c_manager.state.unicast.incoming);
            *guard -= 1;
        });
    }
}
