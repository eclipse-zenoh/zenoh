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
use crate::core::whatami::WhatAmIMatcher;

/// # Scout message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The SCOUT message can be sent at any point in time to solicit HELLO messages from matching parties.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|W|I|  SCOUT  |
/// +-+-+-+-+-------+
/// ~      what     ~ if W==1 -- Otherwise implicitly scouting for Routers
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    pub what: Option<WhatAmIMatcher>,
    pub zid_request: bool,
}

impl Scout {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let what = if rng.gen_bool(0.5) {
            Some(WhatAmIMatcher::rand())
        } else {
            None
        };
        let zid_request = rng.gen_bool(0.5);
        Self { what, zid_request }
    }
}
