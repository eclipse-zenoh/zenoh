//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
mod init;
mod join;
mod open;

use super::core::*;
use super::flag as iflag;
use super::id as iid;
use super::io::ZSlice;
use super::zenoh::ZenohMessage;
use super::{Attachment, Header};
pub use init::*;
pub use join::*;
pub use open::*;
use std::convert::TryFrom;

/*************************************/
/*               IDS                 */
/*************************************/
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TransportId {
    // 0x00: Reserved
    Init = 0x01,
    Open = 0x02,
    Join = 0x03,
    // 0x04: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TransportId {
    const fn id(self) -> u8 {
        self as u8
    }
}

// Transport message IDs -- Re-export of some of the Inner Message IDs
pub mod id {
    // Messages
    pub const CLOSE: u8 = super::iid::CLOSE;
    pub const SYNC: u8 = super::iid::SYNC;
    pub const ACK_NACK: u8 = super::iid::ACK_NACK;
    pub const KEEP_ALIVE: u8 = super::iid::KEEP_ALIVE;
    pub const PING_PONG: u8 = super::iid::PING_PONG;
    pub const FRAME: u8 = super::iid::FRAME;

    // Message decorators
    pub const PRIORITY: u8 = super::iid::PRIORITY;
    pub const ATTACHMENT: u8 = super::iid::ATTACHMENT;
}

// Transport message flags
pub mod flag {
    pub const C: u8 = 1 << 6; // 0x40 Count         if C==1 then number of unacknowledged messages is present
    pub const E: u8 = 1 << 7; // 0x80 End           if E==1 then it is the last FRAME fragment
    pub const F: u8 = 1 << 6; // 0x40 Fragment      if F==1 then the FRAME is a fragment
    pub const I: u8 = 1 << 5; // 0x20 ZenohId        if I==1 then the ZenohId is requested or present
    pub const K: u8 = 1 << 6; // 0x40 CloseLink     if K==1 then close the transport link only
    pub const M: u8 = 1 << 5; // 0x20 Mask          if M==1 then a Mask is present
    pub const P: u8 = 1 << 5; // 0x20 PingOrPong    if P==1 then the message is Ping, otherwise is Pong
    pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then it concerns the reliable channel, best-effort otherwise
    pub const Z: u8 = super::iflag::Z; // 0x20 MixedSlices   if Z==1 then the payload contains a mix of raw and shm_info payload

    // pub const X: u8 = 0; // Unused flags are set to zero
}

/*************************************/
/*       TRANSPORT MESSAGES          */
/*************************************/
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SeqNumBytes {
    One = 1,   // SeqNum max value: 2^7
    Two = 2,   // SeqNum max value: 2^14
    Three = 3, // SeqNum max value: 2^21
    Four = 4,  // SeqNum max value: 2^28
    Five = 5,  // SeqNum max value: 2^35
    Six = 6,   // SeqNum max value: 2^42
    Seven = 7, // SeqNum max value: 2^49
    Eight = 8, // SeqNum max value: 2^56
}

impl SeqNumBytes {
    pub const MIN: SeqNumBytes = SeqNumBytes::One;
    pub const MAX: SeqNumBytes = SeqNumBytes::Eight;

    pub const fn value(self) -> u8 {
        self as u8
    }

    pub const fn resolution(&self) -> ZInt {
        const BASE: ZInt = 2;
        BASE.pow(7 * self.value() as u32)
    }
}

impl TryFrom<u8> for SeqNumBytes {
    type Error = ();

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        const ONE: u8 = SeqNumBytes::One.value();
        const TWO: u8 = SeqNumBytes::Two.value();
        const THREE: u8 = SeqNumBytes::Three.value();
        const FOUR: u8 = SeqNumBytes::Four.value();
        const FIVE: u8 = SeqNumBytes::Five.value();
        const SIX: u8 = SeqNumBytes::Six.value();
        const SEVEN: u8 = SeqNumBytes::Seven.value();
        const EIGHT: u8 = SeqNumBytes::Eight.value();

