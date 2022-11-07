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
use super::core::Encoding;
use super::core::*;
use super::defaults::SEQ_NUM_RES;
use super::io::{ZBuf, ZSlice};
use std::fmt;
use std::time::Duration;
use zenoh_protocol_core::whatami::WhatAmIMatcher;

/*************************************/
/*               IDS                 */
/*************************************/
// Inner Message IDs
pub(crate) mod imsg {
    use super::ZInt;

    pub(crate) mod id {
        // Transport Messages
        pub(crate) const JOIN: u8 = 0x00; // For multicast communications only
        pub(crate) const SCOUT: u8 = 0x01;
        pub(crate) const HELLO: u8 = 0x02;
        pub(crate) const INIT: u8 = 0x03; // For unicast communications only
        pub(crate) const OPEN: u8 = 0x04; // For unicast communications only
        pub(crate) const CLOSE: u8 = 0x05;
        pub(crate) const SYNC: u8 = 0x06;
        pub(crate) const ACK_NACK: u8 = 0x07;
        pub(crate) const KEEP_ALIVE: u8 = 0x08;
        pub(crate) const PING_PONG: u8 = 0x09;
        pub(crate) const FRAME: u8 = 0x0a;

        // Zenoh Messages
        pub(crate) const DECLARE: u8 = 0x0b;
        pub(crate) const DATA: u8 = 0x0c;
        pub(crate) const QUERY: u8 = 0x0d;
        pub(crate) const PULL: u8 = 0x0e;
        pub(crate) const UNIT: u8 = 0x0f;
        pub(crate) const LINK_STATE_LIST: u8 = 0x10;

        // Message decorators
        pub(crate) const PRIORITY: u8 = 0x1c;
        pub(crate) const ROUTING_CONTEXT: u8 = 0x1d;
        pub(crate) const REPLY_CONTEXT: u8 = 0x1e;
        pub(crate) const ATTACHMENT: u8 = 0x1f;
    }

    // Header mask
    pub const HEADER_BITS: u8 = 5;
    pub const HEADER_MASK: u8 = !(0xff << HEADER_BITS);

    pub fn mid(header: u8) -> u8 {
        header & HEADER_MASK
    }

    pub fn flags(header: u8) -> u8 {
        header & !HEADER_MASK
    }

    pub fn has_flag(byte: u8, flag: u8) -> bool {
        byte & flag != 0
    }

    pub fn has_option(options: ZInt, flag: ZInt) -> bool {
        options & flag != 0
    }
}

pub mod tmsg {
    use super::{imsg, Priority, ZInt};

    // Transport message IDs -- Re-export of some of the Inner Message IDs
    pub mod id {
        use super::imsg;

        // Messages
        pub const SCOUT: u8 = imsg::id::SCOUT;
        pub const HELLO: u8 = imsg::id::HELLO;
        pub const INIT: u8 = imsg::id::INIT;
        pub const OPEN: u8 = imsg::id::OPEN;
        pub const CLOSE: u8 = imsg::id::CLOSE;
        pub const SYNC: u8 = imsg::id::SYNC;
        pub const ACK_NACK: u8 = imsg::id::ACK_NACK;
        pub const KEEP_ALIVE: u8 = imsg::id::KEEP_ALIVE;
        pub const PING_PONG: u8 = imsg::id::PING_PONG;
        pub const FRAME: u8 = imsg::id::FRAME;
        pub const JOIN: u8 = imsg::id::JOIN;

        // Message decorators
        pub const PRIORITY: u8 = imsg::id::PRIORITY;
        pub const ATTACHMENT: u8 = imsg::id::ATTACHMENT;
    }

    // Transport message flags
    pub mod flag {
        pub const A: u8 = 1 << 5; // 0x20 Ack           if A==1 then the message is an acknowledgment
        pub const C: u8 = 1 << 6; // 0x40 Count         if C==1 then number of unacknowledged messages is present
        pub const E: u8 = 1 << 7; // 0x80 End           if E==1 then it is the last FRAME fragment
        pub const F: u8 = 1 << 6; // 0x40 Fragment      if F==1 then the FRAME is a fragment
        pub const I: u8 = 1 << 5; // 0x20 PeerID        if I==1 then the PeerID is requested or present
        pub const K: u8 = 1 << 6; // 0x40 CloseLink     if K==1 then close the transport link only
        pub const L: u8 = 1 << 7; // 0x80 Locators      if L==1 then Locators are present
        pub const M: u8 = 1 << 5; // 0x20 Mask          if M==1 then a Mask is present
        pub const O: u8 = 1 << 7; // 0x80 Options       if O==1 then Options are present
        pub const P: u8 = 1 << 5; // 0x20 PingOrPong    if P==1 then the message is Ping, otherwise is Pong
        pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then it concerns the reliable channel, best-effort otherwise
        pub const S: u8 = 1 << 6; // 0x40 SN Resolution if S==1 then the SN Resolution is present
        pub const T1: u8 = 1 << 5; // 0x20 TimeRes       if U==1 then the time resolution is in seconds
        pub const T2: u8 = 1 << 6; // 0x40 TimeRes       if T==1 then the time resolution is in seconds
        pub const W: u8 = 1 << 6; // 0x40 WhatAmI       if W==1 then WhatAmI is indicated
        pub const Z: u8 = 1 << 5; // 0x20 MixedSlices   if Z==1 then the payload contains a mix of raw and shm_info payload

