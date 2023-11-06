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
use crate::transport::TransportSn;
use core::time::Duration;
use zenoh_buffers::ZSlice;

/// # Open message
///
/// After having succesfully complete the [`super::InitSyn`]-[`super::InitAck`] message exchange,
/// the OPEN message is sent on a link to finalize the initialization of the link and
/// associated transport with a zenoh node.
/// For convenience, we call [`OpenSyn`] and [`OpenAck`] an OPEN message with the A flag
/// is set to 0 and 1, respectively.
///
/// The [`OpenSyn`]/[`OpenAck`] message flow is the following:
///
/// ```text
///     A                   B
///     |      OPEN SYN     |
///     |------------------>|
///     |                   |
///     |      OPEN ACK     |
///     |<------------------|
///     |                   |
/// ```
///
/// ```text
/// Flags:
/// - A: Ack            If A==0 then the message is an OpenSyn else it is an OpenAck
/// - T: Lease period   if T==1 then the lease period is in seconds else in milliseconds
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|T|A|   OPEN  |
/// +-+-+-+---------+
/// %     lease     % -- Lease period of the sender of the OPEN message
/// +---------------+
/// %  initial_sn   % -- Initial SN proposed by the sender of the OPEN(*)
/// +---------------+
/// ~    <u8;z16>   ~ if Flag(A)==0 (**) -- Cookie
/// +---------------+
/// ~   [OpenExts]  ~ if Flag(Z)==1
/// +---------------+
///
/// (*)     The initial sequence number MUST be compatible with the sequence number resolution agreed in the
///         [`super::InitSyn`]-[`super::InitAck`] message exchange
/// (**)    The cookie MUST be the same received in the [`super::InitAck`]from the corresponding zenoh node
/// ```
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///

pub mod flag {
    pub const A: u8 = 1 << 5; // 0x20 Ack           if A==0 then the message is an InitSyn else it is an InitAck
    pub const T: u8 = 1 << 6; // 0x40 Lease period  if T==1 then the lease period is in seconds else in milliseconds
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSyn {
    pub lease: Duration,
    pub initial_sn: TransportSn,
    pub cookie: ZSlice,
    pub ext_qos: Option<ext::QoS>,
    pub ext_shm: Option<ext::Shm>,
    pub ext_auth: Option<ext::Auth>,
    pub ext_mlink: Option<ext::MultiLinkSyn>,
    pub ext_lowlatency: Option<ext::LowLatency>,
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

    /// # Shm extension
    /// Used as challenge for probing shared memory capabilities
    pub type Shm = zextz64!(0x2, false);

    /// # Auth extension
    /// Used as challenge for probing authentication rights
    pub type Auth = zextzbuf!(0x3, false);

    /// # Multilink extension
    /// Used as challenge for probing multilink capabilities
    pub type MultiLinkSyn = zextzbuf!(0x4, false);
    pub type MultiLinkAck = zextunit!(0x4, false);

    /// # LowLatency extension
    /// Used to negotiate the use of lowlatency transport
    pub type LowLatency = zextunit!(0x5, false);
}

impl OpenSyn {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::common::{ZExtUnit, ZExtZ64, ZExtZBuf};
        use rand::Rng;

        const MIN: usize = 32;
        const MAX: usize = 1_024;

        let mut rng = rand::thread_rng();

        let lease = if rng.gen_bool(0.5) {
            Duration::from_secs(rng.gen())
        } else {
            Duration::from_millis(rng.gen())
        };

        let initial_sn: TransportSn = rng.gen();
        let cookie = ZSlice::rand(rng.gen_range(MIN..=MAX));
        let ext_qos = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_shm = rng.gen_bool(0.5).then_some(ZExtZ64::rand());
        let ext_auth = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_mlink = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_lowlatency = rng.gen_bool(0.5).then_some(ZExtUnit::rand());

        Self {
            lease,
            initial_sn,
            cookie,
            ext_qos,
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAck {
    pub lease: Duration,
    pub initial_sn: TransportSn,
    pub ext_qos: Option<ext::QoS>,
    pub ext_shm: Option<ext::Shm>,
    pub ext_auth: Option<ext::Auth>,
    pub ext_mlink: Option<ext::MultiLinkAck>,
    pub ext_lowlatency: Option<ext::LowLatency>,
}

impl OpenAck {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::common::{ZExtUnit, ZExtZ64, ZExtZBuf};
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let lease = if rng.gen_bool(0.5) {
            Duration::from_secs(rng.gen())
        } else {
            Duration::from_millis(rng.gen())
        };

        let initial_sn: TransportSn = rng.gen();
        let ext_qos = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_shm = rng.gen_bool(0.5).then_some(ZExtZ64::rand());
        let ext_auth = rng.gen_bool(0.5).then_some(ZExtZBuf::rand());
        let ext_mlink = rng.gen_bool(0.5).then_some(ZExtUnit::rand());
        let ext_lowlatency = rng.gen_bool(0.5).then_some(ZExtUnit::rand());

        Self {
            lease,
            initial_sn,
            ext_qos,
            ext_shm,
            ext_auth,
            ext_mlink,
            ext_lowlatency,
        }
    }
}
