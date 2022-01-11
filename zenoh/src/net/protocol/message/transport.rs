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
use super::core::*;
use super::defaults::SEQ_NUM_RES;
use super::flag as iflag;
use super::id as iid;
use super::io::ZSlice;
use super::zenoh::ZenohMessage;
use super::{Attachment, Header, Options};
use std::time::Duration;

/*************************************/
/*               IDS                 */
/*************************************/
// Transport message IDs -- Re-export of some of the Inner Message IDs
pub mod id {
    // Messages
    pub const INIT: u8 = super::iid::INIT;
    pub const OPEN: u8 = super::iid::OPEN;
    pub const CLOSE: u8 = super::iid::CLOSE;
    pub const SYNC: u8 = super::iid::SYNC;
    pub const ACK_NACK: u8 = super::iid::ACK_NACK;
    pub const KEEP_ALIVE: u8 = super::iid::KEEP_ALIVE;
    pub const PING_PONG: u8 = super::iid::PING_PONG;
    pub const FRAME: u8 = super::iid::FRAME;
    pub const JOIN: u8 = super::iid::JOIN;

    // Message decorators
    pub const PRIORITY: u8 = super::iid::PRIORITY;
    pub const ATTACHMENT: u8 = super::iid::ATTACHMENT;
}

// Transport message flags
pub mod flag {
    pub const A: u8 = 1 << 5; // 0x20 Ack           if A==1 then the message is an acknowledgment
    pub const C: u8 = 1 << 6; // 0x40 Count         if C==1 then number of unacknowledged messages is present
    pub const E: u8 = 1 << 7; // 0x80 End           if E==1 then it is the last FRAME fragment
    pub const F: u8 = 1 << 6; // 0x40 Fragment      if F==1 then the FRAME is a fragment
    pub const I: u8 = 1 << 5; // 0x20 ZenohId        if I==1 then the ZenohId is requested or present
    pub const K: u8 = 1 << 6; // 0x40 CloseLink     if K==1 then close the transport link only
    pub const M: u8 = 1 << 5; // 0x20 Mask          if M==1 then a Mask is present
    pub const O: u8 = 1 << 7; // 0x80 Options       if O==1 then Options are present
    pub const P: u8 = 1 << 5; // 0x20 PingOrPong    if P==1 then the message is Ping, otherwise is Pong
    pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then it concerns the reliable channel, best-effort otherwise
    pub const S: u8 = 1 << 6; // 0x40 SN Resolution if S==1 then the SN Resolution is present
    pub const T1: u8 = 1 << 5; // 0x20 TimeRes       if U==1 then the time resolution is in seconds
    pub const T2: u8 = 1 << 6; // 0x40 TimeRes       if T==1 then the time resolution is in seconds
    pub const Z: u8 = super::iflag::Z; // 0x20 MixedSlices   if Z==1 then the payload contains a mix of raw and shm_info payload

    // pub const X: u8 = 0; // Unused flags are set to zero
}

/*************************************/
/*       TRANSPORT MESSAGES          */
/*************************************/
/// # Init message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The INIT message is sent on a specific Locator to initiate a transport with the peer associated
/// with that Locator. The initiator MUST send an INIT message with the A flag set to 0.  If the
/// corresponding peer deems appropriate to initialize a transport with the initiator, the corresponding
/// peer MUST reply with an INIT message with the A flag set to 1.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|S|A|   INIT  |
/// +-+-+-+-+-------+
/// ~             |Q~ if O==1
/// +---------------+
/// | v_maj | v_min | if A==0 -- Protocol Version VMaj.VMin
/// +-------+-------+
/// ~    whatami    ~ -- Client, Router, Peer or a combination of them
/// +---------------+
/// ~    peer_id    ~ -- PID of the sender of the INIT message
/// +---------------+
/// ~ sn_resolution ~ if S==1 -- the sequence number resolution(*)
/// +---------------+
/// ~     cookie    ~ if A==1
/// +---------------+
///
/// (*) if A==0 and S==0 then 2^28 is assumed.
///     if A==1 and S==0 then the agreed resolution is the one communicated by the initiator.
///
/// - if Q==1 then the initiator/responder support QoS.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub pid: ZenohId,
    pub sn_resolution: ZInt,
    pub is_qos: bool,
}

impl InitSyn {
    pub const OPT_QOS: ZInt = 1 << 0; // 0x01 QoS       if PRIORITY==1 then the transport supports QoS
}
impl InitAck {
    pub const OPT_QOS: ZInt = InitSyn::OPT_QOS; // 0x01 QoS       if PRIORITY==1 then the transport supports QoS
}

impl Header for InitSyn {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::INIT;
        if self.sn_resolution != SEQ_NUM_RES {
            header |= flag::S;
        }
        if self.has_options() {
            header |= flag::O;
        }
        header
    }
}

impl Options for InitSyn {
    fn options(&self) -> ZInt {
        let mut options = 0;
        if self.is_qos {
            options |= Self::OPT_QOS;
        }
        options
    }

    fn has_options(&self) -> bool {
        self.is_qos
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitAck {
    pub whatami: WhatAmI,
    pub pid: ZenohId,
    pub sn_resolution: Option<ZInt>,
    pub is_qos: bool,
    pub cookie: ZSlice,
}

impl Header for InitAck {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::INIT;
        header |= flag::A;
        if self.sn_resolution.is_some() {
            header |= flag::S;
        }
        if self.has_options() {
            header |= flag::O;
        }
        header
    }
}

impl Options for InitAck {
    fn options(&self) -> ZInt {
        let mut options = 0;
        if self.is_qos {
            options |= Self::OPT_QOS;
        }
        options
    }

