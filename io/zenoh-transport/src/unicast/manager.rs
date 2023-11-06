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
#[cfg(feature = "shared-memory")]
use super::shared_memory_unicast::SharedMemoryUnicast;
#[cfg(feature = "transport_auth")]
use crate::unicast::establishment::ext::auth::Auth;
#[cfg(feature = "transport_multilink")]
use crate::unicast::establishment::ext::multilink::MultiLink;
use crate::{
    lowlatency::transport::TransportUnicastLowlatency,
    transport_unicast_inner::TransportUnicastTrait,
    unicast::{TransportConfigUnicast, TransportUnicast},
    universal::transport::TransportUnicastUniversal,
    TransportManager,
};
use async_std::{prelude::FutureExt, sync::Mutex, task};
use std::{collections::HashMap, sync::Arc, time::Duration};
#[cfg(feature = "shared-memory")]
use zenoh_config::SharedMemoryConf;
use zenoh_config::{Config, LinkTxConf, QoSConf, TransportUnicastConf};
use zenoh_core::{zasynclock, zcondfeat};
use zenoh_crypto::PseudoRng;
use zenoh_link::*;
use zenoh_protocol::{
    core::{endpoint, ZenohId},
    transport::close,
};
use zenoh_result::{bail, zerror, Error, ZResult};

/*************************************/
/*         TRANSPORT CONFIG          */
/*************************************/
pub struct TransportManagerConfigUnicast {
    pub lease: Duration,
    pub keep_alive: usize,
    pub accept_timeout: Duration,
    pub accept_pending: usize,
    pub max_sessions: usize,
    pub is_qos: bool,
    pub is_lowlatency: bool,
    #[cfg(feature = "transport_multilink")]
    pub max_links: usize,
    #[cfg(feature = "shared-memory")]
    pub is_shm: bool,
    #[cfg(all(feature = "unstable", feature = "transport_compression"))]
    pub is_compressed: bool,
}

pub struct TransportManagerStateUnicast {
    // Incoming uninitialized transports
    pub(super) incoming: Arc<Mutex<usize>>,
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<String, LinkManagerUnicast>>>,
    // Established transports
    pub(super) transports: Arc<Mutex<HashMap<ZenohId, Arc<dyn TransportUnicastTrait>>>>,
    // Multilink
    #[cfg(feature = "transport_multilink")]
    pub(super) multilink: Arc<MultiLink>,
    // Active authenticators
    #[cfg(feature = "transport_auth")]
    pub(super) authenticator: Arc<Auth>,
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
    pub(super) is_qos: bool,
    #[cfg(feature = "transport_multilink")]
    pub(super) max_links: usize,
    #[cfg(feature = "shared-memory")]
    pub(super) is_shm: bool,
    #[cfg(feature = "transport_compression")]
    pub(super) is_compressed: bool,
    #[cfg(feature = "transport_auth")]
    pub(super) authenticator: Auth,
    pub(super) is_lowlatency: bool,
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

    pub fn qos(mut self, is_qos: bool) -> Self {
        self.is_qos = is_qos;
        self
    }

    pub fn lowlatency(mut self, is_lowlatency: bool) -> Self {
        self.is_lowlatency = is_lowlatency;
        self
    }

    #[cfg(feature = "transport_multilink")]
    pub fn max_links(mut self, max_links: usize) -> Self {
        self.max_links = max_links;
        self
    }

    #[cfg(feature = "transport_auth")]
    pub fn authenticator(mut self, authenticator: Auth) -> Self {
        self.authenticator = authenticator;
        self
    }

    #[cfg(feature = "shared-memory")]
    pub fn shm(mut self, is_shm: bool) -> Self {
        self.is_shm = is_shm;
        self
    }

    #[cfg(all(feature = "unstable", feature = "transport_compression"))]
    pub fn compression(mut self, is_compressed: bool) -> Self {
        self.is_compressed = is_compressed;
        self
    }

    pub async fn from_config(mut self, config: &Config) -> ZResult<TransportManagerBuilderUnicast> {
        self = self.lease(Duration::from_millis(
            *config.transport().link().tx().lease(),
        ));
        self = self.keep_alive(*config.transport().link().tx().keep_alive());
        self = self.accept_timeout(Duration::from_millis(
            *config.transport().unicast().accept_timeout(),
        ));
        self = self.accept_pending(*config.transport().unicast().accept_pending());
        self = self.max_sessions(*config.transport().unicast().max_sessions());
        self = self.qos(*config.transport().qos().enabled());
        self = self.lowlatency(*config.transport().unicast().lowlatency());

        #[cfg(feature = "transport_multilink")]
        {
            self = self.max_links(*config.transport().unicast().max_links());
        }
        #[cfg(feature = "shared-memory")]
        {
            self = self.shm(*config.transport().shared_memory().enabled());
        }
        #[cfg(feature = "transport_auth")]
        {
            self = self.authenticator(Auth::from_config(config).await?);
        }

        Ok(self)
    }

