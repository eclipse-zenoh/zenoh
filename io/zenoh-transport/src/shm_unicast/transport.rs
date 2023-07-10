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
use super::super::TransportManager;
use super::super::{TransportExecutor, TransportPeerEventHandler};
use super::link::ShmTransportLinkUnicast;
#[cfg(feature = "stats")]
use super::TransportUnicastStatsAtomic;
use crate::shm_unicast::oam_extensions::pack_oam_close;
use crate::TransportConfigUnicast;
use async_std::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zenoh_core::{zasynclock, zread, zwrite};
use zenoh_link::{Link, LinkUnicast, LinkUnicastDirection};
use zenoh_protocol::core::{Bits, WhatAmI, ZenohId};
use zenoh_protocol::transport::Close;
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

/*************************************/
/*             TRANSPORT             */
/*************************************/
#[derive(Clone)]
pub(crate) struct ShmTransportUnicastInner {
    // Transport Manager
    pub(crate) manager: TransportManager,
    // Transport config
    pub(super) config: TransportConfigUnicast,
    // The link associated to the channel
    pub(super) link: Arc<RwLock<Option<ShmTransportLinkUnicast>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportPeerEventHandler>>>>,
    // Mutex for notification
    pub(super) alive: Arc<AsyncMutex<bool>>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportUnicastStatsAtomic>,
}

impl ShmTransportUnicastInner {
    pub(super) fn make(
        manager: TransportManager,
        config: TransportConfigUnicast,
    ) -> ZResult<ShmTransportUnicastInner> {
        let t = ShmTransportUnicastInner {
            manager,
            config,
            link: Arc::new(RwLock::new(None)),
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

    pub(super) fn get_link(&self) -> Option<LinkUnicast> {
        zread!(self.link).as_ref().map(|l| l.link.clone())
    }

    /*************************************/
    /*           INITIATION              */
    /*************************************/
    pub(super) async fn sync(&self) -> ZResult<()> {
        // Mark the transport as alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = zasynclock!(self.alive);
        if *a_guard {
            let e = zerror!("Transport already synched with peer: {}", self.config.zid);
            log::trace!("{}", e);
            return Err(e.into());
        }

        *a_guard = true;

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

        // Close and drop the link
        let l = {
            let mut l_guard = zwrite!(self.link);
            l_guard.take()
        };
        if let Some(l) = l {
            l.close().await;
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
        let mut guard = zwrite!(self.link);
        match guard.as_ref() {
            Some(_) => {
                let e = zerror!(
                    "Can not add Link {} with peer {}: link already exists and only unique link is supported!",
                    link,
                    self.config.zid,
                );
                return Err(e.into());
            }
            None => {
                // Create a channel link from a link
                let link = ShmTransportLinkUnicast::new(self.clone(), link, direction);
                *guard = Some(link);
            }
        }

        Ok(())
    }

    pub(super) fn start_tx(
        &self,
        link: &LinkUnicast,
        executor: &TransportExecutor,
        keep_alive: Duration,
    ) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                l.start_keepalive(executor, keep_alive);
                Ok(())
            }
            None => {
                bail!(
                    "Can not start Link TX(keepalive) {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
        }
    }

    pub(super) fn stop_tx(&self, link: &LinkUnicast) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match zlinkgetmut!(guard, link) {
            Some(l) => {
                l.stop_keepalive();
                Ok(())
            }
            None => {
                bail!(
                    "Can not stop Link TX(keepalive) {} with peer: {}",
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
        let mut guard = zwrite!(self.link);
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
        let mut guard = zwrite!(self.link);
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
        // check if the link is ours
        {
            let guard = zwrite!(self.link);
            if let None = zlinkget!(guard, link) {
                bail!(
                    "Can not delete Link {} with peer: {}",
                    link,
                    self.config.zid
                )
            }
            drop(guard);
        }

        // Notify the callback
        if let Some(callback) = zread!(self.callback).as_ref() {
            callback.del_link(Link::from(link));
        }

        self.delete().await
    }
}

impl ShmTransportUnicastInner {
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

        let guard = zread!(self.link);

        let l = zlinkget!(guard, link)
            .ok_or_else(|| zerror!("Cannot close Link {:?}: not found", link))?;

        let close = pack_oam_close(Close {
            reason,
            session: false,
        })?;
        l.send(close).await;

        // Remove the link from the channel
        self.del_link(link).await
    }

    pub(crate) async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing transport with peer: {}", self.config.zid);

        if let Some(l) = zwrite!(self.link).take() {
            let close = pack_oam_close(Close {
                reason,
                session: false,
            })?;
            l.send(close).await;
        }

        // Terminate and clean up the transport
        self.delete().await
    }
}
