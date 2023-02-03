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
use crate::core::{WhatAmI, ZenohId};
use zenoh_buffers::ZSlice;

/// # Init message
///
/// The INIT message is sent on a specific Locator to initiate a transport with the zenoh node
/// associated with that Locator. The initiator MUST send an INIT message with the A flag set to 0.
/// If the corresponding zenohd node deems appropriate to accept the INIT message, the corresponding
/// peer MUST reply with an INIT message with the A flag set to 1. Alternatively, it MAY reply with
/// a [`super::Close`] message. For convenience, we call [`InitSyn`] and [`InitAck`] an INIT message
/// when the A flag is set to 0 and 1, respectively.
///
/// The [`InitSyn`]/[`InitAck`] message flow is the following:
///
/// ```text
///     A                   B
///     |      INIT SYN     |
///     |------------------>|
///     |                   |
///     |      INIT ACK     |
///     |<------------------|
///     |                   |
/// ```
///
/// The INIT message structure is defined as follows:
///
/// ```text
/// Flags:
/// - A: Ack            If A==0 then the message is an InitSyn else it is an InitAck
/// - S: Size params    If S==1 then size parameters are exchanged
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|S|A|   INIT  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |zid_len|x|x|wai| (#)(*)
/// +-------+-+-+---+
/// ~      [u8]     ~ -- ZenohID of the sender of the INIT message
/// +---------------+
/// |x|x|kid|rid|fsn| \                -- SN/ID resolution (+)
/// +---------------+  | if Flag(S)==1
/// |      u16      |  |               -- Batch Size ($)
/// |               | /
/// +---------------+
/// ~    <u8;z16>   ~ -- if Flag(A)==1 -- Cookie
/// +---------------+
/// ~   [InitExts]  ~ -- if Flag(Z)==1
/// +---------------+
///
/// If A==1 and S==0 then size parameters are (ie. S flag) are accepted.
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the INIT message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
///
/// (#) ZID length. It indicates how many bytes are used for the ZenohID bytes.
///     A ZenohID is minimum 1 byte and maximum 16 bytes. Therefore, the actual lenght is computed as:
///         real_zid_len := 1 + zid_len
///
/// (+) Sequence Number/ID resolution. It indicates the resolution and consequently the wire overhead
///     of various SN and ID in Zenoh.
///     - fsn: frame/fragment sequence number resolution. Used in Frame/Fragment messages.
///     - rid: request ID resolution. Used in Request/Response messages.
///     - kid: key expression ID resolution. Used in Push/Request/Response messages.
///     The valid SN/ID resolution values are:
///     - 0b00: 8 bits
///     - 0b01: 16 bits
///     - 0b10: 32 bits
///     - 0b11: 64 bits
///
/// ($) Batch Size. It indicates the maximum size of a batch the sender of the INIT message is willing
///     to accept when reading from the network.
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
/// ```
///

pub mod flag {
    pub const A: u8 = 1 << 5; // 0x20 Ack           if A==0 then the message is an InitSyn else it is an InitAck
    pub const S: u8 = 1 << 6; // 0x40 Size params   if S==1 then size parameters are exchanged
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[repr(u8)]
// The value represents the 2-bit encoded value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bits {
    U8 = 0b00,
    U16 = 0b01,
    U32 = 0b10,
    U64 = 0b11,
}

#[repr(u8)]
// The value indicates the bit offest
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    FrameSN = 0,
    RequestID = 2,
    KeyExprID = 4,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution(u8);

impl Resolution {
    pub const fn as_u8(&self) -> u8 {
        self.0
    }

    pub const fn get(&self, field: Field) -> Bits {
        let value = (self.0 >> (field as u8)) & 0b11;
        unsafe { core::mem::transmute(value) }
    }

    pub fn set(&mut self, field: Field, bits: Bits) {
        self.0 &= !(0b11 << field as u8); // Clear bits
        self.0 |= (bits as u8) << (field as u8); // Set bits
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let v: u8 = rng.gen();
        Self(v & 0b00111111)
    }
}

impl Default for Resolution {
    fn default() -> Self {
        let frame_sn = Bits::U64 as u8;
        let request_id = (Bits::U64 as u8) << 2;
        let keyexpr_id = (Bits::U64 as u8) << 4;
        Self(frame_sn | request_id | keyexpr_id)
    }
}

impl From<u8> for Resolution {
    fn from(v: u8) -> Self {
        Self(v)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub resolution: Resolution,
    pub batch_size: u16,
    pub qos: Option<ext::QoS>,
    pub shm: Option<ext::Shm>,
    pub auth: Option<ext::Auth>,
}

// Extensions
pub mod ext {
    use crate::common::{ZExtUnit, ZExtZSlice};

    pub type QoS = ZExtUnit<0x01>;
    pub type Shm = ZExtZSlice<0x02>;
    pub type Auth = ZExtZSlice<0x03>;
}

impl InitSyn {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::common::{ZExtUnit, ZExtZSlice};
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohId::default();
        let resolution = Resolution::rand();
        let batch_size: u16 = rng.gen();
        let qos = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let shm = rng.gen_bool(0.5).then_some(ZExtZSlice::rand());
        let auth = rng.gen_bool(0.5).then_some(ZExtZSlice::rand());

        Self {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            qos,
            shm,
            auth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct InitAck {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub resolution: Resolution,
    pub batch_size: u16,
    pub cookie: ZSlice,
    pub qos: Option<ext::QoS>,
    pub shm: Option<ext::Shm>,
    pub auth: Option<ext::Auth>,
}

impl InitAck {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::common::{ZExtUnit, ZExtZSlice};
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohId::default();
        let resolution = if rng.gen_bool(0.5) {
            Resolution::default()
        } else {
            Resolution::rand()
        };
        let batch_size: u16 = rng.gen();
        let cookie = ZSlice::rand(64);
        let qos = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let shm = rng.gen_bool(0.5).then_some(ZExtZSlice::rand());
        let auth = rng.gen_bool(0.5).then_some(ZExtZSlice::rand());

        Self {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            cookie,
            qos,
            shm,
            auth,
        }
    }
}
