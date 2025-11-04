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

//! # Init message
//!
//! The INIT message is sent on a specific Locator to initiate a transport with the zenoh node
//! associated with that Locator. The initiator MUST send an INIT message with the A flag set to 0.
//! If the corresponding zenohd node deems appropriate to accept the INIT message, the corresponding
//! peer MUST reply with an INIT message with the A flag set to 1. Alternatively, it MAY reply with
//! a [`super::Close`] message. For convenience, we call [`InitSyn`] and [`InitAck`] an INIT message
//! when the A flag is set to 0 and 1, respectively.
//!
//! The [`InitSyn`]/[`InitAck`] message flow is the following:
//!
//! ```text
//!     A                   B
//!     |      INIT SYN     |
//!     |------------------>|
//!     |                   |
//!     |      INIT ACK     |
//!     |<------------------|
//!     |                   |
//! ```
//!
//! The INIT message structure is defined as follows:
//!
//! ```text
//! Flags:
//! - A: Ack            If A==0 then the message is an InitSyn else it is an InitAck
//! - S: Size params    If S==1 then size parameters are exchanged
//! - Z: Extensions     If Z==1 then zenoh extensions will follow.
//!
//!  7 6 5 4 3 2 1 0
//! +-+-+-+-+-+-+-+-+
//! |Z|S|A|   INIT  |
//! +-+-+-+---------+
//! |    version    |
//! +---------------+
//! |zid_len|x|x|wai| (#)(*)
//! +-------+-+-+---+
//! ~      [u8]     ~ -- ZenohID of the sender of the INIT message
//! +---------------+
//! |x|x|x|x|rid|fsn| \                -- SN/ID resolution (+)
//! +---------------+  | if Flag(S)==1
//! |      u16      |  |               -- Batch Size ($)
//! |               | /
//! +---------------+
//! ~    <u8;z16>   ~ -- if Flag(A)==1 -- Cookie
//! +---------------+
//! ~   [InitExts]  ~ -- if Flag(Z)==1
//! +---------------+
//!
//! If A==1 and S==0 then size parameters are (ie. S flag) are accepted.
//!
//! (*) WhatAmI. It indicates the role of the zenoh node sending the INIT message.
//!    The valid WhatAmI values are:
//!    - 0b00: Router
//!    - 0b01: Peer
//!    - 0b10: Client
//!    - 0b11: Reserved
//!
//! (#) ZID length. It indicates how many bytes are used for the ZenohID bytes.
//!     A ZenohID is minimum 1 byte and maximum 16 bytes. Therefore, the actual length is computed as:
//!         real_zid_len := 1 + zid_len
//!
//! (+) Sequence Number/ID resolution. It indicates the resolution and consequently the wire overhead
//!     of various SN and ID in Zenoh.
//!     - fsn: frame/fragment sequence number resolution. Used in Frame/Fragment messages.
//!     - rid: request ID resolution. Used in Request/Response messages.
//!     The valid SN/ID resolution values are:
//!     - 0b00: 8 bits
//!     - 0b01: 16 bits
//!     - 0b10: 32 bits
//!     - 0b11: 64 bits
//!
//! ($) Batch Size. It indicates the maximum size of a batch the sender of the INIT message is willing
//!     to accept when reading from the network. Default on unicast: 65535.
//!
//! NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
//!       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
//!       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
//!       the boundary of the serialized messages. The length is encoded as little-endian.
//!       In any case, the length of a message must not exceed 65535 bytes.
//! ```

use zenoh_buffers::ZSlice;

use crate::{
    core::{Resolution, WhatAmI, ZenohIdProto},
    transport::BatchSize,
};

