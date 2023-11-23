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
#[cfg(feature = "transport_unixpipe")]
use super::link::send_with_link;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::{
    unicast::{
        link::TransportLinkUnicast, transport_unicast_inner::TransportUnicastTrait,
        TransportConfigUnicast,
    },
    TransportExecutor, TransportManager, TransportPeerEventHandler,
};
use async_executor::Task;
#[cfg(feature = "transport_unixpipe")]
use async_std::sync::RwLockUpgradableReadGuard;
use async_std::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard, RwLock};
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::sync::{Arc, RwLock as SyncRwLock};
use std::time::Duration;
#[cfg(feature = "transport_unixpipe")]
use zenoh_core::zasyncread_upgradable;
use zenoh_core::{zasynclock, zasyncread, zasyncwrite, zread, zwrite};
#[cfg(feature = "transport_unixpipe")]
use zenoh_link::unixpipe::UNIXPIPE_LOCATOR_PREFIX;
#[cfg(feature = "transport_unixpipe")]
use zenoh_link::Link;
use zenoh_protocol::network::NetworkMessage;
use zenoh_protocol::transport::TransportBodyLowLatency;
use zenoh_protocol::transport::TransportMessageLowLatency;
use zenoh_protocol::transport::{Close, TransportSn};
use zenoh_protocol::{
    core::{WhatAmI, ZenohId},
    transport::close,
};
#[cfg(not(feature = "transport_unixpipe"))]
use zenoh_result::bail;
use zenoh_result::{zerror, ZResult};

/*************************************/
/*       LOW-LATENCY TRANSPORT       */
/*************************************/
#[derive(Clone)]
pub(crate) struct TransportUnicastLowlatency {
    // Transport Manager
    pub(super) manager: TransportManager,
    // Transport config
    pub(super) config: TransportConfigUnicast,
    // The link associated to the transport
    pub(super) link: Arc<RwLock<TransportLinkUnicast>>,
    // The callback
    pub(super) callback: Arc<SyncRwLock<Option<Arc<dyn TransportPeerEventHandler>>>>,
    // Mutex for notification
    alive: Arc<AsyncMutex<bool>>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportStats>,

