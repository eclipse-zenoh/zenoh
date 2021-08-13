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
use super::authenticator::*;
use super::core::{PeerId, WhatAmI, ZInt};
use super::defaults::*;
use super::session::SessionManager;
use super::transport::{SessionTransportUnicast, SessionTransportUnicastConfig};
use super::*;
use crate::net::protocol::link::*;
use async_std::prelude::*;
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex};
use async_std::task;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::ConfigProperties;
use zenoh_util::properties::config::*;
use zenoh_util::{zasynclock, zerror, zlock};

pub struct SessionManagerConfigUnicast {
    pub lease: ZInt,
    pub keep_alive: ZInt,
    pub open_timeout: ZInt,
    pub open_incoming_pending: usize,
    pub max_sessions: usize,
    pub max_links: usize,
    pub peer_authenticator: Vec<PeerAuthenticator>,
    pub link_authenticator: Vec<LinkAuthenticator>,
}

impl Default for SessionManagerConfigUnicast {
    fn default() -> SessionManagerConfigUnicast {
        SessionManagerConfigUnicast {
            lease: *ZN_LINK_LEASE,
            keep_alive: *ZN_LINK_KEEP_ALIVE,
            open_timeout: *ZN_OPEN_TIMEOUT,
            open_incoming_pending: *ZN_OPEN_INCOMING_PENDING,
            max_sessions: usize::MAX,
            max_links: usize::MAX,
            peer_authenticator: vec![DummyPeerAuthenticator::make()],
            link_authenticator: vec![DummyLinkAuthenticator::make()],
        }
    }
}

impl SessionManagerConfigUnicast {
    pub async fn from_properties(
        properties: &ConfigProperties,
    ) -> ZResult<SessionManagerConfigUnicast> {
        macro_rules! zparse {
            ($str:expr) => {
                $str.parse().map_err(|_| {
                    let e = format!(
                        "Failed to read configuration: {} is not a valid value",
                        $str
                    );
                    log::warn!("{}", e);
                    zerror2!(ZErrorKind::ValueDecodingFailed { descr: e })
                })
            };
        }

        let mut config = SessionManagerConfigUnicast::default();

        if let Some(v) = properties.get(&ZN_LINK_LEASE_KEY) {
            config.lease = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_LINK_KEEP_ALIVE_KEY) {
            config.keep_alive = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_OPEN_TIMEOUT_KEY) {
            config.open_timeout = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_OPEN_INCOMING_PENDING_KEY) {
            config.open_incoming_pending = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_MAX_SESSIONS_KEY) {
            config.max_sessions = zparse!(v)?;
        }
        if let Some(v) = properties.get(&ZN_MAX_LINKS_KEY) {
            config.max_links = zparse!(v)?;
        }

        config.peer_authenticator = PeerAuthenticator::from_properties(properties).await?;
        config.link_authenticator = LinkAuthenticator::from_properties(properties).await?;

        Ok(config)
    }
}

pub struct SessionManagerStateUnicast {
    // Outgoing and incoming opened (i.e. established) sessions
    pub(super) opened: AsyncArc<AsyncMutex<HashMap<PeerId, Opened>>>,
    // Incoming uninitialized sessions
    pub(super) incoming: AsyncArc<AsyncMutex<HashMap<Link, Option<Vec<u8>>>>>,
    // Established listeners
    pub(super) protocols: Arc<Mutex<HashMap<LocatorProtocol, LinkManagerUnicast>>>,
    // Established sessions
    pub(super) sessions: Arc<Mutex<HashMap<PeerId, Arc<SessionTransportUnicast>>>>,
}

