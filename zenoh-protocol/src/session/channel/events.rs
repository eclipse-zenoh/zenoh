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
use async_std::sync::{Arc, Weak};
use async_trait::async_trait;

use super::{Channel, LinkAlive, TransmissionQueue};

use crate::link::Link;
use crate::proto::{smsg, SessionMessage};
use crate::session::defaults::QUEUE_PRIO_DATA;

use zenoh_util::collections::Timed;

/*************************************/
/*          LINK KEEP ALIVE          */
/*************************************/
pub(super) struct KeepAliveEvent {
    queue: Arc<TransmissionQueue>,
    link: Link,
}

impl KeepAliveEvent {
    pub(super) fn new(queue: Arc<TransmissionQueue>, link: Link) -> KeepAliveEvent {
        KeepAliveEvent { queue, link }
    }
}

#[async_trait]
impl Timed for KeepAliveEvent {
    async fn run(&mut self) {
        log::trace!("Schedule KEEP_ALIVE message for link: {}", self.link);
        // Create the KEEP_ALIVE message
        let pid = None;
        let attachment = None;
        let message = SessionMessage::make_keep_alive(pid, attachment);

        // Push the KEEP_ALIVE messages on the queue
        self.queue
            .push_session_message(message, QUEUE_PRIO_DATA)
            .await;
    }
}

/*************************************/
/*        LINK LEASE EVENT           */
/*************************************/
pub(super) struct LinkLeaseEvent {
    ch: Weak<Channel>,
    alive: Arc<LinkAlive>,
    link: Link,
}

impl LinkLeaseEvent {
    pub(super) fn new(ch: Weak<Channel>, alive: Arc<LinkAlive>, link: Link) -> LinkLeaseEvent {
        LinkLeaseEvent { ch, alive, link }
    }
}

#[async_trait]
impl Timed for LinkLeaseEvent {
    async fn run(&mut self) {
        if self.alive.reset() {
            // The link was alive
            return;
        }

        // Close the link or eventually the whole session
        if let Some(ch) = self.ch.upgrade() {
            let links = ch.get_links().await;
            if links.len() == 1 && links[0] == self.link {
                log::warn!(
                    "Link {} has expired with peer: {}. No links left. Closing the session.",
                    self.link,
                    ch.get_pid()
                );
                // The last link has expired, close the whole session
                let _ = ch.close(smsg::close_reason::EXPIRED).await;
            } else {
                log::warn!("Link {} has expired with peer: {}", self.link, ch.get_pid());
                // Close only the link
                let _ = ch.close_link(&self.link, smsg::close_reason::EXPIRED).await;
            }
        }
    }
}

/*************************************/
/*        SESSION LEASE EVENT        */
/*************************************/
pub(super) struct SessionLeaseEvent {
    ch: Weak<Channel>,
}

impl SessionLeaseEvent {
    pub(super) fn new(ch: Weak<Channel>) -> SessionLeaseEvent {
        SessionLeaseEvent { ch }
    }
}

#[async_trait]
impl Timed for SessionLeaseEvent {
    async fn run(&mut self) {
        if let Some(ch) = self.ch.upgrade() {
            let links = ch.get_links().await;
            if links.is_empty() {
                log::warn!("Session has expired with peer: {}", ch.get_pid());
                // The last link has expired, close the whole session
                let _ = ch.delete().await;
            }
        }
    }
}
