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
use zenoh_buffers::ZSlice;

use crate::core::Reliability;
pub use crate::transport::TransportSn;

pub mod flag {
    pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then the frame is reliable
    pub const M: u8 = 1 << 6; // 0x40 More          if M==1 then another fragment will follow
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// # Fragment message
///
/// The [`Fragment`] message is used to transmit on the wire large [`NetworkMessage`](crate::network::NetworkMessage)
/// that require fragmentation because they are larger than the maximum batch size
/// (i.e. 2^16-1) and/or the link MTU.
///
/// The [`Fragment`] message flow is the following:
///
/// ```text
///     A                   B
///     |  FRAGMENT(MORE)   |
///     |------------------>|
///     |  FRAGMENT(MORE)   |
///     |------------------>|
///     |  FRAGMENT(MORE)   |
///     |------------------>|
///     |  FRAGMENT         |
///     |------------------>|
///     |                   |
/// ```
///
/// The [`Fragment`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - R: Reliable       If R==1 it concerns the reliable channel, else the best-effort channel
/// - M: More           If M==1 then other fragments will follow
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|R| FRAGMENT|
/// +-+-+-+---------+
/// %    seq num    %
/// +---------------+
/// ~   [FragExts]  ~ if Flag(Z)==1
/// +---------------+
/// ~      [u8]     ~
/// +---------------+
/// ```
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub reliability: Reliability,
    pub more: bool,
    pub sn: TransportSn,
    pub payload: ZSlice,
    pub ext_qos: ext::QoSType,
    pub ext_first: Option<ext::First>,
    pub ext_drop: Option<ext::Drop>,
}

// Extensions
pub mod ext {
    use crate::{
        common::{ZExtUnit, ZExtZ64},
        zextunit, zextz64,
    };

    pub type QoS = zextz64!(0x1, true);
    pub type QoSType = crate::transport::ext::QoSType<{ QoS::ID }>;

    /// # Start extension
    /// Mark the first fragment of a fragmented message
    pub type First = zextunit!(0x2, false);
    /// # Stop extension
    /// Indicate that the remaining fragments has been dropped
    pub type Drop = zextunit!(0x3, false);
}

impl Fragment {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let reliability = Reliability::rand();
        let more = rng.gen_bool(0.5);
        let sn: TransportSn = rng.gen();
        let payload = ZSlice::rand(rng.gen_range(8..128));
        let ext_qos = ext::QoSType::rand();
        let ext_first = rng.gen_bool(0.5).then(ext::First::rand);
        let ext_drop = rng.gen_bool(0.5).then(ext::Drop::rand);

        Fragment {
            reliability,
            sn,
            more,
            payload,
            ext_qos,
            ext_first,
            ext_drop,
        }
    }
}

// FragmentHeader
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FragmentHeader {
    pub reliability: Reliability,
    pub more: bool,
    pub sn: TransportSn,
    pub ext_qos: ext::QoSType,
    pub ext_first: Option<ext::First>,
    pub ext_drop: Option<ext::Drop>,
}

impl FragmentHeader {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let reliability = Reliability::rand();
        let more = rng.gen_bool(0.5);
        let sn: TransportSn = rng.gen();
        let ext_qos = ext::QoSType::rand();
        let ext_first = rng.gen_bool(0.5).then(ext::First::rand);
        let ext_drop = rng.gen_bool(0.5).then(ext::Drop::rand);

        FragmentHeader {
            reliability,
            more,
            sn,
            ext_qos,
            ext_first,
            ext_drop,
        }
    }
}
