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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
mod unicast;

use std::str::FromStr;

use async_trait::async_trait;
pub use unicast::*;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::{
    core::{endpoint::Address, EndPoint, Locator, Metadata, Reliability},
    transport::BatchSize,
};
use zenoh_result::ZResult;

// Maximum MTU (Serial PDU) in bytes.
const SERIAL_MAX_MTU: BatchSize = z_serial::MAX_MTU as BatchSize;

const DEFAULT_BAUDRATE: u32 = 9_600;

const DEFAULT_EXCLUSIVE: bool = true;

const DEFAULT_TIMEOUT: u64 = 50_000;

const DEFAULT_RELEASE_ON_CLOSE: bool = true;

pub const SERIAL_LOCATOR_PREFIX: &str = "serial";

const SERIAL_MTU_LIMIT: BatchSize = SERIAL_MAX_MTU;

const IS_RELIABLE: bool = false;

zconfigurable! {
    // Default MTU (UDP PDU) in bytes.
    static ref SERIAL_DEFAULT_MTU: BatchSize = SERIAL_MTU_LIMIT;
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

    fn is_reliable(&self, locator: &Locator) -> ZResult<bool> {
        if let Some(reliability) = locator
            .metadata()
            .get(Metadata::RELIABILITY)
            .map(Reliability::from_str)
            .transpose()?
        {
            Ok(reliability == Reliability::Reliable)
        } else {
            Ok(IS_RELIABLE)
        }
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

pub fn get_timeout(endpoint: &EndPoint) -> u64 {
    if let Some(tout) = endpoint.config().get(config::TIMEOUT_RAW) {
        u64::from_str(tout).unwrap_or(DEFAULT_TIMEOUT)
    } else {
        DEFAULT_TIMEOUT
    }
}

pub fn get_release_on_close(endpoint: &EndPoint) -> bool {
    if let Some(release_on_close) = endpoint.config().get(config::RELEASE_ON_CLOSE) {
        bool::from_str(release_on_close).unwrap_or(DEFAULT_RELEASE_ON_CLOSE)
    } else {
        DEFAULT_RELEASE_ON_CLOSE
    }
}

pub fn get_unix_path_as_string(address: Address<'_>) -> String {
    address.as_str().to_owned()
}

pub mod config {
    pub const PORT_BAUD_RATE_RAW: &str = "baudrate";
    pub const PORT_EXCLUSIVE_RAW: &str = "exclusive";
    pub const TIMEOUT_RAW: &str = "tout";
    pub const RELEASE_ON_CLOSE: &str = "release_on_close";
}