        pub const X: u8 = 0; // Unused flags are set to zero
    }

    pub mod init_options {
        use super::ZInt;

        pub const QOS: ZInt = 1 << 0; // 0x01 QoS       if PRIORITY==1 then the transport supports QoS
    }

    pub mod join_options {
        use super::ZInt;

        pub const QOS: ZInt = 1 << 0; // 0x01 QoS       if PRIORITY==1 then the transport supports QoS
    }

    // Reason for the Close message
    pub mod close_reason {
        pub const GENERIC: u8 = 0x00;
        pub const UNSUPPORTED: u8 = 0x01;
        pub const INVALID: u8 = 0x02;
        pub const MAX_SESSIONS: u8 = 0x03;
        pub const MAX_LINKS: u8 = 0x04;
        pub const EXPIRED: u8 = 0x05;
    }

    pub fn close_reason_to_str(reason: u8) -> &'static str {
        match reason {
            close_reason::GENERIC => "GENERIC",
            close_reason::UNSUPPORTED => "UNSUPPORTED",
            close_reason::INVALID => "INVALID",
            close_reason::MAX_SESSIONS => "MAX_SESSIONS",
            close_reason::MAX_LINKS => "MAX_LINKS",
            close_reason::EXPIRED => "EXPIRED",
            _ => "UNKNOWN",
        }
    }

    pub mod conduit {
        use super::{imsg, Priority};

        pub const CONTROL: u8 = (Priority::Control as u8) << imsg::HEADER_BITS;
        pub const REAL_TIME: u8 = (Priority::RealTime as u8) << imsg::HEADER_BITS;
        pub const INTERACTIVE_HIGH: u8 = (Priority::InteractiveHigh as u8) << imsg::HEADER_BITS;
        pub const INTERACTIVE_LOW: u8 = (Priority::InteractiveLow as u8) << imsg::HEADER_BITS;
        pub const DATA_HIGH: u8 = (Priority::DataHigh as u8) << imsg::HEADER_BITS;
        pub const DATA: u8 = (Priority::Data as u8) << imsg::HEADER_BITS;
        pub const DATA_LOW: u8 = (Priority::DataLow as u8) << imsg::HEADER_BITS;
        pub const BACKGROUND: u8 = (Priority::Background as u8) << imsg::HEADER_BITS;
    }
}

pub mod zmsg {
    use super::{imsg, Channel, CongestionControl, Priority, Reliability, ZInt};

    // Zenoh message IDs -- Re-export of some of the Inner Message IDs
    pub mod id {
        use super::imsg;

        // Messages
        pub const DECLARE: u8 = imsg::id::DECLARE;
        pub const DATA: u8 = imsg::id::DATA;
        pub const QUERY: u8 = imsg::id::QUERY;
        pub const PULL: u8 = imsg::id::PULL;
        pub const UNIT: u8 = imsg::id::UNIT;
        pub const LINK_STATE_LIST: u8 = imsg::id::LINK_STATE_LIST;

        // Message decorators
        pub const PRIORITY: u8 = imsg::id::PRIORITY;
        pub const REPLY_CONTEXT: u8 = imsg::id::REPLY_CONTEXT;
        pub const ATTACHMENT: u8 = imsg::id::ATTACHMENT;
        pub const ROUTING_CONTEXT: u8 = imsg::id::ROUTING_CONTEXT;
    }

    // Zenoh message flags
    pub mod flag {
        pub const B: u8 = 1 << 6; // 0x40 QueryBody     if B==1 then QueryBody is present
        pub const D: u8 = 1 << 5; // 0x20 Drop          if D==1 then the message can be dropped
        pub const F: u8 = 1 << 5; // 0x20 Final         if F==1 then this is the final message (e.g., ReplyContext, Pull)
        pub const I: u8 = 1 << 6; // 0x40 DataInfo      if I==1 then DataInfo is present
        pub const K: u8 = 1 << 7; // 0x80 KeySuffix     if K==1 then key_expr has suffix
        pub const N: u8 = 1 << 6; // 0x40 MaxSamples    if N==1 then the MaxSamples is indicated
        pub const P: u8 = 1 << 0; // 0x01 Zid           if P==1 then the zid is present
        pub const Q: u8 = 1 << 6; // 0x40 QueryableInfo if Q==1 then the queryable info is present
        pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then it concerns the reliable channel, best-effort otherwise
        pub const S: u8 = 1 << 6; // 0x40 SubMode       if S==1 then the declaration SubMode is indicated
        pub const T: u8 = 1 << 5; // 0x20 QueryTAK   if T==1 then the query target is present

