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
use super::defaults::{
    SESSION_BATCH_SIZE, SESSION_KEEP_ALIVE, SESSION_LEASE, SESSION_OPEN_MAX_CONCURRENT,
    SESSION_OPEN_RETRIES, SESSION_OPEN_TIMEOUT, SESSION_SEQ_NUM_RESOLUTION,
};
use super::transport::SessionTransport;
use super::Session;
use super::SessionHandler;
use crate::core::{PeerId, WhatAmI, ZInt};
use crate::io::{RBuf, WBuf};
use crate::link::{Link, LinkManager, LinkManagerBuilder, Locator, LocatorProtocol};
use crate::proto::{
    smsg, Attachment, InitAck, InitSyn, OpenAck, OpenSyn, SessionBody, SessionMessage,
};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zerror};

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

struct SessionManagerConfigInner {
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

struct Opened {
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn: ZInt,
}

#[derive(Clone)]
pub struct SessionManager {
    config: Arc<SessionManagerConfigInner>,
    // Established sessions
    protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManager>>>,
    // Established sessions
    sessions: Arc<Mutex<HashMap<PeerId, Arc<SessionTransport>>>>,
    // Outgoing and incoming opened (i.e. established) sessions
    opened: Arc<Mutex<HashMap<PeerId, Opened>>>,
    // Incoming uninitialized sessions
    incoming: Arc<Mutex<HashSet<Link>>>,
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
            handler: config.handler,
        };

        SessionManager {
            config: Arc::new(config_inner),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            opened: Arc::new(Mutex::new(HashMap::new())),
            incoming: Arc::new(Mutex::new(HashSet::new())),
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
        manager.new_listener(locator).await
    }

