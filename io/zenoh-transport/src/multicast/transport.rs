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
    cmp::min,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use tokio_util::sync::CancellationToken;
use zenoh_core::{zcondfeat, zread, zwrite};
use zenoh_link::{Link, Locator};
use zenoh_protocol::{
    core::{Bits, Field, Priority, Resolution, WhatAmI, ZenohIdProto},
    transport::{batch_size, close, join::ext::PatchType, Close, Join, TransportMessage},
};
use zenoh_result::{bail, ZResult};
use zenoh_task::TaskController;

use super::{
    common::priority::{TransportPriorityRx, TransportPriorityTx},
    link::{TransportLinkMulticastConfigUniversal, TransportLinkMulticastUniversal},
};
#[cfg(feature = "shared-memory")]
use crate::shm_context::MulticastTransportShmContext;
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::{
    multicast::{
        link::TransportLinkMulticast, TransportConfigMulticast, TransportMulticastEventHandler,
    },
    TransportManager, TransportPeer, TransportPeerEventHandler,
};
// use zenoh_util::{Timed, TimedEvent, TimedHandle, Timer};

/*************************************/
/*             TRANSPORT             */
/*************************************/
#[derive(Clone)]
pub(super) struct TransportMulticastPeer {
    pub(super) version: u8,
    pub(super) locator: Locator,
    pub(super) zid: ZenohIdProto,
    pub(super) whatami: WhatAmI,
    pub(super) resolution: Resolution,
    pub(super) lease: Duration,
    pub(super) is_active: Arc<AtomicBool>,
    token: CancellationToken,
    pub(super) priority_rx: Box<[TransportPriorityRx]>,
    pub(super) handler: Arc<dyn TransportPeerEventHandler>,
    pub(super) patch: PatchType,
}

impl TransportMulticastPeer {
    pub(super) fn set_active(&self) {
        self.is_active.store(true, Ordering::Release);
    }

    pub(super) fn is_qos(&self) -> bool {
        self.priority_rx.len() == Priority::NUM
    }
}

#[derive(Clone)]
pub(crate) struct TransportMulticastInner {
    // The manager this channel is associated to
    pub(super) manager: TransportManager,
    // Tx priorities
    pub(super) priority_tx: Arc<[TransportPriorityTx]>,
    // Remote peers
    pub(super) peers: Arc<RwLock<HashMap<Locator, TransportMulticastPeer>>>,
    // The multicast locator - Convenience for logging
    pub(super) locator: Locator,
    // The multicast link
    pub(super) link: Arc<RwLock<Option<TransportLinkMulticastUniversal>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportMulticastEventHandler>>>>,
    // Task controller for safe task cancellation
    task_controller: TaskController,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportStats>,

    #[cfg(feature = "shared-memory")]
    pub(super) shm_context: Option<MulticastTransportShmContext>,
}

impl TransportMulticastInner {
    pub(super) fn make(
        manager: TransportManager,
        config: TransportConfigMulticast,

        #[cfg(feature = "shared-memory")] shm_context: Option<MulticastTransportShmContext>,
    ) -> ZResult<TransportMulticastInner> {
        let mut priority_tx = vec![];
        if (config.initial_sns.len() != 1) != (config.initial_sns.len() != Priority::NUM) {
            for sn in config.initial_sns.iter() {
                let tct = TransportPriorityTx::make(config.sn_resolution)?;
                tct.sync(*sn)?;
                priority_tx.push(tct);
            }
        } else {
            bail!("Invalid QoS configuration");
        }

        #[cfg(feature = "stats")]
        let stats = TransportStats::new(Some(Arc::downgrade(&manager.get_stats())), HashMap::new());

        let ti = TransportMulticastInner {
            manager,
            priority_tx: priority_tx.into_boxed_slice().into(),
            peers: Arc::new(RwLock::new(HashMap::new())),
            locator: config.link.link.get_dst().to_owned(),
            link: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            task_controller: TaskController::default(),
            #[cfg(feature = "stats")]
            stats,
            #[cfg(feature = "shared-memory")]
            shm_context,
        };

        let link = TransportLinkMulticastUniversal::new(ti.clone(), config.link);
        let mut guard = zwrite!(ti.link);
        *guard = Some(link);
        drop(guard);

        Ok(ti)
    }