impl Default for SessionManagerStateUnicast {
    fn default() -> SessionManagerStateUnicast {
        SessionManagerStateUnicast {
            opened: AsyncArc::new(AsyncMutex::new(HashMap::new())),
            incoming: AsyncArc::new(AsyncMutex::new(HashMap::new())),
            protocols: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub(super) struct Opened {
    pub(super) whatami: WhatAmI,
    pub(super) sn_resolution: ZInt,
    pub(super) initial_sn: ZInt,
}

impl SessionManager {
    /*************************************/
    /*            LINK MANAGER           */
    /*************************************/
    async fn get_or_new_link_manager_unicast(
        &self,
        protocol: &LocatorProtocol,
    ) -> LinkManagerUnicast {
        loop {
            match self.get_link_manager_unicast(protocol) {
                Ok(manager) => return manager,
                Err(_) => match self.new_link_manager_unicast(protocol).await {
                    Ok(manager) => return manager,
                    Err(_) => continue,
                },
            }
        }
    }

    async fn new_link_manager_unicast(
        &self,
        protocol: &LocatorProtocol,
    ) -> ZResult<LinkManagerUnicast> {
        let mut w_guard = zlock!(self.state.unicast.protocols);
        if w_guard.contains_key(protocol) {
            return zerror!(ZErrorKind::Other {
                descr: format!(
                    "Can not create the link manager for protocol ({}) because it already exists",
                    protocol
                )
            });
        }

        let lm = LinkManagerBuilderUnicast::make(self.clone(), protocol)?;
        w_guard.insert(protocol.clone(), lm.clone());
        Ok(lm)
    }

    fn get_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<LinkManagerUnicast> {
        match zlock!(self.state.unicast.protocols).get(protocol) {
            Some(manager) => Ok(manager.clone()),
            None => zerror!(ZErrorKind::Other {
                descr: format!(
                    "Can not get the link manager for protocol ({}) because it has not been found",
                    protocol
                )
            }),
        }
    }

    async fn del_link_manager_unicast(&self, protocol: &LocatorProtocol) -> ZResult<()> {
        match zlock!(self.state.unicast.protocols).remove(protocol) {
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
    /*              LISTENER             */
    /*************************************/
    pub async fn add_listener_unicast(&self, locator: &Locator) -> ZResult<Locator> {
        let manager = self
            .get_or_new_link_manager_unicast(&locator.get_proto())
            .await;
        let ps = self.config.locator_property.get(&locator.get_proto());
        manager.new_listener(locator, ps).await
    }

    pub async fn del_listener_unicast(&self, locator: &Locator) -> ZResult<()> {
        let lm = self.get_link_manager_unicast(&locator.get_proto())?;
        lm.del_listener(locator).await?;
        if lm.get_listeners().is_empty() {
            self.del_link_manager_unicast(&locator.get_proto()).await?;
        }
        Ok(())
    }

    pub fn get_listeners_unicast(&self) -> Vec<Locator> {
        let mut vec: Vec<Locator> = vec![];
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
    /*              SESSION              */
    /*************************************/
    pub(super) fn init_session_unicast(
        &self,
        config: SessionConfigUnicast,
    ) -> ZResult<SessionUnicast> {
        let mut guard = zlock!(self.state.unicast.sessions);

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

            if session.is_shm != config.is_shm {
                let e = format!(
                    "Session with peer {} already exist. Invalid is_shm: {}. Execpted: {}.",
                    config.peer, config.is_shm, session.is_shm
                );
                log::trace!("{}", e);
                return zerror!(ZErrorKind::Other { descr: e });
            }

            return Ok(session.into());
        }

        // Then verify that we haven't reached the session number limit
        if guard.len() >= self.config.unicast.max_sessions {
            let e = format!(
                "Max sessions reached ({}). Denying new session with peer: {}",
                self.config.unicast.max_sessions, config.peer
            );
            log::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        // Create the session transport
        let stc = SessionTransportUnicastConfig {
            manager: self.clone(),
            pid: config.peer.clone(),
            whatami: config.whatami,
            sn_resolution: config.sn_resolution,
            initial_sn_tx: config.initial_sn_tx,
            initial_sn_rx: config.initial_sn_rx,
            is_shm: config.is_shm,
            is_qos: config.is_qos,
        };
        let a_st = Arc::new(SessionTransportUnicast::new(stc));

        // Create a weak reference to the session transport
        let session: SessionUnicast = (&a_st).into();
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

    pub async fn open_session_unicast(&self, locator: &Locator) -> ZResult<SessionUnicast> {
        // Automatically create a new link manager for the protocol if it does not exist
        let manager = self
            .get_or_new_link_manager_unicast(&locator.get_proto())
            .await;
        let ps = self.config.locator_property.get(&locator.get_proto());
        // Create a new link associated by calling the Link Manager
        let link = manager.new_link(locator, ps).await?;
        // Open the link
        super::establishment::open_link(self, &link).await
    }

    pub fn get_session_unicast(&self, peer: &PeerId) -> Option<SessionUnicast> {
        zlock!(self.state.unicast.sessions)
            .get(peer)
            .map(|t| t.into())
    }

    pub fn get_sessions_unicast(&self) -> Vec<SessionUnicast> {
        zlock!(self.state.unicast.sessions)
            .values()
            .map(|t| t.into())
            .collect()
    }

    pub(super) async fn del_session_unicast(&self, peer: &PeerId) -> ZResult<()> {
        let _ = zlock!(self.state.unicast.sessions)
            .remove(peer)
            .ok_or_else(|| {
                let e = format!("Can not delete the session of peer: {}", peer);
                log::trace!("{}", e);
                zerror2!(ZErrorKind::Other { descr: e })
            })?;

        for pa in self.config.unicast.peer_authenticator.iter() {
            pa.handle_close(peer).await;
        }
        Ok(())
    }

    pub(crate) async fn handle_new_link_unicast(
        &self,
        link: Link,
        properties: Option<LocatorProperty>,
    ) {
        let mut guard = zasynclock!(self.state.unicast.incoming);
        if guard.len() >= self.config.unicast.open_incoming_pending {
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
        for la in self.config.unicast.link_authenticator.iter() {
            let res = la.handle_new_link(&link, properties.as_ref()).await;
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
                properties,
            };

            let timeout = Duration::from_millis(c_manager.config.unicast.open_timeout);
            let res = super::establishment::accept_link(&c_manager, &link, &auth_link)
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
