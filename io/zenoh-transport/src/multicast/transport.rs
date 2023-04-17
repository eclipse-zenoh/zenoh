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
use super::common::conduit::{TransportConduitRx, TransportConduitTx};
use super::link::{TransportLinkMulticast, TransportLinkMulticastConfig};
#[cfg(feature = "stats")]
use super::TransportMulticastStatsAtomic;
use crate::{
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
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
use zenoh_core::{zread, zwrite};
use zenoh_link::{Link, LinkMulticast, Locator};
use zenoh_protocol::{
    core::{ConduitSnList, Priority, WhatAmI, ZInt, ZenohId},
    transport::{tmsg, Join, TransportMessage},
    zenoh::ZenohMessage,
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
    pub(super) sn_resolution: ZInt,
    pub(super) lease: Duration,
    pub(super) whatchdog: Arc<AtomicBool>,
    pub(super) handle: TimedHandle,
    pub(super) conduit_rx: Box<[TransportConduitRx]>,
    pub(super) handler: Arc<dyn TransportPeerEventHandler>,
}

impl TransportMulticastPeer {
    pub(super) fn active(&self) {
        self.whatchdog.store(true, Ordering::Release);
    }

    pub(super) fn is_qos(&self) -> bool {
        self.conduit_rx.len() == Priority::NUM
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
                .del_peer(&self.locator, tmsg::close_reason::EXPIRED);
        }
    }
}

#[derive(Clone)]
pub(crate) struct TransportMulticastInner {
    // The manager this channel is associated to
    pub(super) manager: TransportManager,
    // The multicast locator
    pub(super) locator: Locator,
    // Tx conduits
    pub(super) conduit_tx: Arc<[TransportConduitTx]>,
    // Remote peers
    pub(super) peers: Arc<RwLock<HashMap<Locator, TransportMulticastPeer>>>,
    // The multicast link
    pub(super) link: Arc<RwLock<Option<TransportLinkMulticast>>>,
    // The callback
    pub(super) callback: Arc<RwLock<Option<Arc<dyn TransportMulticastEventHandler>>>>,
    // The timer for peer leases
    pub(super) timer: Arc<Timer>,
    // Transport statistics
    #[cfg(feature = "stats")]
    pub(super) stats: Arc<TransportMulticastStatsAtomic>,
}

pub(crate) struct TransportMulticastConfig {
    pub(crate) manager: TransportManager,
    pub(crate) initial_sns: ConduitSnList,
    pub(crate) link: LinkMulticast,
}

impl TransportMulticastInner {
    pub(super) fn make(config: TransportMulticastConfig) -> ZResult<TransportMulticastInner> {
        let mut conduit_tx = vec![];

        match config.initial_sns {
            ConduitSnList::Plain(sn) => {
                let tct = TransportConduitTx::make(config.manager.config.sn_resolution)?;
                tct.sync(sn)?;
                conduit_tx.push(tct);
            }
            ConduitSnList::QoS(sns) => {
                for (_, sn) in sns.iter().enumerate() {
                    let tct = TransportConduitTx::make(config.manager.config.sn_resolution)?;
                    tct.sync(*sn)?;
                    conduit_tx.push(tct);
                }
            }
        }

        let ti = TransportMulticastInner {
            manager: config.manager,
            locator: config.link.get_dst().to_owned(),
            conduit_tx: conduit_tx.into_boxed_slice().into(),
            peers: Arc::new(RwLock::new(HashMap::new())),
            link: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            timer: Arc::new(Timer::new(false)),
            #[cfg(feature = "stats")]
            stats: Arc::new(TransportMulticastStatsAtomic::default()),
        };

        let mut w_guard = zwrite!(ti.link);
        *w_guard = Some(TransportLinkMulticast::new(ti.clone(), config.link));
        drop(w_guard);

        Ok(ti)
    }

    pub(super) fn set_callback(&self, callback: Arc<dyn TransportMulticastEventHandler>) {
        let mut guard = zwrite!(self.callback);
        *guard = Some(callback);
    }

    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(crate) fn get_sn_resolution(&self) -> ZInt {
        self.manager.config.sn_resolution
    }

    pub(crate) fn is_qos(&self) -> bool {
        self.conduit_tx.len() > 1
    }

