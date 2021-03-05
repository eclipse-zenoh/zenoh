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
use super::core::{PeerId, WhatAmI, ZInt};
use super::defaults::{
    SESSION_BATCH_SIZE, SESSION_KEEP_ALIVE, SESSION_LEASE, SESSION_OPEN_MAX_CONCURRENT,
    SESSION_OPEN_RETRIES, SESSION_OPEN_TIMEOUT, SESSION_SEQ_NUM_RESOLUTION,
};
use super::link::{
    Link, LinkManager, LinkManagerBuilder, Locator, LocatorProperty, LocatorProtocol,
};
use super::transport::SessionTransport;
use super::{Session, SessionDispatcher};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use rand::{RngCore, SeedableRng};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::crypto::{BlockCipher, PseudoRng};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::{zasynclock, zerror};

/// # Examples
/// ```
/// use async_std::sync::Arc;
/// use async_trait::async_trait;
/// use zenoh::net::protocol::core::{PeerId, WhatAmI, whatami};
/// use zenoh::net::protocol::session::{DummySessionEventHandler, SessionEventHandler, Session, SessionDispatcher, SessionHandler, SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
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
/// #[async_trait]
/// impl SessionHandler for MySH {
///     async fn new_session(&self,
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
///     handler: SessionDispatcher::SessionHandler(Arc::new(MySH::new()))
/// };
/// let manager = SessionManager::new(config, None);
///
/// // Create the SessionManager with optional configuration
/// let config = SessionManagerConfig {
///     version: 0,
///     whatami: whatami::PEER,
///     id: PeerId::from(uuid::Uuid::new_v4()),
///     handler: SessionDispatcher::SessionHandler(Arc::new(MySH::new()))
/// };
/// // Setting a value to None means to use the default value
/// let opt_config = SessionManagerOptionalConfig {
///     lease: Some(1_000),         // Set the default lease to 1s
///     keep_alive: Some(100),      // Set the default keep alive interval to 100ms
///     sn_resolution: None,        // Use the default sequence number resolution
///     batch_size: None,           // Use the default batch size
///     timeout: Some(10_000),      // Timeout of 10s when opening a session
///     retries: Some(3),           // Tries to open a session 3 times before failure
///     max_sessions: Some(5),      // Accept any number of sessions
///     max_links: None,            // Allow any number of links in a single session
///     peer_authenticator: None,   // Accept any incoming session
///     link_authenticator: None,   // Accept any incoming link
///     locator_property: None,     // No specific link property
/// };
/// let manager_opt = SessionManager::new(config, Some(opt_config));
/// ```

pub struct SessionManagerConfig {
    pub version: u8,
    pub whatami: WhatAmI,
    pub id: PeerId,
    pub handler: SessionDispatcher,
}

pub struct SessionManagerOptionalConfig {
    pub lease: Option<ZInt>,
    pub keep_alive: Option<ZInt>,
    pub sn_resolution: Option<ZInt>,
    pub batch_size: Option<usize>,
    pub timeout: Option<u64>,
    pub retries: Option<usize>,
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
        let peer_authenticator = PeerAuthenticator::from_properties(config).await?;
        let link_authenticator = LinkAuthenticator::from_properties(config).await?;
        let locator_property = LocatorProperty::from_properties(config).await?;

