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
use super::link::send_with_link;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::transport_unicast_inner::TransportUnicastTrait;
use crate::TransportConfigUnicast;
use crate::TransportManager;
use crate::TransportPeerEventHandler;
use async_trait::async_trait;
use std::sync::{Arc, RwLock as SyncRwLock};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard, RwLock};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use zenoh_core::zasyncwrite;
use zenoh_core::{zasynclock, zasyncread, zread, zwrite};
#[cfg(feature = "transport_unixpipe")]
use zenoh_link::unixpipe::UNIXPIPE_LOCATOR_PREFIX;
#[cfg(feature = "transport_unixpipe")]
use zenoh_link::Link;
use zenoh_protocol::network::NetworkMessage;
use zenoh_protocol::transport::KeepAlive;
use zenoh_protocol::transport::TransportBodyLowLatency;
use zenoh_protocol::transport::TransportMessageLowLatency;
use zenoh_protocol::transport::{Close, TransportSn};
use zenoh_protocol::{
    core::{WhatAmI, ZenohId},
    transport::close,
};
use zenoh_result::{zerror, ZResult};
use zenoh_runtime::ZRuntime;

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

    pub(crate) msg_queue_tx: Arc<SyncRwLock<Option<flume::Sender<TransportMessageLowLatency>>>>,
    pub(crate) cancellation_token: CancellationToken,
    pub(crate) task_tracker: TaskTracker,
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
            msg_queue_tx: Arc::new(SyncRwLock::new(None)),
            cancellation_token: CancellationToken::new(),
            task_tracker: TaskTracker::new(),
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
        self.cancellation_token.cancel();
        self.task_tracker.close();
        self.task_tracker.wait().await;
        zasyncread!(self.link).close().await?;

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

    fn get_links(&self) -> Vec<LinkUnicast> {
        let handle = tokio::runtime::Handle::current();
        let guard =
            tokio::task::block_in_place(|| handle.block_on(async { zasyncread!(self.link) }));
        // let guard = zenoh_runtime::ZRuntime::Accept
        //     .handle()
        //     .block_on(async { zasyncread!(self.link) });
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

    fn start_tx(&self, _link: &LinkUnicast, keep_alive: Duration, _batch_size: u16) -> ZResult<()> {
        let token = self.cancellation_token.child_token();
        let mut interval = tokio::time::interval(keep_alive);

        // TODO: Check the necessity of clone
        let link = self.link.clone();
        let transport = self.clone();

        // TODO: Dangerous?
        let (tx, rx) = flume::unbounded();
        *self.msg_queue_tx.try_write().map_err(|e| zerror!("{e}"))? = Some(tx);

        let task = async move {
            loop {
                tokio::select! {
                    // Send out the message
                    res = rx.recv_async() => {
                        let msg = res?;
                        transport.send_async(msg).await?;
                    }

                    // Send KeepAlive periodically
                    _ = interval.tick() => {
                        // TODO: Don not create a new message every time
                        let keepailve = TransportMessageLowLatency {
                            body: TransportBodyLowLatency::KeepAlive(KeepAlive),
                        };
                        // TODO: Check the necessity of this guard
                        let guard = zasyncwrite!(link);

                        // TODO: Why this is bounded by the feature?
                        let _ = send_with_link(
                            &guard,
                            keepailve,
                            #[cfg(feature = "stats")]
                            &stats,
                        )
                        .await;
                        drop(guard);
                    }

                    // Stop if cancelled
                    _ = token.cancelled() => {
                        break
                    }
                }
            }
            ZResult::Ok(())
        };
        self.task_tracker.spawn_on(task, &ZRuntime::TX);
        Ok(())
    }

    fn start_rx(&self, _link: &LinkUnicast, lease: Duration, batch_size: u16) -> ZResult<()> {
        self.internal_start_rx(lease, batch_size);
        Ok(())
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

        #[cfg(feature = "transport_unixpipe")]
        {
            // For performance reasons, it first performs a try_write() and,
            // if it fails, it falls back on write().await
            let guard = if let Ok(g) = self.link.try_read() {
                g
            } else {
                self.link.read().await
            };

            let existing_unixpipe = guard.get_dst().protocol().as_str() == UNIXPIPE_LOCATOR_PREFIX;
            let new_unixpipe = link.get_dst().protocol().as_str() == UNIXPIPE_LOCATOR_PREFIX;
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
                    let mut write_guard = self.link.write().await;
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