    pub fn build(
        self,
        #[allow(unused)] prng: &mut PseudoRng, // Required for #[cfg(feature = "transport_multilink")]
    ) -> ZResult<TransportManagerParamsUnicast> {
        if self.is_qos && self.is_lowlatency {
            bail!("'qos' and 'lowlatency' options are incompatible");
        }

        let config = TransportManagerConfigUnicast {
            lease: self.lease,
            keep_alive: self.keep_alive,
            accept_timeout: self.accept_timeout,
            accept_pending: self.accept_pending,
            max_sessions: self.max_sessions,
            is_qos: self.is_qos,
            #[cfg(feature = "transport_multilink")]
            max_links: self.max_links,
            #[cfg(feature = "shared-memory")]
            is_shm: self.is_shm,
            #[cfg(all(feature = "unstable", feature = "transport_compression"))]
            is_compressed: self.is_compressed,
            is_lowlatency: self.is_lowlatency,
        };

        let state = TransportManagerStateUnicast {
            incoming: Arc::new(Mutex::new(0)),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            transports: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "transport_multilink")]
            multilink: Arc::new(MultiLink::make(prng)?),
            #[cfg(feature = "shared-memory")]
            shm: Arc::new(SharedMemoryUnicast::make()?),
            #[cfg(feature = "transport_auth")]
            authenticator: Arc::new(self.authenticator),
        };

        let params = TransportManagerParamsUnicast { config, state };

        Ok(params)
    }
}

