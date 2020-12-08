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
use crate::core::{PeerId, WhatAmI, ZInt};
use crate::io::{RBuf, WBuf};
use crate::link::{Link, Locator};
use crate::proto::{
    smsg, Attachment, Close, InitAck, InitSyn, OpenAck, OpenSyn, SessionBody, SessionMessage,
};
use crate::session::defaults::SESSION_SEQ_NUM_RESOLUTION;
use crate::session::{Action, Session, SessionManagerInner, TransportTrait};
use async_std::sync::{Arc, Mutex, Sender};
use async_trait::async_trait;
use rand::Rng;
use std::collections::HashMap;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zcheck, zerror};

const DEFAULT_WBUF_CAPACITY: usize = 64;

// Macro to send a message on a link
macro_rules! zlinksend {
    ($msg:expr, $link:expr) => {{
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
    }};
}

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

struct Pending {
    notify: Sender<ZResult<Session>>,
}

struct Initialized {
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn: ZInt,
}

struct Opened {
    whatami: WhatAmI,
    pid: PeerId,
    sn_resolution: ZInt,
    initial_sn: ZInt,
    notify: Sender<ZResult<Session>>,
}

pub(super) struct InitialSession {
    manager: Arc<SessionManagerInner>,
    pending: Mutex<HashMap<Link, Pending>>,
    initialized: Mutex<HashMap<PeerId, Initialized>>,
    opened: Mutex<HashMap<Link, Opened>>,
}

impl InitialSession {
    pub(super) fn new(manager: Arc<SessionManagerInner>) -> InitialSession {
        InitialSession {
            manager,
            pending: Mutex::new(HashMap::new()),
            initialized: Mutex::new(HashMap::new()),
            opened: Mutex::new(HashMap::new()),
        }
    }

    /*************************************/
    /*            OPEN/CLOSE             */
    /*************************************/
    pub(super) async fn open(
        &self,
        link: &Link,
        attachment: &Option<Attachment>,
        notify: &Sender<ZResult<Session>>,
    ) -> ZResult<()> {
        // Check if a pending init is already present for this link
        if zasynclock!(self.pending).get(link).is_some() {
            let e = format!("A session opening is already pending on link: {:?}", link);
            log::warn!("{}", e);
            let err = zerror!(ZErrorKind::InvalidLink { descr: e.clone() });
            notify.send(err).await;
            return zerror!(ZErrorKind::InvalidLink { descr: e });
        }

        // Build the fields for the InitSyn Message
        let version = self.manager.config.version;
        let whatami = self.manager.config.whatami;
        let pid = self.manager.config.pid.clone();
        let sn_resolution = if self.manager.config.sn_resolution == *SESSION_SEQ_NUM_RESOLUTION {
            None
        } else {
            Some(self.manager.config.sn_resolution)
        };

        // Build the Open Message
        let message = SessionMessage::make_init_syn(
            version,
            whatami,
            pid.clone(),
            sn_resolution,
            attachment.clone(),
        );

        // Send the message on the link
        if let Err(e) = zlinksend!(message, link) {
            let e = format!(
                "Failed to send open message on link {:?}: {}",
                link,
                e.to_string()
            );
            log::warn!("{}", e);
            let err = zerror!(ZErrorKind::InvalidLink { descr: e.clone() });
            notify.send(err).await;
            return zerror!(ZErrorKind::InvalidLink { descr: e });
        }

        // Store the pending for the callback to be used in the process_message
        zasynclock!(self.pending).insert(
            link.clone(),
            Pending {
                notify: notify.clone(),
            },
        );

        Ok(())
    }

