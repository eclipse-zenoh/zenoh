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
use super::common::priority::{TransportPriorityRx, TransportPriorityTx};
use super::link::{TransportLinkMulticast, TransportLinkMulticastConfig};
#[cfg(feature = "stats")]
use crate::stats::TransportStats;
use crate::{
    TransportConfigMulticast, TransportManager, TransportMulticastEventHandler, TransportPeer,
    TransportPeerEventHandler,
};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use zenoh_core::{zcondfeat, zread, zwrite};
use zenoh_link::{Link, LinkMulticast, Locator};
use zenoh_protocol::core::Resolution;
use zenoh_protocol::transport::{batch_size, Close, TransportMessage};
use zenoh_protocol::{
    core::{Bits, Field, Priority, WhatAmI, ZenohId},
    transport::{close, Join},
};
use zenoh_result::{bail, ZResult};
use zenoh_util::{Timed, TimedEvent, TimedHandle, Timer};

/*************************************/
/*             TRANSPORT             */
/*************************************/
#[derive(Clone)]
pub(super) struct TransportMulticastPeer {
    pub(super) version: u8,
    pub(super) locator: Locator,
    pub(super) zid: ZenohId,
    pub(super) whatami: WhatAmI,
    pub(super) resolution: Resolution,
    pub(super) lease: Duration,
    pub(super) whatchdog: Arc<AtomicBool>,
    pub(super) handle: TimedHandle,
    pub(super) priority_rx: Box<[TransportPriorityRx]>,
    pub(super) handler: Arc<dyn TransportPeerEventHandler>,
}

impl TransportMulticastPeer {
    pub(super) fn active(&self) {
        self.whatchdog.store(true, Ordering::Release);
    }

    pub(super) fn is_qos(&self) -> bool {
        self.priority_rx.len() == Priority::NUM
    }
}

#[derive(Clone)]
pub(super) struct TransportMulticastPeerLeaseTimer {
    pub(super) whatchdog: Arc<AtomicBool>,
    locator: Locator,
    transport: TransportMulticastInner,
}

#[async_trait]
impl Timed for TransportMulticastPeerLeaseTimer {
    async fn run(&mut self) {
        let is_active = self.whatchdog.swap(false, Ordering::AcqRel);
        if !is_active {
            let _ = self
                .transport
                .del_peer(&self.locator, close::reason::EXPIRED);
        }
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
    pub(super) link: Arc<RwLock<Option<TransportLinkMulticast>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportMulticastEventHandler>>>>,
    // The timer for peer leases
    pub(super) timer: Arc<Timer>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportStats>,
}

impl TransportMulticastInner {
    pub(super) fn make(
        manager: TransportManager,
        config: TransportConfigMulticast,
    ) -> ZResult<TransportMulticastInner> {
        let mut priority_tx = vec![];
        if (config.initial_sns.len() != 1) != (config.initial_sns.len() != Priority::NUM) {
            for (_, sn) in config.initial_sns.iter().enumerate() {
                let tct = TransportPriorityTx::make(config.sn_resolution)?;
                tct.sync(*sn)?;
                priority_tx.push(tct);
            }
        } else {
            bail!("Invalid QoS configuration");
        }

        #[cfg(feature = "stats")]
        let stats = Arc::new(TransportStats::new(Some(manager.get_stats().clone())));

        let ti = TransportMulticastInner {
            manager,
            priority_tx: priority_tx.into_boxed_slice().into(),
            peers: Arc::new(RwLock::new(HashMap::new())),
            locator: config.link.get_dst().to_owned(),
            link: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            timer: Arc::new(Timer::new(false)),
            #[cfg(feature = "stats")]
            stats,
        };

        let link = TransportLinkMulticast::new(ti.clone(), config.link);
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
        self.manager.config.multicast.is_shm
    }

    pub(crate) fn get_callback(&self) -> Option<Arc<dyn TransportMulticastEventHandler>> {
        zread!(self.callback).clone()
    }

    pub(crate) fn get_link(&self) -> LinkMulticast {
        zread!(self.link).as_ref().unwrap().link.clone()
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(super) async fn delete(&self) -> ZResult<()> {
        log::debug!("Closing multicast transport on {:?}", self.locator);

        // Notify the callback that we are going to close the transport
        let callback = zwrite!(self.callback).take();
        if let Some(cb) = callback.as_ref() {
            cb.closing();
        }

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

        Ok(())
    }

    pub(crate) async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!(
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
                    .min(l.link.get_mtu())
                    .min(batch_size::MULTICAST);
                let config = TransportLinkMulticastConfig {
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
                    .min(l.link.get_mtu())
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
        let mut link = Link::from(self.get_link());
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
        for (_, sn) in next_sns.iter().enumerate() {
            let tprx = TransportPriorityRx::make(
                join.resolution.get(Field::FrameSN),
                self.manager.config.defrag_buff_size,
            )?;
            tprx.sync(*sn)?;
            priority_rx.push(tprx);
        }
        let priority_rx = priority_rx.into_boxed_slice();

        log::debug!(
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
        let whatchdog = Arc::new(AtomicBool::new(false));
        let event = TransportMulticastPeerLeaseTimer {
            whatchdog: whatchdog.clone(),
            locator: locator.clone(),
            transport: self.clone(),
        };
        let event = TimedEvent::periodic(join.lease, event);
        let handle = event.get_handle();

        // Store the new peer
        let peer = TransportMulticastPeer {
            version: join.version,
            locator: locator.clone(),
            zid: peer.zid,
            whatami: peer.whatami,
            resolution: join.resolution,
            lease: join.lease,
            whatchdog,
            handle,
            priority_rx,
            handler,
        };
        zwrite!(self.peers).insert(locator.clone(), peer);

        // Add the event to the timer
        self.timer.add(event);

        Ok(())
    }

    pub(super) fn del_peer(&self, locator: &Locator, reason: u8) -> ZResult<()> {
        let mut guard = zwrite!(self.peers);
        if let Some(peer) = guard.remove(locator) {
            log::debug!(
                "Peer {}/{}/{} has left multicast {} with reason: {}",
                peer.zid,
                peer.whatami,
                locator,
                self.locator,
                reason
            );
            peer.handle.clone().defuse();

            peer.handler.closing();
            drop(guard);
            peer.handler.closed();
        }
        Ok(())
    }

    pub(super) fn get_peers(&self) -> Vec<TransportPeer> {
        zread!(self.peers)
            .values()
            .map(|p| {
                let mut link = Link::from(self.get_link());
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
