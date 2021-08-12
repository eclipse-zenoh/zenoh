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
use super::authenticator::{
    AuthenticatedPeerLink, DummyLinkAuthenticator, DummyPeerAuthenticator, LinkAuthenticator,
    PeerAuthenticator,
};
use super::common::transport::SessionTransport;
use super::core::{PeerId, WhatAmI, ZInt};
use super::defaults::{
    ZN_DEFAULT_BATCH_SIZE, ZN_DEFAULT_SEQ_NUM_RESOLUTION, ZN_LINK_KEEP_ALIVE, ZN_LINK_LEASE,
    ZN_OPEN_INCOMING_PENDING, ZN_OPEN_TIMEOUT,
};
#[cfg(feature = "zero-copy")]
use super::io::SharedMemoryReader;
use super::link::{
    Link, LinkManager, LinkManagerBuilder, Locator, LocatorProperty, LocatorProtocol,
};
use super::unicast::transport::SessionTransportUnicast;
use super::{Session, SessionConfig, SessionHandler};
use async_std::prelude::*;
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex};
use async_std::task;
use rand::{RngCore, SeedableRng};
use std::collections::HashMap;
#[cfg(feature = "zero-copy")]
use std::sync::RwLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::properties::config::{
    ZN_LINK_KEEP_ALIVE_KEY, ZN_LINK_KEEP_ALIVE_STR, ZN_LINK_LEASE_KEY, ZN_LINK_LEASE_STR,
    ZN_OPEN_INCOMING_PENDING_KEY, ZN_OPEN_INCOMING_PENDING_STR, ZN_OPEN_TIMEOUT_KEY,
    ZN_OPEN_TIMEOUT_STR, ZN_SEQ_NUM_RESOLUTION_KEY, ZN_SEQ_NUM_RESOLUTION_STR,
};
use zenoh_util::{zasynclock, zerror, zlock};

/// # Examples
/// ```
/// use async_std::sync::Arc;
/// use zenoh::net::protocol::core::{PeerId, WhatAmI, whatami};
/// use zenoh::net::protocol::session::{DummySessionEventHandler, SessionEventHandler, Session, SessionHandler, SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
///
/// use zenoh_util::core::ZResult;
///
/// // Create my session handler to be notified when a new session is initiated with me
/// struct MySH;
///
/// impl MySH {
///     fn new() -> MySH {
///         MySH
///     }
/// }
///
/// impl SessionHandler for MySH {
///     fn new_session(&self,
///         _session: Session
///     ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
///         Ok(Arc::new(DummySessionEventHandler::new()))
///     }
/// }
///
/// // Create the SessionManager
/// let config = SessionManagerConfig {
///     version: 0,
///     whatami: whatami::PEER,
///     id: PeerId::from(uuid::Uuid::new_v4()),
///     handler: Arc::new(MySH::new())
/// };
/// let manager = SessionManager::new(config, None);
///
/// // Create the SessionManager with optional configuration
/// let config = SessionManagerConfig {
///     version: 0,
///     whatami: whatami::PEER,
///     id: PeerId::from(uuid::Uuid::new_v4()),
///     handler: Arc::new(MySH::new())
/// };
/// // Setting a value to None means to use the default value
/// let opt_config = SessionManagerOptionalConfig {
///     lease: Some(1_000),             // Set the default lease to 1s
///     keep_alive: Some(100),          // Set the default keep alive interval to 100ms
///     sn_resolution: None,            // Use the default sequence number resolution
///     open_timeout: None,             // Use the default open timeout
///     open_incoming_pending: None,    // Use the default amount of pending incoming sessions
///     batch_size: None,               // Use the default batch size
///     max_sessions: Some(5),          // Accept any number of sessions
///     max_links: None,                // Allow any number of links in a single session
///     peer_authenticator: None,       // Accept any incoming session
///     link_authenticator: None,       // Accept any incoming link
///     locator_property: None,         // No specific link property
/// };
/// let manager_opt = SessionManager::new(config, Some(opt_config));
/// ```
pub struct SessionManagerConfig {
    pub version: u8,
    pub whatami: WhatAmI,
    pub id: PeerId,
    pub handler: Arc<dyn SessionHandler + Send + Sync>,
}

