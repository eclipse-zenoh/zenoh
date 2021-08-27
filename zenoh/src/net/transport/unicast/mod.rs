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
pub mod authenticator;
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
use crate::net::link::LinkUnicast;
pub use manager::*;
use std::any::Any;
use std::fmt;
use std::sync::{Arc, Weak};
use transport::TransportUnicastInner;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror2;

/*************************************/
/*             CALLBACK              */
/*************************************/
pub trait TransportUnicastEventHandler: Send + Sync {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    fn new_link(&self, link: LinkUnicast);
    fn del_link(&self, link: LinkUnicast);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

// Define an empty TransportCallback for the listener transport
#[derive(Default)]
pub struct DummyTransportUnicastEventHandler;

impl TransportUnicastEventHandler for DummyTransportUnicastEventHandler {
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, _link: LinkUnicast) {}
    fn del_link(&self, _link: LinkUnicast) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/*************************************/
/*        TRANSPORT UNICAST          */
/*************************************/
macro_rules! zweak {
    ($var:expr) => {
        $var.upgrade().ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidReference {
                descr: "Transport unicast closed".to_string()
            })
        })
    };
}

pub(crate) struct TransportConfigUnicast {
    pub(crate) peer: PeerId,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_resolution: ZInt,
    pub(crate) initial_sn_tx: ZInt,
    pub(crate) initial_sn_rx: ZInt,
    pub(crate) is_shm: bool,
    pub(crate) is_qos: bool,
}

/// [`TransportUnicast`] is the transport handler returned
/// when opening a new unicast transport
#[derive(Clone)]
pub struct TransportUnicast(Weak<TransportUnicastInner>);

impl TransportUnicast {
    #[inline(always)]
    pub(super) fn get_transport(&self) -> ZResult<Arc<TransportUnicastInner>> {
        zweak!(self.0)
    }

    #[inline(always)]
    pub fn get_pid(&self) -> ZResult<PeerId> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_pid())
    }

    #[inline(always)]
    pub fn get_whatami(&self) -> ZResult<WhatAmI> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_whatami())
    }

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
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn TransportUnicastEventHandler>>> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_callback())
    }

    #[inline(always)]
    pub fn get_links(&self) -> ZResult<Vec<LinkUnicast>> {
        let transport = zweak!(self.0)?;
        Ok(transport.get_links())
    }

    #[inline(always)]
    pub fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        let transport = zweak!(self.0)?;
        transport.schedule(message);
        Ok(())
    }

    #[inline(always)]
    pub async fn close_link(&self, link: &LinkUnicast) -> ZResult<()> {
        let transport = zweak!(self.0)?;
        transport
            .close_link(link, tmsg::close_reason::GENERIC)
            .await?;
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

impl From<&Arc<TransportUnicastInner>> for TransportUnicast {
    fn from(s: &Arc<TransportUnicastInner>) -> TransportUnicast {
        TransportUnicast(Arc::downgrade(s))
    }
}

impl Eq for TransportUnicast {}

impl PartialEq for TransportUnicast {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for TransportUnicast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match zweak!(self.0) {
            Ok(transport) => f
                .debug_struct("Transport Unicast")
                .field("pid", &transport.get_pid())
                .field("whatami", &transport.get_whatami())
                .field("sn_resolution", &transport.get_sn_resolution())
                .field("is_qos", &transport.is_qos())
                .field("is_shm", &transport.is_shm())
                .finish(),
            Err(e) => {
                write!(f, "{}", e)
            }
        }
    }
}
