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
use super::super::{TransportManager, TransportPeerEventHandler};
use super::common::{
    conduit::{TransportConduitRx, TransportConduitTx},
    pipeline::TransmissionPipeline,
};
use super::link::TransportLinkUnicast;
use super::protocol::core::{ConduitSn, Priority, WhatAmI, ZInt, ZenohId};
use super::protocol::message::{Close, CloseReason, ZenohMessage};
#[cfg(feature = "stats")]
use super::TransportUnicastStatsAtomic;
use crate::net::link::{Link, LinkUnicast, LinkUnicastDirection};
use crate::net::protocol::core::SeqNumBytes;
use async_std::sync::{Arc as AsyncArc, Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use std::convert::TryInto;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_util::core::Result as ZResult;

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
pub(crate) struct TransportUnicastConfig {
    pub(crate) manager: TransportManager,
    pub(crate) zid: ZenohId,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_bytes: SeqNumBytes,
    pub(crate) initial_sn_tx: ZInt,
    pub(crate) is_shm: bool,
    pub(crate) is_qos: bool,
}

#[derive(Clone)]
pub(crate) struct TransportUnicastInner {
    // Transport config
    pub(super) config: TransportUnicastConfig,
    // Tx conduits
    pub(super) conduit_tx: Arc<[TransportConduitTx]>,
    // Rx conduits
    pub(super) conduit_rx: Arc<[TransportConduitRx]>,
    // The links associated to the channel
    pub(super) links: Arc<RwLock<Box<[TransportLinkUnicast]>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportPeerEventHandler>>>>,
    // Mutex for notification
    pub(super) alive: AsyncArc<AsyncMutex<bool>>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportUnicastStatsAtomic>,
}

impl TransportUnicastInner {
    pub(super) fn make(config: TransportUnicastConfig) -> ZResult<TransportUnicastInner> {
        let mut conduit_tx = vec![];
        let mut conduit_rx = vec![];

        if config.is_qos {
            for c in 0..Priority::NUM {
                conduit_tx.push(TransportConduitTx::make(
                    (c as u8).try_into().unwrap(),
                    config.sn_bytes,
                )?);
            }

            for c in 0..Priority::NUM {
                conduit_rx.push(TransportConduitRx::make(
                    (c as u8).try_into().unwrap(),
                    config.sn_bytes,
                    config.manager.config.defrag_buff_size,
                )?);
            }
        } else {
            conduit_tx.push(TransportConduitTx::make(
                Priority::default(),
                config.sn_bytes,
            )?);

            conduit_rx.push(TransportConduitRx::make(
                Priority::default(),
                config.sn_bytes,
                config.manager.config.defrag_buff_size,
            )?);
        }

        let initial_sn = ConduitSn {
            reliable: config.initial_sn_tx,
            best_effort: config.initial_sn_tx,
        };
        for c in conduit_tx.iter() {
            let _ = c.sync(initial_sn)?;
        }

        let t = TransportUnicastInner {
            config,
            conduit_tx: conduit_tx.into_boxed_slice().into(),
            conduit_rx: conduit_rx.into_boxed_slice().into(),
            links: Arc::new(RwLock::new(vec![].into_boxed_slice())),
            callback: Arc::new(RwLock::new(None)),
            alive: AsyncArc::new(AsyncMutex::new(false)),
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
    pub(super) async fn sync(&self, initial_sn_rx: ZInt) -> ZResult<()> {
        // Mark the transport as alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = zasynclock!(self.alive);
        if *a_guard {
            let e = zerror!("Transport already synched with peer: {}", self.config.zid);
            log::trace!("{}", e);
            return Err(e.into());
        }

        *a_guard = true;

        let csn = ConduitSn {
            reliable: initial_sn_rx,
            best_effort: initial_sn_rx,
        };
        for c in self.conduit_rx.iter() {
            let _ = c.sync(csn)?;
        }

        Ok(())
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn delete(&self) -> ZResult<()> {
        log::debug!(
            "[{}] Closing transport with peer: {}",
            self.config.manager.config.zid,
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
        let _ = self
            .config
            .manager
            .del_transport_unicast(&self.config.zid)
            .await;

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
            let limit = self.config.manager.config.unicast.max_links;

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
        keep_alive: Duration,
        batch_size: u16,
    ) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                assert!(!self.conduit_tx.is_empty());
                l.start_tx(keep_alive, batch_size, self.conduit_tx.clone());
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

    pub(super) fn start_rx(&self, link: &LinkUnicast, lease: Duration) -> ZResult<()> {
        let mut guard = zwrite!(self.links);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                l.start_rx(lease);
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
                    // Notify the callback
                    if let Some(callback) = zread!(self.callback).as_ref() {
                        callback.del_link(Link::from(link));
                    }
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

    pub(crate) fn get_sn_bytes(&self) -> SeqNumBytes {
        self.config.sn_bytes
    }

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
    pub(crate) async fn close_link(&self, link: &LinkUnicast, reason: CloseReason) -> ZResult<()> {
        log::trace!("Closing link {} with peer: {}", link, self.config.zid);

        let guard = zread!(self.links);
        if let Some(l) = zlinkget!(guard, link) {
            let mut pipeline = l.pipeline.clone();
            // Drop the guard
            drop(guard);

            // Schedule the close message for transmission
            if let Some(pipeline) = pipeline.take() {
                // Close message to be sent on the target link
                let msg = Close::new(reason);
                pipeline.push_transport_message(msg, Priority::Background);
            }

            // Remove the link from the channel
            self.del_link(link).await?;
        }

        Ok(())
    }

    pub(crate) async fn close(&self, reason: CloseReason) -> ZResult<()> {
        log::trace!("Closing transport with peer: {}", self.config.zid);

        let mut pipelines: Vec<Arc<TransmissionPipeline>> = zread!(self.links)
            .iter()
            .filter_map(|sl| sl.pipeline.clone())
            .collect();
        for p in pipelines.drain(..) {
            // Close message to be sent on all the links
            let msg = Close::new(reason);
            p.push_transport_message(msg, Priority::Background);
        }
        // Terminate and clean up the transport
        self.delete().await
    }

    /*************************************/
    /*        SCHEDULE AND SEND TX       */
    /*************************************/
    /// Schedule a Zenoh message on the transmission queue    
    pub(crate) fn schedule(&self, mut message: ZenohMessage) -> bool {
        #[cfg(feature = "shared-memory")]
        {
            let res = if self.config.is_shm {
                message.map_to_shminfo()
            } else {
                message.map_to_shmbuf(self.config.manager.shmr.clone())
            };
            if let Err(e) = res {
                log::trace!("Failed SHM conversion: {}", e);
                return false;
            }
        }

        self.schedule_first_fit(message)
    }

    pub(crate) fn get_links(&self) -> Vec<LinkUnicast> {
        zread!(self.links).iter().map(|l| l.link.clone()).collect()
    }
}