        match b {
            ONE => Ok(Self::One),
            TWO => Ok(Self::Two),
            THREE => Ok(Self::Three),
            FOUR => Ok(Self::Four),
            FIVE => Ok(Self::Five),
            SIX => Ok(Self::Six),
            SEVEN => Ok(Self::Seven),
            EIGHT => Ok(Self::Eight),
            _ => Err(()),
        }
    }
}

impl Default for SeqNumBytes {
    fn default() -> Self {
        Self::Four
    }
}

/// # Close message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The CLOSE message is sent in any of the following two cases:
///     1) in response to an OPEN message which is not accepted;
///     2) at any time to arbitrarly close the transport with the corresponding peer.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|K|I|  CLOSE  |
/// +-+-+-+-+-------+
/// ~    peer_id    ~  if I==1 -- PID of the target peer.
/// +---------------+
/// |     reason    |
/// +---------------+
///
/// - if K==0 then close the whole zenoh transport.
/// - if K==1 then close the transport link the CLOSE message was sent on (e.g., TCP socket) but
///           keep the whole transport open. NOTE: the transport will be automatically closed when
///           the transport's lease period expires.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub pid: Option<ZenohId>,
    pub reason: u8,
    pub link_only: bool,
}

impl Close {
    pub const GENERIC: u8 = 0x00;
    pub const UNSUPPORTED: u8 = 0x01;
    pub const INVALID: u8 = 0x02;
    pub const MAX_SESSIONS: u8 = 0x03;
    pub const MAX_LINKS: u8 = 0x04;
    pub const EXPIRED: u8 = 0x05;
}

impl Header for Close {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::CLOSE;
        if self.pid.is_some() {
            header |= flag::I;
        }
        if self.link_only {
            header |= flag::K;
        }
        header
    }
}

/// # Sync message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The SYNC message allows to signal the corresponding peer the sequence number of the next message
/// to be transmitted on the reliable or best-effort channel. In the case of reliable channel, the
/// peer can optionally include the number of unacknowledged messages. A SYNC sent on the reliable
/// channel triggers the transmission of an ACKNACK message.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|C|R|  SYNC   |
/// +-+-+-+-+-------+
/// ~      sn       ~ -- Sequence number of the next message to be transmitted on this channel.
/// +---------------+
/// ~     count     ~ if R==1 && C==1 -- Number of unacknowledged messages.
/// +---------------+
///
/// - if R==1 then the SYNC concerns the reliable channel, otherwise the best-effort channel.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Sync {
    pub reliability: Reliability,
    pub sn: ZInt,
    pub count: Option<ZInt>,
}

impl Header for Sync {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::SYNC;
        if let Reliability::Reliable = self.reliability {
            header |= flag::R;
        }
        if self.count.is_some() {
            header |= flag::C;
        }
        header
    }
}

/// # AckNack message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The ACKNACK messages is used on the reliable channel to signal the corresponding peer the last
/// sequence number received and optionally a bitmask of the non-received messages.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|M| ACKNACK |
/// +-+-+-+-+-------+
/// ~      sn       ~
/// +---------------+
/// ~     mask      ~ if M==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct AckNack {
    pub sn: ZInt,
    pub mask: Option<ZInt>,
}

impl Header for AckNack {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::ACK_NACK;
        if self.mask.is_some() {
            header |= flag::M;
        }
        header
    }
}

/// # KeepAlive message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The KEEP_ALIVE message can be sent periodically to avoid the expiration of the transport lease
/// period in case there are no messages to be sent.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|I| K_ALIVE |
/// +-+-+-+-+-------+
/// ~    peer_id    ~ if I==1 -- Peer ID of the KEEP_ALIVE sender.
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct KeepAlive {
    pub pid: Option<ZenohId>,
}

impl Header for KeepAlive {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::KEEP_ALIVE;
        if self.pid.is_some() {
            header |= flag::I;
        }
        header
    }
}