    #[allow(unreachable_patterns)]
    pub async fn get_listeners(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = Vec::new();
        for p in zasynclock!(self.protocols).values() {
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

    pub async fn get_locators(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = Vec::new();
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
    ) -> Session {
        loop {
            match self.get_session(peer).await {
                Some(session) => return session,
                None => match self
                    .new_session(peer, whatami, sn_resolution, initial_sn_tx, initial_sn_rx)
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
            Some(_) => Ok(()),
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
        ));

        // Create a weak reference to the session
        let session = Session::new(Arc::downgrade(&a_ch));
        // Add the session to the list of active sessions
        w_guard.insert(peer.clone(), a_ch);

        log::debug!(
            "New session opened with {}: whatami {}, sn resolution {}, initial sn tx {}, initial sn rx {}",
            peer,
            whatami,
            sn_resolution,
            initial_sn_tx,
            initial_sn_rx
        );

        Ok(session)
    }

    pub async fn open_session(
        &self,
        locator: &Locator,
        attachment: &Option<Attachment>,
    ) -> ZResult<Session> {
        // Create the timeout duration
        let to = Duration::from_millis(self.config.timeout);
        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self.get_or_new_link_manager(&locator.get_proto()).await;
        // Create a new link associated by calling the Link Manager
        let link = match manager.new_link(&locator).await {
            Ok(link) => link,
            Err(e) => {
                log::warn!("Can not to create a link to locator {}: {}", locator, e);
                return Err(e);
            }
        };

        // Try a maximum number of times to open a session
        let retries = self.config.retries;
        for i in 0..retries {
            // Check the future result
            match self.open_link(&link, attachment).timeout(to).await {
                Ok(res) => match res {
                    Ok(session) => return Ok(session),
                    Err(e) => {
                        let _ = link.close().await;
                        return Err(e);
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

        let e = format!(
            "Can not open a session to {}: maximum number of attemps reached ({})",
            locator, retries
        );
        log::warn!("{}", e);
        zerror!(ZErrorKind::Other { descr: e })
    }

    pub(super) async fn open_link(
        &self,
        link: &Link,
        attachment: &Option<Attachment>,
    ) -> ZResult<Session> {
        // Build and send an InitSyn Message
        let initsyn_version = self.config.version;
        let initsyn_whatami = self.config.whatami;
        let initsyn_pid = self.config.pid.clone();
        let initsyn_sn_resolution = if self.config.sn_resolution == *SESSION_SEQ_NUM_RESOLUTION {
            None
        } else {
            Some(self.config.sn_resolution)
        };

        // Build and send the InitSyn message
        let message = SessionMessage::make_init_syn(
            initsyn_version,
            initsyn_whatami,
            initsyn_pid,
            initsyn_sn_resolution,
            attachment.clone(),
        );
        let _ = link.write_session_message(message).await?;

        // Wait to read an InitAck
        let mut messages = link.read_session_message().await?;
        if messages.len() != 1 {
            let e = format!(
                "Received multiple messages in response to an InitSyn on link: {:?}",
                link,
            );
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }

        let msg = messages.remove(0);
        let (initack_whatami, initack_pid, initack_sn_resolution, initack_cookie) = match msg.body {
            SessionBody::InitAck(InitAck {
                whatami,
                pid,
                sn_resolution,
                cookie,
            }) => (whatami, pid, sn_resolution, cookie),
            _ => {
                // Build and send a Close message
                let peer_id = Some(self.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = false; // This is should always be true for invalid lease on existing session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                let _ = link.write_session_message(message).await;
                let e = format!(
                    "Received an invalid message in response to an InitSyn on link: {:?}",
                    link,
                );
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        };

        // Check if a session is already open with the target peer
        let mut guard = zasynclock!(self.opened);
        let (sn_resolution, initial_sn_tx) = if let Some(s) = guard.get(&initack_pid) {
            if let Some(sn_resolution) = initack_sn_resolution {
                if sn_resolution != s.sn_resolution {
                    // Build and send a Close message
                    let peer_id = Some(self.config.pid.clone());
                    let reason_id = smsg::close_reason::INVALID;
                    let link_only = false;
                    let attachment = None;
                    let message =
                        SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                    let _ = link.write_session_message(message).await;

                    let e = format!(
                        "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                        link, initack_pid
                    );
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            }
            (s.sn_resolution, s.initial_sn)
        } else {
            let sn_resolution = match initack_sn_resolution {
                Some(sn_resolution) => {
                    if sn_resolution > self.config.sn_resolution {
                        // Build and send a Close message
                        let peer_id = Some(self.config.pid.clone());
                        let reason_id = smsg::close_reason::INVALID;
                        let link_only = true;
                        let attachment = None;
                        let message =
                            SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                        let _ = link.write_session_message(message).await;

                        let e = format!(
                            "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                            link, initack_pid
                        );
                        return zerror!(ZErrorKind::InvalidMessage { descr: e });
                    }
                    sn_resolution
                }
                None => self.config.sn_resolution,
            };
            let initial_sn_tx = {
                let mut rng = rand::thread_rng();
                rng.gen_range(0, sn_resolution)
            };

            // Store the data
            guard.insert(
                initack_pid.clone(),
                Opened {
                    whatami: initack_whatami,
                    sn_resolution,
                    initial_sn: initial_sn_tx,
                },
            );

            (sn_resolution, initial_sn_tx)
        };
        drop(guard);

        // Build and send an OpenSyn message
        let lease = self.config.lease;
        let attachment = None;
        let message =
            SessionMessage::make_open_syn(lease, initial_sn_tx, initack_cookie, attachment);
        let _ = link.write_session_message(message).await?;

        // Wait to read an OpenAck
        let mut messages = link.read_session_message().await?;
        if messages.len() != 1 {
            let e = format!(
                "Received multiple messages in response to an InitSyn on link: {:?}",
                link,
            );
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }

        let msg = messages.remove(0);
        let (lease, initial_sn_rx) = match msg.body {
            SessionBody::OpenAck(OpenAck { lease, initial_sn }) => (lease, initial_sn),
            invalid => {
                // Build and send a Close message
                let peer_id = Some(self.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true;
                let attachment = None;
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                let _ = link.write_session_message(message).await;

                let e = format!(
                    "Received an invalid message in response to an OpenSyn on link {}: {:?}",
                    link, invalid
                );
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        };

        let session = self
            .get_or_new_session(
                &initack_pid,
                &initack_whatami,
                sn_resolution,
                initial_sn_tx,
                initial_sn_rx,
            )
            .await;

        // Retrive the session's transport
        let transport = session.get_transport().await?;

        // Compute a suitable keep alive interval based on the lease
        // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
        //       set the actual keep_alive timeout to one fourth of the agreed session lease.
        //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
        //       check which considers a link as failed when no messages are received in 3.5 times the
        //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
        //       session lease.
        let keep_alive = self.config.keep_alive.min(lease / 4);
        let _ = transport
            .add_link(link.clone(), self.config.batch_size, lease, keep_alive)
            .await?;

        // Start the TX loop
        let _ = transport.start_tx(&link).await?;

        // Assign a callback if the session is new
        if let Some(callback) = transport.get_callback().await {
            // Notify the session handler there is a new link on this session
            callback.new_link(link.clone()).await;
        } else {
            // Notify the session handler that there is a new session and get back a callback
            // NOTE: the read loop of the link the open message was sent on remains blocked
            //       until the new_session() returns. The read_loop in the various links
            //       waits for any eventual transport to associate to.
            let callback = self.config.handler.new_session(session.clone()).await?;
            // Set the callback on the transport
            let _ = transport.set_callback(callback).await;
        }

        // Start the RX loop
        let _ = transport.start_rx(&link).await?;

        let mut guard = zasynclock!(self.opened);
        guard.remove(&initack_pid);
        drop(guard);

        Ok(session)
    }

    pub(crate) async fn handle_new_link(&self, link: Link) {
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

        let c_incoming = self.incoming.clone();
        let c_manager = self.clone();
        task::spawn(async move {
            let to = Duration::from_millis(*SESSION_OPEN_TIMEOUT);
            let res = incoming_link_task(&link, c_manager).timeout(to).await;
            if let Err(e) = res {
                log::debug!("{}", e);
                let _ = link.close().await;
            }
            let mut guard = zasynclock!(c_incoming);
            guard.remove(&link);
        });
    }
}

/*************************************/
/*             COOKIE                */
/*************************************/
struct Cookie {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    src: Locator,
    dst: Locator,
}

impl WBuf {
    fn write_cookie(&mut self, cookie: &Cookie) -> bool {
        zcheck!(self.write_zint(cookie.whatami));
        zcheck!(self.write_peerid(&cookie.pid));
        zcheck!(self.write_zint(cookie.sn_resolution));
        zcheck!(self.write_string(&cookie.src.to_string()));
        zcheck!(self.write_string(&cookie.dst.to_string()));
        true
    }
}

impl RBuf {
    fn read_cookie(&mut self) -> Option<Cookie> {
        let whatami = self.read_zint()?;
        let pid = self.read_peerid()?;
        let sn_resolution = self.read_zint()?;
        let src: Locator = self.read_locator()?;
        let dst: Locator = self.read_locator()?;

        Some(Cookie {
            whatami,
            pid,
            sn_resolution,
            src,
            dst,
        })
    }
}

async fn incoming_link_task(link: &Link, manager: SessionManager) -> ZResult<()> {
    /*************************************/
    /*             InitSyn               */
    /*************************************/
    // Wait to read an InitSyn
    let mut messages = link.read_session_message().await?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages instead of a single InitSyn on link: {:?}",
            link,
        );
        return zerror!(ZErrorKind::InvalidMessage { descr: e });
    }

    let msg = messages.remove(0);
    let (syn_version, syn_whatami, syn_pid, syn_sn_resolution) = match msg.body {
        SessionBody::InitSyn(InitSyn {
            version,
            whatami,
            pid,
            sn_resolution,
        }) => (version, whatami, pid, sn_resolution),

        _ => {
            let e = format!(
                "Received invalid message instead of an InitSyn on link: {:?}",
                link,
            );
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }
    };

    // Check if we are allowed to open more links if the session is established
    if let Some(s) = manager.get_session(&syn_pid).await {
        // Check if we have reached maximum number of links for this session
        if let Some(limit) = manager.config.max_links {
            let links = s.get_links().await?;
            if links.len() >= limit {
                // Build and send a close message
                let peer_id = Some(manager.config.pid.clone());
                let reason_id = smsg::close_reason::MAX_LINKS;
                let link_only = true;
                let attachment = None;
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                let _ = link.write_session_message(message).await;

                let e = format!(
                    "Rejecting Open on link {} because of maximum links limit reached for peer: {}",
                    link, syn_pid
                );
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        }
    }

    // Check if the version is supported
    if syn_version > manager.config.version {
        // Send a close message
        let c_pid = Some(manager.config.pid.clone());
        let reason_id = smsg::close_reason::UNSUPPORTED;
        let link_only = false; // This is should always be false for invalid version
        let attachment = None; // Parameter of open_session
        let message = SessionMessage::make_close(c_pid, reason_id, link_only, attachment);

        // Send the message on the link
        let _ = link.write_session_message(message).await;
        let e = format!(
            "Rejecting InitSyn on link {} because of unsupported Zenoh version from peer: {}",
            link, syn_pid
        );
        return zerror!(ZErrorKind::InvalidMessage { descr: e });
    }

    // Get the SN Resolution
    let syn_sn_resolution = if let Some(snr) = syn_sn_resolution {
        snr
    } else {
        *SESSION_SEQ_NUM_RESOLUTION
    };

    // Compute the minimum SN Resolution
    let agreed_sn_resolution = manager.config.sn_resolution.min(syn_sn_resolution);

    // Create and encode the cookie
    let mut wbuf = WBuf::new(64, false);
    let cookie = Cookie {
        whatami: syn_whatami,
        pid: syn_pid.clone(),
        sn_resolution: agreed_sn_resolution,
        src: link.get_src(),
        dst: link.get_dst(),
    };
    wbuf.write_cookie(&cookie);

    // Build the fields for the InitAck message
    let whatami = manager.config.whatami;
    let apid = manager.config.pid.clone();
    let sn_resolution = if agreed_sn_resolution == syn_sn_resolution {
        None
    } else {
        Some(agreed_sn_resolution)
    };
    let cookie = RBuf::from(wbuf); // @TODO: use HMAC to sign the cookie
    let attachment = None;
    let message = SessionMessage::make_init_ack(whatami, apid, sn_resolution, cookie, attachment);

    // Send the message on the link
    let _ = link.write_session_message(message).await?;

    /*************************************/
    /*             OpenSyn               */
    /*************************************/
    // Wait to read an OpenSyn
    let mut messages = link.read_session_message().await?;
    if messages.len() != 1 {
        let e = format!(
            "Received multiple messages instead of a single OpenSyn on link: {:?}",
            link,
        );
        return zerror!(ZErrorKind::InvalidMessage { descr: e });
    }

    let msg = messages.remove(0);
    let (syn_lease, syn_initial_sn, mut syn_cookie) = match msg.body {
        SessionBody::OpenSyn(OpenSyn {
            lease,
            initial_sn,
            cookie,
        }) => (lease, initial_sn, cookie),
        invalid => {
            let e = format!(
                "Received invalid message instead of an OpenSyn on link {}: {:?}",
                link, invalid
            );
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }
    };

    let cookie = if let Some(cookie) = syn_cookie.read_cookie() {
        // @TODO: verify cookie with HMAC
        cookie
    } else {
        // Build and send a Close message
        let peer_id = Some(manager.config.pid.clone());
        let reason_id = smsg::close_reason::INVALID;
        let link_only = true;
        let attachment = None;
        let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
        let _ = link.write_session_message(message).await;

        let e = format!(
            "Rejecting OpenSyn on link {} because of invalid cookie",
            link,
        );
        return zerror!(ZErrorKind::InvalidMessage { descr: e });
    };

    // Initialize the session if it is new
    let mut guard = zasynclock!(manager.opened);
    let ack_initial_sn = if let Some(opened) = guard.get(&cookie.pid) {
        if opened.whatami != cookie.whatami || opened.sn_resolution != cookie.sn_resolution {
            drop(guard);
            // Build and send a Close message
            let peer_id = Some(manager.config.pid.clone());
            let reason_id = smsg::close_reason::INVALID;
            let link_only = true; // This is should always be true for invalid lease on existing session
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
            let _ = link.write_session_message(message).await;

            let e = format!(
                "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                link, cookie.pid
            );
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }
        opened.initial_sn
    } else {
        let initial_sn = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0, cookie.sn_resolution)
        };
        guard.insert(
            cookie.pid.clone(),
            Opened {
                whatami: cookie.whatami,
                sn_resolution: cookie.sn_resolution,
                initial_sn,
            },
        );
        initial_sn
    };
    drop(guard);

    let session = loop {
        // Check if this open is related to a totally new session (i.e. new peer) or to an exsiting one
        if let Some(s) = manager.get_session(&cookie.pid).await {
            // Check if we have reached maximum number of links for this session
            if let Some(limit) = manager.config.max_links {
                let links = s.get_links().await?;
                if links.len() >= limit {
                    // Send a close message
                    let peer_id = Some(manager.config.pid.clone());
                    let reason_id = smsg::close_reason::MAX_LINKS;
                    let link_only = true; // This is should always be true when the link limit is reached
                    let attachment = None; // Parameter of open_session
                    let message =
                        SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                    // Send the message on the link
                    let _ = link.write_session_message(message).await;
                    let e = format!("Rejecting Open on link {} because of maximum links limit reached for peer: {}", link, cookie.pid);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            }

            // Check if the sn_resolution is valid (i.e. the same of existing session)
            let snr = s.get_sn_resolution()?;
            if cookie.sn_resolution != snr {
                // Send a close message
                let peer_id = Some(manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true; // This is should always be true for invalid sn_resolution on exisisting session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = link.write_session_message(message).await;
                let e = format!("Rejecting Open on link {} because of invalid sequence number resolution on already existing\
                                    session with peer: {}", link, cookie.pid);
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }

            break s;
        } else {
            // Check if a limit for the maximum number of open sessions is set
            if let Some(limit) = manager.config.max_sessions {
                let num = manager.get_sessions().await.len();
                // Check if we have reached the session limit
                if num >= limit {
                    // Send a close message
                    let peer_id = Some(manager.config.pid.clone());
                    let reason_id = smsg::close_reason::MAX_SESSIONS;
                    let link_only = false; // This is should always be false when the session limit is reached
                    let attachment = None; // Parameter of open_session
                    let message =
                        SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                    // Send the message on the link
                    let _ = link.write_session_message(message).await;
                    let e = format!("Rejecting Open on link {} because of maximum sessions limit reached for peer: {}", link, cookie.pid);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            }

            log::debug!("OpenSyn max sessions is valid on link: {}", link);

            // Create a new session
            let res = manager
                .new_session(
                    &cookie.pid,
                    &cookie.whatami,
                    cookie.sn_resolution,
                    ack_initial_sn,
                    syn_initial_sn,
                )
                .await;

            if let Ok(s) = res {
                break s;
            }

            // Concurrency just occured: multiple Open Messages have simultanesouly arrived from different links.
            // Restart from the beginning to check if the Open Messages have compatible parameters
            log::trace!("Multiple Open messages have simultanesouly arrived from different links for peer: {}.\
                             Rechecking validity of Open message recevied on link: {}", cookie.pid, link);
        }
    };

    // Build OpenAck message
    let attachment = None;
    let message = SessionMessage::make_open_ack(manager.config.lease, ack_initial_sn, attachment);

    // Send the message on the link
    let _ = link.write_session_message(message).await?;

    // Retrive the session's transport
    let transport = session.get_transport().await?;

    // Add the link to the session
    // Compute a suitable keep alive interval based on the lease
    // NOTE: In order to consider eventual packet loss and transmission latency and jitter,
    //       set the actual keep_alive timeout to one fourth of the agreed session lease.
    //       This is in-line with the ITU-T G.8013/Y.1731 specification on continous connectivity
    //       check which considers a link as failed when no messages are received in 3.5 times the
    //       target interval. For simplicity, we compute the keep_alive interval as 1/4 of the
    //       session lease.
    let keep_alive = manager.config.keep_alive.min(syn_lease / 4);
    let _ = transport
        .add_link(
            link.clone(),
            manager.config.batch_size,
            syn_lease,
            keep_alive,
        )
        .await?;

    // Start the TX loop
    let _ = transport.start_tx(&link).await?;

    // Assign a callback if the session is new
    if let Some(callback) = transport.get_callback().await {
        // Notify the session handler there is a new link on this session
        callback.new_link(link.clone()).await;
    } else {
        // Notify the session handler that there is a new session and get back a callback
        // NOTE: the read loop of the link the open message was sent on remains blocked
        //       until the new_session() returns. The read_loop in the various links
        //       waits for any eventual transport to associate to.
        let callback = manager.config.handler.new_session(session.clone()).await?;
        // Set the callback on the transport
        let _ = transport.set_callback(callback).await;
    }

    // Start the RX loop
    let _ = transport.start_rx(&link).await?;

    log::debug!("New session link established from {}: {}", cookie.pid, link);
    Ok(())
}