    pub(crate) fn is_shm(&self) -> bool {
        false
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
        log::debug!("Closing multicast transport on {}", self.locator);

        // Notify the callback that we are going to close the transport
        let callback = zwrite!(self.callback).take();
        if let Some(cb) = callback.as_ref() {
            cb.closing();
        }

        // Delete the transport on the manager
        let _ = self.manager.del_transport_multicast(&self.locator);

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

        let pipeline = zread!(self.link)
            .as_ref()
            .unwrap()
            .pipeline
            .as_ref()
            .unwrap()
            .clone();

        // Close message to be sent on all the links
        let peer_id = Some(self.manager.zid());
        let reason_id = reason;
        // link_only should always be false for user-triggered close. However, in case of
        // multiple links, it is safer to close all the links first. When no links are left,
        // the transport is then considered closed.
        let link_only = true;
        let attachment = None; // No attachment here
        let msg = TransportMessage::make_close(peer_id, reason_id, link_only, attachment);

        pipeline.push_transport_message(msg, Priority::Background);

        // Terminate and clean up the transport
        self.delete().await
    }

    /*************************************/
    /*        SCHEDULE AND SEND TX       */
    /*************************************/
    /// Schedule a Zenoh message on the transmission queue    
    #[allow(unused_mut)] // Required with "shared-memory" feature
    pub(crate) fn schedule(&self, mut msg: ZenohMessage) {
        // Multicast transports do not support SHM for the time being
        #[cfg(feature = "shared-memory")]
        {
            let res = crate::shm::map_zmsg_to_shmbuf(&mut msg, &self.manager.shmr);
            if let Err(e) = res {
                log::trace!("Failed SHM conversion: {}", e);
                return;
            }
        }
        self.schedule_first_fit(msg);
    }

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(super) fn start_tx(&self, batch_size: u16) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match guard.as_mut() {
            Some(l) => {
                assert!(!self.conduit_tx.is_empty());
                let config = TransportLinkMulticastConfig {
                    version: self.manager.config.version,
                    zid: self.manager.config.zid,
                    whatami: self.manager.config.whatami,
                    lease: self.manager.config.multicast.lease,
                    keep_alive: self.manager.config.multicast.keep_alive,
                    join_interval: self.manager.config.multicast.join_interval,
                    sn_resolution: self.manager.config.sn_resolution,
                    batch_size,
                };
                l.start_tx(config, self.conduit_tx.clone());
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
                l.start_rx();
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

        let peer = TransportPeer {
            zid: join.zid,
            whatami: join.whatami,
            is_qos: join.is_qos(),
            is_shm: self.is_shm(),
            links: vec![link],
        };

        let handler = match zread!(self.callback).as_ref() {
            Some(cb) => cb.new_peer(peer.clone())?,
            None => return Ok(()),
        };

        let conduit_rx = match join.next_sns {
            ConduitSnList::Plain(sn) => {
                let tcr = TransportConduitRx::make(
                    join.sn_resolution,
                    self.manager.config.defrag_buff_size,
                )?;
                tcr.sync(sn)?;
                vec![tcr]
            }
            ConduitSnList::QoS(ref sns) => {
                let mut tcrs = Vec::with_capacity(sns.len());
                for (_, sn) in sns.iter().enumerate() {
                    let tcr = TransportConduitRx::make(
                        join.sn_resolution,
                        self.manager.config.defrag_buff_size,
                    )?;
                    tcr.sync(*sn)?;
                    tcrs.push(tcr);
                }
                tcrs
            }
        }
        .into_boxed_slice();

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
            sn_resolution: join.sn_resolution,
            lease: join.lease,
            whatchdog,
            handle,
            conduit_rx,
            handler,
        };
        {
            zwrite!(self.peers).insert(locator.clone(), peer);
        }

        // Add the event to the timer
        self.timer.add(event);

        log::debug!(
                "New transport joined on {}: zid {}, whatami {}, sn resolution {}, locator {}, qos {}, initial sn: {}",
                self.locator,
                join.zid,
                join.whatami,
                join.sn_resolution,
                locator,
                join.is_qos(),
                join.next_sns,
            );

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
                    is_shm: self.is_shm(),
                    links: vec![link],
                }
            })
            .collect()
    }
}
