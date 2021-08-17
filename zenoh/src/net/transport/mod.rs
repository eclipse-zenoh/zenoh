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
mod common;
pub mod defaults;
mod manager;
mod multicast;
mod primitives;
pub mod unicast;

use super::protocol;
use super::protocol::core::{PeerId, WhatAmI, ZInt};
use super::protocol::proto::{smsg, ZenohMessage};
use crate::net::link::Link;
pub use manager::*;
// use multicast::TransportMulticast;
pub use primitives::*;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
pub use unicast::manager::*;
use unicast::TransportUnicast;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*********************************************************/
/* Transport Callback to be implemented by the Upper Layer */
/*********************************************************/
pub trait TransportEventHandler: Send + Sync {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    fn new_link(&self, link: Link);
    fn del_link(&self, link: Link);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

pub trait TransportHandler: Send + Sync {
    fn new_transport(&self, transport: Transport) -> ZResult<Arc<dyn TransportEventHandler>>;
}

#[derive(Default)]
pub struct DummyTransportHandler;

impl TransportHandler for DummyTransportHandler {
    fn new_transport(&self, _transport: Transport) -> ZResult<Arc<dyn TransportEventHandler>> {
        Ok(Arc::new(DummyTransportEventHandler::default()))
    }
}

// Define an empty TransportCallback for the listener transport
#[derive(Default)]
pub struct DummyTransportEventHandler;

impl TransportEventHandler for DummyTransportEventHandler {
    fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closing(&self) {}
    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/*************************************/
/*              SESSION              */
/*************************************/
macro_rules! zweakinner {
    ($var:expr) => {
        $var.upgrade().ok_or_else(|| {
            zerror2!(ZErrorKind::InvalidReference {
                descr: "Transport closed".to_string()
            })
        })
    };
}

macro_rules! zweak {
    ($ses:expr) => {{
        match $ses {
            Transport::Unicast(s) => zweakinner!(s),
            Transport::Multicast(s) => zweakinner!(s),
        }
    }};
}

/// [`Transport`] is the transport handler returned when opening a new transport
#[derive(Clone, PartialEq)]
pub enum Transport {
    Unicast(TransportUnicast),
    Multicast(TransportUnicast),
    // @TODO: Multicast(TransportMulticast),
}

impl Transport {
    #[inline(always)]
    pub fn get_pid(&self) -> ZResult<PeerId> {
        let transport = zweak!(self)?;
        Ok(transport.get_pid())
    }

    #[inline(always)]
    pub fn get_whatami(&self) -> ZResult<WhatAmI> {
        let transport = zweak!(self)?;
        Ok(transport.get_whatami())
    }

    #[inline(always)]
    pub fn get_sn_resolution(&self) -> ZResult<ZInt> {
        let transport = zweak!(self)?;
        Ok(transport.get_sn_resolution())
    }

    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = zweak!(self)?;
        Ok(transport.is_shm())
    }

    #[inline(always)]
    pub fn is_qos(&self) -> ZResult<bool> {
        let transport = zweak!(self)?;
        Ok(transport.is_qos())
    }

    #[inline(always)]
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn TransportEventHandler>>> {
        let transport = zweak!(self)?;
        Ok(transport.get_callback())
    }

    #[inline(always)]
    pub fn get_links(&self) -> ZResult<Vec<Link>> {
        let transport = zweak!(self)?;
        Ok(transport.get_links())
    }

    #[inline(always)]
    pub fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        let transport = zweak!(self)?;
        transport.schedule(message);
        Ok(())
    }

    #[inline(always)]
    pub fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.schedule(message)
    }

    #[inline(always)]
    pub async fn close_link(&self, link: &Link) -> ZResult<()> {
        let transport = zweak!(self)?;
        transport
            .close_link(link, smsg::close_reason::GENERIC)
            .await?;
        Ok(())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the transport has already been closed
        match zweak!(self) {
            Ok(transport) => transport.close(smsg::close_reason::GENERIC).await,
            Err(_) => Ok(()),
        }
    }
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match zweak!(self) {
            Ok(transport) => f
                .debug_struct("Transport")
                .field("peer", &transport.get_pid())
                .field("sn_resolution", &transport.get_sn_resolution())
                .field("is_shm", &transport.is_shm())
                .finish(),
            Err(e) => {
                write!(f, "{}", e)
            }
        }
    }
}

impl From<TransportUnicast> for Transport {
    fn from(su: TransportUnicast) -> Transport {
        Transport::Unicast(su)
    }
}

// @TODO
// impl From<TransportMulticast> for Transport {
//     fn from(sm: TransportMulticast) -> Transport {
//         Transport::TransportMulticast(sm)
//     }
// }