        pub const X: u8 = 0; // Unused flags are set to zero
    }

    // Options used for DataInfo
    pub mod data {
        use super::ZInt;

        pub mod info {
            use super::ZInt;

            #[cfg(feature = "shared-memory")]
            pub const SLICED: ZInt = 1 << 0; // 0x01
            pub const KIND: ZInt = 1 << 1; // 0x02
            pub const ENCODING: ZInt = 1 << 2; // 0x04
            pub const TIMESTAMP: ZInt = 1 << 3; // 0x08
                                                // 0x10: Reserved
                                                // 0x20: Reserved
                                                // 0x40: Reserved
            pub const SRCID: ZInt = 1 << 7; // 0x80
            pub const SRCSN: ZInt = 1 << 8; // 0x100
            pub const RTRID: ZInt = 1 << 9; // 0x200
            pub const RTRSN: ZInt = 1 << 10; // 0x400
        }
    }

    pub mod declaration {
        pub mod id {
            // Declarations
            pub const RESOURCE: u8 = 0x01;
            pub const PUBLISHER: u8 = 0x02;
            pub const SUBSCRIBER: u8 = 0x03;
            pub const QUERYABLE: u8 = 0x04;

            pub const FORGET_RESOURCE: u8 = 0x11;
            pub const FORGET_PUBLISHER: u8 = 0x12;
            pub const FORGET_SUBSCRIBER: u8 = 0x13;
            pub const FORGET_QUERYABLE: u8 = 0x14;

            // SubModes
            pub const MODE_PUSH: u8 = 0x00;
            pub const MODE_PULL: u8 = 0x01;
        }

        pub mod flag {
            pub const PERIOD: u8 = 0x80;
        }
    }

    // Options used for LinkState
    pub mod link_state {
        use super::ZInt;

        pub const PID: ZInt = 1; // 0x01
        pub const WAI: ZInt = 1 << 1; // 0x02
        pub const LOC: ZInt = 1 << 2; // 0x04
    }

    pub mod conduit {
        use super::{imsg, Priority};

        pub const CONTROL: u8 = (Priority::Control as u8) << imsg::HEADER_BITS;
        pub const REAL_TIME: u8 = (Priority::RealTime as u8) << imsg::HEADER_BITS;
        pub const INTERACTIVE_HIGH: u8 = (Priority::InteractiveHigh as u8) << imsg::HEADER_BITS;
        pub const INTERACTIVE_LOW: u8 = (Priority::InteractiveLow as u8) << imsg::HEADER_BITS;
        pub const DATA_HIGH: u8 = (Priority::DataHigh as u8) << imsg::HEADER_BITS;
        pub const DATA: u8 = (Priority::Data as u8) << imsg::HEADER_BITS;
        pub const DATA_LOW: u8 = (Priority::DataLow as u8) << imsg::HEADER_BITS;
        pub const BACKGROUND: u8 = (Priority::Background as u8) << imsg::HEADER_BITS;
    }

    // Default reliability for each Zenoh Message
    pub mod default_channel {
        use super::{Channel, Priority, Reliability};

        pub const DECLARE: Channel = Channel {
            priority: Priority::RealTime,
            reliability: Reliability::Reliable,
        };
        pub const DATA: Channel = Channel {
            priority: Priority::Data,
            reliability: Reliability::BestEffort,
        };
        pub const QUERY: Channel = Channel {
            priority: Priority::Data,
            reliability: Reliability::Reliable,
        };
        pub const PULL: Channel = Channel {
            priority: Priority::Data,
            reliability: Reliability::Reliable,
        };
        pub const REPLY: Channel = Channel {
            priority: Priority::Data,
            reliability: Reliability::Reliable,
        };
        pub const UNIT: Channel = Channel {
            priority: Priority::Data,
            reliability: Reliability::BestEffort,
        };
        pub const LINK_STATE_LIST: Channel = Channel {
            priority: Priority::Control,
            reliability: Reliability::Reliable,
        };
    }

    // Default congestion control for each Zenoh Message
    pub mod default_congestion_control {
        use super::CongestionControl;

        pub const DECLARE: CongestionControl = CongestionControl::Block;
        pub const DATA: CongestionControl = CongestionControl::Drop;
        pub const QUERY: CongestionControl = CongestionControl::Block;
        pub const PULL: CongestionControl = CongestionControl::Block;
        pub const REPLY: CongestionControl = CongestionControl::Block;
        pub const UNIT: CongestionControl = CongestionControl::Block;
        pub const LINK_STATE_LIST: CongestionControl = CongestionControl::Block;
    }
}

/*************************************/
/*              TRAITS               */
/*************************************/
pub trait Header {
    fn header(&self) -> u8;
}

pub trait Options {
    fn options(&self) -> ZInt;
    fn has_options(&self) -> bool;
}

