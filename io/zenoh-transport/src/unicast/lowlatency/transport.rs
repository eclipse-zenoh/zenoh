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
use std::{
    sync::{Arc, RwLock as SyncRwLock},
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard, RwLock};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use zenoh_core::{zasynclock, zasyncread, zasyncwrite, zread, zwrite};
use zenoh_link::Link;
use zenoh_protocol::{
    core::{WhatAmI, ZenohIdProto},
    network::NetworkMessageMut,
    transport::{
        close, Close, TransportBodyLowLatencyRef, TransportMessageLowLatencyRef, TransportSn,
    },
};
use zenoh_result::{zerror, ZResult};

#[cfg(feature = "shared-memory")]
use crate::shm_context::UnicastTransportShmContext;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::{
    unicast::{
        authentication::TransportAuthId,
        link::{LinkUnicastWithOpenAck, TransportLinkUnicast},
        transport_unicast_inner::{AddLinkResult, TransportUnicastTrait},
        TransportConfigUnicast,
    },
    TransportManager, TransportPeerEventHandler,
};

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
    pub(crate) token: CancellationToken,
    pub(crate) tracker: TaskTracker,

    #[cfg(feature = "shared-memory")]
    pub(super) shm_context: Option<UnicastTransportShmContext>,
}

impl TransportUnicastLowlatency {
    pub fn make(
        manager: TransportManager,
        config: TransportConfigUnicast,
        #[cfg(feature = "shared-memory")] shm_context: Option<UnicastTransportShmContext>,
        #[cfg(feature = "stats")] stats: Arc<TransportStats>,
    ) -> Arc<dyn TransportUnicastTrait> {
        Arc::new(TransportUnicastLowlatency {
            manager,
            config,
            link: Arc::new(RwLock::new(None)),
            callback: Arc::new(SyncRwLock::new(None)),
            alive: Arc::new(AsyncMutex::new(false)),
            #[cfg(feature = "stats")]
            stats,
            token: CancellationToken::new(),
            tracker: TaskTracker::new(),
            #[cfg(feature = "shared-memory")]
            shm_context,
        }) as Arc<dyn TransportUnicastTrait>
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn finalize(&self, reason: u8) -> ZResult<()> {
        tracing::debug!(
            "[{}] Finalizing transport with peer: {}",
            self.manager.config.zid,
            self.config.zid
        );

        // Send close message on the link
        let close = TransportMessageLowLatencyRef {
            body: TransportBodyLowLatencyRef::Close(Close {
                reason,
                session: false,
            }),
        };
        let _ = self.send_async(close).await;

        // Terminate and clean up the transport
        self.delete().await
    }

    pub(super) async fn delete(&self) -> ZResult<()> {
        tracing::debug!(
            "[{}] Deleting transport with peer: {}",
            self.manager.config.zid,
            self.config.zid
        );
        // Mark the transport as no longer alive and keep the lock
        // to avoid concurrent new_transport and closing/closed notifications
        let mut a_guard = self.get_alive().await;
        *a_guard = false;
        let callback = zwrite!(self.callback).take();

        // Delete the transport on the manager
        let _ = self.manager.del_transport_unicast(&self.config.zid).await;

        // Close and drop the link
        self.token.cancel();
        self.tracker.close();
        self.tracker.wait().await;
        // self.stop_keepalive().await;
        // self.stop_rx().await;

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
            tracing::trace!("{}", e);
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
        let handle = tokio::runtime::Handle::current();
        let guard =
            tokio::task::block_in_place(|| handle.block_on(async { zasyncread!(self.link) }));
        guard.as_ref().map(|l| vec![l.link()]).unwrap_or_default()
    }

    fn get_zid(&self) -> ZenohIdProto {
        self.config.zid
    }

    fn get_auth_ids(&self) -> TransportAuthId {
        // Convert LinkUnicast auth id to AuthId
        let mut transport_auth_id = TransportAuthId::new(self.get_zid());
        let handle = tokio::runtime::Handle::current();
        let guard =
            tokio::task::block_in_place(|| handle.block_on(async { zasyncread!(self.link) }));
        if let Some(val) = guard.as_ref() {
            transport_auth_id.push_link_auth_id(val.link.get_auth_id().clone());
        }
        // Convert usrpwd auth id to AuthId
        #[cfg(feature = "auth_usrpwd")]
        transport_auth_id.set_username(&self.config.auth_id);
        transport_auth_id
    }

    fn get_whatami(&self) -> WhatAmI {
        self.config.whatami
    }

    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool {
        self.config.shm.is_some()
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
    fn stats(&self) -> Arc<TransportStats> {
        self.stats.clone()
    }

    #[cfg(feature = "stats")]
    fn get_link_stats(&self) -> Vec<(Link, Arc<TransportStats>)> {
        self.get_links()
            .into_iter()
            .map(|l| (l, self.stats.clone()))
            .collect()
    }

    /*************************************/
    /*                TX                 */
    /*************************************/
    fn schedule(&self, msg: NetworkMessageMut) -> ZResult<()> {
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
        tracing::trace!("Adding link: {}", link);

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
        let start_tx = Box::new(move || {
            // start keepalive task
            let keep_alive =
                self.manager.config.unicast.lease / self.manager.config.unicast.keep_alive as u32;
            self.start_keepalive(keep_alive);
        });

        let start_rx = Box::new(move || {
            // start RX task
            self.internal_start_rx(other_lease);
        });

        Ok((start_tx, start_rx, ack, None))
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    async fn close(&self, reason: u8) -> ZResult<()> {
        tracing::trace!("Closing transport with peer: {}", self.config.zid);
        self.finalize(reason).await
    }
}
