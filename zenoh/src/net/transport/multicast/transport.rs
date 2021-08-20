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
use super::super::TransportManager;
use super::common::conduit::{TransportConduitRx, TransportConduitTx};
use super::link::TransportLinkMulticast;
use super::protocol::core::{ConduitSnList, PeerId, Priority, WhatAmI, ZInt};
use super::protocol::proto::{TransportMessage, ZenohMessage};
use super::TransportMulticastEventHandler;
use crate::net::link::{LinkMulticast, Locator};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{Arc, RwLock};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*************************************/
/*             TRANSPORT             */
/*************************************/
pub(super) struct TransportMulticastPeer {
    pub(super) pid: PeerId,
    pub(super) whatami: WhatAmI,
    // pub(super) lease: ZInt
    pub(super) conduit_rx: Box<[TransportConduitRx]>,
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
    // The transport can do shm
    pub(super) is_shm: bool,
}

pub(crate) struct TransportMulticastConfig {
    pub(crate) manager: TransportManager,
    pub(crate) is_shm: bool,
    pub(crate) initial_sns: ConduitSnList,
    pub(crate) link: LinkMulticast,
}

impl TransportMulticastInner {
    pub(super) fn new(config: TransportMulticastConfig) -> TransportMulticastInner {
        let mut conduit_tx = vec![];
        // let mut conduit_rx = vec![];

        match config.initial_sns {
            ConduitSnList::Plain(sn) => conduit_tx.push(TransportConduitTx::new(
                Priority::default(),
                config.manager.config.sn_resolution,
                sn,
            )),
            ConduitSnList::QoS(sns) => {
                for (i, sn) in sns.iter().enumerate() {
                    conduit_tx.push(TransportConduitTx::new(
                        (i as u8).try_into().unwrap(),
                        config.manager.config.sn_resolution,
                        *sn,
                    ));
                }
            }
        }

        let ti = TransportMulticastInner {
            manager: config.manager,
            locator: config.link.get_dst(),
            conduit_tx: conduit_tx.into_boxed_slice().into(),
            peers: Arc::new(RwLock::new(HashMap::new())),
            link: Arc::new(RwLock::new(None)),
            callback: Arc::new(RwLock::new(None)),
            is_shm: config.is_shm,
        };

        let mut w_guard = zwrite!(ti.link);
        *w_guard = Some(TransportLinkMulticast::new(ti.clone(), config.link));
        drop(w_guard);

        ti
    }

    pub(super) fn set_callback(&self, callback: Arc<dyn TransportMulticastEventHandler>) {
        let mut guard = zwrite!(self.callback);
        *guard = Some(callback);
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

    /*************************************/
    /*               LINK                */
    /*************************************/
    pub(super) fn start_tx(&self, batch_size: usize) -> ZResult<()> {
        let mut guard = zwrite!(self.link);
        match guard.as_mut() {
            Some(l) => {
                assert!(!self.conduit_tx.is_empty());
                l.start_tx(batch_size, self.conduit_tx.clone());
                Ok(())
            }
            None => {
                zerror!(ZErrorKind::InvalidLink {
                    descr: format!(
                        "Can not start multicast Link TX of peer {}: {}",
                        self.manager.config.pid, self.locator
                    )
                })
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
                zerror!(ZErrorKind::InvalidLink {
                    descr: format!(
                        "Can not stop multicast Link TX of peer {}: {}",
                        self.manager.config.pid, self.locator
                    )
                })
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
                zerror!(ZErrorKind::InvalidLink {
                    descr: format!(
                        "Can not start multicast Link RX of peer {}: {}",
                        self.manager.config.pid, self.locator
                    )
                })
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
                zerror!(ZErrorKind::InvalidLink {
                    descr: format!(
                        "Can not stop multicast Link RX of peer {}: {}",
                        self.manager.config.pid, self.locator
                    )
                })
            }
        }
    }
}

impl TransportMulticastInner {
    /*************************************/
    /*            ACCESSORS              */
    /*************************************/
    pub(crate) fn get_pid(&self) -> PeerId {
        self.manager.config.pid.clone()
    }

    pub(crate) fn get_whatami(&self) -> WhatAmI {
        self.manager.config.whatami
    }

    pub(crate) fn get_sn_resolution(&self) -> ZInt {
        self.manager.config.sn_resolution
    }

    pub(crate) fn is_shm(&self) -> bool {
        self.is_shm
    }

    pub(crate) fn is_qos(&self) -> bool {
        self.conduit_tx.len() > 1
    }

    pub(crate) fn get_callback(&self) -> Option<Arc<dyn TransportMulticastEventHandler>> {
        zread!(self.callback).clone()
    }

    /*************************************/
    /*           TERMINATION             */
    /*************************************/
    pub(crate) async fn close(&self, reason: u8) -> ZResult<()> {
        log::trace!(
            "Closing multicast transport of peer {}: {}",
            self.manager.config.pid,
            self.locator
        );

        let pipeline = zread!(self.link).as_ref().unwrap().get_pipeline().unwrap();

        // Close message to be sent on all the links
        let peer_id = Some(self.manager.pid());
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
    #[cfg(feature = "zero-copy")]
    pub(crate) fn schedule(&self, mut message: ZenohMessage) {
        let res = if self.is_shm {
            message.map_to_shminfo()
        } else {
            message.map_to_shmbuf(self.manager.shmr.clone())
        };
        if let Err(e) = res {
            log::trace!("Failed SHM conversion: {}", e);
            return;
        }
        self.schedule_first_fit(message);
    }

    #[cfg(not(feature = "zero-copy"))]
    pub(crate) fn schedule(&self, message: ZenohMessage) {
        self.schedule_first_fit(message);
    }

    pub(crate) fn get_links(&self) -> Vec<LinkMulticast> {
        match zread!(self.link).as_ref() {
            Some(l) => vec![l.get_link().clone()],
            None => vec![],
        }
    }
}