/*************************************/
/*            DECORATORS             */
/*************************************/
/// # Attachment decorator
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The Attachment can decorate any message (i.e., TransportMessage and ZenohMessage) and it allows to
/// append to the message any additional information. Since the information contained in the
/// Attachement is relevant only to the layer that provided them (e.g., Transport, Zenoh, User) it
/// is the duty of that layer to serialize and de-serialize the attachment whenever deemed necessary.
/// The attachement always contains serialized properties.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|Z|  ATTCH  |
/// +-+-+-+---------+
/// ~   Attachment  ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub buffer: ZBuf,
}

impl Header for Attachment {
    #[inline(always)]
    fn header(&self) -> u8 {
        #[allow(unused_mut)]
        let mut header = tmsg::id::ATTACHMENT;
        #[cfg(feature = "shared-memory")]
        if self.buffer.has_shminfo() {
            header |= tmsg::flag::Z;
        }
        header
    }
}

impl Attachment {
    #[inline(always)]
    pub fn new(buffer: ZBuf) -> Attachment {
        Attachment { buffer }
    }
}

/// # ReplyContext decorator
///
/// ```text
/// The **ReplyContext** is a message decorator for either:
///   - the **Data** messages that result from a query
///   - or a **Unit** message in case the message is a
///     SOURCE_FINAL or REPLY_FINAL.
///  The **replier-id** (queryable id) is represented as a byte-array.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|F|  R_CTX  |
/// +-+-+-+---------+
/// ~      qid      ~
/// +---------------+
/// ~   replier_id  ~ if F==0
/// +---------------+
///
/// - if F==1 then the message is a REPLY_FINAL
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplierInfo {
    pub id: ZenohId,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyContext {
    pub qid: ZInt,
    pub replier: Option<ReplierInfo>,
}

impl Header for ReplyContext {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::id::REPLY_CONTEXT;
        if self.is_final() {
            header |= zmsg::flag::F;
        }
        header
    }
}

impl ReplyContext {
    // Note: id replier_id=None flag F is set, meaning it's a REPLY_FINAL
    #[inline(always)]
    pub fn new(qid: ZInt, replier: Option<ReplierInfo>) -> ReplyContext {
        ReplyContext { qid, replier }
    }

    #[inline(always)]
    pub fn is_final(&self) -> bool {
        self.replier.is_none()
    }
}

/// -- RoutingContext decorator
///
/// ```text
/// The **RoutingContext** is a message decorator containing
/// informations for routing the concerned message.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X| RT_CTX  |
/// +-+-+-+---------+
/// ~      tid      ~
/// +---------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutingContext {
    pub tree_id: ZInt,
}

impl Header for RoutingContext {
    #[inline(always)]
    fn header(&self) -> u8 {
        zmsg::id::ROUTING_CONTEXT
    }
}

impl RoutingContext {
    #[inline(always)]
    pub fn new(tree_id: ZInt) -> RoutingContext {
        RoutingContext { tree_id }
    }
}

/// -- Priority decorator
///
/// ```text
/// The **Priority** is a message decorator containing
/// informations related to the priority of the frame/zenoh message.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// | ID  |  Prio   |
/// +-+-+-+---------+
///
/// ```
impl Header for Priority {
    fn header(&self) -> u8 {
        tmsg::id::PRIORITY | ((*self as u8) << imsg::HEADER_BITS)
    }
}

/*************************************/
/*         ZENOH MESSAGES            */
/*************************************/
/// # DataInfo
///
/// DataInfo data structure is optionally included in Data messages
///
/// ```text
///
/// Options bits
/// -  0: Payload is sliced
/// -  1: Payload kind
/// -  2: Payload encoding
/// -  3: Payload timestamp
/// -  4: Reserved
/// -  5: Reserved
/// -  6: Reserved
/// -  7: Payload source_id
/// -  8: Payload source_sn
/// -  9: First router_id
/// - 10: First router_sn
/// - 11-63: Reserved
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+---------+
/// ~    options    ~
/// +---------------+
/// ~      kind     ~ if options & (1 << 0)
/// +---------------+
/// ~   encoding    ~ if options & (1 << 1)
/// +---------------+
/// ~   timestamp   ~ if options & (1 << 2)
/// +---------------+
/// ~   source_id   ~ if options & (1 << 7)
/// +---------------+
/// ~   source_sn   ~ if options & (1 << 8)
/// +---------------+
///
/// - if options & (1 << 5) then the payload is sliced
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DataInfo {
    #[cfg(feature = "shared-memory")]
    pub sliced: bool,
    pub kind: SampleKind,
    pub encoding: Option<Encoding>,
    pub timestamp: Option<Timestamp>,
    pub source_id: Option<ZenohId>,
    pub source_sn: Option<ZInt>,
}

impl DataInfo {
    pub fn new() -> DataInfo {
        DataInfo::default()
    }
}

