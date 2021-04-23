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
mod defragmentation;
mod link;
mod rx;
mod seq_num;
mod tx;

use super::core;
use super::core::{PeerId, Reliability, WhatAmI, ZInt};
use super::io;
use super::link::Link;
use super::proto;
use super::proto::{SessionMessage, ZenohMessage};
use super::session;
use super::session::defaults::QUEUE_PRIO_DATA;
use super::session::{SessionEventDispatcher, SessionManager};
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use async_std::task;
use defragmentation::*;
use link::*;
pub(super) use seq_num::*;
use std::sync::{Arc, Mutex, RwLock};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

macro_rules! zlinkget {
    ($guard:expr, $link:expr) => {
        $guard.iter().find(|l| l.get_link() == $link)
    };
}

macro_rules! zlinkindex {
    ($guard:expr, $link:expr) => {
        $guard.iter().position(|l| l.get_link() == $link)
    };
}

pub(crate) struct SessionTransportRxReliable {
    sn: SeqNum,
    defrag_buffer: DefragBuffer,
}

impl SessionTransportRxReliable {
    pub(crate) fn new(initial_sn: ZInt, sn_resolution: ZInt) -> SessionTransportRxReliable {
        // Set the sequence number in the state as it had
        // received a message with initial_sn - 1
        let last_initial_sn = if initial_sn == 0 {
            sn_resolution - 1
        } else {
            initial_sn - 1
        };

        SessionTransportRxReliable {
            sn: SeqNum::new(last_initial_sn, sn_resolution),
            defrag_buffer: DefragBuffer::new(initial_sn, sn_resolution, Reliability::Reliable),
        }
    }
}

pub(crate) struct SessionTransportRxBestEffort {
    sn: SeqNum,
    defrag_buffer: DefragBuffer,
}

impl SessionTransportRxBestEffort {
    pub(crate) fn new(initial_sn: ZInt, sn_resolution: ZInt) -> SessionTransportRxBestEffort {
        // Set the sequence number in the state as it had
        // received a message with initial_sn - 1
        let last_initial_sn = if initial_sn == 0 {
            sn_resolution - 1
        } else {
            initial_sn - 1
        };

        SessionTransportRxBestEffort {
            sn: SeqNum::new(last_initial_sn, sn_resolution),
            defrag_buffer: DefragBuffer::new(initial_sn, sn_resolution, Reliability::BestEffort),
        }
    }
}

/*************************************/
/*             TRANSPORT             */
/*************************************/
#[derive(Clone)]
pub(crate) struct SessionTransport {
    // The manager this channel is associated to
    pub(super) manager: SessionManager,
    // The remote peer id
    pub(super) pid: PeerId,
    // The remote whatami
    pub(super) whatami: WhatAmI,
    // The SN resolution
    pub(super) sn_resolution: ZInt,
    // The sn generator for the TX reliable channel
    pub(super) tx_sn_reliable: Arc<Mutex<SeqNumGenerator>>,
    // The sn generator for the TX best_effort channel
    pub(super) tx_sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    // The RX reliable channel
    pub(super) rx_reliable: Arc<Mutex<SessionTransportRxReliable>>,
    // The RX best effort channel
    pub(super) rx_best_effort: Arc<Mutex<SessionTransportRxBestEffort>>,
    // The links associated to the channel
    pub(super) links: Arc<RwLock<Box<[SessionTransportLink]>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<SessionEventDispatcher>>>,
    // Mutex for notification
    pub(super) alive: AsyncArc<AsyncMutex<bool>>,
    // The session transport can do shm
    pub(super) is_shm: bool,
}