    pub(super) fn set_callback(&self, callback: Arc<dyn TransportMulticastEventHandler>) {
        let mut guard = zwrite!(self.callback);
        *guard = Some(callback);
    }

    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(crate) fn get_sn_resolution(&self) -> Bits {
        self.manager.config.resolution.get(Field::FrameSN)
    }

    pub(crate) fn is_qos(&self) -> bool {
        self.priority_tx.len() == Priority::NUM
    }

    #[cfg(feature = "shared-memory")]
    pub(crate) fn is_shm(&self) -> bool {
        self.shm_context.is_some()
    }

    pub(crate) fn get_callback(&self) -> Option<Arc<dyn TransportMulticastEventHandler>> {
        zread!(self.callback).clone()
    }

    pub(crate) fn get_link(&self) -> TransportLinkMulticast {
        zread!(self.link).as_ref().unwrap().link.clone()
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn delete(&self) -> ZResult<()> {
        tracing::debug!("Closing multicast transport on {:?}", self.locator);

        let callback = zwrite!(self.callback).take();

        // Delete the transport on the manager
        let _ = self.manager.del_transport_multicast(&self.locator).await;

        // Close all the links
        let mut link = zwrite!(self.link).take();
        if let Some(l) = link.take() {
            let _ = l.close().await;
        }

        // Notify the callback that we have closed the transport
        if let Some(cb) = callback.as_ref() {
            cb.closed();
        }

        self.task_controller.terminate_all_async().await;

        Ok(())
    }

    pub(crate) async fn close(&self, reason: u8) -> ZResult<()> {
        tracing::trace!(
            "Closing multicast transport of peer {}: {}",
            self.manager.config.zid,
            self.locator
        );

        {
            let r_guard = zread!(self.link);
            if let Some(link) = r_guard.as_ref() {
                if let Some(pipeline) = link.pipeline.as_ref() {
                    let pipeline = pipeline.clone();
                    drop(r_guard);
                    // Close message to be sent on all the links
                    let msg: TransportMessage = Close {
                        reason,
                        session: false,
                    }
                    .into();
                    pipeline.push_transport_message(msg, Priority::Background);
                }
            }
        }

        // Terminate and clean up the transport
        self.delete().await
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(super) fn start_tx(&self) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match guard.as_mut() {
            Some(l) => {
                // For cross-system compatibility reasons we set the default minimal
                // batch size to 8192 bytes unless explicitly configured smaller.
                let batch_size = self
                    .manager
                    .config
                    .batch_size
                    .min(l.link.link.get_mtu())
                    .min(batch_size::MULTICAST);
                let config = TransportLinkMulticastConfigUniversal {
                    version: self.manager.config.version,
                    zid: self.manager.config.zid,
                    whatami: self.manager.config.whatami,
                    lease: self.manager.config.multicast.lease,
                    join_interval: self.manager.config.multicast.join_interval,
                    sn_resolution: self.manager.config.resolution.get(Field::FrameSN),
                    batch_size,
                };
                l.start_tx(config, self.priority_tx.clone());
                Ok(())
            }
            None => {
                bail!(
                    "Can not start multicast Link TX of peer {}: {}",
                    self.manager.config.zid,
                    self.locator
                )
            }
        }
    }

    pub(super) fn stop_tx(&self) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match guard.as_mut() {
            Some(l) => {
                l.stop_tx();
                Ok(())
            }
            None => {
                bail!(
                    "Can not stop multicast Link TX of peer {}: {}",
                    self.manager.config.zid,
                    self.locator
                )
            }
        }
    }