impl Options for DataInfo {
    fn options(&self) -> ZInt {
        let mut options = 0;
        #[cfg(feature = "shared-memory")]
        if self.sliced {
            options |= zmsg::data::info::SLICED;
        }
        if self.kind != SampleKind::Put {
            options |= zmsg::data::info::KIND;
        }
        if self.encoding.is_some() {
            options |= zmsg::data::info::ENCODING;
        }
        if self.timestamp.is_some() {
            options |= zmsg::data::info::TIMESTAMP;
        }
        if self.source_id.is_some() {
            options |= zmsg::data::info::SRCID;
        }
        if self.source_sn.is_some() {
            options |= zmsg::data::info::SRCSN;
        }
        options
    }

    fn has_options(&self) -> bool {
        macro_rules! sliced {
            ($info:expr) => {{
                #[cfg(feature = "shared-memory")]
                {
                    $info.sliced
                }
                #[cfg(not(feature = "shared-memory"))]
                {
                    false
                }
            }};
        }

        sliced!(self)
            || self.kind != SampleKind::Put
            || self.encoding.is_some()
            || self.timestamp.is_some()
            || self.source_id.is_some()
            || self.source_sn.is_some()
    }
}

impl PartialOrd for DataInfo {
    fn partial_cmp(&self, other: &DataInfo) -> Option<std::cmp::Ordering> {
        self.timestamp.partial_cmp(&other.timestamp)
    }
}

/// # Data message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|I|D|  DATA   |
/// +-+-+-+---------+
/// ~    KeyExpr     ~ if K==1 -- Only numerical id
/// +---------------+
/// ~    DataInfo   ~ if I==1
/// +---------------+
/// ~    Payload    ~
/// +---------------+
///
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub key: WireExpr<'static>,
    pub data_info: Option<DataInfo>,
    pub payload: ZBuf,
    pub congestion_control: CongestionControl,
    pub reply_context: Option<ReplyContext>,
}

impl Header for Data {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::id::DATA;
        if self.data_info.is_some() {
            header |= zmsg::flag::I;
        }
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        if self.congestion_control == CongestionControl::Drop {
            header |= zmsg::flag::D;
        }
        header
    }
}

/// # Unit message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|D|  UNIT   |
/// +-+-+-+---------+
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub congestion_control: CongestionControl,
    pub reply_context: Option<ReplyContext>,
}

impl Header for Unit {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::id::UNIT;
        if self.congestion_control == CongestionControl::Drop {
            header |= zmsg::flag::D;
        }
        header
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Resource(Resource),
    ForgetResource(ForgetResource),
    Publisher(Publisher),
    ForgetPublisher(ForgetPublisher),
    Subscriber(Subscriber),
    ForgetSubscriber(ForgetSubscriber),
    Queryable(Queryable),
    ForgetQueryable(ForgetQueryable),
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X| RESOURCE|
/// +---------------+
/// ~      RID      ~
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub expr_id: ZInt,
    pub key: WireExpr<'static>,
}

impl Header for Resource {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::RESOURCE;
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X|  F_RES  |
/// +---------------+
/// ~      RID      ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetResource {
    pub expr_id: ZInt,
}