pub struct SessionManagerOptionalConfig {
    pub lease: Option<ZInt>,
    pub keep_alive: Option<ZInt>,
    pub sn_resolution: Option<ZInt>,
    pub open_timeout: Option<ZInt>,
    pub open_incoming_pending: Option<usize>,
    pub batch_size: Option<usize>,
    pub max_sessions: Option<usize>,
    pub max_links: Option<usize>,
    pub peer_authenticator: Option<Vec<PeerAuthenticator>>,
    pub link_authenticator: Option<Vec<LinkAuthenticator>>,
    pub locator_property: Option<Vec<LocatorProperty>>,
}

impl SessionManagerOptionalConfig {
    pub async fn from_properties(
        config: &ConfigProperties,
    ) -> ZResult<Option<SessionManagerOptionalConfig>> {
        macro_rules! zparse {
            ($key:expr, $str:expr) => {
                match config.get(&$key) {
                    Some(snr) => {
                        let snr = snr.parse().map_err(|_| {
                            let e = format!(
                                "Failed to read configuration {}: {} is not a valid entry",
                                $str, snr
                            );
                            log::warn!("{}", e);
                            zerror2!(ZErrorKind::ValueDecodingFailed { descr: e })
                        })?;
                        Some(snr)
                    }
                    None => None,
                }
            };
        }

        let peer_authenticator = PeerAuthenticator::from_properties(config).await?;
        let link_authenticator = LinkAuthenticator::from_properties(config).await?;
        let locator_property = LocatorProperty::from_properties(config).await?;

        let lease = zparse!(ZN_LINK_LEASE_KEY, ZN_LINK_LEASE_STR);
        let keep_alive = zparse!(ZN_LINK_KEEP_ALIVE_KEY, ZN_LINK_KEEP_ALIVE_STR);
        let sn_resolution = zparse!(ZN_SEQ_NUM_RESOLUTION_KEY, ZN_SEQ_NUM_RESOLUTION_STR);
        let open_timeout = zparse!(ZN_OPEN_TIMEOUT_KEY, ZN_OPEN_TIMEOUT_STR);
        let open_incoming_pending =
            zparse!(ZN_OPEN_INCOMING_PENDING_KEY, ZN_OPEN_INCOMING_PENDING_STR);

        let opt_config = SessionManagerOptionalConfig {
            lease,
            keep_alive,
            sn_resolution,
            open_timeout,
            open_incoming_pending,
            batch_size: None,
            max_sessions: None,
            max_links: None,
            peer_authenticator: if peer_authenticator.is_empty() {
                None
            } else {
                Some(peer_authenticator)
            },
            link_authenticator: if link_authenticator.is_empty() {
                None
            } else {
                Some(link_authenticator)
            },
            locator_property: if locator_property.is_empty() {
                None
            } else {
                Some(locator_property)
            },
        };
        Ok(Some(opt_config))
    }
}

pub(super) struct SessionManagerConfigInner {
    pub(super) version: u8,
    pub(super) whatami: WhatAmI,
    pub(super) pid: PeerId,
    pub(super) lease: ZInt,
    pub(super) keep_alive: ZInt,
    pub(super) sn_resolution: ZInt,
    pub(super) open_timeout: ZInt,
    pub(super) open_incoming_pending: usize,
    pub(super) batch_size: usize,
    pub(super) max_sessions: Option<usize>,
    pub(super) max_links: Option<usize>,
    pub(super) peer_authenticator: Vec<PeerAuthenticator>,
    pub(super) link_authenticator: Vec<LinkAuthenticator>,
    pub(super) locator_property: HashMap<LocatorProtocol, LocatorProperty>,
    pub(super) handler: Arc<dyn SessionHandler + Send + Sync>,
}

