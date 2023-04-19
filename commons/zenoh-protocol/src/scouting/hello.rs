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
use crate::core::{Locator, WhatAmI, ZenohId};
use alloc::vec::Vec;
use core::fmt;

/// # Hello message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The HELLO message is sent in any of the following three cases:
///     1) in response to a SCOUT message;
///     2) to (periodically) advertise (e.g., on multicast) the Peer and the locators it is reachable at;
///     3) in a already established transport to update the corresponding peer on the new capabilities
///        (i.e., whatmai) and/or new set of locators (i.e., added or deleted).
/// Locators are expressed as:
/// <code>
///  udp/192.168.0.2:1234
///  tcp/192.168.0.2:1234
///  udp/239.255.255.123:5555
/// <code>
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |L|W|I|  HELLO  |
/// +-+-+-+-+-------+
/// ~    peer-id    ~ if I==1
/// +---------------+
/// ~    whatami    ~ if W==1 -- Otherwise it is from a Router
/// +---------------+
/// ~   [Locators]  ~ if L==1 -- Otherwise src-address is the locator
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub zid: Option<ZenohId>,
    pub whatami: WhatAmI,
    pub locators: Vec<Locator>,
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Hello")
            .field("zid", &self.zid)
            .field("whatami", &self.whatami)
            .field("locators", &self.locators)
            .finish()
    }
}

impl Hello {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let zid = if rng.gen_bool(0.5) {
            Some(ZenohId::default())
        } else {
            None
        };
        let whatami = WhatAmI::rand();
        let locators = if rng.gen_bool(0.5) {
            Vec::from_iter((1..5).map(|_| Locator::rand()))
        } else {
            vec![]
        };
        Self {
            zid,
            whatami,
            locators,
        }
    }
}
