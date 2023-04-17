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
mod unicast;

use async_trait::async_trait;
use std::str::FromStr;
pub use unicast::*;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::core::{endpoint::Address, EndPoint, Locator};
use zenoh_result::ZResult;

// Maximum MTU (Serial PDU) in bytes.
const SERIAL_MAX_MTU: u16 = z_serial::MAX_MTU as u16;

const DEFAULT_BAUDRATE: u32 = 9_600;

const DEFAULT_EXCLUSIVE: bool = true;

pub const SERIAL_LOCATOR_PREFIX: &str = "serial";

const SERIAL_MTU_LIMIT: u16 = SERIAL_MAX_MTU;

zconfigurable! {
    // Default MTU (UDP PDU) in bytes.
    static ref SERIAL_DEFAULT_MTU: u16 = SERIAL_MTU_LIMIT;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref SERIAL_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[derive(Default, Clone, Copy)]
pub struct SerialLocatorInspector;
#[async_trait]
impl LocatorInspector for SerialLocatorInspector {
    fn protocol(&self) -> &str {
        SERIAL_LOCATOR_PREFIX
    }
    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

pub fn get_baud_rate(endpoint: &EndPoint) -> u32 {
    if let Some(baudrate) = endpoint.config().get(config::PORT_BAUD_RATE_RAW) {
        u32::from_str(baudrate).unwrap_or(DEFAULT_BAUDRATE)
    } else {
        DEFAULT_BAUDRATE
    }
}

pub fn get_exclusive(endpoint: &EndPoint) -> bool {
    if let Some(exclusive) = endpoint.config().get(config::PORT_EXCLUSIVE_RAW) {
        bool::from_str(exclusive).unwrap_or(DEFAULT_EXCLUSIVE)
    } else {
        DEFAULT_EXCLUSIVE
    }
}

pub fn get_unix_path_as_string(address: Address<'_>) -> String {
    address.as_str().to_owned()
}

pub mod config {
    pub const PORT_BAUD_RATE_RAW: &str = "baudrate";
    pub const PORT_EXCLUSIVE_RAW: &str = "exclusive";
}
