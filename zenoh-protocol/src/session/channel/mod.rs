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
mod batch;
mod defragmentation;
mod events;
mod link;
// mod reliability_queue;
mod rx;
mod scheduling;
mod seq_num;
mod tx;

use crate::core::{PeerId, WhatAmI, ZInt};
use crate::link::Link;
use crate::proto::{SessionMessage, ZenohMessage};
use crate::session::defaults::QUEUE_PRIO_DATA;
use crate::session::{SessionEventHandler, SessionManagerInner};
use async_std::sync::{Arc, Mutex, RwLock, Weak};
use async_trait::async_trait;
use batch::*;
use defragmentation::*;
use events::*;
use link::*;
use rx::*;
use scheduling::*;
use seq_num::*;
use std::time::Duration;
use tx::*;
use zenoh_util::collections::{TimedEvent, TimedHandle, Timer};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zasynclock, zasyncopt, zasyncread, zasyncwrite, zerror};

#[async_trait]
pub(crate) trait Scheduling {
    async fn schedule(&self, msg: ZenohMessage, links: &Arc<RwLock<Vec<ChannelLink>>>);
}

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

/*************************************/
/*           CHANNEL STRUCT          */
/*************************************/
pub(crate) struct Channel {
    // The manager this channel is associated to
    pub(super) manager: Arc<SessionManagerInner>,
    // The remote peer id
    pub(super) pid: PeerId,
    // The remote whatami
    pub(super) whatami: WhatAmI,
    // The session lease in seconds
    pub(super) lease: ZInt,
    // Keep alive interval
    pub(super) keep_alive: ZInt,
    // The SN resolution
    pub(super) sn_resolution: ZInt,
    // The batch size
    pub(super) batch_size: usize,
    // The sn generator for the TX reliable channel
    pub(super) tx_sn_reliable: Arc<Mutex<SeqNumGenerator>>,
    // The sn generator for the TX best_effort channel
    pub(super) tx_sn_best_effort: Arc<Mutex<SeqNumGenerator>>,
    // The RX reliable channel
    pub(super) rx_reliable: Mutex<ChannelRxReliable>,
    // The RX best effort channel
    pub(super) rx_best_effort: Mutex<ChannelRxBestEffort>,
    // The links associated to the channel
    pub(super) links: Arc<RwLock<Vec<ChannelLink>>>,
    // The scehduling function
    pub(super) scheduling: Box<dyn Scheduling + Send + Sync>,
    // The internal timer
    pub(super) timer: Timer,
    // The lease event
    pub(super) lease_event_handle: Mutex<Option<TimedHandle>>,
    // The callback
    pub(super) callback: RwLock<Option<Arc<dyn SessionEventHandler + Send + Sync>>>,
    // Weak reference to self
    pub(super) w_self: RwLock<Option<Weak<Self>>>,
}