impl SessionTransport {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        manager: SessionManager,
        pid: PeerId,
        whatami: WhatAmI,
        sn_resolution: ZInt,
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
        is_shm: bool,
    ) -> SessionTransport {
        SessionTransport {
            manager,
            pid,
            whatami,
            sn_resolution,
            tx_sn_reliable: Arc::new(Mutex::new(SeqNumGenerator::new(
                initial_sn_tx,
                sn_resolution,
            ))),
            tx_sn_best_effort: Arc::new(Mutex::new(SeqNumGenerator::new(
                initial_sn_tx,
                sn_resolution,
            ))),
            rx_reliable: Arc::new(Mutex::new(SessionTransportRxReliable::new(
                initial_sn_rx,
                sn_resolution,
            ))),
            rx_best_effort: Arc::new(Mutex::new(SessionTransportRxBestEffort::new(
                initial_sn_rx,
                sn_resolution,
            ))),
            links: Arc::new(RwLock::new(vec![].into_boxed_slice())),
            callback: Arc::new(RwLock::new(None)),
            alive: AsyncArc::new(AsyncMutex::new(true)),
            is_shm,
        }
    }

    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(crate) fn get_callback(&self) -> Option<SessionEventDispatcher> {
        zread!(self.callback).clone()
    }

    pub(crate) fn set_callback(&self, callback: SessionEventDispatcher) {
        let mut guard = zwrite!(self.callback);
        *guard = Some(callback);
    }

    pub(crate) async fn get_alive(&self) -> AsyncMutexGuard<'_, bool> {
        zasynclock!(self.alive)
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    // @TOFIX
    pub(super) fn delete(&self) {
        log::debug!("Closing session with peer: {}", self.pid);

        // Mark the transport as no longer alive and keep the lock
        // to avoid concurrent new_session and closing/closed notifications
        let mut a_guard = task::block_on(self.get_alive());
        *a_guard = false;

        // Notify the callback that we are going to close the session
        let mut c_guard = zwrite!(self.callback);
        if let Some(callback) = c_guard.as_ref() {
            callback.closing();
        }

        // Delete the session on the manager
        let _ = task::block_on(self.manager.del_session(&self.pid));

        // Close all the links
        let mut l_guard = zwrite!(self.links);
        let mut links = l_guard.to_vec();
        *l_guard = vec![].into_boxed_slice();
        drop(l_guard);

        // @TOFIX: stop first link tx, call then callback closed, then stop link rx
        for l in links.drain(..) {
            l.close();
        }

        // Notify the callback that we have closed the session
        if let Some(callback) = c_guard.take() {
            callback.closed();
        }
    }

    pub(crate) fn close_link(&self, link: &Link, reason: u8) -> ZResult<()> {
        log::trace!("Closing link {} with peer: {}", link, self.pid);

        let guard = zread!(self.links);
        if let Some(l) = zlinkget!(guard, link) {
            let c_l = l.clone();
            // Drop the guard
            drop(guard);

            // Close message to be sent on the target link
            let peer_id = Some(self.manager.pid());
            let reason_id = reason;
            let link_only = true; // This is should always be true when closing a link
            let attachment = None; // No attachment here
            let msg = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Schedule the close message for transmission
            c_l.schedule_session_message(msg, QUEUE_PRIO_DATA);

            // Remove the link from the channel
            self.del_link(&link)?;
        }

        Ok(())
    }

    pub(crate) fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing session with peer: {}", self.pid);

        // Close message to be sent on all the links
        let peer_id = Some(self.manager.pid());
        let reason_id = reason;
        // link_only should always be false for user-triggered close. However, in case of
        // multiple links, it is safer to close all the links first. When no links are left,
        // the session is then considered closed.
        let link_only = true;
        let attachment = None; // No attachment here
        let msg = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

        for link in zread!(self.links).iter() {
            link.schedule_session_message(msg.clone(), QUEUE_PRIO_DATA);
        }
        // Terminate and clean up the session
        self.delete();

        Ok(())
    }

    /*************************************/
    /*        SCHEDULE AND SEND TX       */
    /*************************************/
    /// Schedule a Zenoh message on the transmission queue    
    #[cfg(feature = "zero-copy")]
    pub(crate) fn schedule(&self, mut message: ZenohMessage) {
        if self.is_shm {
            message.inc_ref_shm();
        } else {
            message.flatten_shm();
        }
        self.schedule_first_fit(message);
    }

    #[cfg(not(feature = "zero-copy"))]
    pub(crate) fn schedule(&self, message: ZenohMessage) {
        self.schedule_first_fit(message);
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(crate) fn add_link(
        &self,
        link: Link,
        batch_size: usize,
        lease: ZInt,
        keep_alive: ZInt,
    ) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        if zlinkget!(guard, &link).is_some() {
            return zerror!(ZErrorKind::InvalidLink {
                descr: format!("Can not add Link {} with peer: {}", link, self.pid)
            });
        }

        // Create a channel link from a link
        let link = SessionTransportLink::new(
            self.clone(),
            link,
            batch_size,
            keep_alive,
            lease,
            self.tx_sn_reliable.clone(),
            self.tx_sn_best_effort.clone(),
        );

        // Add the link to the channel
        let mut links = Vec::with_capacity(guard.len() + 1);
        links.extend_from_slice(&guard);
        links.push(link);
        *guard = links.into_boxed_slice();

        Ok(())
    }

    pub(crate) fn start_tx(&self, link: &Link) -> ZResult<()> {
        let guard = zread!(self.links);
        if let Some(l) = zlinkget!(guard, link) {
            l.start_tx();
        }
        Ok(())
    }

    pub(crate) fn start_rx(&self, link: &Link) -> ZResult<()> {
        let guard = zread!(self.links);
        if let Some(l) = zlinkget!(guard, link) {
            l.start_rx();
        }
        Ok(())
    }

    pub(crate) fn del_link(&self, link: &Link) -> ZResult<()> {
        // Try to remove the link
        let mut guard = zwrite!(self.links);
        log::trace!("{}: deleting 1", link);
        for l in guard.iter() {
            log::trace!("\t{}", l.get_link());
        }
        if let Some(index) = zlinkindex!(guard, link) {
            let is_last = guard.len() == 1;
            if is_last {
                // Close the whole session
                drop(guard);
                log::trace!("{}: deleting 2", link);
                self.delete();
                Ok(())
            } else {
                // Remove the link
                log::trace!("{}: deleting 3", link);
                let mut links = guard.to_vec();
                let link = links.remove(index);
                *guard = links.into_boxed_slice();
                drop(guard);
                // Notify the callback
                if let Some(callback) = zread!(self.callback).as_ref() {
                    callback.del_link(link.get_link().clone());
                }
                log::trace!("{}: deleting 4", link.get_link());
                link.close();
                Ok(())
            }
        } else {
            log::trace!("{}: deleting -1", link);
            zerror!(ZErrorKind::InvalidLink {
                descr: format!("Can not delete Link {} with peer: {}", link, self.pid)
            })
        }
    }

    pub(crate) fn get_links(&self) -> Vec<Link> {
        zread!(self.links)
            .iter()
            .map(|l| l.get_link().clone())
            .collect()
    }
}