impl Default for TransportManagerBuilderUnicast {
    fn default() -> Self {
        let transport = TransportUnicastConf::default();
        let link_tx = LinkTxConf::default();
        let qos = QoSConf::default();
        #[cfg(feature = "shared-memory")]
        let shm = SharedMemoryConf::default();

        Self {
            lease: Duration::from_millis(*link_tx.lease()),
            keep_alive: *link_tx.keep_alive(),
            accept_timeout: Duration::from_millis(*transport.accept_timeout()),
            accept_pending: *transport.accept_pending(),
            max_sessions: *transport.max_sessions(),
            is_qos: *qos.enabled(),
            #[cfg(feature = "transport_multilink")]
            max_links: *transport.max_links(),
            #[cfg(feature = "shared-memory")]
            is_shm: *shm.enabled(),
            #[cfg(feature = "transport_compression")]
            is_compressed: false,
            #[cfg(feature = "transport_auth")]
            authenticator: Auth::default(),
            is_lowlatency: *transport.lowlatency(),
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

    #[cfg(feature = "shared-memory")]
    pub(crate) fn shm(&self) -> &Arc<SharedMemoryUnicast> {
        &self.state.unicast.shm
    }

    pub async fn close_unicast(&self) {
        log::trace!("TransportManagerUnicast::clear())");

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
            .collect::<Vec<Arc<dyn TransportUnicastTrait>>>();
        for tu in tu_guard.drain(..) {
            let _ = tu.close(close::reason::GENERIC).await;
        }
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    async fn new_link_manager_unicast(&self, protocol: &str) -> ZResult<LinkManagerUnicast> {
        if !self.config.protocols.iter().any(|x| x.as_str() == protocol) {
            bail!(
                "Unsupported protocol: {}. Supported protocols are: {:?}",
                protocol,
                self.config.protocols
            );
        }

        let mut w_guard = zasynclock!(self.state.unicast.protocols);
        if let Some(lm) = w_guard.get(protocol) {
            Ok(lm.clone())
        } else {
            let lm =
                LinkManagerBuilderUnicast::make(self.new_unicast_link_sender.clone(), protocol)?;
            w_guard.insert(protocol.to_string(), lm.clone());
            Ok(lm)
        }
    }

    async fn get_link_manager_unicast(&self, protocol: &str) -> ZResult<LinkManagerUnicast> {
        match zasynclock!(self.state.unicast.protocols).get(protocol) {
            Some(manager) => Ok(manager.clone()),
            None => bail!(
                "Can not get the link manager for protocol ({}) because it has not been found",
                protocol
            ),
        }
    }

    async fn del_link_manager_unicast(&self, protocol: &str) -> ZResult<()> {
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
        if self
            .locator_inspector
            .is_multicast(&endpoint.to_locator())
            .await?
        {
            bail!(
                "Can not listen on unicast endpoint with a multicast endpoint: {}.",
                endpoint
            )
        }

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
        link: LinkUnicast,
        direction: LinkUnicastDirection,
    ) -> Result<TransportUnicast, (Error, Option<u8>)> {
        let mut guard = zasynclock!(self.state.unicast.transports);

        // First verify if the transport already exists
        match guard.get(&config.zid) {
            Some(transport) => {
                let existing_config = transport.get_config();
                // If it exists, verify that fundamental parameters like are correct.
                // Ignore the non fundamental parameters like initial SN.
                if *existing_config != config {
                    let e = zerror!(
                        "Transport with peer {} already exist. Invalid config: {:?}. Expected: {:?}.",
                        config.zid,
                        config,
                        existing_config
                    );
                    log::trace!("{}", e);
                    return Err((e.into(), Some(close::reason::INVALID)));
                }

                // Add the link to the transport
                transport
                    .add_link(link, direction)
                    .await
                    .map_err(|e| (e, Some(close::reason::MAX_LINKS)))?;

                Ok(TransportUnicast(Arc::downgrade(transport)))
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
                    return Err((e.into(), Some(close::reason::INVALID)));
                }

                // Create the transport
                let is_multilink =
                    zcondfeat!("transport_multilink", config.multilink.is_some(), false);

                // select and create transport implementation depending on the cfg and enabled features
                let a_t = {
                    if config.is_lowlatency {
                        log::debug!("Will use LowLatency transport!");
                        TransportUnicastLowlatency::make(self.clone(), config.clone(), link)
                            .map_err(|e| (e, Some(close::reason::INVALID)))
                            .map(|v| Arc::new(v) as Arc<dyn TransportUnicastTrait>)?
                    } else {
                        log::debug!("Will use Universal transport!");
                        let t: Arc<dyn TransportUnicastTrait> =
                            TransportUnicastUniversal::make(self.clone(), config.clone())
                                .map_err(|e| (e, Some(close::reason::INVALID)))
                                .map(|v| Arc::new(v) as Arc<dyn TransportUnicastTrait>)?;
                        // Add the link to the transport
                        t.add_link(link, direction)
                            .await
                            .map_err(|e| (e, Some(close::reason::MAX_LINKS)))?;
                        t
                    }
                };

                // Add the transport transport to the list of active transports
                let transport = TransportUnicast(Arc::downgrade(&a_t));
                guard.insert(config.zid, a_t);

                zcondfeat!(
                    "shared-memory",
                    {
                        log::debug!(
                            "New transport opened between {} and {} - whatami: {}, sn resolution: {:?}, initial sn: {:?}, qos: {}, shm: {}, multilink: {}, lowlatency: {}",
                            self.config.zid,
                            config.zid,
                            config.whatami,
                            config.sn_resolution,
                            config.tx_initial_sn,
                            config.is_qos,
                            config.is_shm,
                            is_multilink,
                            config.is_lowlatency
                        );
                    },
                    {
                        log::debug!(
                            "New transport opened between {} and {} - whatami: {}, sn resolution: {:?}, initial sn: {:?}, qos: {}, multilink: {}, lowlatency: {}",
                            self.config.zid,
                            config.zid,
                            config.whatami,
                            config.sn_resolution,
                            config.tx_initial_sn,
                            config.is_qos,
                            is_multilink,
                            config.is_lowlatency
                        );
                    }
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
            .map(|t| {
                // todo: I cannot find a way to make transport.into() work for TransportUnicastTrait
                let weak = Arc::downgrade(t);
                TransportUnicast(weak)
            })
    }

    pub async fn get_transports_unicast(&self) -> Vec<TransportUnicast> {
        zasynclock!(self.state.unicast.transports)
            .values()
            .map(|t| {
                // todo: I cannot find a way to make transport.into() work for TransportUnicastTrait
                let weak = Arc::downgrade(t);
                TransportUnicast(weak)
            })
            .collect()
    }

    pub(super) async fn del_transport_unicast(&self, peer: &ZenohId) -> ZResult<()> {
        zasynclock!(self.state.unicast.transports)
            .remove(peer)
            .ok_or_else(|| {
                let e = zerror!("Can not delete the transport of peer: {}", peer);
                log::trace!("{}", e);
                e
            })?;
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

#[cfg(all(feature = "test", feature = "transport_auth"))]
impl TransportManager {
    pub fn get_auth_handle_unicast(&self) -> Arc<Auth> {
        self.state.unicast.authenticator.clone()
    }
}
