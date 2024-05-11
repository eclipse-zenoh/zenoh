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
// filetag{rust.time}

use std::convert::TryFrom;

use zenoh_protocol::core::{Timestamp, TimestampId};

/// Generates a reception [`Timestamp`] with id=0x01.
/// This operation should be called if a timestamp is required for an incoming [`zenoh::Sample`](crate::Sample)
/// that doesn't contain any timestamp.
pub fn new_reception_timestamp() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    Timestamp::new(now.into(), TimestampId::try_from([1]).unwrap())
}
