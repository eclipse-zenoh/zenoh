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
use core::time::Duration;

use crate::{
    core::{Priority, Resolution, WhatAmI, ZenohIdProto},
    transport::{BatchSize, PrioritySn},
};

/// # Join message
///
/// The JOIN message is sent on a multicast Locator to advertise the transport parameters.
///
/// The [`Join`] message flow is the following:
///
/// ```text
///     A                   B
///     |       JOIN        |
///     |------------------>|
///     |                   |
///    ...                 ...
///     |       JOIN        |
///     |------------------>|
///     |                   |
/// ```
///
/// The JOIN message structure is defined as follows:
///
/// ```text
/// Flags:
/// - T: Lease period   if T==1 then the lease period is in seconds otherwise it is in milliseconds
/// - S: Size params    If S==1 then size parameters are exchanged
/// - Z: Extensions     If Z==1 then Zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|S|T|   JOIN  |
/// +-+-+-+---------+
/// |    version    |
/// +---------------+
/// |zid_len|x|x|wai| (#)(*)
/// +-------+-+-+---+
/// ~      [u8]     ~ -- ZenohID of the sender of the JOIN message
/// +---------------+
/// |x|x|x|x|rid|fsn| \                -- SN/ID resolution (+)
/// +---------------+  | if Flag(S)==1
/// |      u16      |  |               -- Batch Size ($)
/// |               | /
/// +---------------+
/// %     lease     % -- Lease period of the sender of the JOIN message
/// +---------------+
/// %    next_sn    % -- Next SN to be sent by the sender of the JOIN message (^)
/// +---------------+
/// ~   [JoinExts]  ~ -- if Flag(Z)==1
/// +---------------+
///
/// (*) WhatAmI. It indicates the role of the zenoh node sending the JOIN message.
///    The valid WhatAmI values are:
///    - 0b00: Router
///    - 0b01: Peer
///    - 0b10: Client
///    - 0b11: Reserved
///
/// (#) ZID length. It indicates how many bytes are used for the ZenohID bytes.
///     A ZenohID is minimum 1 byte and maximum 16 bytes. Therefore, the actual length is computed as:
///         real_zid_len := 1 + zid_len
///
/// (+) Sequence Number/ID resolution. It indicates the resolution and consequently the wire overhead
///     of various SN and ID in Zenoh.
///     - fsn: frame/fragment sequence number resolution. Used in Frame/Fragment messages.
///     - rid: request ID resolution. Used in Request/Response messages.
///     The valid SN/ID resolution values are:
///     - 0b00: 8 bits
///     - 0b01: 16 bits
///     - 0b10: 32 bits
///     - 0b11: 64 bits
///
/// ($) Batch Size. It indicates the maximum size of a batch the sender of the JOIN message is willing
///     to accept when reading from the network. Default on multicast: 8192.
///
/// (^) The next sequence number MUST be compatible with the adverstised Sequence Number resolution
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohIdProto,
    pub resolution: Resolution,
    pub batch_size: BatchSize,
    pub lease: Duration,
    pub next_sn: PrioritySn,
    pub ext_qos: Option<ext::QoSType>,
    pub ext_shm: Option<ext::Shm>,
    pub ext_patch: ext::PatchType,
}

pub mod flag {
    pub const T: u8 = 1 << 5; // 0x20 Lease period  if T==1 then the lease period is in seconds else in milliseconds
    pub const S: u8 = 1 << 6; // 0x40 Size params   if S==1 then size parameters are advertised
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

// Extensions
pub mod ext {
    use alloc::boxed::Box;

    use super::{Priority, PrioritySn};
    use crate::{
        common::{ZExtZ64, ZExtZBuf},
        zextz64, zextzbuf,
    };

    /// # QoS extension
    /// Used to announce next sn when QoS is enabled
    pub type QoS = zextzbuf!(0x1, true);
    pub type QoSType = Box<[PrioritySn; Priority::NUM]>;

    /// # Shm extension
    /// Used to advertise shared memory capabilities
    pub type Shm = zextzbuf!(0x2, true);

    /// # Patch extension
    /// Used to negotiate the patch version of the protocol
    /// if not present (or 0), then protocol as released with 1.0.0
    /// if >= 1, then fragmentation first/drop markers
    pub type Patch = zextz64!(0x7, false); // use the same id as Init
    pub type PatchType = crate::transport::ext::PatchType<{ Patch::ID }>;
}

impl Join {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        use crate::common::ZExtZBuf;

        let mut rng = rand::thread_rng();

        let version: u8 = rng.gen();
        let whatami = WhatAmI::rand();
        let zid = ZenohIdProto::default();
        let resolution = Resolution::rand();
        let batch_size: BatchSize = rng.gen();
        let lease = if rng.gen_bool(0.5) {
            Duration::from_secs(rng.gen())
        } else {
            Duration::from_millis(rng.gen())
        };
        let next_sn = PrioritySn::rand();
        let ext_qos = rng
            .gen_bool(0.5)
            .then_some(Box::new([PrioritySn::rand(); Priority::NUM]));
        let ext_shm = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_patch = ext::PatchType::rand();

        Self {
            version,
            whatami,
            zid,
            resolution,
            batch_size,
            lease,
            next_sn,
            ext_qos,
            ext_shm,
            ext_patch,
        }
    }
}
