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
use std::str::FromStr;

use async_trait::async_trait;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::core::{endpoint::Address, Locator, Metadata, Reliability};
#[cfg(target_family = "unix")]
use zenoh_protocol::transport::BatchSize;
use zenoh_result::ZResult;

#[cfg(target_family = "unix")]
mod unicast;
#[cfg(target_family = "unix")]
pub use unicast::*;

// Default MTU (UnixSocketStream PDU) in bytes.
// NOTE: Since UnixSocketStream is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the UNIXSOCKSTREAM MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
#[cfg(target_family = "unix")]
const UNIXSOCKSTREAM_MAX_MTU: BatchSize = BatchSize::MAX;

pub const UNIXSOCKSTREAM_LOCATOR_PREFIX: &str = "unixsock-stream";

const IS_RELIABLE: bool = true;

zconfigurable! {
    // Default MTU (UNIXSOCKSTREAM PDU) in bytes.
    #[cfg(target_family = "unix")]
    static ref UNIXSOCKSTREAM_DEFAULT_MTU: BatchSize = UNIXSOCKSTREAM_MAX_MTU;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    #[cfg(target_family = "unix")]
    static ref UNIXSOCKSTREAM_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

#[derive(Default, Clone, Copy)]
pub struct UnixSockStreamLocatorInspector;
#[async_trait]
impl LocatorInspector for UnixSockStreamLocatorInspector {
    fn protocol(&self) -> &str {
        UNIXSOCKSTREAM_LOCATOR_PREFIX
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

pub fn get_unix_path_as_string(address: Address<'_>) -> String {
    address.to_string()
}
