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
pub use manager::*;
pub use multicast::*;
pub use primitives::*;
use std::sync::Arc;
pub use unicast::*;
use zenoh_util::core::ZResult;

/*************************************/
/*             HANDLER               */
/*************************************/
pub trait TransportEventHandler: Send + Sync {
    fn new_unicast(
        &self,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportUnicastEventHandler>>;

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
    ) -> ZResult<Arc<dyn TransportUnicastEventHandler>> {
        Ok(Arc::new(DummyTransportUnicastEventHandler::default()))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        Ok(Arc::new(DummyTransportMulticastEventHandler::default()))
    }
}
