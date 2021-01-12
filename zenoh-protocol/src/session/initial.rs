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
    smsg, Attachment, InitAck, InitSyn, OpenAck, OpenSyn, SessionBody, SessionMessage,
};
use crate::session::defaults::{
    SESSION_OPEN_MAX_CONCURRENT, SESSION_OPEN_TIMEOUT, SESSION_SEQ_NUM_RESOLUTION,
};
use crate::session::{Session, SessionManagerInner};
use async_std::prelude::*;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zcheck, zerror};

const DEFAULT_WBUF_CAPACITY: usize = 64;

// Initial read task for a link
pub(super) async fn read_session_message(link: &Link) -> ZResult<Vec<SessionMessage>> {
    // Read from the link
    let buffer = if link.is_streamed() {
        // Read and decode the message length
        let mut length_bytes = [0u8; 2];
        let _ = link.read_exact(&mut length_bytes).await?;
        let to_read = u16::from_le_bytes(length_bytes) as usize;
        // Read the message
        let mut buffer = vec![0u8; to_read];
        let _ = link.read_exact(&mut buffer).await?;
        buffer
    } else {
        // Read the message
        let mut buffer = vec![0u8; link.get_mtu()];
        let n = link.read(&mut buffer).await?;
        buffer.truncate(n);
        buffer
    };

    let mut rbuf = RBuf::from(buffer);
    let mut messages: Vec<SessionMessage> = Vec::with_capacity(1);
    while rbuf.can_read() {
        match rbuf.read_session_message() {
            Some(msg) => messages.push(msg),
            None => {
                let e = format!("Decoding error on link: {}", link);
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        }
    }

    Ok(messages)
}

// Macro to send a message on a link
async fn write_session_message(link: &Link, msg: SessionMessage) -> ZResult<usize> {
    // Create the buffer for serializing the message
    let mut wbuf = WBuf::new(DEFAULT_WBUF_CAPACITY, false);
    if link.is_streamed() {
        // Reserve 16 bits to write the length
        wbuf.write_bytes(&[0u8, 0u8]);
    }
    // Serialize the message
    wbuf.write_session_message(&msg);
    if link.is_streamed() {
        // Write the length on the first 16 bits
        let length: u16 = wbuf.len() as u16 - 2;
        let bits = wbuf.get_first_slice_mut(..2);
        bits.copy_from_slice(&length.to_le_bytes());
    }
    let mut buffer = vec![0u8; wbuf.len()];
    wbuf.copy_into_slice(&mut buffer[..]);

    // Send the message on the link
    let res = link.write(&buffer).await;
    log::trace!("Sending on {}: {:?}. {:?}", link, msg, res);

    res
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

/*************************************/
/*             MANAGER               */
/*************************************/
// The opened (i.e. established) sessions
struct Opened {
    whatami: WhatAmI,
    sn_resolution: ZInt,
    initial_sn: ZInt,
}

// The initial session
pub(crate) struct SessionManagerInitial {
    // Reference to the main Session Manager
    manager: Arc<SessionManagerInner>,
    // Outgoing and incoming opened (i.e. established) sessions
    opened: Arc<Mutex<HashMap<PeerId, Opened>>>,
    // Incoming uninitialized sessions
    incoming: Arc<Mutex<HashSet<Link>>>,
}

impl SessionManagerInitial {
    pub(super) fn new(manager: Arc<SessionManagerInner>) -> SessionManagerInitial {
        SessionManagerInitial {
            manager,
            opened: Arc::new(Mutex::new(HashMap::new())),
            incoming: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /*************************************/
    /*            OPEN/CLOSE             */
    /*************************************/
    pub(super) async fn open(
        &self,
        link: &Link,
        attachment: &Option<Attachment>,
    ) -> ZResult<Session> {
        // Build and send an InitSyn Message
        let initsyn_version = self.manager.config.version;
        let initsyn_whatami = self.manager.config.whatami;
        let initsyn_pid = self.manager.config.pid.clone();
        let initsyn_sn_resolution =
            if self.manager.config.sn_resolution == *SESSION_SEQ_NUM_RESOLUTION {
                None
            } else {
                Some(self.manager.config.sn_resolution)
            };

        // Build and send the InitSyn message
        let message = SessionMessage::make_init_syn(
            initsyn_version,
            initsyn_whatami,
            initsyn_pid,
            initsyn_sn_resolution,
            attachment.clone(),
        );
        let _ = write_session_message(link, message).await?;

        // Wait to read an InitAck
        let mut messages = read_session_message(link).await?;
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
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = false; // This is should always be true for invalid lease on existing session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                let _ = write_session_message(link, message).await;
                let e = format!(
                    "Reived an invalid message in response to an InitSyn on link: {:?}",
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
                    let peer_id = Some(self.manager.config.pid.clone());
                    let reason_id = smsg::close_reason::INVALID;
                    let link_only = false;
                    let attachment = None;
                    let message =
                        SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                    let _ = write_session_message(link, message).await;

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
                    if sn_resolution > self.manager.config.sn_resolution {
                        // Build and send a Close message
                        let peer_id = Some(self.manager.config.pid.clone());
                        let reason_id = smsg::close_reason::INVALID;
                        let link_only = true;
                        let attachment = None;
                        let message =
                            SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                        let _ = write_session_message(link, message).await;

                        let e = format!(
                            "Rejecting InitAck on link {} because of invalid sn resolution: {}",
                            link, initack_pid
                        );
                        return zerror!(ZErrorKind::InvalidMessage { descr: e });
                    }
                    sn_resolution
                }
                None => self.manager.config.sn_resolution,
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
        let lease = self.manager.config.lease;
        let attachment = None;
        let message =
            SessionMessage::make_open_syn(lease, initial_sn_tx, initack_cookie, attachment);
        let _ = write_session_message(link, message).await?;

        // Wait to read an OpenAck
        let mut messages = read_session_message(link).await?;
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
            _ => {
                // Build and send a Close message
                let peer_id = Some(self.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true;
                let attachment = None;
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                let _ = write_session_message(link, message).await;

                let e = format!(
                    "Reived an invalid message in response to an OpenSyn on link: {:?}",
                    link,
                );
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        };

        let session = self
            .manager
            .get_or_new_session(
                &self.manager,
                &initack_pid,
                &initack_whatami,
                lease,
                sn_resolution,
                initial_sn_tx,
                initial_sn_rx,
            )
            .await;

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
        let initial = self.manager.get_initial_manager().await;
        task::spawn(async move {
            let to = Duration::from_millis(*SESSION_OPEN_TIMEOUT);
            let res = incoming_link_task(&link, initial).timeout(to).await;
            if res.is_err() {
                let _ = link.close().await;
            }
            let mut guard = zasynclock!(c_incoming);
            guard.remove(&link);
        });
    }
}

async fn incoming_link_task(link: &Link, initial: Arc<SessionManagerInitial>) -> ZResult<()> {
    /*************************************/
    /*             InitSyn               */
    /*************************************/
    // Wait to read an InitSyn
    let mut messages = read_session_message(&link).await?;
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
    if let Ok(s) = initial.manager.get_session(&syn_pid).await {
        // Check if we have reached maximum number of links for this session
        if let Some(limit) = initial.manager.config.max_links {
            let links = s.get_links().await?;
            if links.len() >= limit {
                // Build and send a close message
                let peer_id = Some(initial.manager.config.pid.clone());
                let reason_id = smsg::close_reason::MAX_LINKS;
                let link_only = true;
                let attachment = None;
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
                let _ = write_session_message(link, message).await;

                let e = format!(
                    "Rejecting Open on link {} because of maximum links limit reached for peer: {}",
                    link, syn_pid
                );
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }
        }
    }

    // Check if the version is supported
    if syn_version > initial.manager.config.version {
        // Send a close message
        let c_pid = Some(initial.manager.config.pid.clone());
        let reason_id = smsg::close_reason::UNSUPPORTED;
        let link_only = false; // This is should always be false for invalid version
        let attachment = None; // Parameter of open_session
        let message = SessionMessage::make_close(c_pid, reason_id, link_only, attachment);

        // Send the message on the link
        let _ = write_session_message(link, message).await;
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
    let agreed_sn_resolution = initial.manager.config.sn_resolution.min(syn_sn_resolution);

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
    let whatami = initial.manager.config.whatami;
    let apid = initial.manager.config.pid.clone();
    let sn_resolution = if agreed_sn_resolution == syn_sn_resolution {
        None
    } else {
        Some(agreed_sn_resolution)
    };
    let cookie = RBuf::from(wbuf); // @TODO: use HMAC to sign the cookie
    let attachment = None;
    let message = SessionMessage::make_init_ack(whatami, apid, sn_resolution, cookie, attachment);

    // Send the message on the link
    let _ = write_session_message(link, message).await?;

    /*************************************/
    /*             OpenSyn               */
    /*************************************/
    // Wait to read an OpenSyn
    let mut messages = read_session_message(&link).await?;
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

        _ => {
            let e = format!(
                "Received invalid message instead of an OpenSyn on link: {:?}",
                link,
            );
            return zerror!(ZErrorKind::InvalidMessage { descr: e });
        }
    };

    let cookie = if let Some(cookie) = syn_cookie.read_cookie() {
        // @TODO: verify cookie with HMAC
        cookie
    } else {
        // Build and send a Close message
        let peer_id = Some(initial.manager.config.pid.clone());
        let reason_id = smsg::close_reason::INVALID;
        let link_only = true;
        let attachment = None;
        let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
        let _ = write_session_message(link, message).await;

        let e = format!(
            "Rejecting OpenSyn on link {} because of invalid cookie",
            link,
        );
        return zerror!(ZErrorKind::InvalidMessage { descr: e });
    };

    // Initialize the session if it is new
    let mut guard = zasynclock!(initial.opened);
    let ack_initial_sn = if let Some(opened) = guard.get(&cookie.pid) {
        if opened.whatami != cookie.whatami || opened.sn_resolution != cookie.sn_resolution {
            drop(guard);
            // Build and send a Close message
            let peer_id = Some(initial.manager.config.pid.clone());
            let reason_id = smsg::close_reason::INVALID;
            let link_only = true; // This is should always be true for invalid lease on existing session
            let attachment = None; // Parameter of open_session
            let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);
            let _ = write_session_message(link, message).await;

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
        if let Ok(s) = initial.manager.get_session(&cookie.pid).await {
            // Check if we have reached maximum number of links for this session
            if let Some(limit) = initial.manager.config.max_links {
                let links = s.get_links().await?;
                if links.len() >= limit {
                    // Send a close message
                    let peer_id = Some(initial.manager.config.pid.clone());
                    let reason_id = smsg::close_reason::MAX_LINKS;
                    let link_only = true; // This is should always be true when the link limit is reached
                    let attachment = None; // Parameter of open_session
                    let message =
                        SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                    // Send the message on the link
                    let _ = write_session_message(link, message).await;
                    let e = format!("Rejecting Open on link {} because of maximum links limit reached for peer: {}", link, cookie.pid);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            }

            // Check if the lease is valid (i.e. the same of existing session)
            let l = s.get_lease()?;
            if syn_lease != l {
                // Send a close message
                let peer_id = Some(initial.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true; // This is should always be true for invalid lease on existing session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = write_session_message(link, message).await;
                let e = format!("Rejecting Open on link {} because of invalid lease on already existing session with peer: {}", link, cookie.pid);
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }

            // Check if the sn_resolution is valid (i.e. the same of existing session)
            let snr = s.get_sn_resolution()?;
            if cookie.sn_resolution != snr {
                // Send a close message
                let peer_id = Some(initial.manager.config.pid.clone());
                let reason_id = smsg::close_reason::INVALID;
                let link_only = true; // This is should always be true for invalid sn_resolution on exisisting session
                let attachment = None; // Parameter of open_session
                let message = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                // Send the message on the link
                let _ = write_session_message(link, message).await;
                let e = format!("Rejecting Open on link {} because of invalid sequence number resolution on already existing\
                                    session with peer: {}", link, cookie.pid);
                return zerror!(ZErrorKind::InvalidMessage { descr: e });
            }

            break s;
        } else {
            // Check if a limit for the maximum number of open sessions is set
            if let Some(limit) = initial.manager.config.max_sessions {
                let num = initial.manager.get_sessions().await.len();
                // Check if we have reached the session limit
                if num >= limit {
                    // Send a close message
                    let peer_id = Some(initial.manager.config.pid.clone());
                    let reason_id = smsg::close_reason::MAX_SESSIONS;
                    let link_only = false; // This is should always be false when the session limit is reached
                    let attachment = None; // Parameter of open_session
                    let message =
                        SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

                    // Send the message on the link
                    let _ = write_session_message(link, message).await;
                    let e = format!("Rejecting Open on link {} because of maximum sessions limit reached for peer: {}", link, cookie.pid);
                    return zerror!(ZErrorKind::InvalidMessage { descr: e });
                }
            }

            // Create a new session
            let res = initial
                .manager
                .new_session(
                    &initial.manager,
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
    let _ = session.add_link(link.clone()).await?;

    // Build OpenAck message
    let attachment = None;
    let message =
        SessionMessage::make_open_ack(initial.manager.config.lease, ack_initial_sn, attachment);

    // Send the message on the link
    let _ = write_session_message(link, message).await?;

    // Assign a callback if the session is new
    let c = session.get_callback().await?;
    if let Some(callback) = c {
        // Notify the session handler there is a new link on this session
        callback.new_link(link.clone()).await;
    } else {
        // Notify the session handler that there is a new session and get back a callback
        // NOTE: the read loop of the link the open message was sent on remains blocked
        //       until the new_session() returns. The read_loop in the various links
        //       waits for any eventual transport to associate to.
        let callback = initial
            .manager
            .config
            .handler
            .new_session(session.clone())
            .await?;
        // Set the callback on the transport
        let _ = session.set_callback(callback).await?;
    }

    log::debug!("New session link established from {}: {}", cookie.pid, link);
    Ok(())
}
