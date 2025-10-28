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
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
    time::Duration,
};

use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
#[cfg(feature = "transport_compression")]
use zenoh_config::CompressionUnicastConf;
use zenoh_config::{Config, LinkTxConf, QoSUnicastConf, TransportUnicastConf};
use zenoh_core::{zasynclock, zcondfeat};
use zenoh_crypto::PseudoRng;
use zenoh_link::*;
use zenoh_protocol::{
    core::{parameters, Reliability, ZenohIdProto},
    transport::{close, TransportSn},
};
use zenoh_result::{bail, zerror, ZResult};

use super::{link::LinkUnicastWithOpenAck, transport_unicast_inner::InitTransportResult};
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
#[cfg(feature = "transport_auth")]
use crate::unicast::establishment::ext::auth::Auth;
#[cfg(feature = "transport_multilink")]
use crate::unicast::establishment::ext::multilink::MultiLink;
use crate::{
    unicast::{
        lowlatency::transport::TransportUnicastLowlatency,
        transport_unicast_inner::{InitTransportError, TransportUnicastTrait},
        universal::transport::TransportUnicastUniversal,
        TransportConfigUnicast, TransportUnicast,
    },
    TransportManager, TransportPeer,
};

/*************************************/
/*         TRANSPORT CONFIG          */
/*************************************/
pub struct TransportManagerConfigUnicast {
    pub lease: Duration,
    pub keep_alive: usize,
    pub open_timeout: Duration,
    pub accept_timeout: Duration,
    pub accept_pending: usize,
    pub max_sessions: usize,
    pub is_qos: bool,
    pub is_lowlatency: bool,
    #[cfg(feature = "transport_multilink")]
    pub max_links: usize,
    #[cfg(feature = "transport_compression")]
    pub is_compression: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkKey {
    protocol: String,
    reliability: Option<Reliability>,
}

impl LinkKey {
    pub fn with_protocol(protocol: &str) -> Self {
        LinkKey {
            protocol: protocol.to_string(),
            reliability: None,
        }
    }
}

impl From<&EndPoint> for LinkKey {
    fn from(endpoint: &EndPoint) -> Self {
        let protocol = endpoint.protocol().to_string();
        let reliability = Reliability::from(
            LocatorInspector::default()
                .is_reliable(&endpoint.to_locator())
                .expect("endpoint protocol should be valid"),
        );

        Self {
            protocol,
            reliability: Some(reliability),
        }
    }
}

pub struct TransportManagerStateUnicast {
    // Incoming uninitialized transports
    pub(super) incoming: Arc<AtomicUsize>,
    // Established listeners
    pub(super) link_managers: Arc<AsyncMutex<HashMap<LinkKey, LinkManagerUnicast>>>,
    // Established transports
    pub(super) transports: Arc<AsyncMutex<HashMap<ZenohIdProto, Arc<dyn TransportUnicastTrait>>>>,
    // Multilink
    #[cfg(feature = "transport_multilink")]
    pub(super) multilink: Arc<MultiLink>,
    // Active authenticators
    #[cfg(feature = "transport_auth")]
    pub(super) authenticator: Arc<Auth>,
}

pub struct TransportManagerParamsUnicast {
    pub config: TransportManagerConfigUnicast,
    pub state: TransportManagerStateUnicast,
}

pub struct TransportManagerBuilderUnicast {
    // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
    //       set the actual keep_alive timeout to one fourth of the lease time.
    //       This is in-line with the ITU-T G.8013/Y.1731 specification on continuous connectivity
    //       check which considers a link as failed when no messages are received in 3.5 times the
    //       target interval.
    pub(super) lease: Duration,
    pub(super) keep_alive: usize,
    pub(super) open_timeout: Duration,
    pub(super) accept_timeout: Duration,
    pub(super) accept_pending: usize,
    pub(super) max_sessions: usize,
    pub(super) is_qos: bool,
    #[cfg(feature = "transport_multilink")]
    pub(super) max_links: usize,
    #[cfg(feature = "transport_auth")]
    pub(super) authenticator: Auth,
    pub(super) is_lowlatency: bool,
    #[cfg(feature = "transport_compression")]
    pub(super) is_compression: bool,
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

