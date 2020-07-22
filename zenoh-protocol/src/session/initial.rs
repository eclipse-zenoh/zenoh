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
use async_std::sync::{Arc, RwLock, Sender};
use async_trait::async_trait;
use rand::Rng;
use std::collections::HashMap;

use crate::core::{PeerId, ZInt, WhatAmI};
use crate::io::WBuf;
use crate::link::{Link, Locator};
use crate::proto::{Attachment, SessionMessage, SessionBody, smsg};
use crate::session::defaults::SESSION_SEQ_NUM_RESOLUTION;
use crate::session::{Action, Session, SessionManagerInner, TransportTrait};

use zenoh_util::{zasyncwrite, zerror};
use zenoh_util::core::{ZResult, ZError, ZErrorKind};


const DEFAULT_WBUF_CAPACITY: usize = 64;

// Macro to send a message on a link
macro_rules! zlinksend {
    ($msg:expr, $link:expr) => ({       
        // Create the buffer for serializing the message
        let mut wbuf = WBuf::new(DEFAULT_WBUF_CAPACITY, false);
        if $link.is_streamed() {
            // Reserve 16 bits to write the length
            wbuf.write_bytes(&[0u8, 0u8]); 
        }
        // Serialize the message
        wbuf.write_session_message(&$msg);
        if $link.is_streamed() {
            // Write the length on the first 16 bits
            let length: u16 = wbuf.len() as u16 - 2;            
            let bits = wbuf.get_first_slice_mut(..2);
            bits.copy_from_slice(&length.to_le_bytes());
        }
        let mut buffer = vec![0u8; wbuf.len()];
        wbuf.copy_into_slice(&mut buffer[..]);

        // Send the message on the link
        let res = $link.send(&buffer).await;
        log::trace!("Sending on {}: {:?}. {:?}", $link, $msg, res);

        res
    });
}

struct PendingOpen {
    lease: ZInt,
    initial_sn: ZInt,
    sn_resolution: ZInt,
    notify: Sender<ZResult<Session>>
}

impl PendingOpen {
    fn new(
        lease: ZInt,         
        sn_resolution: ZInt, 
        notify: Sender<ZResult<Session>>
    ) -> PendingOpen {
        let mut rng = rand::thread_rng();
        PendingOpen {
            lease,
            initial_sn: rng.gen_range(0, sn_resolution),
            sn_resolution,
            notify
        }
    }
}

pub(super) struct InitialSession {
    manager: Arc<SessionManagerInner>,
    pending: RwLock<HashMap<Link, PendingOpen>>
}

impl InitialSession {
    pub(super) fn new(manager: Arc<SessionManagerInner>) -> InitialSession {
        InitialSession {
            manager,
            pending: RwLock::new(HashMap::new())
        }
    }

    /*************************************/
    /*            OPEN/CLOSE             */
    /*************************************/
    pub(super) async fn open(&self, 
        link: &Link, 
        attachment: &Option<Attachment>,
        notify: &Sender<ZResult<Session>>
    ) -> ZResult<()> {
        let pending = PendingOpen::new(
            self.manager.config.lease, 
            self.manager.config.sn_resolution,
            notify.clone()
        );

        // Build the fields for the Open Message
        let version = self.manager.config.version;
        let whatami = self.manager.config.whatami;
        let pid = self.manager.config.pid.clone();
        let lease = pending.lease;
        let initial_sn = pending.initial_sn;
        let sn_resolution = if pending.sn_resolution == *SESSION_SEQ_NUM_RESOLUTION {
            None
        } else {
            Some(pending.sn_resolution)
        };
        let locators = self.manager.get_locators().await;
        let locators = match locators.len() {
            0 => None,
            _ => Some(locators),
        };        
        
        // Build the Open Message
        let message = SessionMessage::make_open(
            version, 
            whatami, 
            pid, 
            lease,
            initial_sn,
            sn_resolution,
            locators, 
            attachment.clone()
        );

        // Store the pending  for the callback to be used in the process_message
        let key = link.clone();
        zasyncwrite!(self.pending).insert(key, pending);

        // Send the message on the link
        zlinksend!(message, link)?;

        Ok(())
    }