impl Header for ForgetResource {
    #[inline(always)]
    fn header(&self) -> u8 {
        zmsg::declaration::id::FORGET_RESOURCE
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X|   PUB   |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publisher {
    pub key: WireExpr<'static>,
}

impl Header for Publisher {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::PUBLISHER;
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X|  F_PUB  |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetPublisher {
    pub key: WireExpr<'static>,
}

impl Header for ForgetPublisher {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::FORGET_PUBLISHER;
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|S|R|   SUB   |  R for Reliable
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// |P|  SubMode    | if S==1. Otherwise: SubMode=Push
/// +---------------+
/// ~    Period     ~ if P==1. Otherwise: None
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscriber {
    pub key: WireExpr<'static>,
    pub info: SubInfo,
}

impl Header for Subscriber {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::SUBSCRIBER;
        if self.info.reliability == Reliability::Reliable {
            header |= zmsg::flag::R;
        }
        if self.info.mode != SubMode::Push {
            header |= zmsg::flag::S;
        }
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X|  F_SUB  |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetSubscriber {
    pub key: WireExpr<'static>,
}

impl Header for ForgetSubscriber {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::FORGET_SUBSCRIBER;
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|Q|X|  QABLE  |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ~   QablInfo    ~ if Q==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queryable {
    pub key: WireExpr<'static>,
    pub info: QueryableInfo,
}

impl Header for Queryable {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::QUERYABLE;
        if self.info != QueryableInfo::default() {
            header |= zmsg::flag::Q;
        }
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X| F_QABLE |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetQueryable {
    pub key: WireExpr<'static>,
}

impl Header for ForgetQueryable {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::declaration::id::FORGET_QUERYABLE;
        if self.key.has_suffix() {
            header |= zmsg::flag::K
        }
        header
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X| DECLARE |
/// +-+-+-+---------+
/// ~  Num of Decl  ~
/// +---------------+
/// ~ [Declaration] ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    pub declarations: Vec<Declaration>,
}

impl Header for Declare {
    #[inline(always)]
    fn header(&self) -> u8 {
        zmsg::id::DECLARE
    }
}

/// # Pull message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|N|F|  PULL   |
/// +-+-+-+---------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ~    pullid     ~
/// +---------------+
/// ~  max_samples  ~ if N==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub key: WireExpr<'static>,
    pub pull_id: ZInt,
    pub max_samples: Option<ZInt>,
    pub is_final: bool,
}

impl Header for Pull {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::id::PULL;
        if self.is_final {
            header |= zmsg::flag::F;
        }
        if self.max_samples.is_some() {
            header |= zmsg::flag::N;
        }
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

/// # QueryBody
///
/// QueryBody data structure is optionally included in Query messages
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+---------+
/// ~    DataInfo   ~
/// +---------------+
/// ~    Payload    ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QueryBody {
    pub data_info: DataInfo,
    pub payload: ZBuf,
}

/// # Query message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|B|T|  QUERY  |
/// +-+-+-+---------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ~selector_params~
/// +---------------+
/// ~      qid      ~
/// +---------------+
/// ~     target    ~ if T==1
/// +---------------+
/// ~ consolidation ~
/// +---------------+
/// ~   QueryBody   ~ if B==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub key: WireExpr<'static>,
    pub parameters: String,
    pub qid: ZInt,
    pub target: Option<QueryTarget>,
    pub consolidation: ConsolidationMode,
    pub body: Option<QueryBody>,
}

impl Header for Query {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = zmsg::id::QUERY;
        if self.target.is_some() {
            header |= zmsg::flag::T;
        }
        if self.body.is_some() {
            header |= zmsg::flag::B;
        }
        if self.key.has_suffix() {
            header |= zmsg::flag::K;
        }
        header
    }
}

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~X|X|X|X|X|L|W|P~
// +-+-+-+-+-+-+-+-+
// ~     psid      ~
// +---------------+
// ~      sn       ~
// +---------------+
// ~      zid      ~ if P == 1
// +---------------+
// ~    whatami    ~ if W == 1
// +---------------+
// ~  [locators]   ~ if L == 1
// +---------------+
// ~    [links]    ~
// +---------------+
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkState {
    pub psid: ZInt,
    pub sn: ZInt,
    pub zid: Option<ZenohId>,
    pub whatami: Option<WhatAmI>,
    pub locators: Option<Vec<Locator>>,
    pub links: Vec<ZInt>,
}

impl Options for LinkState {
    fn options(&self) -> ZInt {
        let mut opts = 0;
        if self.zid.is_some() {
            opts |= zmsg::link_state::PID;
        }
        if self.whatami.is_some() {
            opts |= zmsg::link_state::WAI;
        }
        if self.locators.is_some() {
            opts |= zmsg::link_state::LOC;
        }
        opts
    }

    fn has_options(&self) -> bool {
        self.options() > 0
    }
}

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// |X|X|X|LK_ST_LS |
// +-+-+-+---------+
// ~ [link_states] ~
// +---------------+
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkStateList {
    pub link_states: Vec<LinkState>,
}

impl Header for LinkStateList {
    #[inline(always)]
    fn header(&self) -> u8 {
        zmsg::id::LINK_STATE_LIST
    }
}

// Zenoh messages at zenoh level
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ZenohBody {
    Data(Data),
    Declare(Declare),
    Query(Query),
    Pull(Pull),
    Unit(Unit),
    LinkStateList(LinkStateList),
}

#[derive(Clone, PartialEq)]
pub struct ZenohMessage {
    pub body: ZenohBody,
    pub channel: Channel,
    pub routing_context: Option<RoutingContext>,
    pub attachment: Option<Attachment>,
    #[cfg(feature = "stats")]
    pub size: Option<std::num::NonZeroUsize>,
}

impl fmt::Debug for ZenohMessage {
    #[cfg(feature = "stats")]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?} {:?} {:?} {:?} {:?}",
            self.body, self.channel, self.routing_context, self.attachment, self.size
        )
    }

    #[cfg(not(feature = "stats"))]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?} {:?} {:?} {:?}",
            self.body, self.channel, self.routing_context, self.attachment,
        )
    }
}

