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
use async_std::prelude::*;
use async_std::sync::{channel, Arc, RwLock, Weak};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::time::Duration;

use super::channel::Channel;
use super::defaults::{
    SESSION_BATCH_SIZE, SESSION_KEEP_ALIVE, SESSION_LEASE, SESSION_OPEN_RETRIES,
    SESSION_OPEN_TIMEOUT, SESSION_SEQ_NUM_RESOLUTION,
};
use super::{InitialSession, SessionEventHandler, SessionHandler, Transport};

use crate::core::{PeerId, WhatAmI, ZInt};
use crate::link::{Link, LinkManager, LinkManagerBuilder, Locator, LocatorProtocol};
use crate::proto::{smsg, Attachment, ZenohMessage};

use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasyncread, zasyncwrite, zerror};

/// # Examples
/// ```
/// use async_std::sync::Arc;
/// use async_trait::async_trait;
/// use zenoh_protocol::core::{PeerId, WhatAmI, whatami};
/// use zenoh_protocol::session::{DummyHandler, SessionEventHandler, Session, SessionHandler, SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
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
///         Ok(Arc::new(DummyHandler::new()))
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
///     lease: Some(1_000),     // Set the default lease to 1s
///     keep_alive: Some(100),  // Set the default keep alive interval to 100ms
///     sn_resolution: None,    // Use the default sequence number resolution
///     batch_size: None,       // Use the default batch size
///     timeout: Some(10_000),  // Timeout of 10s when opening a session
///     retries: Some(3),       // Tries to open a session 3 times before failure
///     max_sessions: Some(5),  // Accept any number of sessions
///     max_links: None         // Allow any number of links in a single session
/// };
/// let manager_opt = SessionManager::new(config, Some(opt_config));
/// ```

#[derive(Clone)]
pub struct SessionManager(Arc<SessionManagerInner>);

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
    pub batch_size: Option<usize>,
    pub timeout: Option<u64>,
    pub retries: Option<usize>,
    pub max_sessions: Option<usize>,
    pub max_links: Option<usize>,
}

impl SessionManager {
    pub fn new(
        config: SessionManagerConfig,
        opt_config: Option<SessionManagerOptionalConfig>,
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

        // Override default values if provided
        if let Some(opt) = opt_config {
            if let Some(v) = opt.lease {
                lease = v;
            }
            if let Some(v) = opt.keep_alive {
                keep_alive = v;
            }
            if let Some(v) = opt.sn_resolution {
                sn_resolution = v;
            }
            if let Some(v) = opt.batch_size {
                batch_size = v;
            }
            if let Some(v) = opt.timeout {
                timeout = v;
            }
            if let Some(v) = opt.retries {
                retries = v;
            }
            max_sessions = opt.max_sessions;
            max_links = opt.max_links;
        }

        let inner_config = SessionManagerInnerConfig {
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
            handler: config.handler,
        };
        // Create the inner session manager
        let manager_inner = Arc::new(SessionManagerInner::new(inner_config));
        // Create the initial session used to establish new connections
        let initial_session = Arc::new(InitialSession::new(manager_inner.clone()));
        // Add the session to the inner session manager
        manager_inner.init_initial_session(initial_session);

        SessionManager(manager_inner)
    }

    pub fn pid(&self) -> PeerId {
        self.0.config.pid.clone()
    }