    /*************************************/
    /*          PROCESS MESSAGES         */
    /*************************************/
    #[allow(clippy::too_many_arguments)]
    async fn process_open(
        &self,
        link: &Link,
        version: u8,
        whatami: WhatAmI,
        pid: PeerId,
        lease: ZInt,
        initial_sn: ZInt,
        sn_resolution: Option<ZInt>,        
        _locators: Option<Vec<Locator>>,
    ) -> Action {
        // @TODO: Handle the locators

        // Check if the version is supported
        if version > self.manager.config.version {
            // Send a close message
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::UNSUPPORTED;              
            let link_only = false;  // This is should always be false for invalid version                
            let attachment = None;  // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
        
            // Send the message on the link
            let _ = zlinksend!(message, link);
            log::warn!("Rejecting Open on link {} because of unsupported Zenoh version from peer: {}", link, pid);

            // Close the link
            return Action::Close
        }

        // Get the SN Resolution
        let sn_resolution = if let Some(snr) = sn_resolution {
            snr
        } else  {
            *SESSION_SEQ_NUM_RESOLUTION
        };

        let (session, agreed_lease, agreed_sn_resolution, agreed_initial_sn) = loop {
            // Check if this open is related to a totally new session (i.e. new peer) or to an exsiting one
            if let Ok(s) = self.manager.get_session(&pid).await {
                // Check if we have reached maximum number of links for this session
                if let Some(limit) = self.manager.config.max_links {                
                    if let Ok(links) = s.get_links().await {
                        if links.len() >= limit {
                            // Send a close message
                            let peer_id = Some(self.manager.config.pid.clone());
                            let reason_id = smsg::close_reason::MAX_LINKS;               
                            let link_only = true;  // This is should always be true when the link limit is reached                
                            let attachment = None;  // Parameter of open_session
                            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                            // Send the message on the link
                            let _ = zlinksend!(message, link);
                            log::warn!("Rejecting Open on link {} because of maximum links limit reached for peer: {}", link, pid);

                            // Close the link
                            return Action::Close
                        }
                    }
                }

                // Check if the lease is valid (i.e. the same of existing session)
                if let Ok(l) = s.get_lease() {
                    if lease != l {
                        // Send a close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::INVALID;               
                        let link_only = true;  // This is should always be true for invalid lease on existing session               
                        let attachment = None;  // Parameter of open_session
                        let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                        // Send the message on the link
                        let _ = zlinksend!(message, link);
                        log::warn!("Rejecting Open on link {} because of invalid lease on already existing session with peer: {}", link, pid);

                        // Close the link
                        return Action::Close
                    }
                }
                
                // Check if the sn_resolution is valid (i.e. the same of existing session)
                if let Ok(snr) = s.get_sn_resolution() {
                    if sn_resolution != snr {
                        // Send a close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::INVALID;               
                        let link_only = true;  // This is should always be true for invalid sn_resolution on exisisting session   
                        let attachment = None;  // Parameter of open_session
                        let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                        // Send the message on the link
                        let _ = zlinksend!(message, link);
                        log::warn!("Rejecting Open on link {} because of invalid sequence number resolution on already existing\
                                    session with peer: {}", link, pid);

                        // Close the link
                        return Action::Close
                    }
                }
            } else {
                // Check if a limit for the maximum number of open sessions is set
                if let Some(limit) = self.manager.config.max_sessions {
                    let num = self.manager.get_sessions().await.len();
                    // Check if we have reached the session limit
                    if num >= limit {
                        // Send a close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::MAX_SESSIONS;                
                        let link_only = false;  // This is should always be false when the session limit is reached                
                        let attachment = None;  // Parameter of open_session
                        let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                        // Send the message on the link
                        let _ = zlinksend!(message, link);
                        log::warn!("Rejecting Open on link {} because of maximum sessions limit reached for peer: {}", link, pid);

                        // Close the link
                        return Action::Close
                    }
                }
            }       
            
            // Compute the minimum SN Resolution 
            let agreed_sn_resolution = self.manager.config.sn_resolution.min(sn_resolution);
            let initial_sn_tx = {
                // @TODO: in case the session is already active, we should return the last SN
                let mut rng = rand::thread_rng();
                rng.gen_range(0, agreed_sn_resolution)
            };
            // Compute the Initial SN in reception 
            let initial_sn_rx = if agreed_sn_resolution < sn_resolution {
                initial_sn % agreed_sn_resolution
            } else {
                initial_sn
            };
            // Compute the minimum lease
            let agreed_lease = self.manager.config.lease.min(lease);

            // Get the session associated to the peer
            if let Ok(s) = self.manager.get_session(&pid).await {
                break (s, agreed_lease, agreed_sn_resolution, initial_sn_tx)
            } else {
                // Create a new session
                let res = self.manager.new_session(
                    &self.manager, &pid, &whatami, agreed_lease, agreed_sn_resolution, initial_sn_tx, initial_sn_rx
                ).await;

                if let Ok(s) = res {
                    break (s, agreed_lease, agreed_sn_resolution, initial_sn_tx)
                }

                // Concurrency just occured: multiple Open Messages have simultanesouly arrived from different links.
                // Restart from the beginning to check if the Open Messages have compatible parameters
                log::trace!("Multiple Open messages have simultanesouly arrived from different links for peer: {}.\
                             Rechecking validity of Open message recevied on link: {}", pid, link);
            }
        };

        // Add the link to the session
        if let Err(e) = session.add_link(link.clone()).await {
            log::warn!("Unable to add link {} to the session with peer {}: {}", link, pid, e);
            return Action::Close
        }

        // Build Accept message
        let opid = pid.clone();
        let apid = self.manager.config.pid.clone();
        let initial_sn = agreed_initial_sn;
        let sn_resolution = if agreed_sn_resolution != sn_resolution {
            Some(agreed_sn_resolution)
        } else {
            None
        };   
        let lease = if agreed_lease != lease {
            Some(agreed_lease)
        } else {
            None
        };
        let locators = {
            let mut locs = self.manager.get_locators().await;
            // Get link source
            let src = link.get_src();
            // Remove the source locator from the list of additional locators
            if let Some(index) = locs.iter().position(|x| x == &src) {
                locs.remove(index);
                if !locs.is_empty() {
                    Some(locs)
                } else {
                    None
                }
            } else {
                None
            }
        };
        let attachment = None;
        let message = SessionMessage::make_accept(self.manager.config.whatami, 
            opid, apid, initial_sn, sn_resolution, lease, locators, attachment);
        
        // Send the message on the link
        if let Err(e) = zlinksend!(message, link) {
            log::warn!("Unable to send Accept on link {} for peer {}: {}", link, pid, e);
            return Action::Close
        }

        // Assign a callback if the session is new
        let callback = match session.get_callback().await {
            Ok(callback) => callback,
            Err(e) => {
                log::warn!("Unable to get callback for peer {}: {}", pid, e);
                return Action::Close
            }
        };

        if let Some(callback) = callback {
            callback.new_link(link.clone()).await;
        } else {
            // Notify the session handler that there is a new session and get back a callback
            // NOTE: the read loop of the link the open message was sent on remains blocked
            //       until the new_session() returns. The read_loop in the various links
            //       waits for any eventual transport to associate to. This transport is
            //       returned only by the process_open() -- this function.
            let callback = match self.manager.config.handler.new_session(session.clone()).await {
                Ok(callback) => callback,
                Err(e) => {
                    log::warn!("Unable to get session event handler for peer {}: {}", pid, e);
                    return Action::Close 
                }
            };             
            // Set the callback on the transport
            if let Err(e) = session.set_callback(callback).await {
                log::warn!("Unable to set callback for peer {}: {}", pid, e);
                return Action::Close
            }
        }

        log::debug!("New session opened from: {}", pid);

        // Return the target transport to use in the link
        match session.get_transport() {
            Ok(transport) => Action::ChangeTransport(transport),
            Err(_) => {
                Action::Close
            }
        }   
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_accept(
        &self,
        link: &Link,
        whatami: WhatAmI,
        opid: PeerId,
        apid: PeerId,
        initial_sn: ZInt,
        sn_resolution: Option<ZInt>,
        lease: Option<ZInt>,
        _locators: Option<Vec<Locator>>
    ) -> Action {
        // @TODO: Handle the locators

        // Check if we had previously triggered the opening of a new connection
        let res = zasyncwrite!(self.pending).remove(link);
        if let Some(pending) = res {
            // Check if the opener peer of this accept was me
            if opid != self.manager.config.pid {
                let e = format!("Rejecting Accept with invalid Opener Peer Id: {}. Expected: {}", opid, self.manager.config.pid);
                log::debug!("{}", e);

                // Invalid value, send a Close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;              
                let link_only = false;  // This is should always be true for invalid opener ID                
                let attachment = None;  // No attachment here
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);

                // Notify
                let err = zerror!(ZErrorKind::InvalidMessage { 
                    descr: e
                });
                pending.notify.send(err).await;
                return Action::Close
            }

            // Get the agreed lease
            let lease = if let Some(l) = lease {
                if l <= pending.lease {
                    l
                } else {
                    let e = format!("Rejecting Accept with invalid Lease: {}. Expected to be at most: {}.", l, pending.lease);
                    log::debug!("{}", e);

                    // Invalid value, send a Close message
                    let peer_id = Some(self.manager.config.pid.clone());
                    let reason_id = smsg::close_reason::INVALID;              
                    let link_only = false;  // This is should always be true for invalid lease                
                    let attachment = None;  // No attachment here
                    let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                    // Send the message on the link
                    let _ = zlinksend!(message, link);

                    // Notify
                    let err = zerror!(ZErrorKind::InvalidMessage { 
                        descr: e
                    });
                    pending.notify.send(err).await;
                    return Action::Close
                }
            } else {
                pending.lease
            };

            // Get the agreed SN Resolution
            let sn_resolution = if let Some(r) = sn_resolution {
                if r <= pending.sn_resolution {
                    r
                } else {
                    let e = format!("Rejecting Accept with invalid SN Resolution: {}. Expected to be at most: {}.", r, pending.sn_resolution);
                    log::debug!("{}", e);

                    // Invalid value, send a Close message
                    let peer_id = Some(self.manager.config.pid.clone());
                    let reason_id = smsg::close_reason::INVALID;              
                    let link_only = false;  // This is should always be false for invalid sn resolution                
                    let attachment = None;  // No attachment here
                    let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                    // Send the message on the link
                    let _ = zlinksend!(message, link);

                    // Notify
                    let err = zerror!(ZErrorKind::InvalidMessage { 
                        descr: e 
                    });
                    pending.notify.send(err).await;
                    return Action::Close
                }
            } else {
                pending.sn_resolution
            };

            // Get the agreed Initial SN for reception
            let initial_sn_rx = if initial_sn < sn_resolution {
                initial_sn
            } else {
                let e = format!("Rejecting Accept with invalid Initial SN: {}. Expected to be smaller than: {}.", initial_sn, sn_resolution);
                log::debug!("{}", e);

                // Invalid value, send a Close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;              
                let link_only = false;  // This is should always be false for invalid initial sn                
                let attachment = None;  // No attachment here
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);                

                // Notify
                let err = zerror!(ZErrorKind::InvalidMessage { 
                    descr: e
                });
                pending.notify.send(err).await;
                return Action::Close
            };

            // Get the agreed Initial SN for transmission
            let initial_sn_tx = if pending.initial_sn < sn_resolution {
                pending.initial_sn
            } else {                
                let new = pending.initial_sn % sn_resolution;
                log::trace!("The Initial SN {} proposed in the Open message is too big for the agreed SN Resolution: {}. \
                             Computing the modulo of the Initial SN. The Initial SN is now: {}", pending.initial_sn, sn_resolution, new);
                new
            };

            // Get a new or an existing session
            // NOTE: In case of exsisting session, all the parameters in the accept are ignored
            let session = self.manager.get_or_new_session(
                &self.manager, &apid, &whatami, lease, sn_resolution, initial_sn_tx, initial_sn_rx
            ).await;

            // Add this link to the session
            let res = session.add_link(link.clone()).await;
            if let Err(e) = res {                
                // Invalid value, send a Close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;              
                let link_only = true;  // This is should always be true for error when adding the link                
                let attachment = None;  // No attachment here
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);

                // Notify
                pending.notify.send(Err(e)).await;
                return Action::Close
            }