/// # PingPong message
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
/// |X|X|P|  P_PONG |
/// +-+-+-+-+-------+
/// ~     hash      ~
/// +---------------+
///
/// - if P==1 then the message is Ping, otherwise is Pong.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Ping {
    pub hash: ZInt,
}

impl Header for Ping {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::PING_PONG;
        header |= flag::P;
        header
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pong {
    pub hash: ZInt,
}

impl Header for Pong {
    #[inline(always)]
    fn header(&self) -> u8 {
        id::PING_PONG
    }
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub channel: Channel,
    pub sn: ZInt,
    pub payload: FramePayload,
}

impl Header for Frame {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::FRAME;
        if let Reliability::Reliable = self.channel.reliability {
            header |= flag::R;
        }
        if let FramePayload::Fragment { is_final, .. } = self.payload {
            header |= flag::F;
            if is_final {
                header |= flag::E;
            }
        }
        header
    }
}

impl Frame {
    pub fn make_header(reliability: Reliability, is_fragment: Option<bool>) -> u8 {
        let mut header = id::FRAME;
        if let Reliability::Reliable = reliability {
            header |= flag::R;
        }
        if let Some(is_final) = is_fragment {
            header |= flag::F;
            if is_final {
                header |= flag::E;
            }
        }
        header
    }
}

#[derive(Debug, Clone, PartialEq)]
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

// Zenoh messages at zenoh-transport level
#[derive(Debug, Clone, PartialEq)]
pub enum TransportBody {
    InitSyn(InitSyn),
    InitAck(InitAck),
    OpenSyn(OpenSyn),
    OpenAck(OpenAck),
    Join(Join),
    Close(Close),
    Sync(Sync),
    AckNack(AckNack),
    KeepAlive(KeepAlive),
    Ping(Ping),
    Pong(Pong),
    Frame(Frame),
}

impl From<InitSyn> for TransportBody {
    fn from(msg: InitSyn) -> Self {
        Self::InitSyn(msg)
    }
}

impl From<InitAck> for TransportBody {
    fn from(msg: InitAck) -> Self {
        Self::InitAck(msg)
    }
}

impl From<OpenSyn> for TransportBody {
    fn from(msg: OpenSyn) -> Self {
        Self::OpenSyn(msg)
    }
}

impl From<OpenAck> for TransportBody {
    fn from(msg: OpenAck) -> Self {
        Self::OpenAck(msg)
    }
}

impl From<Join> for TransportBody {
    fn from(msg: Join) -> Self {
        Self::Join(msg)
    }
}

#[derive(Debug, Clone)]
pub struct TransportMessage {
    pub body: TransportBody,
    pub attachment: Option<Attachment>,
    #[cfg(feature = "stats")]
    pub size: Option<NonZeroUsize>,
}

impl TransportMessage {
    pub fn make_close(
        pid: Option<ZenohId>,
        reason: u8,
        link_only: bool,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Close(Close {
                pid,
                reason,
                link_only,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_sync(
        reliability: Reliability,
        sn: ZInt,
        count: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Sync(Sync {
                reliability,
                sn,
                count,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_ack_nack(
        sn: ZInt,
        mask: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::AckNack(AckNack { sn, mask }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_keep_alive(
        pid: Option<ZenohId>,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::KeepAlive(KeepAlive { pid }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_ping(hash: ZInt, attachment: Option<Attachment>) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Ping(Ping { hash }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_pong(hash: ZInt, attachment: Option<Attachment>) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Pong(Pong { hash }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_frame(
        channel: Channel,
        sn: ZInt,
        payload: FramePayload,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Frame(Frame {
                channel,
                sn,
                payload,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}

impl PartialEq for TransportMessage {
    fn eq(&self, other: &Self) -> bool {
        self.body.eq(&other.body) && self.attachment.eq(&other.attachment)
    }
}

impl<T: Into<TransportBody>> From<T> for TransportMessage {
    fn from(msg: T) -> Self {
        Self {
            body: msg.into(),
            attachment: None,
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}
