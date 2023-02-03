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
use crate::{
    core::{Channel, Reliability, ZInt},
    zenoh::ZenohMessage,
};
use alloc::vec::Vec;
use zenoh_buffers::ZSlice;
use zenoh_collections::SingleOrVec;

/// # Frame message
///
/// The [`Frame`] message is used to transmit one ore more complete serialized
/// [`crate::net::protocol::message::ZenohMessage`]. I.e., the total length of the
/// serialized [`crate::net::protocol::message::ZenohMessage`] (s) MUST be smaller
/// than the maximum batch size (i.e. 2^16-1) and the link MTU.
/// The [`Frame`] message is used as means to aggreate multiple
/// [`crate::net::protocol::message::ZenohMessage`] in a single atomic message that
/// goes on the wire. By doing so, many small messages can be batched together and
/// share common information like the sequence number.
///
/// The [`Frame`] message flow is the following:
///
/// ```text
///     A                   B
///     |      FRAME        |
///     |------------------>|
///     |                   |
/// ```
///
/// The [`Frame`] message structure is defined as follows:
///
/// ```text
/// Flags:
/// - R: Reliable       If R==1 it concerns the reliable channel, else the best-effort channel
/// - X: Reserved
/// - Z: Extensions     If Z==1 then zenoh extensions will follow.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|R|  FRAME  |
/// +-+-+-+---------+
/// %    seq num    %
/// +---------------+
/// ~  [FrameExts]  ~ if Flag(Z)==1
/// +---------------+
/// ~  [NetworkMsg] ~
/// +---------------+
/// ```
///
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65535 bytes.
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Frame {
    pub reliability: Reliability,
    pub sn: ZInt,
    pub qos: ext::QoS,
    pub payload: SingleOrVec<()>,
}

// Extensions
pub mod ext {
    use crate::common::ZExtZInt;

    pub type QoS = ZExtZInt<0x01>;
}

impl Frame {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::core::{Priority, Reliability};
        use core::convert::TryInto;
        use rand::Rng;

        const MIN: usize = 1;
        const MAX: usize = 1_024;

        let mut rng = rand::thread_rng();

        let priority: Priority = rng
            .gen_range(Priority::MAX as u8..=Priority::MIN as u8)
            .try_into()
            .unwrap();
        let reliability = if rng.gen_bool(0.5) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        };
        let channel = Channel {
            priority,
            reliability,
        };
        let sn: ZInt = rng.gen();
        // let payload = if rng.gen_bool(0.5) {
        //     FramePayload::Fragment {
        //         buffer: ZSlice::rand(rng.gen_range(MIN..=MAX)),
        //         is_final: rng.gen_bool(0.5),
        //     }
        // } else {
        //     let n = rng.gen_range(1..16);
        //     let messages = (0..n)
        //         .map(|_| {
        //             let mut m = ZenohMessage::rand();
        //             m.channel = channel;
        //             m
        //         })
        //         .collect::<Vec<ZenohMessage>>();
        //     FramePayload::Messages { messages }
        // };

        // Frame {
        //     channel,
        //     sn,
        //     payload,
        // }
        panic!()
    }
}

// FrameHeader

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FrameHeader {
    pub reliability: Reliability,
    pub sn: ZInt,
    pub qos: ext::QoS,
}

impl FrameHeader {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::core::{Priority, Reliability};
        use core::convert::TryInto;
        use rand::{seq::SliceRandom, Rng};

        let mut rng = rand::thread_rng();

        let priority: Priority = rng
            .gen_range(Priority::MAX as u8..=Priority::MIN as u8)
            .try_into()
            .unwrap();
        let reliability = if rng.gen_bool(0.5) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        };
        let channel = Channel {
            priority,
            reliability,
        };
        let sn: ZInt = rng.gen();
        // let kind = *[
        //     FrameKind::Messages,
        //     FrameKind::SomeFragment,
        //     FrameKind::LastFragment,
        // ]
        // .choose(&mut rng)
        // .unwrap();

        // FrameHeader { channel, sn, kind }
        panic!();
    }
}