            // Set the callback on the session if needed
            let callback = match session.get_callback().await {
                Ok(callback) => callback,
                Err(e) => {
                    // Notify
                    pending.notify.send(Err(e)).await;
                    return Action::Close
                }
            };

            if let Some(callback) = callback {
                callback.new_link(link.clone()).await;
            } else {
                // Notify the session handler that there is a new session and get back a callback
                let callback = match self.manager.config.handler.new_session(session.clone()).await {
                    Ok(callback) => callback,
                    Err(e) => {
                        log::warn!("Unable to get session event handler for peer {}: {}", apid, e);
                        return Action::Close 
                    }
                }; 
                // Set the callback on the transport
                if let Err(e) = session.set_callback(callback).await {
                    log::warn!("{}", e);
                    // Notify
                    pending.notify.send(Err(e)).await;
                    return Action::Close
                }
            }

            // Return the target transport to use in the link
            match session.get_transport() {
                Ok(transport) => {
                    // Notify
                    log::debug!("New session opened with: {}", apid);
                    pending.notify.send(Ok(session)).await;
                    Action::ChangeTransport(transport)
                }
                Err(e) => {
                    // Notify                    
                    pending.notify.send(Err(e)).await;
                    Action::Close
                }
            }            
        } else { 
            log::debug!("Received an unsolicited Accept on link {} from: {}", link, apid);
            Action::Read
        }
    }

    async fn process_close(&self, link: &Link, pid: Option<PeerId>, reason: u8, _link_only: bool) -> Action {
        if let Some(pending) = zasyncwrite!(self.pending).remove(link) {            
            let mut e = "Session closed by the remote peer".to_string();
            if let Some(pid) = pid {
                e.push_str(&format!(" {}", pid));
            }
            e.push_str(&format!(" with reason: {}", reason));
            log::debug!("{}", e);

            // Notify
            let err = zerror!(ZErrorKind::Other { 
                descr: e 
            });
            pending.notify.send(err).await;
        }

        Action::Close
    }
}

