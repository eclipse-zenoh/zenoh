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
pub mod defaults;
mod initial;
mod manager;
mod primitives;
mod transport;

use super::core;
use super::core::{PeerId, WhatAmI, ZInt};
use super::io;
use super::link;
use super::link::Link;
use super::proto;
use super::proto::{smsg, ZenohMessage};
use super::session;
use async_std::sync::{Arc, Weak};
pub use manager::*;
pub use primitives::*;
use std::any::Any;
use std::fmt;
use transport::*;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*********************************************************/
/* Session Callback to be implemented by the Upper Layer */
/*********************************************************/
pub trait SessionEventHandler {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    fn new_link(&self, link: Link);
    fn del_link(&self, link: Link);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

pub trait SessionHandler {
    fn new_session(&self, session: Session) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>>;
}

// Define an empty SessionCallback for the listener session
#[derive(Default)]
pub struct DummySessionEventHandler;

impl DummySessionEventHandler {
    pub fn new() -> Self {
        Self
    }
}

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
const STR_ERR: &str = "Session closed";

pub(crate) struct SessionConfig {
    pub(crate) peer: PeerId,
    pub(crate) whatami: WhatAmI,
    pub(crate) sn_resolution: ZInt,
    pub(crate) initial_sn_tx: ZInt,
    pub(crate) initial_sn_rx: ZInt,
    pub(crate) is_shm: bool,
    pub(crate) is_qos: bool,
}

/// [`Session`] is the session handler returned when opening a new session
#[derive(Clone)]
pub struct Session(Weak<SessionTransport>);

impl Session {
    fn new(inner: Weak<SessionTransport>) -> Self {
        Self(inner)
    }

    /*************************************/
    /*         SESSION ACCESSORS         */
    /*************************************/
    #[inline(always)]
    pub(super) fn get_transport(&self) -> ZResult<Arc<SessionTransport>> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport)
    }

    /*************************************/
    /*          PUBLIC ACCESSORS         */
    /*************************************/
    #[inline(always)]
    pub fn get_pid(&self) -> ZResult<PeerId> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.pid.clone())
    }

    #[inline(always)]
    pub fn get_whatami(&self) -> ZResult<WhatAmI> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.whatami)
    }

    #[inline(always)]
    pub fn get_sn_resolution(&self) -> ZResult<ZInt> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.sn_resolution)
    }

    #[inline(always)]
    pub fn is_shm(&self) -> ZResult<bool> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.is_shm())
    }

    #[inline(always)]
    pub fn get_callback(&self) -> ZResult<Option<Arc<dyn SessionEventHandler + Send + Sync>>> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.get_callback())
    }

    #[inline(always)]
    pub fn get_links(&self) -> ZResult<Vec<Link>> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.get_links())
    }

    #[inline(always)]
    pub fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        let transport = zweak!(self.0, STR_ERR);
        transport.schedule(message);
        Ok(())
    }

    #[inline(always)]
    pub fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.schedule(message)
    }

    #[inline(always)]
    pub async fn close_link(&self, link: &Link) -> ZResult<()> {
        let transport = zweak!(self.0, STR_ERR);
        transport
            .close_link(link, smsg::close_reason::GENERIC)
            .await?;
        Ok(())
    }

    #[inline(always)]
    pub async fn close(&self) -> ZResult<()> {
        // Return Ok if the session has already been closed
        match self.0.upgrade() {
            Some(transport) => transport.close(smsg::close_reason::GENERIC).await,
            None => Ok(()),
        }
    }
}

impl Eq for Session {}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(transport) = self.0.upgrade() {
            f.debug_struct("Session")
                .field("peer", &transport.pid)
                .field("sn_resolution", &transport.sn_resolution)
                .field("is_shm", &transport.is_shm())
                .finish()
        } else {
            write!(f, "{}", STR_ERR)
        }
    }
}
