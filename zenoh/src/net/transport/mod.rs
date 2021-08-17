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
// use multicast::SessionMulticast;
pub use primitives::*;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
pub use unicast::manager::*;
use unicast::SessionUnicast;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*********************************************************/
/* Session Callback to be implemented by the Upper Layer */
/*********************************************************/
pub trait SessionEventHandler: Send + Sync {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    fn new_link(&self, link: Link);
    fn del_link(&self, link: Link);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

pub trait SessionHandler: Send + Sync {
    fn new_session(&self, session: Session) -> ZResult<Arc<dyn SessionEventHandler>>;
}

#[derive(Default)]
pub struct DummySessionHandler;

impl SessionHandler for DummySessionHandler {
    fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        Ok(Arc::new(DummySessionEventHandler::default()))
    }
}

// Define an empty SessionCallback for the listener session
#[derive(Default)]
pub struct DummySessionEventHandler;

impl SessionEventHandler for DummySessionEventHandler {
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
                descr: "Session closed".to_string()
            })
        })
    };
}

macro_rules! zweak {
    ($ses:expr) => {{
        match $ses {
            Session::Unicast(s) => zweakinner!(s),
            Session::Multicast(s) => zweakinner!(s),
        }
    }};
}

/// [`Session`] is the session handler returned when opening a new session
#[derive(Clone, PartialEq)]
pub enum Session {
    Unicast(SessionUnicast),
    Multicast(SessionUnicast),
    // @TODO: Multicast(SessionMulticast),
}

impl Session {
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
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn SessionEventHandler>>> {
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
        // Return Ok if the session has already been closed
        match zweak!(self) {
            Ok(transport) => transport.close(smsg::close_reason::GENERIC).await,
            Err(_) => Ok(()),
        }
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match zweak!(self) {
            Ok(transport) => f
                .debug_struct("Session")
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

impl From<SessionUnicast> for Session {
    fn from(su: SessionUnicast) -> Session {
        Session::Unicast(su)
    }
}

// @TODO
// impl From<SessionMulticast> for Session {
//     fn from(sm: SessionMulticast) -> Session {
//         Session::SessionMulticast(sm)
//     }
// }
