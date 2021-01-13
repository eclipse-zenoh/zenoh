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
pub mod defaults;
mod manager;
mod primitives;
mod transport;

use crate::core::{PeerId, WhatAmI, ZInt};
use crate::link::Link;
use crate::proto::{smsg, ZenohMessage};
use async_std::sync::{Arc, Weak};
use async_trait::async_trait;
pub use manager::*;
pub use primitives::*;
use std::fmt;
use transport::*;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*********************************************************/
/* Session Callback to be implemented by the Upper Layer */
/*********************************************************/
#[async_trait]
pub trait SessionEventHandler {
    async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    async fn new_link(&self, link: Link);
    async fn del_link(&self, link: Link);
    async fn closing(&self);
    async fn closed(&self);
}

#[async_trait]
pub trait SessionHandler {
    async fn new_session(
        &self,
        session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>>;
}

// Define an empty SessionCallback for the listener session
#[derive(Default)]
pub struct DummyHandler;

impl DummyHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for DummyHandler {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

    async fn closing(&self) {}

    async fn closed(&self) {}
}

/*************************************/
/*              SESSION              */
/*************************************/
const STR_ERR: &str = "Session not available";

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
    #[inline]
    pub(super) fn get_transport(&self) -> ZResult<Arc<SessionTransport>> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport)
    }

    /*************************************/
    /*          PUBLIC ACCESSORS         */
    /*************************************/
    #[inline]
    pub fn get_pid(&self) -> ZResult<PeerId> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.get_pid())
    }

    #[inline]
    pub fn get_whatami(&self) -> ZResult<WhatAmI> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.get_whatami())
    }

    #[inline]
    pub fn get_sn_resolution(&self) -> ZResult<ZInt> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.get_sn_resolution())
    }

    #[inline]
    pub async fn close(&self) -> ZResult<()> {
        log::trace!("{:?}. Close", self);
        let transport = zweak!(self.0, STR_ERR);
        transport.close(smsg::close_reason::GENERIC).await
    }

    #[inline]
    pub async fn close_link(&self, link: &Link) -> ZResult<()> {
        let transport = zweak!(self.0, STR_ERR);
        transport
            .close_link(link, smsg::close_reason::GENERIC)
            .await?;
        Ok(())
    }

    #[inline]
    pub async fn get_links(&self) -> ZResult<Vec<Link>> {
        log::trace!("{:?}. Get links", self);
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.get_links().await)
    }

    #[inline]
    pub async fn schedule(&self, message: ZenohMessage) -> ZResult<()> {
        log::trace!("{:?}. Schedule: {:?}", self, message);
        let transport = zweak!(self.0, STR_ERR);
        transport.schedule(message).await;
        Ok(())
    }
}

#[async_trait]
impl SessionEventHandler for Session {
    #[inline]
    async fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.schedule(message).await
    }

    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
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
                .field("peer", &transport.get_pid())
                .field("sn_resolution", &transport.get_sn_resolution())
                .finish()
        } else {
            write!(f, "Session closed")
        }
    }
}
