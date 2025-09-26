//
// Copyright (c) 2022 ZettaScale Technology
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
use crate::common::ZExtBody;

pub type OamId = u16;

pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Transport
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

pub mod id {
    use super::OamId;

    pub const OAM_LINKSTATE: OamId = 0x0001;
}

/// ```text
/// Flags:
/// - E |: Encoding     The encoding of the extension
/// - E/
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|ENC|  OAM    |
/// +-+-+-+---------+
/// ~    id:z16     ~
/// +---------------+
/// %    length     % -- If ENC == Z64 || ENC == ZBuf (z32)
/// +---------------+
/// ~     [u8]      ~ -- If ENC == ZBuf
/// +---------------+
///
/// Encoding:
/// - 0b00: Unit
/// - 0b01: Z64
/// - 0b10: ZBuf
/// - 0b11: Reserved
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oam {
    pub id: OamId,
    pub body: ZExtBody,
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
}

pub mod ext {
    use crate::{
        common::{ZExtZ64, ZExtZBuf},
        zextz64, zextzbuf,
    };

    pub type QoS = zextz64!(0x1, false);
    pub type QoSType = crate::network::ext::QoSType<{ QoS::ID }>;

    pub type Timestamp = zextzbuf!(0x2, false);
    pub type TimestampType = crate::network::ext::TimestampType<{ Timestamp::ID }>;
}

impl Oam {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let id: OamId = rng.gen();
        let body = ZExtBody::rand();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);

        Self {
            id,
            body,
            ext_qos,
            ext_tstamp,
        }
    }
}