pub(super) struct Opened {
    pub(super) whatami: WhatAmI,
    pub(super) sn_resolution: ZInt,
    pub(super) initial_sn: ZInt,
}

#[derive(Clone)]
pub struct SessionManager {
    pub(super) unicast: SessionManagerUnicast,
    pub(crate) multicast: SessionManagerMulticast,
    pub(super) config: Arc<SessionManagerConfigInner>,
    // Outgoing and incoming opened (i.e. established) sessions
    pub(super) opened: AsyncArc<AsyncMutex<HashMap<PeerId, Opened>>>,
    // Incoming uninitialized sessions
    pub(super) incoming: AsyncArc<AsyncMutex<HashMap<Link, Option<Vec<u8>>>>>,
    // Default PRNG
    pub(super) prng: AsyncArc<AsyncMutex<PseudoRng>>,
    // Default cipher for cookies
    pub(super) cipher: Arc<BlockCipher>,
    // Established listeners
    protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManager>>>,
    // Established sessions
    sessions: Arc<Mutex<HashMap<PeerId, Arc<SessionTransport>>>>,
    #[cfg(feature = "zero-copy")]
    pub(super) shmr: Arc<RwLock<SharedMemoryReader>>,
}

impl SessionManager {
    pub fn new(
        config: SessionManagerConfig,
        mut opt_config: Option<SessionManagerOptionalConfig>,
    ) -> SessionManager {
        // Set default optional values
        let mut lease = *ZN_LINK_LEASE;
        let mut keep_alive = *ZN_LINK_KEEP_ALIVE;
        let mut sn_resolution = ZN_DEFAULT_SEQ_NUM_RESOLUTION;
        let mut open_timeout = *ZN_OPEN_TIMEOUT;
        let mut open_incoming_pending = *ZN_OPEN_INCOMING_PENDING;
        let mut batch_size = ZN_DEFAULT_BATCH_SIZE;
        let mut max_sessions = None;
        let mut max_links = None;
        let mut peer_authenticator = vec![DummyPeerAuthenticator::make()];
        let mut link_authenticator = vec![DummyLinkAuthenticator::make()];
        let mut locator_property = HashMap::new();

        // Override default values if provided
        if let Some(mut opt) = opt_config.take() {
            if let Some(v) = opt.lease.take() {
                lease = v;
            }
            if let Some(v) = opt.keep_alive.take() {
                keep_alive = v;
            }
            if let Some(v) = opt.sn_resolution.take() {
                sn_resolution = v;
            }
            if let Some(v) = opt.open_timeout.take() {
                open_timeout = v;
            }
            if let Some(v) = opt.open_incoming_pending.take() {
                open_incoming_pending = v;
            }
            if let Some(v) = opt.batch_size.take() {
                batch_size = v;
            }
            max_sessions = opt.max_sessions;
            max_links = opt.max_links;
            if let Some(v) = opt.peer_authenticator.take() {
                peer_authenticator = v;
            }
            if let Some(v) = opt.link_authenticator.take() {
                link_authenticator = v;
            }
            if let Some(mut v) = opt.locator_property.take() {
                for p in v.drain(..) {
                    locator_property.insert(p.get_proto(), p);
                }
            }
        }

        let config_inner = SessionManagerConfigInner {
            version: config.version,
            whatami: config.whatami,
            pid: config.id.clone(),
            lease,
            keep_alive,
            sn_resolution,
            open_timeout,
            open_incoming_pending,
            batch_size,
            max_sessions,
            max_links,
            peer_authenticator,
            link_authenticator,
            locator_property,
            handler: config.handler,
        };

        // Initialize the PRNG and the Cipher
        let mut prng = PseudoRng::from_entropy();
        let mut key = [0u8; BlockCipher::BLOCK_SIZE];
        prng.fill_bytes(&mut key);
        let cipher = BlockCipher::new(key);

        SessionManager {
            config: Arc::new(config_inner),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            opened: AsyncArc::new(AsyncMutex::new(HashMap::new())),
            incoming: AsyncArc::new(AsyncMutex::new(HashMap::new())),
            prng: AsyncArc::new(AsyncMutex::new(prng)),
            cipher: Arc::new(cipher),
            #[cfg(feature = "zero-copy")]
            shmr: Arc::new(RwLock::new(SharedMemoryReader::new())),
        }
    }

