//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use super::super::{TransportExecutor, TransportManager, TransportPeerEventHandler};
use super::common::priority::{TransportPriorityRx, TransportPriorityTx};
use super::link::TransportLinkUnicast;
#[cfg(feature = "stats")]
use super::TransportUnicastStatsAtomic;
use crate::TransportConfigUnicast;
use async_std::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::{zasynclock, zcondfeat, zread, zwrite};
use zenoh_link::{Link, LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::{
    core::{Bits, Priority, WhatAmI, ZenohId},
    transport::{Close, PrioritySn, TransportMessage, TransportSn},
};
use zenoh_result::{bail, zerror, ZResult};

macro_rules! zlinkget {
    ($guard:expr, $link:expr) => {
        $guard.iter().find(|tl| &tl.link == $link)
    };
}

macro_rules! zlinkgetmut {
    ($guard:expr, $link:expr) => {
        $guard.iter_mut().find(|tl| &tl.link == $link)
    };
}

macro_rules! zlinkindex {
    ($guard:expr, $link:expr) => {
        $guard.iter().position(|tl| &tl.link == $link)
    };
}

/*************************************/
/*             TRANSPORT             */
/*************************************/
#[derive(Clone)]
pub(crate) struct TransportUnicastInner {
    // Transport Manager
    pub(crate) manager: TransportManager,
    // Transport config
    pub(super) config: TransportConfigUnicast,
    // Tx priorities
    pub(super) priority_tx: Arc<[TransportPriorityTx]>,
    // Rx priorities
    pub(super) priority_rx: Arc<[TransportPriorityRx]>,
    // The links associated to the channel
    pub(super) links: Arc<RwLock<Box<[TransportLinkUnicast]>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportPeerEventHandler>>>>,
    // Mutex for notification
    pub(super) alive: Arc<AsyncMutex<bool>>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportUnicastStatsAtomic>,
}

impl TransportUnicastInner {
    pub(super) fn make(
        manager: TransportManager,
        config: TransportConfigUnicast,
    ) -> ZResult<TransportUnicastInner> {
        let mut priority_tx = vec![];
        let mut priority_rx = vec![];

        let num = if config.is_qos { Priority::NUM } else { 1 };
        for _ in 0..num {
            priority_tx.push(TransportPriorityTx::make(config.sn_resolution)?);
        }

        for _ in 0..Priority::NUM {
            priority_rx.push(TransportPriorityRx::make(
                config.sn_resolution,
                manager.config.defrag_buff_size,
            )?);
        }

        let initial_sn = PrioritySn {
            reliable: config.tx_initial_sn,
            best_effort: config.tx_initial_sn,
        };
        for c in priority_tx.iter() {
            c.sync(initial_sn)?;
        }

        let t = TransportUnicastInner {
            manager,
            config,
            priority_tx: priority_tx.into_boxed_slice().into(),
            priority_rx: priority_rx.into_boxed_slice().into(),
            links: Arc::new(RwLock::new(vec![].into_boxed_slice())),
            callback: Arc::new(RwLock::new(None)),
            alive: Arc::new(AsyncMutex::new(false)),
            #[cfg(feature = "stats")]
            stats: Arc::new(TransportUnicastStatsAtomic::default()),
        };

        Ok(t)
    }

    pub(super) fn set_callback(&self, callback: Arc<dyn TransportPeerEventHandler>) {
        let mut guard = zwrite!(self.callback);
        *guard = Some(callback);
    }

    pub(super) async fn get_alive(&self) -> AsyncMutexGuard<'_, bool> {
        zasynclock!(self.alive)
    }

    /*************************************/
    /*           INITIATION              */
    /*************************************/
    pub(super) async fn sync(&self, initial_sn_rx: TransportSn) -> ZResult<()> {
        // Mark the transport as alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = zasynclock!(self.alive);
        if *a_guard {
            let e = zerror!("Transport already synched with peer: {}", self.config.zid);
            log::trace!("{}", e);
            return Err(e.into());
        }

        *a_guard = true;

        let csn = PrioritySn {
            reliable: initial_sn_rx,
            best_effort: initial_sn_rx,
        };
        for c in self.priority_rx.iter() {
            c.sync(csn)?;
        }

        Ok(())
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn delete(&self) -> ZResult<()> {
        log::debug!(
            "[{}] Closing transport with peer: {}",
            self.manager.config.zid,
            self.config.zid
        );
        // Mark the transport as no longer alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = self.get_alive().await;
        *a_guard = false;

        // Notify the callback that we are going to close the transport
        let callback = zwrite!(self.callback).take();
        if let Some(cb) = callback.as_ref() {
            cb.closing();
        }

        // Delete the transport on the manager
        let _ = self.manager.del_transport_unicast(&self.config.zid).await;

        // Close all the links
        let mut links = {
            let mut l_guard = zwrite!(self.links);
            let links = l_guard.to_vec();
            *l_guard = vec![].into_boxed_slice();
            links
        };
        for l in links.drain(..) {
            let _ = l.close().await;
        }

        // Notify the callback that we have closed the transport
        if let Some(cb) = callback.as_ref() {
            cb.closed();
        }

        Ok(())
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(super) fn add_link(
        &self,
        link: LinkUnicast,
        direction: LinkUnicastDirection,
    ) -> ZResult<()> {
        // Add the link to the channel
        let mut guard = zwrite!(self.links);

        // Check if we can add more inbound links
        if let LinkUnicastDirection::Inbound = direction {
            let count = guard.iter().filter(|l| l.direction == direction).count();

            let limit = zcondfeat!(
                "transport_multilink",
                match self.config.multilink {
                    Some(_) => self.manager.config.unicast.max_links,
                    None => 1,
                },
                1
            );

            if count >= limit {
                let e = zerror!(
                    "Can not add Link {} with peer {}: max num of links reached {}/{}",
                    link,
                    self.config.zid,
                    count,
                    limit
                );
                return Err(e.into());
            }
        }

        // Create a channel link from a link
        let link = TransportLinkUnicast::new(self.clone(), link, direction);

        let mut links = Vec::with_capacity(guard.len() + 1);
        links.extend_from_slice(&guard);
        links.push(link);
        *guard = links.into_boxed_slice();

        Ok(())
    }

    pub(super) fn start_tx(
        &self,
        link: &LinkUnicast,
        executor: &TransportExecutor,
        keep_alive: Duration,
        batch_size: u16,
    ) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                assert!(!self.priority_tx.is_empty());
                l.start_tx(executor, keep_alive, batch_size, &self.priority_tx);
                Ok(())
            }
            None => {
                bail!(
                    "Can not start Link TX {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
        }
    }

    pub(super) fn stop_tx(&self, link: &LinkUnicast) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                l.stop_tx();
                Ok(())
            }
            None => {
                bail!(
                    "Can not stop Link TX {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
        }
    }

    pub(super) fn start_rx(
        &self,
        link: &LinkUnicast,
        lease: Duration,
        batch_size: u16,
    ) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                l.start_rx(lease, batch_size);
                Ok(())
            }
            None => {
                bail!(
                    "Can not start Link RX {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
        }
    }

    pub(super) fn stop_rx(&self, link: &LinkUnicast) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                l.stop_rx();
                Ok(())
            }
            None => {
                bail!(
                    "Can not stop Link RX {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
        }
    }

    pub(crate) async fn del_link(&self, link: &LinkUnicast) -> ZResult<()> {
        enum Target {
            Transport,
            Link(Box<TransportLinkUnicast>),
        }

        // Try to remove the link
        let target = {
            let mut guard = zwrite!(self.links);

            if let Some(index) = zlinkindex!(guard, link) {
                let is_last = guard.len() == 1;
                if is_last {
                    // Close the whole transport
                    drop(guard);
                    Target::Transport
                } else {
                    // Remove the link
                    let mut links = guard.to_vec();
                    let stl = links.remove(index);
                    *guard = links.into_boxed_slice();
                    drop(guard);
                    Target::Link(stl.into())
                }
            } else {
                bail!(
                    "Can not delete Link {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
        };

        // Notify the callback
        if let Some(callback) = zread!(self.callback).as_ref() {
            callback.del_link(Link::from(link));
        }

        match target {
            Target::Transport => self.delete().await,
            Target::Link(stl) => stl.close().await,
        }
    }
}

impl TransportUnicastInner {
    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(crate) fn get_zid(&self) -> ZenohId {
        self.config.zid
    }

    pub(crate) fn get_whatami(&self) -> WhatAmI {
        self.config.whatami
    }

    pub(crate) fn get_sn_resolution(&self) -> Bits {
        self.config.sn_resolution
    }

    #[cfg(feature = "shared-memory")]
    pub(crate) fn is_shm(&self) -> bool {
        self.config.is_shm
    }

    pub(crate) fn is_qos(&self) -> bool {
        self.config.is_qos
    }

    pub(crate) fn get_callback(&self) -> Option<Arc<dyn TransportPeerEventHandler>> {
        zread!(self.callback).clone()
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(crate) async fn close_link(&self, link: &LinkUnicast, reason: u8) -> ZResult<()> {
        log::trace!("Closing link {} with peer: {}", link, self.config.zid);

        let mut pipeline = zlinkget!(zread!(self.links), link)
            .map(|l| l.pipeline.clone())
            .ok_or_else(|| zerror!("Cannot close Link {:?}: not found", link))?;

        if let Some(p) = pipeline.take() {
            // Close message to be sent on the target link
            let msg: TransportMessage = Close {
                reason,
                session: false,
            }
            .into();

            p.push_transport_message(msg, Priority::Background);
        }

        // Remove the link from the channel
        self.del_link(link).await
    }

    pub(crate) async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing transport with peer: {}", self.config.zid);

        let mut pipelines = zread!(self.links)
            .iter()
            .filter_map(|sl| sl.pipeline.clone())
            .collect::<Vec<_>>();
        for p in pipelines.drain(..) {
            // Close message to be sent on all the links
            // session should always be true for user-triggered close. However, in case of
            // multiple links, it is safer to close all the links first. When no links are left,
            // the transport is then considered closed.
            let msg: TransportMessage = Close {
                reason,
                session: false,
            }
            .into();

            p.push_transport_message(msg, Priority::Background);
        }
        // Terminate and clean up the transport
        self.delete().await
    }

    pub(crate) fn get_links(&self) -> Vec<LinkUnicast> {
        zread!(self.links).iter().map(|l| l.link.clone()).collect()
    }
}