#[async_trait]
impl TransportTrait for InitialSession {
    async fn receive_message(&self, link: &Link, message: SessionMessage) -> Action {
        match message.body {
            SessionBody::Open { version, whatami, pid, lease, initial_sn, sn_resolution, locators } => {
                self.process_open(link, version, whatami, pid, lease, initial_sn, sn_resolution, locators).await
            },

            SessionBody::Accept { whatami, opid, apid, initial_sn, sn_resolution, lease, locators } => {
                self.process_accept(link, whatami, opid, apid, initial_sn, sn_resolution, lease, locators).await
            },

            SessionBody::Close { pid, reason, link_only } => {
                self.process_close(link, pid, reason, link_only).await
            },

            _ => {
                let e = format!("Invalid message received on link: {}", link);
                log::debug!("{}. Message: {:?}", e, message);

                // Invalid message, send a Close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;              
                let link_only = false;  // This is should always be false for invalid messages                
                let attachment = None;  // No attachment here
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);                

                // Notify
                if let Some(pending) = zasyncwrite!(self.pending).remove(link) {     
                    // Notify
                    let err = zerror!(ZErrorKind::IOError { 
                        descr: e
                    });
                    pending.notify.send(err).await;
                }

                Action::Close
            }
        }
    }

    async fn link_err(&self, link: &Link) {        
        if let Some(pending) = zasyncwrite!(self.pending).remove(link) {
            let e = format!("Unexpected error on link: {}", link);
            log::debug!("{}", e);

            // Notify            
            let err = zerror!(ZErrorKind::IOError { 
                descr: e 
            });
            pending.notify.send(err).await;
        }
    }
}