    pub fn open_timeout(mut self, open_timeout: Duration) -> Self {
        self.open_timeout = open_timeout;
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

    #[cfg(feature = "transport_compression")]
    pub fn compression(mut self, is_compression: bool) -> Self {
        self.is_compression = is_compression;
        self
    }

    pub async fn from_config(mut self, config: &Config) -> ZResult<TransportManagerBuilderUnicast> {
        self = self.lease(Duration::from_millis(
            *config.transport().link().tx().lease(),
        ));
        self = self.keep_alive(*config.transport().link().tx().keep_alive());
        self = self.open_timeout(Duration::from_millis(
            *config.transport().unicast().open_timeout(),
        ));
        self = self.accept_timeout(Duration::from_millis(
            *config.transport().unicast().accept_timeout(),
        ));
        self = self.accept_pending(*config.transport().unicast().accept_pending());
        self = self.max_sessions(*config.transport().unicast().max_sessions());
        self = self.qos(*config.transport().unicast().qos().enabled());
        self = self.lowlatency(*config.transport().unicast().lowlatency());

        #[cfg(feature = "transport_multilink")]
        {
            self = self.max_links(*config.transport().unicast().max_links());
        }
        #[cfg(feature = "transport_auth")]
        {
            self = self.authenticator(Auth::from_config(config).await?);
        }
        #[cfg(feature = "transport_compression")]
        {
            self = self.compression(*config.transport().unicast().compression().enabled());
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
            open_timeout: self.open_timeout,
            accept_timeout: self.accept_timeout,
            accept_pending: self.accept_pending,
            max_sessions: self.max_sessions,
            is_qos: self.is_qos,
            #[cfg(feature = "transport_multilink")]
            max_links: self.max_links,
            is_lowlatency: self.is_lowlatency,
            #[cfg(feature = "transport_compression")]
            is_compression: self.is_compression,
        };

        let state = TransportManagerStateUnicast {
            incoming: Arc::new(AtomicUsize::new(0)),
            link_managers: Arc::new(AsyncMutex::new(HashMap::new())),
            transports: Arc::new(AsyncMutex::new(HashMap::new())),
            #[cfg(feature = "transport_multilink")]
            multilink: Arc::new(MultiLink::make(prng, config.max_links > 1)?),
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
        let qos = QoSUnicastConf::default();
        #[cfg(feature = "transport_compression")]
        let compression = CompressionUnicastConf::default();

        Self {
            lease: Duration::from_millis(*link_tx.lease()),
            keep_alive: *link_tx.keep_alive(),
            open_timeout: Duration::from_millis(*transport.open_timeout()),
            accept_timeout: Duration::from_millis(*transport.accept_timeout()),
            accept_pending: *transport.accept_pending(),
            max_sessions: *transport.max_sessions(),
            is_qos: *qos.enabled(),
            #[cfg(feature = "transport_multilink")]
            max_links: *transport.max_links(),
            #[cfg(feature = "transport_auth")]
            authenticator: Auth::default(),
            is_lowlatency: *transport.lowlatency(),
            #[cfg(feature = "transport_compression")]
            is_compression: *compression.enabled(),
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
        tracing::trace!("TransportManagerUnicast::clear())");

        let mut pl_guard = zasynclock!(self.state.unicast.link_managers)
            .drain()
            .map(|(_, v)| v)
            .collect::<Vec<Arc<dyn LinkManagerUnicastTrait>>>();

        for pl in pl_guard.drain(..) {
            for ep in pl.get_listeners().await.iter() {
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
    async fn new_link_manager_unicast(&self, endpoint: &EndPoint) -> ZResult<LinkManagerUnicast> {
        let protocol = endpoint.protocol().as_str();
        let link_kind = LinkKind::try_from(endpoint)?;
        if !self.config.supported_links.contains(&link_kind) {
            bail!(
                "Unsupported protocol: {}. Supported protocols are: {:?}",
                protocol,
                self.config.supported_links
            );
        }

        let mut w_guard = zasynclock!(self.state.unicast.link_managers);
        let key = LinkKey::from(endpoint);
        if let Some(lm) = w_guard.get(&key) {
            Ok(lm.clone())
        } else {
            let lm =
                LinkManagerBuilderUnicast::make(self.new_unicast_link_sender.clone(), endpoint)?;
            w_guard.insert(key, lm.clone());
            Ok(lm)
        }
    }

    async fn get_link_manager_unicast(&self, endpoint: &EndPoint) -> ZResult<LinkManagerUnicast> {
        match zasynclock!(self.state.unicast.link_managers).get(&LinkKey::from(endpoint)) {
            Some(manager) => Ok(manager.clone()),
            None => bail!(
                "Can not get the link manager for protocol ({}) because it has not been found",
                endpoint.protocol()
            ),
        }
    }

    async fn del_link_manager_unicast(&self, endpoint: &EndPoint) -> ZResult<()> {
        match zasynclock!(self.state.unicast.link_managers).remove(&LinkKey::from(endpoint)) {
            Some(_) => Ok(()),
            None => bail!(
                "Can not delete the link manager for protocol ({}) because it has not been found.",
                endpoint.protocol()
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

        let manager = self.new_link_manager_unicast(&endpoint).await?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self
            .config
            .link_configs
            .get(&LinkKind::try_from(&endpoint)?)
        {
            let mut config = parameters::Parameters::from(config.as_str());
            // Overwrite config with current endpoint parameters
            config.extend_from_iter(endpoint.config().iter());
            endpoint = EndPoint::new(
                endpoint.protocol(),
                endpoint.address(),
                endpoint.metadata(),
                config.as_str(),
            )?;
        };
        manager.new_listener(endpoint).await
    }

    pub async fn del_listener_unicast(&self, endpoint: &EndPoint) -> ZResult<()> {
        let lm = self.get_link_manager_unicast(endpoint).await?;
        lm.del_listener(endpoint).await?;
        if lm.get_listeners().await.is_empty() {
            self.del_link_manager_unicast(endpoint).await?;
        }
        Ok(())
    }

    pub async fn get_listeners_unicast(&self) -> Vec<EndPoint> {
        let mut vec: Vec<EndPoint> = vec![];
        for p in zasynclock!(self.state.unicast.link_managers).values() {
            vec.extend_from_slice(&p.get_listeners().await);
        }
        vec
    }

    pub async fn get_locators_unicast(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zasynclock!(self.state.unicast.link_managers).values() {
            vec.extend_from_slice(&p.get_locators().await);
        }
        vec
    }

    /*************************************/
    /*             TRANSPORT             */
    /*************************************/
    async fn init_existing_transport_unicast(
        &self,
        config: TransportConfigUnicast,
        link: LinkUnicastWithOpenAck,
        other_initial_sn: TransportSn,
        other_lease: Duration,
        transport: Arc<dyn TransportUnicastTrait>,
    ) -> InitTransportResult {
        let existing_config = transport.get_config();
        // Verify that fundamental parameters are correct.
        // Ignore the non fundamental parameters like initial SN.
        if *existing_config != config {
            let e = zerror!(
                "Transport with peer {} already exist. Invalid config: {:?}. Expected: {:?}.",
                config.zid,
                config,
                existing_config
            );
            tracing::trace!("{}", e);
            return Err(InitTransportError::Link((
                e.into(),
                link.fail(),
                close::reason::INVALID,
            )));
        }

        // Add the link to the transport
        let (start_tx, start_rx, ack, add_link_guard) = transport
            .add_link(link, other_initial_sn, other_lease)
            .await
            .map_err(InitTransportError::Link)?;

        // complete establish procedure
        let c_link = ack.link();
        let c_t = transport.clone();
        ack.send_open_ack()
            .await
            .map_err(|e| InitTransportError::Transport((e, c_t, close::reason::GENERIC)))?;

        start_tx();

        // notify transport's callback interface that there is a new link
        Self::notify_new_link_unicast(&transport, c_link);

        start_rx();

        drop(add_link_guard);

        Ok(transport)
    }

    fn notify_new_link_unicast(transport: &Arc<dyn TransportUnicastTrait>, link: Link) {
        if let Some(callback) = &transport.get_callback() {
            callback.new_link(link);
        }
    }

    fn notify_new_transport_unicast(
        &self,
        transport: &Arc<dyn TransportUnicastTrait>,
    ) -> ZResult<()> {
        // Assign a callback to the new transport
        let peer = TransportPeer {
            zid: transport.get_zid(),
            whatami: transport.get_whatami(),
            links: transport.get_links(),
            is_qos: transport.get_config().is_qos,
            #[cfg(feature = "shared-memory")]
            is_shm: transport.is_shm(),
        };
        // Notify the transport handler that there is a new transport and get back a callback
        // NOTE: the read loop of the link the open message was sent on remains blocked
        //       until new_unicast() returns. The read_loop in the various links
        //       waits for any eventual transport to associate to.
        let callback = self
            .config
            .handler
            .new_unicast(peer, TransportUnicast(Arc::downgrade(transport)))?;

        // Set the callback on the transport
        transport.set_callback(callback);

        Ok(())
    }

    pub(super) async fn init_new_transport_unicast(
        &self,
        config: TransportConfigUnicast,
        link: LinkUnicastWithOpenAck,
        other_initial_sn: TransportSn,
        other_lease: Duration,
        mut guard: AsyncMutexGuard<'_, HashMap<ZenohIdProto, Arc<dyn TransportUnicastTrait>>>,
    ) -> InitTransportResult {
        macro_rules! link_error {
            ($s:expr, $reason:expr) => {
                match $s {
                    Ok(output) => output,
                    Err(e) => {
                        return Err(InitTransportError::Link((e, link.fail(), $reason)));
                    }
                }
            };
        }

        // Verify that the node does not try to connect to itself
        if config.zid == self.zid() {
            let e = zerror!("{} Attempt to establish transport to itself", self.zid());
            tracing::warn!("{e}");
            return Err(InitTransportError::Link((
                e.into(),
                link.fail(),
                close::reason::CONNECTION_TO_SELF,
            )));
        }

        // Verify that we haven't reached the transport number limit
        if guard.len() >= self.config.unicast.max_sessions {
            let e = zerror!(
                "Max transports reached ({}). Denying new transport with peer: {}",
                self.config.unicast.max_sessions,
                config.zid
            );
            tracing::trace!("{e}");
            return Err(InitTransportError::Link((
                e.into(),
                link.fail(),
                close::reason::INVALID,
            )));
        }

        // Create the transport
        let is_multilink = zcondfeat!("transport_multilink", config.multilink.is_some(), false);

        #[cfg(feature = "stats")]
        let mut labels = HashMap::from([("zid".to_string(), config.zid.to_string())]);
        #[cfg(feature = "stats")]
        if let Some(cert_common_name) = link.link.link.get_auth_id().get_cert_common_name() {
            labels.insert("cert_common_name".to_owned(), cert_common_name.to_owned());
        }
        #[cfg(feature = "stats")]
        let stats = TransportStats::new(Some(Arc::downgrade(&self.get_stats())), labels);

        #[cfg(feature = "shared-memory")]
        let shm_context = match &config.shm {
            Some(shm_config) => self.state.shm_context.as_ref().map(|context| {
                use zenoh_shm::api::protocol_implementations::posix::protocol_id::POSIX_PROTOCOL_ID;

                use crate::{shm::PartnerShmConfig, shm_context::UnicastTransportShmContext};

                let shm_provider = if shm_config.supports_protocol(POSIX_PROTOCOL_ID) {
                    context.shm_provider.clone()
                } else {
                    None
                };

                UnicastTransportShmContext::new(
                    context.shm_reader.clone(),
                    shm_provider,
                    shm_config.clone(),
                )
            }),
            None => None,
        };

        // Select and create transport implementation depending on the cfg and enabled features
        let t = if config.is_lowlatency {
            tracing::debug!("Will use LowLatency transport!");
            TransportUnicastLowlatency::make(
                self.clone(),
                config.clone(),
                #[cfg(feature = "shared-memory")]
                shm_context,
                #[cfg(feature = "stats")]
                stats,
            )
        } else {
            tracing::debug!("Will use Universal transport!");
            link_error!(
                TransportUnicastUniversal::make(
                    self.clone(),
                    config.clone(),
                    #[cfg(feature = "shared-memory")]
                    shm_context,
                    #[cfg(feature = "stats")]
                    stats
                ),
                close::reason::INVALID
            )
        };

        // Add the link to the transport
        let (start_tx, start_rx, ack, add_link_guard) =
            match t.add_link(link, other_initial_sn, other_lease).await {
                Ok(val) => val,
                Err(e) => {
                    let _ = t.close(e.2).await;
                    return Err(InitTransportError::Link(e));
                }
            };

        macro_rules! transport_error {
            ($s:expr, $reason:expr) => {
                match $s {
                    Ok(output) => output,
                    Err(e) => {
                        return Err(InitTransportError::Transport((e, t.clone(), $reason)));
                    }
                }
            };
        }

        // Complete establish procedure
        let c_link = ack.link();
        transport_error!(ack.send_open_ack().await, close::reason::GENERIC);

        // Add the transport transport to the list of active transports
        guard.insert(config.zid, t.clone());
        drop(guard);

        start_tx();

        // Notify manager's interface that there is a new transport
        transport_error!(
            self.notify_new_transport_unicast(&t),
            close::reason::GENERIC
        );

        // Notify transport's callback interface that there is a new link
        Self::notify_new_link_unicast(&t, c_link);

        start_rx();

        drop(add_link_guard);

        zcondfeat!(
            "shared-memory",
            {
                tracing::debug!(
            "New transport opened between {} and {} - whatami: {}, sn resolution: {:?}, initial sn: {:?}, qos: {}, shm: {:?}, multilink: {}, lowlatency: {}",
            self.config.zid,
            config.zid,
            config.whatami,
            config.sn_resolution,
            config.tx_initial_sn,
            config.is_qos,
            config.shm,
            is_multilink,
            config.is_lowlatency
        );
            },
            {
                tracing::debug!(
            "New transport opened between {} and {} - whatami: {}, sn resolution: {:?}, initial sn: {:?}, qos: {:?}, multilink: {}, lowlatency: {}",
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

        Ok(t)
    }

    pub(super) async fn init_transport_unicast(
        &self,
        config: TransportConfigUnicast,
        link: LinkUnicastWithOpenAck,
        other_initial_sn: TransportSn,
        other_lease: Duration,
    ) -> ZResult<TransportUnicast> {
        // First verify if the transport already exists
        let init_result = {
            let guard = zasynclock!(self.state.unicast.transports);
            match guard.get(&config.zid) {
                Some(transport) => {
                    let transport = transport.clone();
                    drop(guard);
                    self.init_existing_transport_unicast(
                        config,
                        link,
                        other_initial_sn,
                        other_lease,
                        transport,
                    )
                    .await
                }
                None => {
                    self.init_new_transport_unicast(
                        config,
                        link,
                        other_initial_sn,
                        other_lease,
                        guard,
                    )
                    .await
                }
            }
        };

        match init_result {
            Ok(transport) => Ok(TransportUnicast(Arc::downgrade(&transport))),
            Err(InitTransportError::Link((e, link, reason))) => {
                let _ = link.close(Some(reason)).await;
                Err(e)
            }
            Err(InitTransportError::Transport((e, transport, reason))) => {
                let _ = transport.close(reason).await;
                Err(e)
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
        let manager = self.new_link_manager_unicast(&endpoint).await?;
        // Fill and merge the endpoint configuration
        if let Some(config) = self
            .config
            .link_configs
            .get(&LinkKind::try_from(&endpoint)?)
        {
            let mut config = parameters::Parameters::from(config.as_str());
            // Overwrite config with current endpoint parameters
            config.extend_from_iter(endpoint.config().iter());
            endpoint = EndPoint::new(
                endpoint.protocol(),
                endpoint.address(),
                endpoint.metadata(),
                config.as_str(),
            )?;
        };

        // Open the link
        tokio::time::timeout(self.config.unicast.open_timeout, async {
            match manager.new_link(endpoint.clone()).await {
                Ok(link) => super::establishment::open::open_link(endpoint, link, self).await,
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| zerror!("{e}"))?
    }

    pub async fn get_transport_unicast(&self, peer: &ZenohIdProto) -> Option<TransportUnicast> {
        zasynclock!(self.state.unicast.transports)
            .get(peer)
            .map(|t| TransportUnicast(Arc::downgrade(t)))
    }

    pub async fn get_transports_unicast(&self) -> Vec<TransportUnicast> {
        zasynclock!(self.state.unicast.transports)
            .values()
            .map(|t| TransportUnicast(Arc::downgrade(t)))
            .collect()
    }

    pub(super) async fn del_transport_unicast(&self, peer: &ZenohIdProto) -> ZResult<()> {
        zasynclock!(self.state.unicast.transports)
            .remove(peer)
            .ok_or_else(|| {
                let e = zerror!("Can not delete the transport of peer: {}", peer);
                tracing::trace!("{}", e);
                e
            })?;
        Ok(())
    }

    pub(crate) async fn handle_new_link_unicast(&self, link: LinkUnicast) {
        let incoming_counter = self.state.unicast.incoming.clone();
        if incoming_counter.load(SeqCst) >= self.config.unicast.accept_pending {
            // We reached the limit of concurrent incoming transport, this means two things:
            // - the values configured for ZN_OPEN_INCOMING_PENDING and ZN_OPEN_TIMEOUT
            //   are too small for the scenario zenoh is deployed in;
            // - there is a tentative of DoS attack.
            // In both cases, let's close the link straight away with no additional notification
            tracing::trace!("Closing link for preventing potential DoS: {}", link);
            let _ = link.close().await;
            return;
        }

        // A new link is available
        tracing::trace!("Accepting link... {}", link);
        self.state.unicast.incoming.fetch_add(1, SeqCst);

        // Spawn a task to accept the link
        let c_manager = self.clone();
        self.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Acceptor, async move {
                if tokio::time::timeout(
                    c_manager.config.unicast.accept_timeout,
                    super::establishment::accept::accept_link(link, &c_manager),
                )
                .await
                .is_err()
                {
                    tracing::debug!(
                        "Failed to accept link before deadline ({}ms)",
                        c_manager.config.unicast.accept_timeout.as_millis()
                    );
                }
                incoming_counter.fetch_sub(1, SeqCst);
            });
    }
}

#[cfg(all(feature = "test", feature = "transport_auth"))]
impl TransportManager {
    pub fn get_auth_handle_unicast(&self) -> Arc<Auth> {
        self.state.unicast.authenticator.clone()
    }
}
