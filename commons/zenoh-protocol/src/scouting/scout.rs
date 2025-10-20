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
use crate::core::{whatami::WhatAmIMatcher, ZenohIdProto};

/// # Scout message
///
/// The [`Scout`] message MAY be sent at any point in time to discover the available zenoh nodes in the
/// network. The [`Scout`] message SHOULD be sent in a multicast or broadcast fashion. Upon receiving a
/// [`Scout`] message, a zenoh node MUST first verify whether the matching criteria are satisfied, then
/// it SHOULD reply with a [`super::HelloProto`] message in a unicast fashion including all the requested
/// information.
///
/// The scouting message flow is the following:
///
/// ```text
/// A                   B                   C
/// |       SCOUT       |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// |       HELLO       |                   |
/// |<------------------|                   |
/// |                   |      HELLO        |
/// |<--------------------------------------|
/// |                   |                   |
/// ```
///
/// The SCOUT message structure is defined as follows:
///
/// ```text
/// Header flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X|  SCOUT  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |zid_len|I| what| (#)(*)
/// +-+-+-+-+-+-+-+-+
/// ~      [u8]     ~ if Flag(I)==1 -- ZenohID
/// +---------------+
///
/// (#) ZID length. If Flag(I)==1 it indicates how many bytes are used for the ZenohID bytes.
///     A ZenohID is minimum 1 byte and maximum 16 bytes. Therefore, the actual length is computed as:
///         real_zid_len := 1 + zid_len
///
/// (*) What. It indicates a bitmap of WhatAmI interests.
///    The valid bitflags are:
///    - 0b001: Router
///    - 0b010: Peer
///    - 0b100: Client
/// ```
pub mod flag {
    pub const I: u8 = 1 << 3; // 0x04 ZenohID       if I==1 then the ZenohID is present
                              // pub const X: u8 = 1 << 6; // 0x40       Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    pub version: u8,
    pub what: WhatAmIMatcher,
    pub zid: Option<ZenohIdProto>,
}

impl Scout {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let what = WhatAmIMatcher::rand();
        let zid = rng.gen_bool(0.5).then_some(ZenohIdProto::rand());
        Self { version, what, zid }
    }
}
