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
use alloc::vec::Vec;
use core::fmt;

use crate::core::{Locator, WhatAmI, ZenohIdProto};

/// # Hello message
///
/// The [`Hello`] message is used to advertise the locators a zenoh node is reachable at.
/// The [`Hello`] message SHOULD be sent in a unicast fashion in response to a [`super::Scout`]
/// message as shown below:
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
/// Moreover, a [`Hello`] message MAY be sent in the network in a multicast
/// fashion to advertise the presence of zenoh node. The advertisement operation MAY be performed
/// periodically as shown below:
///
///```text
/// A                   B                   C
/// |       HELLO       |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// ~        ...        ~        ...        ~
/// |                   |                   |
/// |       HELLO       |                   |
/// |------------------>|                   |
/// |         \---------------------------->|
/// |                   |                   |
/// ~        ...        ~        ...        ~
/// |                   |                   |
/// ```
///
/// Examples of locators included in the [`Hello`] message are:
///
/// ```text
///  udp/192.168.1.1:7447
///  tcp/192.168.1.1:7447
///  udp/224.0.0.224:7447
///  tcp/localhost:7447
/// ```
///
/// The [`Hello`] message structure is defined as follows:
///
/// ```text
/// Header flags:
/// - L: Locators       If L==1 then the list of locators is present, else the src address is the locator
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|L|  HELLO  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |zid_len|X|X|wai| (*)
/// +-+-+-+-+-+-+-+-+
/// ~     [u8]      ~ -- ZenohID
/// +---------------+
/// ~   <utf8;z8>   ~ if Flag(L)==1 -- List of locators
/// +---------------+
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the HELLO message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
/// ```
///
pub mod flag {
    pub const L: u8 = 1 << 5; // 0x20 Locators      if L==1 then the list of locators is present, else the src address is the locator
                              // pub const X: u8 = 1 << 6; // 0x40       Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloProto {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohIdProto,
    pub locators: Vec<Locator>,
}

impl HelloProto {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let zid = ZenohIdProto::default();
        let whatami = WhatAmI::rand();
        let locators = if rng.gen_bool(0.5) {
            Vec::from_iter((1..5).map(|_| Locator::rand()))
        } else {
            vec![]
        };
        Self {
            version,
            zid,
            whatami,
            locators,
        }
    }
}

/// A zenoh Hello message.
pub struct Hello(HelloProto);

impl Hello {
    /// Get the locators of this Hello message.
    pub fn locators(&self) -> &[Locator] {
        &self.0.locators
    }

    /// Get the zenoh id of this Hello message.
    pub fn zid(&self) -> ZenohIdProto {
        self.0.zid
    }

    /// Get the whatami of this Hello message.
    pub fn whatami(&self) -> WhatAmI {
        self.0.whatami
    }
}

impl From<HelloProto> for Hello {
    fn from(inner: HelloProto) -> Self {
        Hello(inner)
    }
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Hello")
            .field("zid", &self.zid())
            .field("whatami", &self.whatami())
            .field("locators", &self.locators())
            .finish()
    }
}
