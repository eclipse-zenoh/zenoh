//
// Copyright (c) 2022 ZettaScale Technology
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
#[cfg(feature = "shared-memory")]
use crate::unicast::shm::SharedMemoryUnicast;
use crate::{
    unicast::{
        establishment::ext::auth::Auth, transport::TransportUnicastInner, TransportConfigUnicast,
        TransportUnicast,
    },
    TransportManager,
};
use async_std::{
    prelude::FutureExt,
    sync::{Mutex, RwLock},
    task,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
#[cfg(feature = "shared-memory")]
use zenoh_config::SharedMemoryConf;
use zenoh_config::{Config, LinkTxConf, QoSConf, TransportUnicastConf};
use zenoh_core::zasynclock;
use zenoh_link::*;
use zenoh_protocol::{
    core::{endpoint, locator::LocatorProtocol, ZenohId},
    transport::close,
};
use zenoh_result::{bail, zerror, ZResult};

/*************************************/
/*         TRANSPORT CONFIG          */
/*************************************/
pub struct TransportManagerConfigUnicast {
    pub lease: Duration,
    pub keep_alive: usize,
    pub accept_timeout: Duration,
    pub accept_pending: usize,
    pub max_sessions: usize,
    pub max_links: usize,
    pub is_qos: bool,
    #[cfg(feature = "shared-memory")]
    pub is_shm: bool,
}

pub struct TransportManagerStateUnicast {
    // Incoming uninitialized transports
    pub(super) incoming: Arc<Mutex<usize>>,
    // Active authenticators
    pub(super) authenticator: Arc<RwLock<Auth>>,
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<String, LinkManagerUnicast>>>,
    // Established transports
    pub(super) transports: Arc<Mutex<HashMap<ZenohId, Arc<TransportUnicastInner>>>>,
    // Shared memory
    #[cfg(feature = "shared-memory")]
    pub(super) shm: Arc<SharedMemoryUnicast>,
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
    pub(super) keep_alive: usize,
    pub(super) accept_timeout: Duration,
    pub(super) accept_pending: usize,
    pub(super) max_sessions: usize,
    pub(super) max_links: usize,
    pub(super) is_qos: bool,
    #[cfg(feature = "shared-memory")]
    pub(super) is_shm: bool,
    pub(super) authenticator: Auth,
}

impl TransportManagerBuilderUnicast {
    pub fn lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    pub fn keep_alive(mut self, keep_alive: usize) -> Self {
        self.keep_alive = keep_alive;
        self
    }

    pub fn accept_timeout(mut self, accept_timeout: Duration) -> Self {
        self.accept_timeout = accept_timeout;
        self
    }

    pub fn accept_pending(mut self, accept_pending: usize) -> Self {
        self.accept_pending = accept_pending;
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

    pub fn authenticator(mut self, authenticator: Auth) -> Self {
        self.authenticator = authenticator;
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

    pub async fn from_config(mut self, config: &Config) -> ZResult<TransportManagerBuilderUnicast> {
        self = self.lease(Duration::from_millis(
            config.transport().link().tx().lease().unwrap(),
        ));
        self = self.keep_alive(config.transport().link().tx().keep_alive().unwrap());
        self = self.accept_timeout(Duration::from_millis(
            config.transport().unicast().accept_timeout().unwrap(),
        ));
        self = self.accept_pending(config.transport().unicast().accept_pending().unwrap());
        self = self.max_sessions(config.transport().unicast().max_sessions().unwrap());
        self = self.max_links(config.transport().unicast().max_links().unwrap());
        self = self.qos(*config.transport().qos().enabled());

        #[cfg(feature = "shared-memory")]
        {
            self = self.shm(*config.transport().shared_memory().enabled());
        }
        self = self.authenticator(Auth::from_config(config).await?);

        Ok(self)
    }

    pub fn build(
        #[allow(unused_mut)] // auth_pubkey and shared-memory features require mut
        mut self,
    ) -> ZResult<TransportManagerParamsUnicast> {
        let config = TransportManagerConfigUnicast {
            lease: self.lease,
            keep_alive: self.keep_alive,
            accept_timeout: self.accept_timeout,
            accept_pending: self.accept_pending,
            max_sessions: self.max_sessions,
            max_links: self.max_links,
            is_qos: self.is_qos,
            #[cfg(feature = "shared-memory")]
            is_shm: self.is_shm,
        };

        // Enable pubkey authentication by default to avoid ZenohId spoofing
        // #[cfg(feature = "auth_pubkey")]
        // if !self
        //     .peer_authenticator
        //     .iter()
        //     .any(|a| a.id() == PeerAuthenticatorId::PublicKey)
        // {
        //     self.peer_authenticator
        //         .insert(PubKeyAuthenticator::make()?.into());
        // } @TODO

        let state = TransportManagerStateUnicast {
            incoming: Arc::new(Mutex::new(0)),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
            authenticator: Arc::new(RwLock::new(self.authenticator)),
            #[cfg(feature = "shared-memory")]
            shm: Arc::new(SharedMemoryUnicast::make()?),
        };

        let params = TransportManagerParamsUnicast { config, state };

        Ok(params)
    }
}

impl Default for TransportManagerBuilderUnicast {
    fn default() -> Self {
        let link_tx = LinkTxConf::default();
        let transport = TransportUnicastConf::default();
        let qos = QoSConf::default();
        #[cfg(feature = "shared-memory")]
        let shm = SharedMemoryConf::default();

        Self {
            lease: Duration::from_millis(link_tx.lease().unwrap()),
            keep_alive: link_tx.keep_alive().unwrap(),
            accept_timeout: Duration::from_millis(transport.accept_timeout().unwrap()),
            accept_pending: transport.accept_pending().unwrap(),
            max_sessions: transport.max_sessions().unwrap(),
            max_links: transport.max_links().unwrap(),
            is_qos: *qos.enabled(),
            #[cfg(feature = "shared-memory")]
            is_shm: *shm.enabled(),
            authenticator: Auth::default(),
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

        // let mut pa_guard = zasyncwrite!(self.state.unicast.peer_authenticator);

        // for pa in pa_guard.drain() {
        //     pa.close().await;
        // } @TODO

        let mut pl_guard = zasynclock!(self.state.unicast.protocols)
            .drain()
            .map(|(_, v)| v)
            .collect::<Vec<Arc<dyn LinkManagerUnicastTrait>>>();

        for pl in pl_guard.drain(..) {
            for ep in pl.get_listeners().iter() {
                let _ = pl.del_listener(ep).await;
            }
        }

        let mut tu_guard = zasynclock!(self.state.unicast.transports)
            .drain()
            .map(|(_, v)| v)
            .collect::<Vec<Arc<TransportUnicastInner>>>();
        for tu in tu_guard.drain(..) {
            let _ = tu.close(close::reason::GENERIC).await;
        }
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    async fn new_link_manager_unicast(&self, protocol: &str) -> ZResult<LinkManagerUnicast> {
        let mut w_guard = zasynclock!(self.state.unicast.protocols);
        if let Some(lm) = w_guard.get(protocol) {
            Ok(lm.clone())
        } else {
            let lm =
                LinkManagerBuilderUnicast::make(self.new_unicast_link_sender.clone(), protocol)?;
            w_guard.insert(protocol.to_owned(), lm.clone());
            Ok(lm)
        }
    }

    async fn get_link_manager_unicast(
        &self,
        protocol: &LocatorProtocol,
    ) -> ZResult<LinkManagerUnicast> {
        match zasynclock!(self.state.unicast.protocols).get(protocol) {
            Some(manager) => Ok(manager.clone()),
            None => bail!(
                "Can not get the link manager for protocol ({}) because it has not been found",
                protocol
            ),
        }
    }

    async fn del_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zasynclock!(self.state.unicast.protocols).remove(protocol) {
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
        let manager = self
            .new_link_manager_unicast(endpoint.protocol().as_str())
            .await?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self.config.endpoints.get(endpoint.protocol().as_str()) {
            endpoint
                .config_mut()
                .extend(endpoint::Parameters::iter(config))?;
        };
        manager.new_listener(endpoint).await
    }

    pub async fn del_listener_unicast(&self, endpoint: &EndPoint) -> ZResult<()> {
        let lm = self
            .get_link_manager_unicast(endpoint.protocol().as_str())
            .await?;
        lm.del_listener(endpoint).await?;
        if lm.get_listeners().is_empty() {
            self.del_link_manager_unicast(endpoint.protocol().as_str())
                .await?;
        }
        Ok(())
    }

    pub async fn get_listeners_unicast(&self) -> Vec<EndPoint> {
        let mut vec: Vec<EndPoint> = vec![];
        for p in zasynclock!(self.state.unicast.protocols).values() {
            vec.extend_from_slice(&p.get_listeners());
        }
        vec
    }

    pub async fn get_locators_unicast(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zasynclock!(self.state.unicast.protocols).values() {
            vec.extend_from_slice(&p.get_locators());
        }
        vec
    }

    /*************************************/
    /*             TRANSPORT             */
    /*************************************/
    pub(super) async fn init_transport_unicast(
        &self,
        config: TransportConfigUnicast,
    ) -> ZResult<TransportUnicast> {
        let mut guard = zasynclock!(self.state.unicast.transports);

        // First verify if the transport already exists
        match guard.get(&config.zid) {
            Some(transport) => {
                // If it exists, verify that fundamental parameters like are correct.
                // Ignore the non fundamental parameters like initial SN.
                if transport.config != config {
                    let e = zerror!(
                        "Transport with peer {} already exist. Invalid config: {:?}. Expected: {:?}.",
                        config.zid,
                        config,
                        transport.config
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
                        config.zid
                    );
                    log::trace!("{}", e);
                    return Err(e.into());
                }

                // Create the transport
                let stc = TransportConfigUnicast {
                    zid: config.zid,
                    whatami: config.whatami,
                    sn_resolution: config.sn_resolution,
                    tx_initial_sn: config.tx_initial_sn,
                    is_shm: config.is_shm,
                    is_qos: config.is_qos,
                };
                let a_t = Arc::new(TransportUnicastInner::make(self.clone(), stc)?);

                // Add the transport transport to the list of active transports
                let transport: TransportUnicast = (&a_t).into();
                guard.insert(config.zid, a_t);

                log::debug!(
                    "New transport opened between {} and {}: whatami {}, sn resolution {:?}, initial sn {:?}, shm: {}, qos: {}",
                    self.config.zid,
                    config.zid,
                    config.whatami,
                    config.sn_resolution,
                    config.tx_initial_sn,
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
        if self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            bail!(
                "Can not open a unicast transport with a multicast endpoint: {}.",
                endpoint
            )
        }

        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self
            .new_link_manager_unicast(endpoint.protocol().as_str())
            .await?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self.config.endpoints.get(endpoint.protocol().as_str()) {
            endpoint
                .config_mut()
                .extend(endpoint::Parameters::iter(config))?;
        };

        // Create a new link associated by calling the Link Manager
        let link = manager.new_link(endpoint).await?;
        // Open the link
        super::establishment::open::open_link(&link, self).await
    }

    pub async fn get_transport_unicast(&self, peer: &ZenohId) -> Option<TransportUnicast> {
        zasynclock!(self.state.unicast.transports)
            .get(peer)
            .map(|t| t.into())
    }

    pub async fn get_transports_unicast(&self) -> Vec<TransportUnicast> {
        zasynclock!(self.state.unicast.transports)
            .values()
            .map(|t| t.into())
            .collect()
    }

    pub(super) async fn del_transport_unicast(&self, peer: &ZenohId) -> ZResult<()> {
        let _ = zasynclock!(self.state.unicast.transports)
            .remove(peer)
            .ok_or_else(|| {
                let e = zerror!("Can not delete the transport of peer: {}", peer);
                log::trace!("{}", e);
                e
            })?;

        // for pa in zasyncread!(self.state.unicast.peer_authenticator).iter() {
        //     pa.handle_close(peer).await;
        // } @TODO

        Ok(())
    }

    pub(crate) async fn handle_new_link_unicast(&self, link: LinkUnicast) {
        let mut guard = zasynclock!(self.state.unicast.incoming);
        if *guard >= self.config.unicast.accept_pending {
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

        // Spawn a task to accept the link
        let c_manager = self.clone();
        task::spawn(async move {
            if let Err(e) = super::establishment::accept::accept_link(&link, &c_manager)
                .timeout(c_manager.config.unicast.accept_timeout)
                .await
            {
                log::debug!("{}", e);
                let _ = link.close().await;
            }
            let mut guard = zasynclock!(c_manager.state.unicast.incoming);
            *guard -= 1;
        });
    }
}

#[cfg(feature = "test")]
impl TransportManager {
    pub fn get_auth_handle_unicast(&self) -> Arc<RwLock<Auth>> {
        self.state.unicast.authenticator.clone()
    }
}