    fn has_options(&self) -> bool {
        self.is_qos
    }
}

/// # Open message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The OPEN message is sent on a link to finally open an initialized transport with the peer.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|T|A|   OPEN  |
/// +-+-+-+-+-------+
/// ~    lease      ~ -- Lease period of the sender of the OPEN message(*)
/// +---------------+
/// ~  initial_sn   ~ -- Initial SN proposed by the sender of the OPEN(**)
/// +---------------+
/// ~    cookie     ~ if A==0(***)
/// +---------------+
///
/// (*)   if T==1 then the lease period is expressed in seconds, otherwise in milliseconds
/// (**)  the initial sequence number MUST be compatible with the sequence number resolution agreed in the
///       InitSyn/InitAck message exchange
/// (***) the cookie MUST be the same received in the INIT message with A==1 from the corresponding peer
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSyn {
    pub lease: Duration,
    pub initial_sn: ZInt,
    pub cookie: ZSlice,
}

impl Header for OpenSyn {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::OPEN;
        if self.lease.as_millis() % 1_000 == 0 {
            header |= flag::T2;
        }
        header
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAck {
    pub lease: Duration,
    pub initial_sn: ZInt,
}

impl Header for OpenAck {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::OPEN;
        header |= flag::A;
        if self.lease.as_millis() % 1_000 == 0 {
            header |= flag::T2;
        }
        header
    }
}

/// # Join message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The JOIN message is sent on a multicast Locator to advertise the transport parameters.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |O|S|T|   JOIN  |
/// +-+-+-+-+-------+
/// ~             |Q~ if O==1
/// +---------------+
/// | v_maj | v_min | -- Protocol Version VMaj.VMin
/// +-------+-------+
/// ~    whatami    ~ -- Router, Peer or a combination of them
/// +---------------+
/// ~    peer_id    ~ -- PID of the sender of the JOIN message
/// +---------------+
/// ~     lease     ~ -- Lease period of the sender of the JOIN message(*)
/// +---------------+
/// ~ sn_resolution ~ if S==1(*) -- Otherwise 2^28 is assumed(**)
/// +---------------+
/// ~   [next_sn]   ~ (***)
/// +---------------+
///
/// - if Q==1 then the sender supports QoS.
///
/// (*)   if T==1 then the lease period is expressed in seconds, otherwise in milliseconds
/// (**)  if S==0 then 2^28 is assumed.
/// (***) if Q==1 then 8 sequence numbers are present: one for each priority.
///       if Q==0 then only one sequence number is present.
///
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    pub version: u8,
    pub whatami: WhatAmI,
    pub pid: ZenohId,
    pub lease: Duration,
    pub sn_resolution: ZInt,
    pub next_sns: ConduitSnList,
}

impl Join {
    pub const OPT_QOS: ZInt = 1 << 0; // 0x01 QoS       if PRIORITY==1 then the transport supports QoS

    pub fn is_qos(&self) -> bool {
        match self.next_sns {
            ConduitSnList::QoS(_) => true,
            ConduitSnList::Plain(_) => false,
        }
    }
}

impl Header for Join {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::JOIN;
        if self.lease.as_millis() % 1_000 == 0 {
            header |= flag::T1;
        }
        if self.sn_resolution != SEQ_NUM_RES {
            header |= flag::S;
        }
        if self.has_options() {
            header |= flag::O;
        }
        header
    }
}

impl Options for Join {
    fn options(&self) -> ZInt {
        let mut options = 0;
        if self.is_qos() {
            options |= Self::OPT_QOS;
        }
        options
    }

    fn has_options(&self) -> bool {
        self.options() > 0
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

#[derive(Debug, Clone)]
pub struct TransportMessage {
    pub body: TransportBody,
    pub attachment: Option<Attachment>,
    #[cfg(feature = "stats")]
    pub size: Option<std::num::NonZeroUsize>,
}

impl TransportMessage {
    pub fn make_init_syn(
        version: u8,
        whatami: WhatAmI,
        pid: ZenohId,
        sn_resolution: ZInt,
        is_qos: bool,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::InitSyn(InitSyn {
                version,
                whatami,
                pid,
                sn_resolution,
                is_qos,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_init_ack(
        whatami: WhatAmI,
        pid: ZenohId,
        sn_resolution: Option<ZInt>,
        is_qos: bool,
        cookie: ZSlice,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::InitAck(InitAck {
                whatami,
                pid,
                sn_resolution,
                is_qos,
                cookie,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_open_syn(
        lease: Duration,
        initial_sn: ZInt,
        cookie: ZSlice,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::OpenSyn(OpenSyn {
                lease,
                initial_sn,
                cookie,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_open_ack(
        lease: Duration,
        initial_sn: ZInt,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::OpenAck(OpenAck { lease, initial_sn }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_join(
        version: u8,
        whatami: WhatAmI,
        pid: ZenohId,
        lease: Duration,
        sn_resolution: ZInt,
        next_sns: ConduitSnList,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Join(Join {
                version,
                whatami,
                pid,
                lease,
                sn_resolution,
                next_sns,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

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
