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

use super::link::Link;
use super::protocol;
use super::protocol::proto::ZenohMessage;
pub use manager::*;
pub use multicast::*;
pub use primitives::*;
use std::any::Any;
use std::sync::Arc;
pub use unicast::*;
use zenoh_util::core::ZResult;

/*************************************/
/*             GENERAL               */
/*************************************/
pub enum Transport {
    Unicast(TransportUnicast),
    Multicast(TransportMulticast),
}

// impl Transport {
//     fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
//         match self {
//             Transport::Unicast(tu) => tu.handle_message(message),
//             Transport::Multicast(tm) => tm.handle_message(message),
//         }
//     }
// }

/*************************************/
/*             HANDLER               */
/*************************************/
pub trait TransportEventHandler: Send + Sync {
    fn new_unicast(
        &self,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>>;

    fn new_multicast(
        &self,
        transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>>;
}

#[derive(Default)]
pub struct DummyTransportEventHandler;

impl TransportEventHandler for DummyTransportEventHandler {
    fn new_unicast(
        &self,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        Ok(Arc::new(DummyTransportMulticastEventHandler::default()))
    }
}

/*************************************/
/*             CALLBACK              */
/*************************************/
pub trait TransportPeerEventHandler: Send + Sync {
    fn handle_message(&self, msg: ZenohMessage) -> ZResult<()>;
    fn new_link(&self, link: Link);
    fn del_link(&self, link: Link);
    fn closing(&self);
    fn closed(&self);
    fn as_any(&self) -> &dyn Any;
}

// Define an empty TransportCallback for the listener transport
#[derive(Default)]
pub struct DummyTransportPeerEventHandler;

impl TransportPeerEventHandler for DummyTransportPeerEventHandler {
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
