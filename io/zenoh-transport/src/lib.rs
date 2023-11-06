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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
mod common;
mod manager;
mod multicast;
mod primitives;
pub mod unicast;

#[cfg(feature = "stats")]
pub use common::stats;

#[cfg(feature = "shared-memory")]
mod shm;

pub use manager::*;
pub use multicast::*;
pub use primitives::*;
use serde::Serialize;
use std::any::Any;
use std::sync::Arc;
pub use unicast::*;
use zenoh_link::Link;
use zenoh_protocol::core::{WhatAmI, ZenohId};
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;

/*************************************/
/*            TRANSPORT              */
/*************************************/
pub trait TransportEventHandler: Send + Sync {
    fn new_unicast(
        &self,
        peer: TransportPeer,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>>;

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>>;
}

#[derive(Default)]
pub struct DummyTransportEventHandler;

impl TransportEventHandler for DummyTransportEventHandler {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        Ok(Arc::new(DummyTransportMulticastEventHandler))
    }
}

/*************************************/
/*            MULTICAST              */
/*************************************/
pub trait TransportMulticastEventHandler: Send + Sync {
    fn new_peer(&self, peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>>;
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

// Define an empty TransportCallback for the listener transport
#[derive(Default)]
pub struct DummyTransportMulticastEventHandler;

impl TransportMulticastEventHandler for DummyTransportMulticastEventHandler {
    fn new_peer(&self, _peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }
    fn closing(&self) {}
    fn closed(&self) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/*************************************/
/*             CALLBACK              */
/*************************************/
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename = "Transport")]
pub struct TransportPeer {
    pub zid: ZenohId,
    pub whatami: WhatAmI,
    pub is_qos: bool,
    #[serde(skip)]
    pub links: Vec<Link>,
    #[cfg(feature = "shared-memory")]
    pub is_shm: bool,
}

pub trait TransportPeerEventHandler: Send + Sync {
    fn handle_message(&self, msg: NetworkMessage) -> ZResult<()>;
    fn new_link(&self, src: Link);
    fn del_link(&self, link: Link);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

// Define an empty TransportCallback for the listener transport
#[derive(Default)]
pub struct DummyTransportPeerEventHandler;

impl TransportPeerEventHandler for DummyTransportPeerEventHandler {
    fn handle_message(&self, _message: NetworkMessage) -> ZResult<()> {
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