    /*************************************/
    /*              SESSION              */
    /*************************************/
    pub async fn open_session(
        &self,
        locator: &Locator,
        attachment: &Option<Attachment>,
    ) -> ZResult<Session> {
        // Retrieve the initial session
        let initial = self.0.get_initial_session().await;
        let transport = self.0.get_initial_transport().await;
        // Create the timeout duration
        let to = Duration::from_millis(self.0.config.timeout);

        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self
            .0
            .get_or_new_link_manager(&self.0, &locator.get_proto())
            .await;
        // Create a new link associated by calling the Link Manager
        let link = match manager.new_link(&locator, &transport).await {
            Ok(link) => link,
            Err(e) => {
                log::warn!("Can not to create a link to locator {}: {}", locator, e);
                return Err(e);
            }
        };
        // Create a channel for knowing when a session is open
        let (sender, receiver) = channel::<ZResult<Session>>(1);

        // Try a maximum number of times to open a session
        let retries = self.0.config.retries;
        for i in 0..retries {
            // Create the open future
            let open_fut = initial.open(&link, attachment, &sender).timeout(to);
            let channel_fut = receiver.recv().timeout(to);

            // Check the future result
            match open_fut.try_join(channel_fut).await {
                // Future timeout result
                Ok((_, channel_res)) => match channel_res {
                    // Channel result
                    Ok(res) => match res {
                        Ok(session) => return Ok(session),
                        Err(e) => {
                            let e = format!("Can not open a session to {}: {}", locator, e);
                            log::warn!("{}", e);
                            return zerror!(ZErrorKind::Other { descr: e });
                        }
                    },
                    Err(e) => {
                        let e = format!("Can not open a session to {}: {}", locator, e);
                        log::warn!("{}", e);
                        return zerror!(ZErrorKind::Other { descr: e });
                    }
                },
                Err(e) => {
                    log::debug!(
                        "Can not open a session to {}: {}. Timeout: {:?}. Attempt: {}/{}",
                        locator,
                        e,
                        to,
                        i + 1,
                        retries
                    );
                    continue;
                }
            }
        }

        // Delete the link on the link manager
        let _ = manager.del_link(&link.get_src(), &link.get_dst()).await;

        let e = format!(
            "Can not open a session to {}: maximum number of attemps reached ({})",
            locator, retries
        );
        log::warn!("{}", e);
        zerror!(ZErrorKind::Other { descr: e })
    }

    pub async fn get_session(&self, peer: &PeerId) -> Option<Session> {
        match self.0.get_session(peer).await {
            Ok(session) => Some(session),
            Err(_) => None,
        }
    }

    pub async fn get_sessions(&self) -> Vec<Session> {
        self.0.get_sessions().await
    }

    /*************************************/
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener(&self, locator: &Locator) -> ZResult<Locator> {
        let manager = self
            .0
            .get_or_new_link_manager(&self.0, &locator.get_proto())
            .await;
        manager.new_listener(locator).await
    }

    pub async fn del_listener(&self, locator: &Locator) -> ZResult<()> {
        let manager = self.0.get_link_manager(&locator.get_proto()).await?;
        manager.del_listener(locator).await?;
        if manager.get_listeners().await.is_empty() {
            self.0.del_link_manager(&locator.get_proto()).await?;
        }
        Ok(())
    }

    pub async fn get_listeners(&self) -> Vec<Locator> {
        self.0.get_listeners().await
    }

    pub async fn get_locators(&self) -> Vec<Locator> {
        self.0.get_locators().await
    }
}

pub(crate) struct SessionManagerInnerConfig {
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
    pub(super) handler: Arc<dyn SessionHandler + Send + Sync>,
}

pub(crate) struct SessionManagerInner {
    pub(crate) config: SessionManagerInnerConfig,
    initial: RwLock<Option<Arc<InitialSession>>>,
    protocols: RwLock<HashMap<LocatorProtocol, LinkManager>>,
    sessions: RwLock<HashMap<PeerId, Arc<Channel>>>,
}

