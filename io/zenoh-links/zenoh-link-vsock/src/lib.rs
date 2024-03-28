//
// Copyright (c) 2024 ZettaScale Technology
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
//!
//! Implements [vsock](https://man7.org/linux/man-pages/man7/vsock.7.html) link support.
use async_trait::async_trait;
use zenoh_core::zconfigurable;
use zenoh_link_commons::LocatorInspector;
use zenoh_protocol::core::Locator;
use zenoh_result::ZResult;

#[cfg(target_os = "linux")]
mod unicast;
#[cfg(target_os = "linux")]
pub use unicast::*;

pub const VSOCK_LOCATOR_PREFIX: &str = "vsock";

#[derive(Default, Clone, Copy)]
pub struct VsockLocatorInspector;
#[async_trait]
impl LocatorInspector for VsockLocatorInspector {
    fn protocol(&self) -> &str {
        VSOCK_LOCATOR_PREFIX
    }

    async fn is_multicast(&self, _locator: &Locator) -> ZResult<bool> {
        Ok(false)
    }
}

zconfigurable! {
    // Default MTU in bytes.
    static ref VSOCK_DEFAULT_MTU: u16 = u16::MAX;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref VSOCK_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}
