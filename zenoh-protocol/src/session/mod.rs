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
mod channel;
mod defaults;
mod initial;
mod manager;

pub use manager::*;

use channel::*;
use initial::*;

use async_std::sync::{Arc, Weak};
use async_trait::async_trait;

use crate::link::Link;
use crate::proto::{SessionMessage, WhatAmI, ZenohMessage};

use zenoh_util::{zerror, zweak};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};

/*********************************************************/
/*           Trait for implementing a transport          */
/*********************************************************/
const STR_ERR: &str = "Transport not available";

#[derive(Clone)]
pub struct Transport(Weak<dyn TransportTrait + Send + Sync>);

impl Transport {
    pub fn new(transport: Arc<dyn TransportTrait + Send + Sync>) -> Transport {
        let transport = Arc::downgrade(&transport);
        Transport(transport)
    }
}

impl Transport {
    pub async fn receive_message(&self, link: &Link, msg: SessionMessage) -> ZResult<Action> {
        let transport = zweak!(self.0, STR_ERR);
        Ok(transport.receive_message(link, msg).await)
    }

    pub async fn link_err(&self, link: &Link) -> ZResult<()> {
        let transport = zweak!(self.0, STR_ERR);
        transport.link_err(link).await;
        Ok(())
    }
}

#[async_trait]
pub trait TransportTrait {
    async fn receive_message(&self, link: &Link, msg: SessionMessage) -> Action;
    async fn link_err(&self, link: &Link);
}

pub enum Action {
    ChangeTransport(Transport),
    Close,
    Read
}

/*********************************************************/
/* Session Callback to be implemented by the Upper Layer */
/*********************************************************/
#[async_trait]
pub trait MsgHandler {
    async fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    async fn close(&self);
}

#[async_trait]
pub trait SessionHandler {
    async fn new_session(
        &self,
        whatami: WhatAmI,
        session: Arc<dyn MsgHandler + Send + Sync>,
    ) -> Arc<dyn MsgHandler + Send + Sync>;
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
impl MsgHandler for DummyHandler {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }
    async fn close(&self) {}
}
