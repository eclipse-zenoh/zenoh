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
use super::super::authentication::AuthId;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::{
    unicast::{
        link::{LinkUnicastWithOpenAck, TransportLinkUnicast},
        transport_unicast_inner::{AddLinkResult, TransportUnicastTrait},
        TransportConfigUnicast,
    },
    TransportManager, TransportPeerEventHandler,
};
use async_executor::Task;
use async_std::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard, RwLock};
use async_std::task::JoinHandle;
use async_trait::async_trait;
use std::sync::{Arc, RwLock as SyncRwLock};
use std::time::Duration;
use zenoh_core::{zasynclock, zasyncread, zasyncwrite, zread, zwrite};
use zenoh_link::Link;
use zenoh_protocol::network::NetworkMessage;
use zenoh_protocol::transport::TransportBodyLowLatency;
use zenoh_protocol::transport::TransportMessageLowLatency;
use zenoh_protocol::transport::{Close, TransportSn};
use zenoh_protocol::{
    core::{WhatAmI, ZenohId},
    transport::close,
};
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
    pub(super) link: Arc<RwLock<Option<TransportLinkUnicast>>>,
    // The callback
    pub(super) callback: Arc<SyncRwLock<Option<Arc<dyn TransportPeerEventHandler>>>>,
    // Mutex for notification
    alive: Arc<AsyncMutex<bool>>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportStats>,

    // The handles for TX/RX tasks
    pub(crate) handle_keepalive: Arc<RwLock<Option<Task<()>>>>,
    pub(crate) handle_rx: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl TransportUnicastLowlatency {
    pub fn make(
        manager: TransportManager,
        config: TransportConfigUnicast,
    ) -> Arc<dyn TransportUnicastTrait> {
        #[cfg(feature = "stats")]
        let stats = Arc::new(TransportStats::new(Some(manager.get_stats().clone())));
        Arc::new(TransportUnicastLowlatency {
            manager,
            config,
            link: Arc::new(RwLock::new(None)),
            callback: Arc::new(SyncRwLock::new(None)),
            alive: Arc::new(AsyncMutex::new(false)),
            #[cfg(feature = "stats")]
            stats,
            handle_keepalive: Arc::new(RwLock::new(None)),
            handle_rx: Arc::new(RwLock::new(None)),
        }) as Arc<dyn TransportUnicastTrait>
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
        if let Some(val) = zasyncwrite!(self.link).as_ref() {
            let _ = val.close(Some(close::reason::GENERIC)).await;
        }

        // Notify the callback that we have closed the transport
        if let Some(cb) = callback.as_ref() {
            cb.closed();
        }

        Ok(())
    }

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
}

#[async_trait]
impl TransportUnicastTrait for TransportUnicastLowlatency {
    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    fn set_callback(&self, callback: Arc<dyn TransportPeerEventHandler>) {
        *zwrite!(self.callback) = Some(callback);
    }

    async fn get_alive(&self) -> AsyncMutexGuard<'_, bool> {
        zasynclock!(self.alive)
    }

    fn get_links(&self) -> Vec<Link> {
        let guard = async_std::task::block_on(async { zasyncread!(self.link) });
        if let Some(val) = guard.as_ref() {
            return [val.link()].to_vec();
        }
        vec![]
    }

    fn get_zid(&self) -> ZenohId {
        self.config.zid
    }

    fn get_auth_ids(&self) -> Vec<AuthId> {
        vec![]
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

    /*************************************/
    /*               LINK                */
    /*************************************/
    async fn add_link(
        &self,
        link: LinkUnicastWithOpenAck,
        other_initial_sn: TransportSn,
        other_lease: Duration,
    ) -> AddLinkResult {
        log::trace!("Adding link: {}", link);

        let _ = self.sync(other_initial_sn).await;

        let mut guard = zasyncwrite!(self.link);
        if guard.is_some() {
            return Err((
                zerror!("Lowlatency transport cannot support more than one link!").into(),
                link.fail(),
                close::reason::GENERIC,
            ));
        }
        let (link, ack) = link.unpack();
        *guard = Some(link);
        drop(guard);

        // create a callback to start the link
        let start_link = Box::new(move || {
            // start keepalive task
            let keep_alive =
                self.manager.config.unicast.lease / self.manager.config.unicast.keep_alive as u32;
            self.start_keepalive(&self.manager.tx_executor, keep_alive);

            // start RX task
            self.internal_start_rx(other_lease);
        });

        return Ok((start_link, ack));
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    async fn close_link(&self, link: Link, reason: u8) -> ZResult<()> {
        log::trace!("Closing link {} with peer: {}", link, self.config.zid);
        self.finalize(reason).await
    }

    async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!("Closing transport with peer: {}", self.config.zid);
        self.finalize(reason).await
    }
}