        let opt_config = SessionManagerOptionalConfig {
            lease: None,
            keep_alive: None,
            sn_resolution: None,
            batch_size: None,
            timeout: None,
            retries: None,
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
    pub(super) batch_size: usize,
    pub(super) timeout: u64,
    pub(super) retries: usize,
    pub(super) max_sessions: Option<usize>,
    pub(super) max_links: Option<usize>,
    pub(super) peer_authenticator: Vec<PeerAuthenticator>,
    pub(super) link_authenticator: Vec<LinkAuthenticator>,
    pub(super) locator_property: HashMap<LocatorProtocol, LocatorProperty>,
    pub(super) handler: SessionDispatcher,
}

pub(super) struct Opened {
    pub(super) whatami: WhatAmI,
    pub(super) sn_resolution: ZInt,
    pub(super) initial_sn: ZInt,
}

#[derive(Clone)]
pub struct SessionManager {
    pub(super) config: Arc<SessionManagerConfigInner>,
    // Outgoing and incoming opened (i.e. established) sessions
    pub(super) opened: Arc<Mutex<HashMap<PeerId, Opened>>>,
    // Incoming uninitialized sessions
    pub(super) incoming: Arc<Mutex<HashSet<Link>>>,
    // Default PRNG
    pub(super) prng: Arc<Mutex<PseudoRng>>,
    // Default cipher for cookies
    pub(super) cipher: Arc<BlockCipher>,
    // Established listeners
    protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManager>>>,
    // Established sessions
    sessions: Arc<Mutex<HashMap<PeerId, Arc<SessionTransport>>>>,
}

impl SessionManager {
    pub fn new(
        config: SessionManagerConfig,
        mut opt_config: Option<SessionManagerOptionalConfig>,
    ) -> SessionManager {
        // Set default optional values
        let mut lease = *SESSION_LEASE;
        let mut keep_alive = *SESSION_KEEP_ALIVE;
        let mut sn_resolution = *SESSION_SEQ_NUM_RESOLUTION;
        let mut batch_size = *SESSION_BATCH_SIZE;
        let mut timeout = *SESSION_OPEN_TIMEOUT;
        let mut retries = *SESSION_OPEN_RETRIES;
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
            if let Some(v) = opt.batch_size.take() {
                batch_size = v;
            }
            if let Some(v) = opt.timeout.take() {
                timeout = v;
            }
            if let Some(v) = opt.retries.take() {
                retries = v;
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
            batch_size,
            timeout,
            retries,
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
            opened: Arc::new(Mutex::new(HashMap::new())),
            incoming: Arc::new(Mutex::new(HashSet::new())),
            prng: Arc::new(Mutex::new(prng)),
            cipher: Arc::new(cipher),
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

    pub async fn get_listeners(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zasynclock!(self.protocols).values() {
            vec.extend_from_slice(&p.get_listeners().await);
        }
        vec
    }

    pub async fn get_locators(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
        for p in zasynclock!(self.protocols).values() {
            vec.extend_from_slice(&p.get_locators().await);
        }
        vec
    }

    pub async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let manager = self.get_link_manager(&locator.get_proto()).await?;
        manager.del_listener(locator).await?;
        if manager.get_listeners().await.is_empty() {
            self.del_link_manager(&locator.get_proto()).await?;
        }
        Ok(())
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    async fn get_or_new_link_manager(&self, protocol: &LocatorProtocol) -> LinkManager {
        loop {
            match self.get_link_manager(protocol).await {
                Ok(manager) => return manager,
                Err(_) => match self.new_link_manager(protocol).await {
                    Ok(manager) => return manager,
                    Err(_) => continue,
                },
            }
        }
    }

    async fn new_link_manager(&self, protocol: &LocatorProtocol) -> ZResult<LinkManager> {
        let mut w_guard = zasynclock!(self.protocols);
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

    async fn get_link_manager(&self, protocol: &LocatorProtocol) -> ZResult<LinkManager> {
        match zasynclock!(self.protocols).get(protocol) {
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
        match zasynclock!(self.protocols).remove(protocol) {
            Some(_) => Ok(()),
            None => zerror!(ZErrorKind::Other {
                descr: format!("Can not delete the link manager for protocol ({}) because it has not been found.", protocol)
            })
        }
    }

    /*************************************/
    /*              SESSION              */
    /*************************************/
    pub async fn get_session(&self, peer: &PeerId) -> Option<Session> {
        zasynclock!(self.sessions)
            .get(peer)
            .map(|t| Session::new(Arc::downgrade(&t)))
    }

    pub async fn get_sessions(&self) -> Vec<Session> {
        zasynclock!(self.sessions)
            .values()
            .map(|t| Session::new(Arc::downgrade(&t)))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn get_or_new_session(
        &self,
        peer: &PeerId,
        whatami: &WhatAmI,
        sn_resolution: ZInt,
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
        is_local: bool,
    ) -> Session {
        loop {
            match self.get_session(peer).await {
                Some(session) => return session,
                None => match self
                    .new_session(
                        peer,
                        whatami,
                        sn_resolution,
                        initial_sn_tx,
                        initial_sn_rx,
                        is_local,
                    )
                    .await
                {
                    Ok(session) => return session,
                    Err(_) => continue,
                },
            }
        }
    }

    pub(super) async fn del_session(&self, peer: &PeerId) -> ZResult<()> {
        match zasynclock!(self.sessions).remove(peer) {
            Some(_) => {
                for pa in self.config.peer_authenticator.iter() {
                    pa.handle_close(peer).await;
                }
                Ok(())
            }
            None => {
                let e = format!("Can not delete the session of peer: {}", peer);
                log::trace!("{}", e);
                zerror!(ZErrorKind::Other { descr: e })
            }
        }
    }

    pub(super) async fn new_session(
        &self,
        peer: &PeerId,
        whatami: &WhatAmI,
        sn_resolution: ZInt,
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
        is_local: bool,
    ) -> ZResult<Session> {
        let mut w_guard = zasynclock!(self.sessions);
        if w_guard.contains_key(peer) {
            let e = format!("Can not create a new session for peer: {}", peer);
            log::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        // Create the channel object
        let a_ch = Arc::new(SessionTransport::new(
            self.clone(),
            peer.clone(),
            *whatami,
            sn_resolution,
            initial_sn_tx,
            initial_sn_rx,
            is_local,
        ));

        // Create a weak reference to the session
        let session = Session::new(Arc::downgrade(&a_ch));
        // Add the session to the list of active sessions
        w_guard.insert(peer.clone(), a_ch);

        log::debug!(
            "New session opened with {}: whatami {}, sn resolution {}, initial sn tx {}, initial sn rx {}, is_local: {}",
            peer,
            whatami,
            sn_resolution,
            initial_sn_tx,
            initial_sn_rx,
            is_local
        );

        Ok(session)
    }

    pub fn open_session<'async_trait>(
        &'async_trait self,
        locator: &'async_trait Locator,
    ) -> async_std::pin::Pin<
        Box<dyn std::future::Future<Output = ZResult<Session>> + Send + 'async_trait>,
    >
    where
        Self: Sync + 'async_trait,
    {
        Box::pin(async move {
            // Create the timeout duration
            let to = Duration::from_millis(self.config.timeout);
            // Automatically create a new link manager for the protocol if it does not exist
            let manager = self.get_or_new_link_manager(&locator.get_proto()).await;
            let ps = self.config.locator_property.get(&locator.get_proto());
            // Create a new link associated by calling the Link Manager
            let link = manager.new_link(&locator, ps).await?;

            // Try a maximum number of times to open a session
            let retries = self.config.retries;
            for i in 0..retries {
                // Check the future result
                match super::initial::open_link(self, &link).timeout(to).await {
                    Ok(res) => return res,
                    Err(e) => log::debug!(
                        "Can not open a session to {}: {}. Timeout: {:?}. Attempt: {}/{}",
                        locator,
                        e,
                        to,
                        i + 1,
                        retries
                    ),
                }
            }

            let e = format!(
                "Can not open a session to {}: maximum number of attemps reached ({})",
                locator, retries
            );
            log::warn!("{}", e);
            zerror!(ZErrorKind::Other { descr: e })
        })
    }

    pub(crate) async fn handle_new_link(&self, link: Link, properties: Option<LocatorProperty>) {
        let mut guard = zasynclock!(self.incoming);
        if guard.len() >= *SESSION_OPEN_MAX_CONCURRENT {
            // We reached the limit of concurrent incoming session, this means two things:
            // - the values configured for SESSION_OPEN_MAX_CONCURRENT and SESSION_OPEN_TIMEOUT
            //   are too small for the scenario zenoh is deployed in;
            // - there is a tentative of DoS attack.
            // In both cases, let's close the link straight away with no additional notification
            log::trace!("Closing link for preventing potential DoS: {}", link);
            let _ = link.close().await;
            return;
        }

        // A new link is available
        log::trace!("New link waiting... {}", link);
        guard.insert(link.clone());
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

            let to = Duration::from_millis(*SESSION_OPEN_TIMEOUT);
            let res = super::initial::accept_link(&c_manager, &link, &auth_link)
                .timeout(to)
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