impl fmt::Display for ZenohMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl ZenohMessage {
    pub fn make_declare(
        declarations: Vec<Declaration>,
        routing_context: Option<RoutingContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::Declare(Declare { declarations }),
            channel: zmsg::default_channel::DECLARE,
            routing_context,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn make_data(
        key: WireExpr<'static>,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
        reply_context: Option<ReplyContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::Data(Data {
                key,
                data_info,
                payload,
                congestion_control,
                reply_context,
            }),
            channel,
            routing_context,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_unit(
        channel: Channel,
        congestion_control: CongestionControl,
        reply_context: Option<ReplyContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::Unit(Unit {
                congestion_control,
                reply_context,
            }),
            channel,
            routing_context: None,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_pull(
        is_final: bool,
        key: WireExpr<'static>,
        pull_id: ZInt,
        max_samples: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::Pull(Pull {
                key,
                pull_id,
                max_samples,
                is_final,
            }),
            channel: zmsg::default_channel::PULL,
            routing_context: None,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn make_query(
        key: WireExpr<'static>,
        parameters: String,
        qid: ZInt,
        target: Option<QueryTarget>,
        consolidation: ConsolidationMode,
        body: Option<QueryBody>,
        routing_context: Option<RoutingContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::Query(Query {
                key,
                parameters,
                qid,
                target,
                consolidation,
                body,
            }),
            channel: zmsg::default_channel::QUERY,
            routing_context,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_link_state_list(
        link_states: Vec<LinkState>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::LinkStateList(LinkStateList { link_states }),
            channel: zmsg::default_channel::LINK_STATE_LIST,
            routing_context: None,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    // -- Message Predicates
    #[inline]
    pub fn is_reliable(&self) -> bool {
        self.channel.reliability == Reliability::Reliable
    }

    #[inline]
    pub fn is_droppable(&self) -> bool {
        if !self.is_reliable() {
            return true;
        }

        let cc = match &self.body {
            ZenohBody::Data(data) => data.congestion_control,
            ZenohBody::Unit(unit) => unit.congestion_control,
            ZenohBody::Declare(_) => zmsg::default_congestion_control::DECLARE,
            ZenohBody::Pull(_) => zmsg::default_congestion_control::PULL,
            ZenohBody::Query(_) => zmsg::default_congestion_control::QUERY,
            ZenohBody::LinkStateList(_) => zmsg::default_congestion_control::LINK_STATE_LIST,
        };

        cc == CongestionControl::Drop
    }
}

/*************************************/
/*       TRANSPORT MESSAGES          */
/*************************************/
#[derive(Debug, Clone)]
pub enum TransportMode {
    Push,
    Pull,
    PeriodicPush(u32),
    PeriodicPull(u32),
    PushPull,
}

/// # Scout message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The SCOUT message can be sent at any point in time to solicit HELLO messages from matching parties.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|W|I|  SCOUT  |
/// +-+-+-+-+-------+
/// ~      what     ~ if W==1 -- Otherwise implicitly scouting for Routers
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scout {
    pub what: Option<WhatAmIMatcher>,
    pub zid_request: bool,
}

impl Header for Scout {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::SCOUT;
        if self.zid_request {
            header |= tmsg::flag::I;
        }
        if self.what.is_some() {
            header |= tmsg::flag::W;
        }
        header
    }
}

/// # Hello message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The HELLO message is sent in any of the following three cases:
///     1) in response to a SCOUT message;
///     2) to (periodically) advertise (e.g., on multicast) the Peer and the locators it is reachable at;
///     3) in a already established transport to update the corresponding peer on the new capabilities
///        (i.e., whatmai) and/or new set of locators (i.e., added or deleted).
/// Locators are expressed as:
/// <code>
///  udp/192.168.0.2:1234
///  tcp/192.168.0.2:1234
///  udp/239.255.255.123:5555
/// <code>
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |L|W|I|  HELLO  |
/// +-+-+-+-+-------+
/// ~    peer-id    ~ if I==1
/// +---------------+
/// ~    whatami    ~ if W==1 -- Otherwise it is from a Router
/// +---------------+
/// ~   [Locators]  ~ if L==1 -- Otherwise src-address is the locator
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub zid: Option<ZenohId>,
    pub whatami: Option<WhatAmI>,
    pub locators: Option<Vec<Locator>>,
}

impl Header for Hello {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::HELLO;
        if self.zid.is_some() {
            header |= tmsg::flag::I
        }
        if self.whatami.is_some() && self.whatami.unwrap() != WhatAmI::Router {
            header |= tmsg::flag::W;
        }
        if self.locators.is_some() {
            header |= tmsg::flag::L;
        }
        header
    }
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let what = match self.whatami {
            Some(what) => what.to_str(),
            None => WhatAmI::Router.to_str(),
        };
        let locators = match &self.locators {
            Some(locators) => locators
                .iter()
                .map(|locator| locator.to_string())
                .collect::<Vec<String>>(),
            None => vec![],
        };
        f.debug_struct("Hello")
            .field("zid", &self.zid)
            .field("whatami", &what)
            .field("locators", &locators)
            .finish()
    }
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: ZInt,
    pub is_qos: bool,
}

impl Header for InitSyn {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::INIT;
        if self.sn_resolution != SEQ_NUM_RES {
            header |= tmsg::flag::S;
        }
        if self.has_options() {
            header |= tmsg::flag::O;
        }
        header
    }
}

impl Options for InitSyn {
    fn options(&self) -> ZInt {
        let mut options = 0;
        if self.is_qos {
            options |= tmsg::init_options::QOS;
        }
        options
    }