    /*************************************/
    /*          PROCESS MESSAGES         */
    /*************************************/
    async fn process_init_syn(
        &self,
        link: &Link,
        syn_version: u8,
        syn_whatami: WhatAmI,
        syn_pid: PeerId,
        syn_sn_resolution: Option<ZInt>,
    ) -> Action {
        // Check if the version is supported
        if syn_version > self.manager.config.version {
            // Send a close message
            let c_pid = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::UNSUPPORTED;
            let link_only = false; // This is should always be false for invalid version
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(c_pid, reason_id, link_only, attachment);

            // Send the message on the link
            let _ = zlinksend!(message, link);
            log::warn!(
                "Rejecting InitSyn on link {} because of unsupported Zenoh version from peer: {}",
                link,
                syn_pid
            );

            // Close the link
            return Action::Close;
        }

        // Get the SN Resolution
        let syn_sn_resolution = if let Some(snr) = syn_sn_resolution {
            snr
        } else {
            *SESSION_SEQ_NUM_RESOLUTION
        };

        // Compute the minimum SN Resolution
        let agreed_sn_resolution = self.manager.config.sn_resolution.min(syn_sn_resolution);

        // Create the cookie
        let mut wbuf = WBuf::new(64, false);
        let cookie = Cookie {
            whatami: syn_whatami,
            pid: syn_pid.clone(),
            sn_resolution: agreed_sn_resolution,
            src: link.get_src(),
            dst: link.get_dst(),
        };

        // Encode the cookie
        if !wbuf.write_cookie(&cookie) {
            // Send a close message
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::MAX_SESSIONS;
            let link_only = false; // This is should always be false when the session limit is reached
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Send the message on the link
            let _ = zlinksend!(message, link);
            log::warn!(
                "Rejecting InitSyn on link {} because cannot generate cookie for peer: {}",
                link,
                cookie.pid
            );

            // Close the link
            return Action::Close;
        }

        // Build the fields for the InitSyn message
        let whatami = self.manager.config.whatami;
        let apid = self.manager.config.pid.clone();
        let sn_resolution = if agreed_sn_resolution == syn_sn_resolution {
            None
        } else {
            Some(agreed_sn_resolution)
        };
        let cookie = RBuf::from(wbuf); // @TODO: use HMAC to sign the cookie
        let attachment = None;
        let message =
            SessionMessage::make_init_ack(whatami, apid, sn_resolution, cookie, attachment);

        // Send the message on the link
        if let Err(e) = zlinksend!(message, link) {
            log::warn!(
                "Unable to send InitAck on link {} to peer {}: {}",
                link,
                syn_pid,
                e
            );
            return Action::Close;
        }

        Action::Read
    }

    async fn process_init_ack(
        &self,
        link: &Link,
        ack_whatami: WhatAmI,
        ack_pid: PeerId,
        ack_sn_resolution: Option<ZInt>,
        ack_cookie: RBuf,
    ) -> Action {
        // Check if a pending init is already present for this link
        let pending = if let Some(pending) = zasynclock!(self.pending).remove(link) {
            pending
        } else {
            // Send a close message
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::INVALID;
            let link_only = true; // This is should always be true for invalid lease on existing session
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Send the message on the link
            let _ = zlinksend!(message, link);
            log::warn!(
                "Rejecting InitAck on link {} because no InitSyn was sent on the link to: {}",
                link,
                ack_pid
            );

            // Close the link
            return Action::Close;
        };

        // Get the sn resolution
        let sn_resolution = if let Some(sn_resolution) = ack_sn_resolution {
            if sn_resolution > self.manager.config.sn_resolution {
                // Send a close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true; // This is should always be true for invalid lease on existing session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);
                log::warn!(
                    "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                    link,
                    ack_pid
                );

                // Close the link
                return Action::Close;
            }
            sn_resolution
        } else {
            self.manager.config.sn_resolution
        };

        // Initialize the session if it is new
        let mut guard = zasynclock!(self.initialized);
        let initial_sn = if let Some(initialized) = guard.get(&ack_pid) {
            if initialized.whatami != ack_whatami || initialized.sn_resolution != sn_resolution {
                drop(guard);
                // Send a close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true; // This is should always be true for invalid lease on existing session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);
                log::warn!(
                    "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                    link,
                    ack_pid
                );

                // Close the link
                return Action::Close;
            }
            initialized.initial_sn
        } else {
            let initial_sn = {
                let mut rng = rand::thread_rng();
                rng.gen_range(0, sn_resolution)
            };
            guard.insert(
                ack_pid.clone(),
                Initialized {
                    whatami: ack_whatami,
                    sn_resolution,
                    initial_sn,
                },
            );
            log::trace!(
                "InitAck received from peer: {}. Initial seq num is: {}",
                ack_pid,
                initial_sn
            );
            initial_sn
        };
        drop(guard);

        // Build the fields for the OpenSyn message
        let lease = self.manager.config.lease;
        let attachment = None;
        let message = SessionMessage::make_open_syn(lease, initial_sn, ack_cookie, attachment);

        // Send the message on the link
        if let Err(e) = zlinksend!(message, link) {
            let e = format!(
                "Failed to send OpenSyn message on link {:?}: {}",
                link,
                e.to_string()
            );
            log::warn!("{}", e);
            let err = zerror!(ZErrorKind::InvalidLink { descr: e.clone() });
            pending.notify.send(err).await;
            return Action::Close;
        }

        let mut guard = zasynclock!(self.opened);
        guard.insert(
            link.clone(),
            Opened {
                whatami: ack_whatami,
                pid: ack_pid,
                sn_resolution,
                initial_sn,
                notify: pending.notify,
            },
        );

        Action::Read
    }

