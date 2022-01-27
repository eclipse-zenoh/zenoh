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

use std::path::Path;

use zenoh_core::zconfigurable;

mod unicast;
pub use unicast::*;
use zenoh_protocol_core::Locator;
// Default MTU (UnixSocketStream PDU) in bytes.
// NOTE: Since UnixSocketStream is a byte-stream oriented transport, theoretically it has
//       no limit regarding the MTU. However, given the batching strategy
//       adopted in Zenoh and the usage of 16 bits in Zenoh to encode the
//       payload length in byte-streamed, the UNIXSOCKSTREAM MTU is constrained to
//       2^16 - 1 bytes (i.e., 65535).
const UNIXSOCKSTREAM_MAX_MTU: u16 = u16::MAX;

pub const UNIXSOCKSTREAM_LOCATOR_PREFIX: &str = "unixsock-stream";

zconfigurable! {
    // Default MTU (UNIXSOCKSTREAM PDU) in bytes.
    static ref UNIXSOCKSTREAM_DEFAULT_MTU: u16 = UNIXSOCKSTREAM_MAX_MTU;
    // Amount of time in microseconds to throttle the accept loop upon an error.
    // Default set to 100 ms.
    static ref UNIXSOCKSTREAM_ACCEPT_THROTTLE_TIME: u64 = 100_000;
}

pub fn get_unix_path(locator: &Locator) -> &Path {
    locator.address().as_ref()
}

pub fn get_unix_path_as_string(locator: &Locator) -> String {
    locator.address().to_owned()
}