    pub fn pid(&self) -> PeerId {
        self.config.pid.clone()
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let manager = self.get_or_new_link_manager(&locator.get_proto()).await;
        let ps = self.config.locator_property.get(&locator.get_proto());
        manager.new_listener(locator, ps).await
    }

    pub async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let manager = self.get_link_manager(&locator.get_proto())?;
        manager.del_listener(locator).await?;
        if manager.get_listeners().is_empty() {
            self.del_link_manager(&locator.get_proto()).await?;
        }
        Ok(())
    }

    pub fn get_listeners(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zlock!(self.protocols).values() {
            vec.extend_from_slice(&p.get_listeners());
        }
        vec
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zlock!(self.protocols).values() {
            vec.extend_from_slice(&p.get_locators());
        }
        vec
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    async fn get_or_new_link_manager(&self, protocol: &LocatorProtocol) -> LinkManager {
        loop {
            match self.get_link_manager(protocol) {
                Ok(manager) => return manager,
                Err(_) => match self.new_link_manager(protocol).await {
                    Ok(manager) => return manager,
                    Err(_) => continue,
                },
            }
        }
    }

    async fn new_link_manager(&self, protocol: &LocatorProtocol) -> ZResult<LinkManager> {
        let mut w_guard = zlock!(self.protocols);
        if w_guard.contains_key(protocol) {
            return zerror!(ZErrorKind::Other {
                descr: format!(
                    "Can not create the link manager for protocol ({}) because it already exists",
                    protocol
                )
            });
        }

        let lm = LinkManagerBuilder::make(self.clone(), protocol);
        w_guard.insert(protocol.clone(), lm.clone());
        Ok(lm)
    }

    fn get_link_manager(&self, protocol: &LocatorProtocol) -> ZResult<LinkManager> {
        match zlock!(self.protocols).get(protocol) {
            Some(manager) => Ok(manager.clone()),
            None => zerror!(ZErrorKind::Other {
                descr: format!(
                    "Can not get the link manager for protocol ({}) because it has not been found",
                    protocol
                )
            }),
        }
    }

    async fn del_link_manager(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zlock!(self.protocols).remove(protocol) {
            Some(lm) => {
                let mut listeners = lm.get_listeners();
                for l in listeners.drain(..) {
                    let _ = lm.del_listener(&l).await;
                }
                Ok(())
            },
            None => zerror!(ZErrorKind::Other {
                descr: format!("Can not delete the link manager for protocol ({}) because it has not been found.", protocol)
            })
        }
    }

    /*************************************/
    /*              SESSION              */
    /*************************************/
    pub fn get_session(&self, peer: &PeerId) -> Option<Session> {
        zlock!(self.sessions)
            .get(peer)
            .map(|t| Session::new(Arc::downgrade(t)))
    }

    pub fn get_sessions(&self) -> Vec<Session> {
        zlock!(self.sessions)
            .values()
            .map(|t| Session::new(Arc::downgrade(t)))
            .collect()
    }

    pub(super) fn init_session(&self, config: SessionConfig) -> ZResult<Session> {
        let mut guard = zlock!(self.sessions);

        // First verify if the session already exists
        if let Some(session) = guard.get(&config.peer) {
            if session.whatami != config.whatami {
                let e = format!(
                    "Session with peer {} already exist. Invalid whatami: {}. Execpted: {}.",
                    config.peer, config.whatami, session.whatami
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            if session.sn_resolution != config.sn_resolution {
                let e = format!(
                    "Session with peer {} already exist. Invalid sn resolution: {}. Execpted: {}.",
                    config.peer, config.sn_resolution, session.sn_resolution
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            if session.is_shm() != config.is_shm {
                let e = format!(
                    "Session with peer {} already exist. Invalid is_shm: {}. Execpted: {}.",
                    config.peer,
                    config.is_shm,
                    session.is_shm()
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            return Ok(Session::new(Arc::downgrade(session)));
        }

        // Then verify that we haven't reached the session number limit
        if let Some(limit) = self.config.max_sessions {
            if guard.len() == limit {
                let e = format!(
                    "Max sessions reached ({}). Denying new session with peer: {}",
                    limit, config.peer
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }
        }

        // Create the session transport
        let stc = SessionTransportConfig {
            manager: self.clone(),
            pid: config.peer.clone(),
            whatami: config.whatami,
            sn_resolution: config.sn_resolution,
            initial_sn_tx: config.initial_sn_tx,
            initial_sn_rx: config.initial_sn_rx,
            is_shm: config.is_shm,
            is_qos: config.is_qos,
        };
        let a_st = Arc::new(SessionTransport::new(stc));

        // Create a weak reference to the session transport
        let session = Session::new(Arc::downgrade(&a_st));
        // Add the session transport to the list of active sessions
        guard.insert(config.peer.clone(), a_st);

        log::debug!(
            "New session opened with {}: whatami {}, sn resolution {}, initial sn tx {}, initial sn rx {}, shm: {}, qos: {}",
            config.peer,
            config.whatami,
            config.sn_resolution,
            config.initial_sn_tx,
            config.initial_sn_rx,
            config.is_shm,
            config.is_qos
        );

        Ok(session)
    }

    pub(super) async fn del_session(&self, peer: &PeerId) -> ZResult<()> {
        let _ = zlock!(self.sessions).remove(peer).ok_or_else(|| {
            let e = format!("Can not delete the session of peer: {}", peer);
            log::trace!("{}", e);
            zerror2!(ZErrorKind::Other { descr: e })
        })?;

        for pa in self.config.peer_authenticator.iter() {
            pa.handle_close(peer).await;
        }
        Ok(())
    }

    pub async fn open_session(&self, locator: &Locator) -> ZResult<Session> {
        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self.get_or_new_link_manager(&locator.get_proto()).await;
        let ps = self.config.locator_property.get(&locator.get_proto());
        // Create a new link associated by calling the Link Manager
        let link = manager.new_link(locator, ps).await?;
        // Open the link
        super::establishment::unicast::open_link(self, &link).await
    }

    pub async fn join_group(&self, _locator: &Locator) -> ZResult<Session> {
        // @TODO: implement the multicast join
        Ok(Session(std::sync::Weak::new()))
    }

    pub(crate) async fn handle_new_link(&self, link: Link, properties: Option<LocatorProperty>) {
        let mut guard = zasynclock!(self.incoming);
        if guard.len() >= self.config.open_incoming_pending {
            // We reached the limit of concurrent incoming session, this means two things:
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
        for la in self.config.link_authenticator.iter() {
            let res = la.handle_new_link(&link, properties.as_ref()).await;
            match res {
                Ok(pid) => {
                    // Check that all the peer authenticators, eventually return the same PeerId
                    if let Some(pid1) = peer_id.as_ref() {
                        if let Some(pid2) = pid.as_ref() {
                            if pid1 != pid2 {
                                log::debug!("Ambigous PeerID identification for link: {}", link);
                                let _ = link.close().await;
                                zasynclock!(self.incoming).remove(&link);
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
        let c_incoming = self.incoming.clone();
        let c_manager = self.clone();
        task::spawn(async move {
            let auth_link = AuthenticatedPeerLink {
                src: link.get_src(),
                dst: link.get_dst(),
                peer_id,
                properties,
            };

            let timeout = Duration::from_millis(c_manager.config.open_timeout);
            let res = super::establishment::unicast::accept_link(&c_manager, &link, &auth_link)
                .timeout(timeout)
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