    fn has_options(&self) -> bool {
        self.is_qos
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitAck {
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub sn_resolution: Option<ZInt>,
    pub is_qos: bool,
    pub cookie: ZSlice,
}

impl Header for InitAck {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::INIT;
        header |= tmsg::flag::A;
        if self.sn_resolution.is_some() {
            header |= tmsg::flag::S;
        }
        if self.has_options() {
            header |= tmsg::flag::O;
        }
        header
    }
}

impl Options for InitAck {
    fn options(&self) -> ZInt {
        let mut options = 0;
        if self.is_qos {
            options |= tmsg::init_options::QOS;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSyn {
    pub lease: Duration,
    pub initial_sn: ZInt,
    pub cookie: ZSlice,
}

impl Header for OpenSyn {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::OPEN;
        if self.lease.as_millis() % 1_000 == 0 {
            header |= tmsg::flag::T2;
        }
        header
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAck {
    pub lease: Duration,
    pub initial_sn: ZInt,
}

impl Header for OpenAck {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::OPEN;
        header |= tmsg::flag::A;
        if self.lease.as_millis() % 1_000 == 0 {
            header |= tmsg::flag::T2;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    pub version: u8,
    pub whatami: WhatAmI,
    pub zid: ZenohId,
    pub lease: Duration,
    pub sn_resolution: ZInt,
    pub next_sns: ConduitSnList,
}

impl Join {
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
        let mut header = tmsg::id::JOIN;
        if self.lease.as_millis() % 1_000 == 0 {
            header |= tmsg::flag::T1;
        }
        if self.sn_resolution != SEQ_NUM_RES {
            header |= tmsg::flag::S;
        }
        if self.has_options() {
            header |= tmsg::flag::O;
        }
        header
    }
}

impl Options for Join {
    fn options(&self) -> ZInt {
        let mut options = 0;
        if self.is_qos() {
            options |= tmsg::join_options::QOS;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Close {
    pub zid: Option<ZenohId>,
    pub reason: u8,
    pub link_only: bool,
}

impl Header for Close {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::CLOSE;
        if self.zid.is_some() {
            header |= tmsg::flag::I;
        }
        if self.link_only {
            header |= tmsg::flag::K;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sync {
    pub reliability: Reliability,
    pub sn: ZInt,
    pub count: Option<ZInt>,
}

impl Header for Sync {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::SYNC;
        if let Reliability::Reliable = self.reliability {
            header |= tmsg::flag::R;
        }
        if self.count.is_some() {
            header |= tmsg::flag::C;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckNack {
    pub sn: ZInt,
    pub mask: Option<ZInt>,
}

impl Header for AckNack {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::ACK_NACK;
        if self.mask.is_some() {
            header |= tmsg::flag::M;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAlive {
    pub zid: Option<ZenohId>,
}

impl Header for KeepAlive {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::KEEP_ALIVE;
        if self.zid.is_some() {
            header |= tmsg::flag::I;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ping {
    pub hash: ZInt,
}

impl Header for Ping {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = tmsg::id::PING_PONG;
        header |= tmsg::flag::P;
        header
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pong {
    pub hash: ZInt,
}

impl Header for Pong {
    #[inline(always)]
    fn header(&self) -> u8 {
        tmsg::id::PING_PONG
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
        let mut header = tmsg::id::FRAME;
        if let Reliability::Reliable = self.channel.reliability {
            header |= tmsg::flag::R;
        }
        if let FramePayload::Fragment { is_final, .. } = self.payload {
            header |= tmsg::flag::F;
            if is_final {
                header |= tmsg::flag::E;
            }
        }
        header
    }
}

impl Frame {
    pub fn make_header(reliability: Reliability, is_fragment: Option<bool>) -> u8 {
        let mut header = tmsg::id::FRAME;
        if let Reliability::Reliable = reliability {
            header |= tmsg::flag::R;
        }
        if let Some(is_final) = is_fragment {
            header |= tmsg::flag::F;
            if is_final {
                header |= tmsg::flag::E;
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
    Scout(Scout),
    Hello(Hello),
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
    pub fn make_scout(
        what: Option<WhatAmIMatcher>,
        zid_request: bool,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Scout(Scout { what, zid_request }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_hello(
        zid: Option<ZenohId>,
        whatami: Option<WhatAmI>,
        locators: Option<Vec<Locator>>,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Hello(Hello {
                zid,
                whatami,
                locators,
            }),
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_init_syn(
        version: u8,
        whatami: WhatAmI,
        zid: ZenohId,
        sn_resolution: ZInt,
        is_qos: bool,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::InitSyn(InitSyn {
                version,
                whatami,
                zid,
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
        zid: ZenohId,
        sn_resolution: Option<ZInt>,
        is_qos: bool,
        cookie: ZSlice,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::InitAck(InitAck {
                whatami,
                zid,
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
        zid: ZenohId,
        lease: Duration,
        sn_resolution: ZInt,
        next_sns: ConduitSnList,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Join(Join {
                version,
                whatami,
                zid,
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
        zid: Option<ZenohId>,
        reason: u8,
        link_only: bool,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Close(Close {
                zid,
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
        zid: Option<ZenohId>,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::KeepAlive(KeepAlive { zid }),
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
