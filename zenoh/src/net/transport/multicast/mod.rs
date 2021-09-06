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
use super::protocol::core::ZInt;
use super::protocol::proto::{tmsg, ZenohMessage};
use crate::net::link::Link;
use crate::net::transport::{TransportMulticastEventHandler, TransportPeer};
pub use manager::*;
use std::fmt;
use std::sync::{Arc, Weak};
use transport::{TransportMulticastConfig, TransportMulticastInner};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror2;

/*************************************/
/*       TRANSPORT MULTICAST         */
/*************************************/
#[derive(Clone)]
pub struct TransportMulticast(Weak<TransportMulticastInner>);

impl TransportMulticast {
    #[inline(always)]
    fn get_transport(&self) -> ZResult<Arc<TransportMulticastInner>> {
        self.0.upgrade().ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidReference {
                descr: "Transport multicast closed".to_string()
            })
        })
    }

    #[inline(always)]
    pub fn get_sn_resolution(&self) -> ZResult<ZInt> {
        let transport = self.get_transport()?;
        Ok(transport.get_sn_resolution())
    }

    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = self.get_transport()?;
        Ok(transport.is_shm())
    }

    #[inline(always)]
    pub fn is_qos(&self) -> ZResult<bool> {
        let transport = self.get_transport()?;
        Ok(transport.is_qos())
    }

    #[inline(always)]
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn TransportMulticastEventHandler>>> {
        let transport = self.get_transport()?;
        Ok(transport.get_callback())
    }

    #[inline(always)]
    pub fn get_link(&self) -> ZResult<Link> {
        let transport = self.get_transport()?;
        Ok(transport.get_link().into())
    }

    #[inline(always)]
    pub fn get_peers(&self) -> ZResult<Vec<TransportPeer>> {
        let transport = self.get_transport()?;
        Ok(transport.get_peers())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the transport has already been closed
        match self.get_transport() {
            Ok(transport) => transport.close(tmsg::close_reason::GENERIC).await,
            Err(_) => Ok(()),
        }
    }

    #[inline(always)]
    pub fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        let transport = self.get_transport()?;
        transport.schedule(message);
        Ok(())
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
        match self.get_transport() {
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
