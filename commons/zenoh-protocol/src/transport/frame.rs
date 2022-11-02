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
    core::{Channel, Priority, Reliability, ZInt},
    proto::ZenohMessage,
};
use zenoh_buffers::ZSlice;

/// # Frame message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |E|F|R|  FRAME  |
/// +-+-+-+-+-------+
/// ~      SN       ~
/// +---------------+
/// ~  FramePayload ~ -- if F==1 then the payload is a fragment of a single Zenoh Message, a list of complete Zenoh Messages otherwise.
/// +---------------+
///
/// - if R==1 then the FRAME is sent on the reliable channel, best-effort otherwise.
/// - if F==1 then the FRAME is a fragment.
/// - if E==1 then the FRAME is the last fragment. E==1 is valid iff F==1.
///
/// NOTE: Only one bit would be sufficient to signal fragmentation in a IP-like fashion as follows:
///         - if F==1 then this FRAME is a fragment and more fragment will follow;
///         - if F==0 then the message is the last fragment if SN-1 had F==1,
///           otherwise it's a non-fragmented message.
///       However, this would require to always perform a two-steps de-serialization: first
///       de-serialize the FRAME and then the Payload. This is due to the fact the F==0 is ambigous
///       w.r.t. detecting if the FRAME is a fragment or not before SN re-ordering has occured.
///       By using the F bit to only signal whether the FRAME is fragmented or not, it allows to
///       de-serialize the payload in one single pass when F==0 since no re-ordering needs to take
///       place at this stage. Then, the F bit is used to detect the last fragment during re-ordering.
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: Channel,
    pub sn: ZInt,
    pub payload: FramePayload,
}

// impl Header for Frame {
//     #[inline(always)]
//     fn header(&self) -> u8 {
//         let mut header = tmsg::id::FRAME;
//         if let Reliability::Reliable = self.channel.reliability {
//             header |= tmsg::flag::R;
//         }
//         if let FramePayload::Fragment { is_final, .. } = self.payload {
//             header |= tmsg::flag::F;
//             if is_final {
//                 header |= tmsg::flag::E;
//             }
//         }
//         header
//     }
// }

// impl Frame {
//     pub fn make_header(reliability: Reliability, is_fragment: Option<bool>) -> u8 {
//         let mut header = tmsg::id::FRAME;
//         if let Reliability::Reliable = reliability {
//             header |= tmsg::flag::R;
//         }
//         if let Some(is_final) = is_fragment {
//             header |= tmsg::flag::F;
//             if is_final {
//                 header |= tmsg::flag::E;
//             }
//         }
//         header
//     }
// }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramePayload {
    /// ```text
    /// The Payload of a fragmented Frame.
    ///    
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// ~ payload bytes ~
    /// +---------------+
    /// ```
    Fragment { buffer: ZSlice, is_final: bool },
    /// ```text
    /// The Payload of a batched Frame.
    ///
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// ~  ZenohMessage ~
    /// +---------------+
    /// ~      ...      ~ - Additional complete Zenoh messages.
    /// +---------------+
    ///
    /// NOTE: A batched Frame must contain at least one complete Zenoh message.
    ///       There is no upper limit to the number of Zenoh messages that can
    ///       be batched together in the same frame.
    /// ```
    Messages { messages: Vec<ZenohMessage> },
}

// Functions mainly used for testing
impl Frame {
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        use std::convert::TryInto;

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
        let payload = if rng.gen_bool(0.5) {
            FramePayload::Fragment {
                buffer: ZSlice::rand(rng.gen_range(MIN..=MAX)),
                is_final: rng.gen_bool(0.5),
            }
        } else {
            // @TODO: update once ZenohMessage is implemented
            FramePayload::Messages { messages: vec![] }
        };

        Frame {
            channel,
            sn,
            payload,
        }
    }
}