impl Channel {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        manager: Arc<SessionManagerInner>,
        pid: PeerId,
        whatami: WhatAmI,
        lease: ZInt,
        keep_alive: ZInt,
        sn_resolution: ZInt,
        initial_sn_tx: ZInt,
        initial_sn_rx: ZInt,
        batch_size: usize,
    ) -> Channel {
        Channel {
            manager,
            pid,
            whatami,
            lease,
            keep_alive,
            sn_resolution,
            batch_size,
            tx_sn_reliable: Arc::new(Mutex::new(SeqNumGenerator::new(
                initial_sn_tx,
                sn_resolution,
            ))),
            tx_sn_best_effort: Arc::new(Mutex::new(SeqNumGenerator::new(
                initial_sn_tx,
                sn_resolution,
            ))),
            rx_reliable: Mutex::new(ChannelRxReliable::new(initial_sn_rx, sn_resolution)),
            rx_best_effort: Mutex::new(ChannelRxBestEffort::new(initial_sn_rx, sn_resolution)),
            links: Arc::new(RwLock::new(Vec::new())),
            scheduling: Box::new(FirstMatch::new()),
            timer: Timer::new(),
            lease_event_handle: Mutex::new(None),
            callback: RwLock::new(None),
            w_self: RwLock::new(None),
        }
    }

    pub(crate) async fn initialize(&self, w_self: Weak<Self>) {
        // Initialize the weak reference to self
        *zasyncwrite!(self.w_self) = Some(w_self.clone());
        // Start the session lease timeout
        self.start_session_lease().await;
    }

    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(crate) fn get_pid(&self) -> PeerId {
        self.pid.clone()
    }

    pub(crate) fn get_whatami(&self) -> WhatAmI {
        self.whatami
    }

    pub(crate) fn get_lease(&self) -> ZInt {
        self.lease
    }

    pub(crate) fn get_keep_alive(&self) -> ZInt {
        self.keep_alive
    }

    pub(crate) fn get_sn_resolution(&self) -> ZInt {
        self.sn_resolution
    }

    pub(crate) async fn get_callback(&self) -> Option<Arc<dyn SessionEventHandler + Send + Sync>> {
        zasyncread!(self.callback).clone()
    }

    pub(crate) async fn set_callback(&self, callback: Arc<dyn SessionEventHandler + Send + Sync>) {
        let mut guard = zasyncwrite!(self.callback);
        *guard = Some(callback.clone());
    }

    /*************************************/
    /*          SESSION LEASE            */
    /*************************************/
    async fn start_session_lease(&self) {
        // Lease event
        let event = SessionLeaseEvent::new(zasyncopt!(self.w_self).clone());
        // Session lease interval is expressed in seconds
        let interval = Duration::from_millis(self.lease as u64);
        let event = TimedEvent::periodic(interval, event);
        let handle = event.get_handle();
        // Update the handle
        let mut guard = zasynclock!(self.lease_event_handle);
        if let Some(old_handle) = guard.take() {
            old_handle.defuse();
        }
        *guard = Some(handle);
        // Add the event to the timer
        self.timer.add(event).await;
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn delete(&self) {
        log::debug!("Closing the session with peer: {}", self.pid);
        // Notify the callback that we are going to close the session
        let mut c_guard = zasyncwrite!(self.callback);
        if let Some(callback) = c_guard.as_ref() {
            callback.closing().await;
        }

        // Delete the session on the manager
        let _ = self.manager.del_session(&self.pid).await;

        // Notify the callback that we have closed the session
        if let Some(callback) = c_guard.take() {
            callback.closed().await;
        }

        // Close all the links
        let mut l_guard = zasyncwrite!(self.links);
        for l in l_guard.drain(..) {
            let _ = l.close().await;
        }
    }

    pub(crate) async fn close_link(&self, link: &Link, reason: u8) -> ZResult<()> {
        log::trace!("Closing link {} with peer: {}", link, self.pid);

        let guard = zasyncread!(self.links);
        if let Some(l) = zlinkget!(guard, link) {
            // Close message to be sent on the target link
            let peer_id = Some(self.manager.config.pid.clone());
            let reason_id = reason;
            let link_only = true; // This is should always be true when closing a link
            let attachment = None; // No attachment here
            let msg = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

            // Schedule the close message for transmission
            l.schedule_session_message(msg, QUEUE_PRIO_DATA).await;

            // Drop the guard
            drop(guard);

            // Remove the link from the channel
            self.del_link(&link).await?;
        }

        Ok(())
    }

    pub(crate) async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing session with peer: {}", self.pid);

        // Close message to be sent on all the links
        let peer_id = Some(self.manager.config.pid.clone());
        let reason_id = reason;
        let link_only = false; // This is should always be false for user-triggered close
        let attachment = None; // No attachment here
        let msg = SessionMessage::make_close(peer_id, reason_id, link_only, attachment);

        let mut links = zasyncread!(self.links).clone();
        for link in links.drain(..) {
            link.schedule_session_message(msg.clone(), QUEUE_PRIO_DATA)
                .await;
        }

        // Terminate and clean up the session
        self.delete().await;

        Ok(())
    }

    /*************************************/
    /*        SCHEDULE AND SEND TX       */
    /*************************************/
    /// Schedule a Zenoh message on the transmission queue
    #[inline]
    pub(crate) async fn schedule(&self, message: ZenohMessage) {
        self.scheduling.schedule(message, &self.links).await;
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(crate) async fn add_link(&self, link: Link) -> ZResult<()> {
        let mut guard = zasyncwrite!(self.links);
        if zlinkget!(guard, &link).is_some() {
            return zerror!(ZErrorKind::InvalidLink {
                descr: format!("Can not add Link {} with peer: {}", link, self.pid)
            });
        }

        // Create a channel link from a link
        let link = ChannelLink::new(
            zasyncopt!(self.w_self).clone(),
            link,
            self.batch_size,
            self.keep_alive,
            self.lease,
            self.tx_sn_reliable.clone(),
            self.tx_sn_best_effort.clone(),
            self.timer.clone(),
        );

        // Add the link to the channel
        guard.push(link);

        // Restart the session lease event
        self.start_session_lease().await;

        Ok(())
    }

    pub(crate) async fn del_link(&self, link: &Link) -> ZResult<()> {
        // Try to remove the link
        let mut guard = zasyncwrite!(self.links);
        if let Some(index) = zlinkindex!(guard, link) {
            // Restart the session lease event
            self.start_session_lease().await;
            // Remove and close the link
            let link = guard.remove(index);
            link.close().await
        } else {
            zerror!(ZErrorKind::InvalidLink {
                descr: format!("Can not delete Link {} with peer: {}", link, self.pid)
            })
        }
    }

    pub(crate) async fn get_links(&self) -> Vec<Link> {
        zasyncread!(self.links)
            .iter()
            .map(|l| l.get_link().clone())
            .collect()
    }
}