    async fn process_open_syn(
        &self,
        link: &Link,
        syn_lease: ZInt,
        syn_initial_sn: ZInt,
        mut syn_cookie: RBuf,
    ) -> Action {
        let cookie = if let Some(cookie) = syn_cookie.read_cookie() {
            // @TODO: verify cookie with HMAC
            cookie
        } else {
            // Send a close message
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::INVALID;
            let link_only = true; // This is should always be true for invalid lease on existing session
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Send the message on the link
            let _ = zlinksend!(message, link);
            log::warn!(
                "Rejecting OpenSyn on link {} because of invalid cookie",
                link,
            );

            // Close the link
            return Action::Close;
        };

        // Initialize the session if it is new
        let mut guard = zasynclock!(self.initialized);
        let ack_initial_sn = if let Some(initialized) = guard.get(&cookie.pid) {
            if initialized.whatami != cookie.whatami
                || initialized.sn_resolution != cookie.sn_resolution
            {
                drop(guard);
                // Send a close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true; // This is should always be true for invalid lease on existing session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = zlinksend!(message, link);
                log::warn!(
                    "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                    link,
                    cookie.pid
                );

                // Close the link
                return Action::Close;
            }
            initialized.initial_sn
        } else {
            let initial_sn = {
                let mut rng = rand::thread_rng();
                rng.gen_range(0, cookie.sn_resolution)
            };
            guard.insert(
                cookie.pid.clone(),
                Initialized {
                    whatami: cookie.whatami,
                    sn_resolution: cookie.sn_resolution,
                    initial_sn,
                },
            );
            log::trace!(
                "OpenSyn received from peer: {}. Initial seq num is: {}",
                cookie.pid,
                initial_sn
            );
            initial_sn
        };
        drop(guard);

        let session = loop {
            // Check if this open is related to a totally new session (i.e. new peer) or to an exsiting one
            if let Ok(s) = self.manager.get_session(&cookie.pid).await {
                // Check if we have reached maximum number of links for this session
                if let Some(limit) = self.manager.config.max_links {
                    if let Ok(links) = s.get_links().await {
                        if links.len() >= limit {
                            // Send a close message
                            let peer_id = Some(self.manager.config.pid.clone());
                            let reason_id = smsg::close_reason::MAX_LINKS;
                            let link_only = true; // This is should always be true when the link limit is reached
                            let attachment = None; // Parameter of open_session
                            let message = SessionMessage::make_close(
                                peer_id, reason_id, link_only, attachment,
                            );

                            // Send the message on the link
                            let _ = zlinksend!(message, link);
                            log::warn!("Rejecting Open on link {} because of maximum links limit reached for peer: {}", link, cookie.pid);

                            // Close the link
                            return Action::Close;
                        }
                    } else {
                        // Close the link
                        return Action::Close;
                    }
                }

                // Check if the lease is valid (i.e. the same of existing session)
                if let Ok(l) = s.get_lease() {
                    if syn_lease != l {
                        // Send a close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::INVALID;
                        let link_only = true; // This is should always be true for invalid lease on existing session
                        let attachment = None; // Parameter of open_session
                        let message =
                            SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                        // Send the message on the link
                        let _ = zlinksend!(message, link);
                        log::warn!("Rejecting Open on link {} because of invalid lease on already existing session with peer: {}", link, cookie.pid);

                        // Close the link
                        return Action::Close;
                    }
                } else {
                    // Close the link
                    return Action::Close;
                }

                // Check if the sn_resolution is valid (i.e. the same of existing session)
                if let Ok(snr) = s.get_sn_resolution() {
                    if cookie.sn_resolution != snr {
                        // Send a close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::INVALID;
                        let link_only = true; // This is should always be true for invalid sn_resolution on exisisting session
                        let attachment = None; // Parameter of open_session
                        let message =
                            SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                        // Send the message on the link
                        let _ = zlinksend!(message, link);
                        log::warn!("Rejecting Open on link {} because of invalid sequence number resolution on already existing\
                                    session with peer: {}", link, cookie.pid);

                        // Close the link
                        return Action::Close;
                    }

                    snr
                } else {
                    // Close the link
                    return Action::Close;
                };

                break s;
            } else {
                // Check if a limit for the maximum number of open sessions is set
                if let Some(limit) = self.manager.config.max_sessions {
                    let num = self.manager.get_sessions().await.len();
                    // Check if we have reached the session limit
                    if num >= limit {
                        // Send a close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::MAX_SESSIONS;
                        let link_only = false; // This is should always be false when the session limit is reached
                        let attachment = None; // Parameter of open_session
                        let message =
                            SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                        // Send the message on the link
                        let _ = zlinksend!(message, link);
                        log::warn!("Rejecting Open on link {} because of maximum sessions limit reached for peer: {}", link, cookie.pid);

                        // Close the link
                        return Action::Close;
                    }
                }

                // Create a new session
                let res = self
                    .manager
                    .new_session(
                        &self.manager,
                        &cookie.pid,
                        &cookie.whatami,
                        syn_lease,
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

        // Add the link to the session
        if let Err(e) = session.add_link(link.clone()).await {
            log::warn!(
                "Unable to add link {} to the session with peer {}: {}",
                link,
                cookie.pid,
                e
            );
            return Action::Close;
        }

        // Build OpenAck message
        let attachment = None;
        let message =
            SessionMessage::make_open_ack(self.manager.config.lease, ack_initial_sn, attachment);

        // Send the message on the link
        if let Err(e) = zlinksend!(message, link) {
            log::warn!(
                "Unable to send OpenAck on link {} for peer {}: {}",
                link,
                cookie.pid,
                e
            );
            return Action::Close;
        }

        // Assign a callback if the session is new
        match session.get_callback().await {
            Ok(c) => {
                if let Some(callback) = c {
                    // Notify the session handler there is a new link on this session
                    callback.new_link(link.clone()).await;
                } else {
                    // Notify the session handler that there is a new session and get back a callback
                    // NOTE: the read loop of the link the open message was sent on remains blocked
                    //       until the new_session() returns. The read_loop in the various links
                    //       waits for any eventual transport to associate to. This transport is
                    //       returned only by the process_open() -- this function.
                    let callback = match self
                        .manager
                        .config
                        .handler
                        .new_session(session.clone())
                        .await
                    {
                        Ok(callback) => callback,
                        Err(e) => {
                            log::warn!(
                                "Unable to get session event handler for peer {}: {}",
                                cookie.pid,
                                e
                            );
                            return Action::Close;
                        }
                    };
                    // Set the callback on the transport
                    if let Err(e) = session.set_callback(callback).await {
                        log::warn!("Unable to set callback for peer {}: {}", cookie.pid, e);
                        return Action::Close;
                    }
                }
            }
            Err(e) => {
                log::warn!("Unable to get callback for peer {}: {}", cookie.pid, e);
                return Action::Close;
            }
        };

        log::debug!("New session link established from {}: {}", cookie.pid, link);

        // Return the target transport to use in the link
        match session.get_transport() {
            Ok(transport) => Action::ChangeTransport(transport),
            Err(_) => Action::Close,
        }
    }

    async fn process_open_ack(&self, link: &Link, lease: ZInt, initial_sn: ZInt) -> Action {
        let opened = if let Some(opened) = zasynclock!(self.opened).remove(link) {
            opened
        } else {
            // Send a close message
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::INVALID;
            let link_only = true; // This is should always be true for invalid lease on existing session
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Send the message on the link
            let _ = zlinksend!(message, link);
            log::warn!(
                "Rejecting OpenAck on link {} because no OpenSyn was sent on the link.",
                link,
            );

            // Close the link
            return Action::Close;
        };

        // Get a new or an existing session
        // NOTE: In case of exsisting session, all the parameters in the accept are ignored
        let session = self
            .manager
            .get_or_new_session(
                &self.manager,
                &opened.pid,
                &opened.whatami,
                lease,
                opened.sn_resolution,
                opened.initial_sn,
                initial_sn,
            )
            .await;

        // Add this link to the session
        let res = session.add_link(link.clone()).await;
        if let Err(e) = res {
            // Invalid value, send a Close message
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = smsg::close_reason::INVALID;
            let link_only = true; // This is should always be true for error when adding the link
            let attachment = None; // No attachment here
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Send the message on the link
            let _ = zlinksend!(message, link);

            // Notify
            opened.notify.send(Err(e)).await;
            return Action::Close;
        }

        // Set the callback on the session if needed
        let callback = match session.get_callback().await {
            Ok(callback) => callback,
            Err(e) => {
                // Notify
                opened.notify.send(Err(e)).await;
                return Action::Close;
            }
        };

        if let Some(callback) = callback {
            callback.new_link(link.clone()).await;
        } else {
            // Notify the session handler that there is a new session and get back a callback
            let callback = match self
                .manager
                .config
                .handler
                .new_session(session.clone())
                .await
            {
                Ok(callback) => callback,
                Err(e) => {
                    log::warn!(
                        "Unable to get session event handler for peer {}: {}",
                        opened.pid,
                        e
                    );
                    return Action::Close;
                }
            };
            // Set the callback on the transport
            if let Err(e) = session.set_callback(callback).await {
                log::warn!("{}", e);
                // Notify
                opened.notify.send(Err(e)).await;
                return Action::Close;
            }
        }

        // Return the target transport to use in the link
        match session.get_transport() {
            Ok(transport) => {
                // Notify
                log::debug!("New session link established with {}: {}", opened.pid, link);
                opened.notify.send(Ok(session)).await;
                Action::ChangeTransport(transport)
            }
            Err(e) => {
                // Notify
                opened.notify.send(Err(e)).await;
                Action::Close
            }
        }
    }

    async fn process_close(
        &self,
        link: &Link,
        pid: Option<PeerId>,
        reason: u8,
        link_only: bool,
    ) -> Action {
        if !link_only {
            if let Some(pid) = pid.as_ref() {
                zasynclock!(self.initialized).remove(pid);
            }
        }

        let notify = if let Some(pending) = zasynclock!(self.pending).remove(link) {
            Some(pending.notify)
        } else if let Some(opened) = zasynclock!(self.opened).remove(link) {
            Some(opened.notify)
        } else {
            None
        };

        let mut e = "Session closed by the remote peer".to_string();
        if let Some(pid) = pid {
            e.push_str(&format!(" {}", pid));
        }
        e.push_str(&format!(" with reason: {}", reason));
        log::debug!("{}", e);

        // Notify
        if let Some(notify) = notify {
            let err = zerror!(ZErrorKind::Other { descr: e });
            notify.send(err).await;
        }

        Action::Close
    }

    async fn process_invalid(&self, link: &Link, message: SessionMessage) -> Action {
        let e = format!("Invalid message received on link: {}", link);
        log::debug!("{}. Message: {:?}", e, message);

        // Invalid message, send a Close message
        let peer_id = Some(self.manager.config.pid.clone());
        let reason_id = smsg::close_reason::INVALID;
        let link_only = false; // This is should always be false for invalid messages
        let attachment = None; // No attachment here
        let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

        // Send the message on the link
        let _ = zlinksend!(message, link);

        // Notify
        if let Some(pending) = zasynclock!(self.pending).remove(link) {
            // Notify
            let err = zerror!(ZErrorKind::IOError { descr: e });
            pending.notify.send(err).await;
        }

        Action::Close
    }
}

#[async_trait]
impl TransportTrait for InitialSession {
    async fn receive_message(&self, link: &Link, message: SessionMessage) -> Action {
        match message.body {
            SessionBody::InitSyn(InitSyn {
                version,
                whatami,
                pid,
                sn_resolution,
            }) => {
                self.process_init_syn(link, version, whatami, pid, sn_resolution)
                    .await
            }

            SessionBody::InitAck(InitAck {
                whatami,
                pid,
                sn_resolution,
                cookie,
            }) => {
                self.process_init_ack(link, whatami, pid, sn_resolution, cookie)
                    .await
            }

            SessionBody::OpenSyn(OpenSyn {
                lease,
                initial_sn,
                cookie,
            }) => self.process_open_syn(link, lease, initial_sn, cookie).await,

            SessionBody::OpenAck(OpenAck { lease, initial_sn }) => {
                self.process_open_ack(link, lease, initial_sn).await
            }

            SessionBody::Close(Close {
                pid,
                reason,
                link_only,
            }) => self.process_close(link, pid, reason, link_only).await,

            _ => self.process_invalid(link, message).await,
        }
    }

    async fn link_err(&self, link: &Link) {
        if let Some(pending) = zasynclock!(self.pending).remove(link) {
            let e = format!("Unexpected error on link: {}", link);
            log::debug!("{}", e);

            // Notify
            let err = zerror!(ZErrorKind::IOError { descr: e });
            pending.notify.send(err).await;
        }
    }
}
