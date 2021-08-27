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
// pub(crate) mod authenticator;
pub(crate) mod establishment;
pub(crate) mod link;
pub(crate) mod manager;
pub(crate) mod rx;
pub(crate) mod transport;
pub(crate) mod tx;

use super::common;
use super::protocol;
use super::protocol::core::{PeerId, WhatAmI, ZInt};
use super::protocol::proto::{tmsg, ZenohMessage};
use crate::net::link::{LinkMulticast, Locator};
pub use manager::*;
use std::any::Any;
use std::fmt;
use std::sync::{Arc, Weak};
use std::time::Duration;
use transport::{TransportMulticastConfig, TransportMulticastInner, TransportMulticastPeer};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror2;

/*************************************/
/*             CALLBACK              */
/*************************************/
pub trait TransportMulticastEventHandler: Send + Sync {
    fn handle_message(&self, msg: ZenohMessage, peer: &PeerId) -> ZResult<()>;
    fn new_peer(&self, peer: MulticastPeer);
    fn del_peer(&self, peer: MulticastPeer);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

// Define an empty TransportCallback for the listener transport
#[derive(Default)]
pub struct DummyTransportMulticastEventHandler;

impl TransportMulticastEventHandler for DummyTransportMulticastEventHandler {
    fn handle_message(&self, _msg: ZenohMessage, _peer: &PeerId) -> ZResult<()> {
        Ok(())
    }

    fn new_peer(&self, _peer: MulticastPeer) {}
    fn del_peer(&self, _peer: MulticastPeer) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/*************************************/
/*       TRANSPORT MULTICAST         */
/*************************************/
macro_rules! zweak {
    ($var:expr) => {
        $var.upgrade().ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidReference {
                descr: "Transport multicast closed".to_string()
            })
        })
    };
}

pub struct MulticastPeer {
    pub locator: Locator,
    pub pid: PeerId,
    pub whatami: WhatAmI,
    pub sn_resolution: ZInt,
    pub lease: Duration,
    pub is_qos: bool,
}

impl From<TransportMulticastPeer> for MulticastPeer {
    fn from(tmp: TransportMulticastPeer) -> MulticastPeer {
        Self::from(&tmp)
    }
}

impl From<&TransportMulticastPeer> for MulticastPeer {
    fn from(tmp: &TransportMulticastPeer) -> MulticastPeer {
        MulticastPeer {
            locator: tmp.locator.clone(),
            pid: tmp.pid,
            whatami: tmp.whatami,
            sn_resolution: tmp.sn_resolution,
            lease: tmp.lease,
            is_qos: tmp.conduit_rx.len() > 1,
        }
    }
}

#[derive(Clone)]
pub struct TransportMulticast(Weak<TransportMulticastInner>);

impl TransportMulticast {
    #[inline(always)]
    pub fn get_sn_resolution(&self) -> ZResult<ZInt> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_sn_resolution())
    }

    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = zweak!(self.0)?;
        Ok(transport.is_shm())
    }

    #[inline(always)]
    pub fn is_qos(&self) -> ZResult<bool> {
        let transport = zweak!(self.0)?;
        Ok(transport.is_qos())
    }

    #[inline(always)]
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn TransportMulticastEventHandler>>> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_callback())
    }

    #[inline(always)]
    pub fn get_link(&self) -> ZResult<LinkMulticast> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_link())
    }

    #[inline(always)]
    pub fn get_peers(&self) -> ZResult<Vec<MulticastPeer>> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_peers())
    }

    #[inline(always)]
    pub fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        let transport = zweak!(self.0)?;
        transport.schedule(message);
        Ok(())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the transport has already been closed
        match zweak!(self.0) {
            Ok(transport) => transport.close(tmsg::close_reason::GENERIC).await,
            Err(_) => Ok(()),
        }
    }

    #[inline(always)]
    pub fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.schedule(message)
    }
}

impl From<&Arc<TransportMulticastInner>> for TransportMulticast {
    fn from(s: &Arc<TransportMulticastInner>) -> TransportMulticast {
        TransportMulticast(Arc::downgrade(s))
    }
}

impl Eq for TransportMulticast {}

impl PartialEq for TransportMulticast {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for TransportMulticast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match zweak!(self.0) {
            Ok(transport) => {
                let peers: String = zread!(transport.peers)
                    .iter()
                    .map(|(l, p)| {
                        format!("(locator: {}, pid: {}, whatami: {})", l, p.pid, p.whatami)
                    })
                    .collect();

                f.debug_struct("Transport Multicast")
                    .field("sn_resolution", &transport.get_sn_resolution())
                    .field("is_qos", &transport.is_qos())
                    .field("is_shm", &transport.is_shm())
                    .field("peers", &peers)
                    .finish()
            }
            Err(e) => {
                write!(f, "{}", e)
            }
        }
    }
}