pub mod flag {
    pub const A: u8 = 1 << 5; // 0x20 Ack           if A==0 then the message is an InitSyn else it is an InitAck
    pub const S: u8 = 1 << 6; // 0x40 Size params   if S==1 then size parameters are exchanged
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// # InitSyn message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// +---------------+
///
/// ZExtUnit
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohIdProto,
    pub resolution: Resolution,
    pub batch_size: BatchSize,
    pub ext_qos: Option<ext::QoS>,
    pub ext_qos_link: Option<ext::QoSLink>,
    #[cfg(feature = "shared-memory")]
    pub ext_shm: Option<ext::Shm>,
    pub ext_auth: Option<ext::Auth>,
    pub ext_mlink: Option<ext::MultiLink>,
    pub ext_lowlatency: Option<ext::LowLatency>,
    pub ext_compression: Option<ext::Compression>,
    pub ext_patch: ext::PatchType,
}

// Extensions
pub mod ext {
    use crate::{
        common::{ZExtUnit, ZExtZ64, ZExtZBuf},
        zextunit, zextz64, zextzbuf,
    };

    /// # QoS extension
    /// Used to negotiate the use of QoS
    pub type QoS = zextunit!(0x1, false);
    pub type QoSLink = zextz64!(0x1, false);

    /// # Shm extension
    /// Used as challenge for probing shared memory capabilities
    #[cfg(feature = "shared-memory")]
    pub type Shm = zextzbuf!(0x2, false);

    /// # Auth extension
    /// Used as challenge for probing authentication rights
    pub type Auth = zextzbuf!(0x3, false);

    /// # Multilink extension
    /// Used as challenge for probing multilink capabilities
    pub type MultiLink = zextzbuf!(0x4, false);

    /// # LowLatency extension
    /// Used to negotiate the use of lowlatency transport
    pub type LowLatency = zextunit!(0x5, false);

    /// # Compression extension
    /// Used to negotiate the use of compression on the link
    pub type Compression = zextunit!(0x6, false);

    /// # Patch extension
    /// Used to negotiate the patch version of the protocol
    /// if not present (or 0), then protocol as released with 1.0.0
    /// if >= 1, then fragmentation first/drop markers
    pub type Patch = zextz64!(0x7, false);
    pub type PatchType = crate::transport::ext::PatchType<{ Patch::ID }>;
}

impl InitSyn {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        use crate::common::{ZExtUnit, ZExtZ64, ZExtZBuf};

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohIdProto::default();
        let resolution = Resolution::rand();
        let batch_size: BatchSize = rng.gen();
        let ext_qos = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_qos_link = rng.gen_bool(0.5).then_some(ZExtZ64::rand());
        #[cfg(feature = "shared-memory")]
        let ext_shm = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_auth = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_mlink = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_lowlatency = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_compression = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_patch = ext::PatchType::rand();

        Self {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        }
    }
}

/// # InitAck message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// ~     nonce     ~
/// +---------------+
///
/// ZExtZ64
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitAck {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohIdProto,
    pub resolution: Resolution,
    pub batch_size: BatchSize,
    pub cookie: ZSlice,
    pub ext_qos: Option<ext::QoS>,
    pub ext_qos_link: Option<ext::QoSLink>,
    #[cfg(feature = "shared-memory")]
    pub ext_shm: Option<ext::Shm>,
    pub ext_auth: Option<ext::Auth>,
    pub ext_mlink: Option<ext::MultiLink>,
    pub ext_lowlatency: Option<ext::LowLatency>,
    pub ext_compression: Option<ext::Compression>,
    pub ext_patch: ext::PatchType,
}

impl InitAck {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        use crate::common::{ZExtUnit, ZExtZ64, ZExtZBuf};

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohIdProto::default();
        let resolution = if rng.gen_bool(0.5) {
            Resolution::default()
        } else {
            Resolution::rand()
        };
        let batch_size: BatchSize = rng.gen();
        let cookie = ZSlice::rand(64);
        let ext_qos = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_qos_link = rng.gen_bool(0.5).then_some(ZExtZ64::rand());
        #[cfg(feature = "shared-memory")]
        let ext_shm = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_auth = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_mlink = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_lowlatency = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_compression = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_patch = ext::PatchType::rand();

        Self {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            cookie,
            ext_qos,
            ext_qos_link,
            #[cfg(feature = "shared-memory")]
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
            ext_compression,
            ext_patch,
        }
    }
}