    pub(super) fn start_rx(&self) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match guard.as_mut() {
            Some(l) => {
                // For cross-system compatibility reasons we set the default minimal
                // batch size to 8192 bytes unless explicitly configured smaller.
                let batch_size = self
                    .manager
                    .config
                    .batch_size
                    .min(l.link.link.get_mtu())
                    .min(batch_size::MULTICAST);
                l.start_rx(batch_size);
                Ok(())
            }
            None => {
                bail!(
                    "Can not start multicast Link RX of peer {}: {}",
                    self.manager.config.zid,
                    self.locator
                )
            }
        }
    }

    pub(super) fn stop_rx(&self) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match guard.as_mut() {
            Some(l) => {
                l.stop_rx();
                Ok(())
            }
            None => {
                bail!(
                    "Can not stop multicast Link RX of peer {}: {}",
                    self.manager.config.zid,
                    self.locator
                )
            }
        }
    }

    /*************************************/
    /*               PEER                */
    /*************************************/
    pub(super) fn new_peer(&self, locator: &Locator, join: Join) -> ZResult<()> {
        let mut link = Link::new_multicast(&self.get_link().link);
        link.dst = locator.clone();

        let is_shm = zcondfeat!("shared-memory", join.ext_shm.is_some(), false);
        let peer = TransportPeer {
            zid: join.zid,
            whatami: join.whatami,
            is_qos: join.ext_qos.is_some(),
            #[cfg(feature = "shared-memory")]
            is_shm,
            links: vec![link],
        };

        let handler = match zread!(self.callback).as_ref() {
            Some(cb) => cb.new_peer(peer.clone())?,
            None => return Ok(()),
        };

        // Build next SNs
        let next_sns = match join.ext_qos.as_ref() {
            Some(sns) => sns.to_vec(),
            None => vec![join.next_sn],
        }
        .into_boxed_slice();

        let mut priority_rx = Vec::with_capacity(next_sns.len());
        for sn in next_sns.iter() {
            let tprx = TransportPriorityRx::make(
                join.resolution.get(Field::FrameSN),
                self.manager.config.defrag_buff_size,
            )?;
            tprx.sync(*sn)?;
            priority_rx.push(tprx);
        }
        let priority_rx = priority_rx.into_boxed_slice();

        tracing::debug!(
                "New transport joined on {}: zid {}, whatami {}, resolution {:?}, locator {}, is_qos {}, is_shm {}, initial sn: {:?}",
                self.locator,
                peer.zid,
                peer.whatami,
                join.resolution,
                locator,
                peer.is_qos,
                is_shm,
                next_sns,
            );

        // Create lease event
        // TODO(yuyuan): refine the clone behaviors
        let is_active = Arc::new(AtomicBool::new(false));
        let c_is_active = is_active.clone();
        let token = self.task_controller.get_cancellation_token();
        let c_token = token.clone();
        let c_self = self.clone();
        let c_locator = locator.clone();
        let task = async move {
            let mut interval =
                tokio::time::interval_at(tokio::time::Instant::now() + join.lease, join.lease);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !c_is_active.swap(false, Ordering::AcqRel) {
                            break
                        }
                    }
                    _ = c_token.cancelled() => break
                }
            }
            let _ = c_self.del_peer(&c_locator, close::reason::EXPIRED);
        };

        self.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Acceptor, task);

        // TODO(yuyuan): Integrate the above async task into TransportMulticastPeer
        // Store the new peer
        let peer = TransportMulticastPeer {
            version: join.version,
            locator: locator.clone(),
            zid: peer.zid,
            whatami: peer.whatami,
            resolution: join.resolution,
            lease: join.lease,
            is_active,
            token,
            priority_rx,
            handler,
            patch: min(PatchType::CURRENT, join.ext_patch),
        };
        zwrite!(self.peers).insert(locator.clone(), peer);

        Ok(())
    }

    pub(super) fn del_peer(&self, locator: &Locator, reason: u8) -> ZResult<()> {
        let mut guard = zwrite!(self.peers);
        if let Some(peer) = guard.remove(locator) {
            tracing::debug!(
                "Peer {}/{}/{} has left multicast {} with reason: {}",
                peer.zid,
                peer.whatami,
                locator,
                self.locator,
                reason
            );

            // TODO(yuyuan): Unify the termination
            peer.token.cancel();
            drop(guard);
            peer.handler.closed();
        }
        Ok(())
    }

    pub(super) fn get_peers(&self) -> Vec<TransportPeer> {
        zread!(self.peers)
            .values()
            .map(|p| {
                let mut link = Link::new_multicast(&self.get_link().link);
                link.dst = p.locator.clone();

                TransportPeer {
                    zid: p.zid,
                    whatami: p.whatami,
                    is_qos: p.is_qos(),
                    #[cfg(feature = "shared-memory")]
                    is_shm: self.is_shm(),
                    links: vec![link],
                }
            })
            .collect()
    }
}