impl SessionManagerInner {
    fn new(config: SessionManagerInnerConfig) -> SessionManagerInner {
        SessionManagerInner {
            config,
            initial: RwLock::new(None),
            protocols: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /*************************************/
    /*          INITIALIZATION           */
    /*************************************/
    fn init_initial_session(&self, session: Arc<InitialSession>) {
        *self.initial.try_write().unwrap() = Some(session);
    }

    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    async fn get_or_new_link_manager(
        &self,
        a_self: &Arc<Self>,
        protocol: &LocatorProtocol,
    ) -> LinkManager {
        loop {
            match self.get_link_manager(protocol).await {
                Ok(manager) => return manager,
                Err(_) => match self.new_link_manager(a_self, protocol).await {
                    Ok(manager) => return manager,
                    Err(_) => continue,
                },
            }
        }
    }

    async fn new_link_manager(
        &self,
        a_self: &Arc<Self>,
        protocol: &LocatorProtocol,
    ) -> ZResult<LinkManager> {
        let mut w_guard = zasyncwrite!(self.protocols);
        if w_guard.contains_key(protocol) {
            return zerror!(ZErrorKind::Other {
                descr: format!(
                    "Can not create the link manager for protocol ({}) because it already exists",
                    protocol
                )
            });
        }

        let lm = LinkManagerBuilder::make(a_self.clone(), protocol);
        w_guard.insert(protocol.clone(), lm.clone());
        Ok(lm)
    }

    async fn get_link_manager(&self, protocol: &LocatorProtocol) -> ZResult<LinkManager> {
        match zasyncread!(self.protocols).get(protocol) {
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
        match zasyncwrite!(self.protocols).remove(protocol) {
            Some(_) => Ok(()),
            None => zerror!(ZErrorKind::Other {
                descr: format!("Can not delete the link manager for protocol ({}) because it has not been found.", protocol)
            })
        }
    }

    pub(super) async fn get_listeners(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = Vec::new();
        for p in zasyncread!(self.protocols).values() {
            vec.extend_from_slice(&p.get_listeners().await);
        }

        let mut result = vec![];
        for locator in vec {
            match locator {
                Locator::Tcp(addr) => {
                    if addr.ip() == Ipv4Addr::new(0, 0, 0, 0) {
                        match zenoh_util::net::get_local_addresses() {
                            Ok(ipaddrs) => {
                                for ipaddr in ipaddrs {
                                    if !ipaddr.is_loopback() && ipaddr.is_ipv4() {
                                        result.push(
                                            format!("tcp/{}:{}", ipaddr.to_string(), addr.port())
                                                .parse()
                                                .unwrap(),
                                        );
                                    }
                                }
                            }
                            Err(err) => log::error!("Unable to get local addresses : {}", err),
                        }
                    } else {
                        result.push(locator)
                    }
                }
                locator => result.push(locator),
            }
        }
        result
    }

    pub(super) async fn get_locators(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = Vec::new();
        for p in zasyncread!(self.protocols).values() {
            vec.extend_from_slice(&p.get_locators().await);
        }
        vec
    }

    /*************************************/
    /*              SESSION              */
    /*************************************/
    pub(crate) async fn get_initial_transport(&self) -> Transport {
        let initial = zasyncread!(self.initial).as_ref().unwrap().clone();
        Transport::new(initial)
    }

    pub(super) async fn get_initial_session(&self) -> Arc<InitialSession> {
        zasyncread!(self.initial).as_ref().unwrap().clone()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn get_or_new_session(
        &self,
        a_self: &Arc<Self>,
        peer: &PeerId,
        whatami: &WhatAmI,
        lease: ZInt,
        sn_resolution: ZInt,
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
    ) -> Session {
        loop {
            match self.get_session(peer).await {
                Ok(session) => return session,
                Err(_) => match self
                    .new_session(
                        a_self,
                        peer,
                        whatami,
                        lease,
                        sn_resolution,
                        initial_sn_tx,
                        initial_sn_rx,
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
        match zasyncwrite!(self.sessions).remove(peer) {
            Some(_) => Ok(()),
            None => {
                let e = format!("Can not delete the session of peer: {}", peer);
                log::trace!("{}", e);
                zerror!(ZErrorKind::Other { descr: e })
            }
        }
    }

    pub(super) async fn get_session(&self, peer: &PeerId) -> ZResult<Session> {
        match zasyncread!(self.sessions).get(peer) {
            Some(channel) => Ok(Session::new(Arc::downgrade(&channel))),
            None => {
                let e = format!("Can not get the session of peer: {}", peer);
                log::trace!("{}", e);
                zerror!(ZErrorKind::Other { descr: e })
            }
        }
    }

    pub(super) async fn get_sessions(&self) -> Vec<Session> {
        zasyncread!(self.sessions)
            .values()
            .map(|x| Session::new(Arc::downgrade(&x)))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn new_session(
        &self,
        a_self: &Arc<Self>,
        peer: &PeerId,
        whatami: &WhatAmI,
        lease: ZInt,
        sn_resolution: ZInt,
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
    ) -> ZResult<Session> {
        let mut w_guard = zasyncwrite!(self.sessions);
        if w_guard.contains_key(peer) {
            let e = format!("Can not create a new session for peer: {}", peer);
            log::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        // Compute a suitable keep alive interval based on the lease
        // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
        //       set the actual keep_alive timeout to one fourth of the agreed session lease.
        //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
        //       check which considers a link as failed when no messages are received in 3.5 times the
        //       target interval.
        let interval = lease / 4;
        let keep_alive = self.config.keep_alive.min(interval);

        // Create the channel object
        let a_ch = Arc::new(Channel::new(
            a_self.clone(),
            peer.clone(),
            *whatami,
            lease,
            keep_alive,
            sn_resolution,
            initial_sn_tx,
            initial_sn_rx,
            self.config.batch_size,
        ));
        // Start the channel
        a_ch.initialize(Arc::downgrade(&a_ch)).await;

        // Create a weak reference to the session
        let session = Session::new(Arc::downgrade(&a_ch));
        // Add the session to the list of active sessions
        w_guard.insert(peer.clone(), a_ch);

        log::debug!(
            "New session opened with {}: whatami {}, lease {:?}, keep alive {}, sn resolution {}, initial sn tx {}, initial sn rx {}",
            peer,
            whatami,
            Duration::from_millis(lease),
            keep_alive,
            sn_resolution,
            initial_sn_tx,
            initial_sn_rx
        );

        Ok(session)
    }
}

/*************************************/
/*              SESSION              */
/*************************************/
const STR_ERR: &str = "Session not available";

/// [`Session`] is the session handler returned when opening a new session
#[derive(Clone)]
pub struct Session(Weak<Channel>);

impl Session {
    fn new(inner: Weak<Channel>) -> Self {
        Self(inner)
    }

    /*************************************/
    /*         SESSION ACCESSORS         */
    /*************************************/
    #[inline]
    pub(super) fn get_transport(&self) -> ZResult<Transport> {
        let channel = zweak!(self.0, STR_ERR);
        Ok(Transport::new(channel))
    }

    #[inline]
    pub(super) async fn add_link(&self, link: Link) -> ZResult<()> {
        let channel = zweak!(self.0, STR_ERR);
        channel.add_link(link).await?;
        Ok(())
    }

    #[inline]
    pub(super) async fn _del_link(&self, link: &Link) -> ZResult<()> {
        let channel = zweak!(self.0, STR_ERR);
        channel.del_link(&link).await?;
        Ok(())
    }

    #[inline]
    pub(super) async fn get_callback(
        &self,
    ) -> ZResult<Option<Arc<dyn SessionEventHandler + Send + Sync>>> {
        let channel = zweak!(self.0, STR_ERR);
        let callback = channel.get_callback().await;
        Ok(callback)
    }

    #[inline]
    pub(super) async fn set_callback(
        &self,
        callback: Arc<dyn SessionEventHandler + Send + Sync>,
    ) -> ZResult<()> {
        let channel = zweak!(self.0, STR_ERR);
        channel.set_callback(callback).await;
        Ok(())
    }

    /*************************************/
    /*          PUBLIC ACCESSORS         */
    /*************************************/
    #[inline]
    pub fn get_pid(&self) -> ZResult<PeerId> {
        let channel = zweak!(self.0, STR_ERR);
        Ok(channel.get_pid())
    }

    #[inline]
    pub fn get_whatami(&self) -> ZResult<WhatAmI> {
        let channel = zweak!(self.0, STR_ERR);
        Ok(channel.get_whatami())
    }

    #[inline]
    pub fn get_lease(&self) -> ZResult<ZInt> {
        let channel = zweak!(self.0, STR_ERR);
        Ok(channel.get_lease())
    }

    #[inline]
    pub fn get_sn_resolution(&self) -> ZResult<ZInt> {
        let channel = zweak!(self.0, STR_ERR);
        Ok(channel.get_sn_resolution())
    }

    #[inline]
    pub async fn close(&self) -> ZResult<()> {
        log::trace!("{:?}. Close", self);
        let channel = zweak!(self.0, STR_ERR);
        channel.close(smsg::close_reason::GENERIC).await
    }

    #[inline]
    pub async fn close_link(&self, link: &Link) -> ZResult<()> {
        let channel = zweak!(self.0, STR_ERR);
        channel
            .close_link(link, smsg::close_reason::GENERIC)
            .await?;
        Ok(())
    }

    #[inline]
    pub async fn get_links(&self) -> ZResult<Vec<Link>> {
        log::trace!("{:?}. Get links", self);
        let channel = zweak!(self.0, STR_ERR);
        Ok(channel.get_links().await)
    }

    #[inline]
    pub async fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        log::trace!("{:?}. Schedule: {:?}", self, message);
        let channel = zweak!(self.0, STR_ERR);
        channel.schedule(message).await;
        Ok(())
    }
}

#[async_trait]
impl SessionEventHandler for Session {
    #[inline]
    async fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.schedule(message).await
    }

    #[inline]
    async fn new_link(&self, _link: Link) {}

    #[inline]
    async fn del_link(&self, _link: Link) {}

    #[inline]
    async fn closing(&self) {}

    #[inline]
    async fn closed(&self) {}
}

impl Eq for Session {}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(channel) = self.0.upgrade() {
            f.debug_struct("Session")
                .field("peer", &channel.get_pid())
                .field("lease", &channel.get_lease())
                .field("keep_alive", &channel.get_keep_alive())
                .field("sn_resolution", &channel.get_sn_resolution())
                .finish()
        } else {
            write!(f, "Session closed")
        }
    }
}