    // The flags to stop TX/RX tasks
    pub(crate) handle_keepalive: Arc<RwLock<Option<Task<()>>>>,
    pub(crate) handle_rx: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl TransportUnicastLowlatency {
    pub fn make(
        manager: TransportManager,
        config: TransportConfigUnicast,
        link: TransportLinkUnicast,
    ) -> ZResult<TransportUnicastLowlatency> {
        #[cfg(feature = "stats")]
        let stats = Arc::new(TransportStats::new(Some(manager.get_stats().clone())));
        let t = TransportUnicastLowlatency {
            manager,
            config,
            link: Arc::new(RwLock::new(link)),
            callback: Arc::new(SyncRwLock::new(None)),
            alive: Arc::new(AsyncMutex::new(false)),
            #[cfg(feature = "stats")]
            stats,
            handle_keepalive: Arc::new(RwLock::new(None)),
            handle_rx: Arc::new(RwLock::new(None)),
        };

        Ok(t)
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn finalize(&self, reason: u8) -> ZResult<()> {
        log::debug!(
            "[{}] Finalizing transport with peer: {}",
            self.manager.config.zid,
            self.config.zid
        );

        // Send close message on the link
        let close = TransportMessageLowLatency {
            body: TransportBodyLowLatency::Close(Close {
                reason,
                session: false,
            }),
        };
        let _ = self.send_async(close).await;

        // Terminate and clean up the transport
        self.delete().await
    }

    pub(super) async fn delete(&self) -> ZResult<()> {
        log::debug!(
            "[{}] Deleting transport with peer: {}",
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
        self.stop_keepalive().await;
        self.stop_rx().await;
        let _ = zasyncwrite!(self.link)
            .close(Some(close::reason::GENERIC))
            .await;

        // Notify the callback that we have closed the transport
        if let Some(cb) = callback.as_ref() {
            cb.closed();
        }

        Ok(())
    }
}

#[async_trait]
impl TransportUnicastTrait for TransportUnicastLowlatency {
    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    fn set_callback(&self, callback: Arc<dyn TransportPeerEventHandler>) {
        let mut guard = zwrite!(self.callback);
        *guard = Some(callback);
    }

    async fn get_alive(&self) -> AsyncMutexGuard<'_, bool> {
        zasynclock!(self.alive)
    }

    fn get_links(&self) -> Vec<TransportLinkUnicast> {
        let guard = async_std::task::block_on(async { zasyncread!(self.link) });
        [guard.clone()].to_vec()
    }

    fn get_zid(&self) -> ZenohId {
        self.config.zid
    }

    fn get_whatami(&self) -> WhatAmI {
        self.config.whatami
    }

    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool {
        self.config.is_shm
    }

    fn is_qos(&self) -> bool {
        self.config.is_qos
    }

    fn get_callback(&self) -> Option<Arc<dyn TransportPeerEventHandler>> {
        zread!(self.callback).clone()
    }

    fn get_config(&self) -> &TransportConfigUnicast {
        &self.config
    }

    #[cfg(feature = "stats")]
    fn stats(&self) -> std::sync::Arc<crate::stats::TransportStats> {
        self.stats.clone()
    }

    /*************************************/
    /*                TX                 */
    /*************************************/
    fn schedule(&self, msg: NetworkMessage) -> ZResult<()> {
        self.internal_schedule(msg)
    }

    fn start_tx(
        &self,
        _link: &TransportLinkUnicast,
        executor: &TransportExecutor,
        keep_alive: Duration,
    ) -> ZResult<()> {
        self.start_keepalive(executor, keep_alive);
        Ok(())
    }

    fn start_rx(&self, _link: &TransportLinkUnicast, lease: Duration) -> ZResult<()> {
        self.internal_start_rx(lease);
        Ok(())
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    async fn add_link(&self, link: TransportLinkUnicast) -> ZResult<()> {
        log::trace!("Adding link: {}", link);

        #[cfg(not(feature = "transport_unixpipe"))]
        bail!(
            "Can not add Link {} with peer {}: link already exists and only unique link is supported!",
            link,
            self.config.zid,
        );

        #[cfg(feature = "transport_unixpipe")]
        {
            let guard = zasyncread_upgradable!(self.link);

            let existing_unixpipe =
                guard.link.get_dst().protocol().as_str() == UNIXPIPE_LOCATOR_PREFIX;
            let new_unixpipe = link.link.get_dst().protocol().as_str() == UNIXPIPE_LOCATOR_PREFIX;
            match (existing_unixpipe, new_unixpipe) {
                (false, true) => {
                    // LowLatency transport suports only a single link, but code here also handles upgrade from non-unixpipe link to unixpipe link!
                    log::trace!(
                        "Upgrading {} LowLatency transport's link from {} to {}",
                        self.config.zid,
                        guard,
                        link
                    );

                    // Prepare and send close message on old link
                    {
                        let close = TransportMessageLowLatency {
                            body: TransportBodyLowLatency::Close(Close {
                                reason: 0,
                                session: false,
                            }),
                        };
                        let _ = send_with_link(
                            &guard,
                            close,
                            #[cfg(feature = "stats")]
                            &self.stats,
                        )
                        .await;
                    };
                    // Notify the callback
                    if let Some(callback) = zread!(self.callback).as_ref() {
                        callback.del_link(Link::from(guard.clone()));
                    }

                    // Set the new link
                    let mut write_guard = RwLockUpgradableReadGuard::upgrade(guard).await;
                    *write_guard = link;

                    Ok(())
                }
                _ => {
                    let e = zerror!(
                    "Can not add Link {} with peer {}: link already exists and only unique link is supported!",
                    link,
                    self.config.zid,
                );
                    Err(e.into())
                }
            }
        }
    }

    /*************************************/
    /*           INITIATION              */
    /*************************************/
    async fn sync(&self, _initial_sn_rx: TransportSn) -> ZResult<()> {
        // Mark the transport as alive
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
    async fn close_link(&self, link: &TransportLinkUnicast, reason: u8) -> ZResult<()> {
        log::trace!("Closing link {} with peer: {}", link, self.config.zid);
        self.finalize(reason).await
    }

    async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing transport with peer: {}", self.config.zid);
        self.finalize(reason).await
    }
}
