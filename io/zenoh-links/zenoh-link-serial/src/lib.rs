//
// Copyright (c) 2022 ZettaScale Technology
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

mod unicast;

use std::{convert::TryFrom, net::SocketAddr, path::Path};

use async_trait::async_trait;
pub use unicast::*;
use zenoh_core::{bail, zconfigurable, Result as ZResult};
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol_core::Locator;

// Maximum MTU (Serial PDU) in bytes.
const SERIAL_MAX_MTU: u16 = 8_192;

pub const SERIAL_LOCATOR_PREFIX: &str = "serial";

const SERIAL_MTU_LIMIT: u16 = SERIAL_MAX_MTU;

zconfigurable! {
    // Default MTU (UDP PDU) in bytes.
    static ref SERIAL_DEFAULT_MTU: u16 = SERIAL_MTU_LIMIT;
}

#[derive(Default, Clone, Copy)]
pub struct SerialLocatorInspector;
#[async_trait]
impl LocatorInspector for SerialLocatorInspector {
    fn protocol(&self) -> &str {
        SERIAL_LOCATOR_PREFIX
    }
    async fn is_multicast(&self, locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

pub fn get_unix_path(locator: &Locator) -> &Path {
    locator.address().as_ref()
}

pub fn get_unix_path_as_string(locator: &Locator) -> String {
    locator.address().to_owned()
}
