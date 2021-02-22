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
use crate::core::*;
use crate::io::RBuf;
use crate::link::Locator;
use std::fmt;

/// # Attachment decorator
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The Attachment can decorate any message (i.e., SessionMessage and ZenohMessage) and it allows to
/// append to the message any additional information. Since the information contained in the
/// Attachement is relevant only to the layer that provided them (e.g., Session, Zenoh, User) it
/// is the duty of that layer to serialize and de-serialize the attachment whenever deemed necessary.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// | ENC |  ATTCH  |
/// +-+-+-+---------+
/// ~   Attachment  ~
/// +---------------+
///
/// ENC values:
/// - 0x00 => Zenoh Properties
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub encoding: u8,
    pub buffer: RBuf,
}

impl Attachment {
    pub fn make(encoding: u8, buffer: RBuf) -> Attachment {
        Attachment { encoding, buffer }
    }
}

/// # ReplyContext decorator
///
/// ```text
/// The **ReplyContext** is a message decorator for either:
///   - the **Data** messages that result from a query
///   - or a **Unit** message in case the message is a
///     SOURCE_FINAL or REPLY_FINAL.
///  The **replier-id** (eval or storage id) is represented as a byte-array.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|F|  R_CTX  |
/// +-+-+-+---------+
/// ~      qid      ~
/// +---------------+
/// ~  source_kind  ~
/// +---------------+
/// ~   replier_id  ~ if F==0
/// +---------------+
///
/// - if F==1 then the message is a REPLY_FINAL
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyContext {
    pub is_final: bool,
    pub qid: ZInt,
    pub source_kind: ZInt,
    pub replier_id: Option<PeerId>,
}

impl ReplyContext {
    // Note: id replier_id=None flag F is set, meaning it's a REPLY_FINAL
    pub fn make(qid: ZInt, source_kind: ZInt, replier_id: Option<PeerId>) -> ReplyContext {
        ReplyContext {
            is_final: replier_id.is_none(),
            qid,
            source_kind,
            replier_id,
        }
    }
}

/// -- RoutingContext decorator
///
/// The **RoutingContext** is a message decorator containing
/// informations for routing the concerned message.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X| RT_CTX  |
/// +-+-+-+---------+
/// ~      tid      ~
/// +---------------+
///
pub type RoutingContext = ZInt;

// Inner Message IDs
mod imsg {
    pub(super) mod id {
        // Session Messages
        pub(crate) const SCOUT: u8 = 0x01;
        pub(crate) const HELLO: u8 = 0x02;
        pub(crate) const INIT: u8 = 0x03;
        pub(crate) const OPEN: u8 = 0x04;
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
        pub(crate) const ROUTING_CONTEXT: u8 = 0x1d;
        pub(crate) const REPLY_CONTEXT: u8 = 0x1e;
        pub(crate) const ATTACHMENT: u8 = 0x1f;
    }

    pub(super) mod attachment {
        pub(crate) const PROPERTIES: u8 = 0x00;
    }
}

/*************************************/
/*         ZENOH MESSAGES            */
/*************************************/
pub mod zmsg {
    use super::imsg;
    use crate::core::{CongestionControl, Reliability, ZInt};

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
        pub const REPLY_CONTEXT: u8 = imsg::id::REPLY_CONTEXT;
        pub const ATTACHMENT: u8 = imsg::id::ATTACHMENT;
        pub const ROUTING_CONTEXT: u8 = imsg::id::ROUTING_CONTEXT;
    }

    pub mod attachment {
        use super::imsg;

        pub const PROPERTIES: u8 = imsg::attachment::PROPERTIES;
    }

    // Zenoh message flags
    pub mod flag {
        pub const D: u8 = 1 << 5; // 0x20 Dropping     if D==1 then the message can be dropped
        pub const F: u8 = 1 << 5; // 0x20 Final        if F==1 then this is the final message (e.g., ReplyContext, Pull)
        pub const I: u8 = 1 << 6; // 0x40 DataInfo     if I==1 then DataInfo is present
        pub const K: u8 = 1 << 7; // 0x80 ResourceKey  if K==1 then only numerical ID
        pub const N: u8 = 1 << 6; // 0x40 MaxSamples   if N==1 then the MaxSamples is indicated
        pub const P: u8 = 1; // 0x01 Pid          if P==1 then the pid is present
        pub const R: u8 = 1 << 5; // 0x20 Reliable     if R==1 then it concerns the reliable channel, best-effort otherwise
        pub const S: u8 = 1 << 6; // 0x40 SubMode      if S==1 then the declaration SubMode is indicated
        pub const T: u8 = 1 << 5; // 0x20 QueryTarget  if T==1 then the query target is present

        pub const X: u8 = 0; // Unused flags are set to zero
    }

    // Options used for DataInfo
    pub mod data {
        use super::ZInt;

        pub mod info {
            use super::ZInt;

            pub const SRCID: ZInt = 1; // 0x01
            pub const SRCSN: ZInt = 1 << 1; // 0x02
            pub const RTRID: ZInt = 1 << 2; // 0x04
            pub const RTRSN: ZInt = 1 << 3; // 0x08
            pub const TS: ZInt = 1 << 4; // 0x10
            pub const KIND: ZInt = 1 << 5; // 0x20
            pub const ENC: ZInt = 1 << 6; // 0x40
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

    // Default reliability for each Zenoh Message
    pub mod default_reliability {
        use super::Reliability;

        pub const DECLARE: Reliability = Reliability::Reliable;
        pub const DATA: Reliability = Reliability::BestEffort;
        pub const QUERY: Reliability = Reliability::Reliable;
        pub const PULL: Reliability = Reliability::Reliable;
        pub const REPLY: Reliability = Reliability::Reliable;
        pub const UNIT: Reliability = Reliability::BestEffort;
        pub const LINK_STATE_LIST: Reliability = Reliability::Reliable;
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

    // Header mask
    pub const HEADER_MASK: u8 = 0x1f;

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

#[derive(Debug, Clone, PartialEq)]
pub enum Declaration {
    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|X|X| RESOURCE|
    /// +---------------+
    /// ~      RID      ~
    /// +---------------+
    /// ~    ResKey     ~ if  K==1 then only numerical id
    /// +---------------+
    /// ```
    Resource { rid: ZInt, key: ResKey },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |X|X|X|  F_RES  |
    /// +---------------+
    /// ~      RID      ~
    /// +---------------+
    /// ```
    ForgetResource { rid: ZInt },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|X|X|   PUB   |
    /// +---------------+
    /// ~    ResKey     ~ if  K==1 then only numerical id
    /// +---------------+
    /// ```
    Publisher { key: ResKey },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|X|X|  F_PUB  |
    /// +---------------+
    /// ~    ResKey     ~ if  K==1 then only numerical id
    /// +---------------+
    /// ```
    ForgetPublisher { key: ResKey },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|S|R|   SUB   |  R for Reliable
    /// +---------------+
    /// ~    ResKey     ~ if K==1 then only numerical id
    /// +---------------+
    /// |P|  SubMode    | if S==1. Otherwise: SubMode=Push
    /// +---------------+
    /// ~    Period     ~ if P==1. Otherwise: None
    /// +---------------+
    /// ```
    Subscriber { key: ResKey, info: SubInfo },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|X|X|  F_SUB  |
    /// +---------------+
    /// ~    ResKey     ~ if  K==1 then only numerical id
    /// +---------------+
    /// ```
    ForgetSubscriber { key: ResKey },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|X|X|  QABLE  |
    /// +---------------+
    /// ~     ResKey    ~ if  K==1 then only numerical id
    /// +---------------+
    /// ```
    Queryable { key: ResKey },

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |K|X|X| F_QABLE |
    /// +---------------+
    /// ~    ResKey     ~ if  K==1 then only numerical id
    /// +---------------+
    /// ```
    ForgetQueryable { key: ResKey },
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
#[derive(Debug, Clone, PartialEq)]
pub struct Declare {
    pub declarations: Vec<Declaration>,
}

/// # DataInfo
///
/// DataInfo data structure is optionally included in Data messages
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+---------+
/// ~X|G|F|E|D|C|B|A~ -- encoded as ZInt
/// +---------------+
/// ~   source_id   ~ if A==1
/// +---------------+
/// ~   source_sn   ~ if B==1
/// +---------------+
/// ~first_router_id~ if C==1
/// +---------------+
/// ~first_router_sn~ if D==1
/// +---------------+
/// ~   timestamp   ~ if E==1
/// +---------------+
/// ~      kind     ~ if F==1
/// +---------------+
/// ~   encoding    ~ if G==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct DataInfo {
    pub source_id: Option<PeerId>,
    pub source_sn: Option<ZInt>,
    pub first_router_id: Option<PeerId>,
    pub first_router_sn: Option<ZInt>,
    pub timestamp: Option<Timestamp>,
    pub kind: Option<ZInt>,
    pub encoding: Option<ZInt>,
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
/// ~    ResKey     ~ if K==1 -- Only numerical id
/// +---------------+
/// ~    DataInfo   ~ if I==1
/// +---------------+
/// ~    Payload    ~
/// +---------------+
///
/// - if D==1 then the message can be dropped for congestion control reasons.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub key: ResKey,
    pub data_info: Option<DataInfo>,
    pub payload: RBuf,
}

/// # Unit message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|D|  UNIT   |
/// +-+-+-+---------+
///
/// - if D==1 then the message can be dropped for congestion control reasons.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {}

/// # Pull message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|N|F|  PULL   |
/// +-+-+-+---------+
/// ~    ResKey     ~ if K==1 then only numerical id
/// +---------------+
/// ~    pullid     ~
/// +---------------+
/// ~  max_samples  ~ if N==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Pull {
    pub key: ResKey,
    pub pull_id: ZInt,
    pub max_samples: Option<ZInt>,
    pub is_final: bool,
}

/// # Query message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|T|  QUERY  |
/// +-+-+-+---------+
/// ~    ResKey     ~ if K==1 then only numerical id
/// +---------------+
/// ~   predicate   ~
/// +---------------+
/// ~      qid      ~
/// +---------------+
/// ~     target    ~ if T==1
/// +---------------+
/// ~ consolidation ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub key: ResKey,
    pub predicate: String,
    pub qid: ZInt,
    pub target: Option<QueryTarget>,
    pub consolidation: QueryConsolidation,
}

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// |X|X|X|X|X|L|W|P|
// +-+-+-+-+-+-+-+-+
// ~     psid      ~
// +---------------+
// ~      sn       ~
// +---------------+
// ~      pid      ~ if P == 1
// +---------------+
// ~    whatami    ~ if W == 1
// +---------------+
// ~  [locators]   ~ if L == 1
// +---------------+
// ~    [links]    ~
// +---------------+
#[derive(Debug, Clone, PartialEq)]
pub struct LinkState {
    pub psid: ZInt,
    pub sn: ZInt,
    pub pid: Option<PeerId>,
    pub whatami: Option<WhatAmI>,
    pub locators: Option<Vec<Locator>>,
    pub links: Vec<ZInt>,
}

//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// |X|X|X|LK_ST_LS |
// +-+-+-+---------+
// ~ [link_states] ~
// +---------------+
#[derive(Debug, Clone, PartialEq)]
pub struct LinkStateList {
    pub link_states: Vec<LinkState>,
}

// Zenoh messages at zenoh level
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ZenohBody {
    Declare(Declare),
    Data(Data),
    Query(Query),
    Pull(Pull),
    Unit(Unit),
    LinkStateList(LinkStateList),
}

#[derive(Clone, PartialEq)]
pub struct ZenohMessage {
    pub header: u8,
    pub body: ZenohBody,
    pub reliability: Reliability,
    pub congestion_control: CongestionControl,
    pub routing_context: Option<RoutingContext>,
    pub reply_context: Option<ReplyContext>,
    pub attachment: Option<Attachment>,
}

impl std::fmt::Debug for ZenohMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            self.body,
            self.reliability,
            self.congestion_control,
            self.routing_context,
            self.reply_context,
            self.attachment
        )
    }
}

impl std::fmt::Display for ZenohMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl ZenohMessage {
    pub fn make_declare(
        declarations: Vec<Declaration>,
        routing_context: Option<RoutingContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        let header = zmsg::id::DECLARE;

        ZenohMessage {
            header,
            body: ZenohBody::Declare(Declare { declarations }),
            reliability: zmsg::default_reliability::DECLARE,
            congestion_control: zmsg::default_congestion_control::DECLARE,
            routing_context,
            reply_context: None,
            attachment,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn make_data(
        key: ResKey,
        payload: RBuf,
        reliability: Reliability,
        congestion_control: CongestionControl,
        data_info: Option<DataInfo>,
        routing_context: Option<RoutingContext>,
        reply_context: Option<ReplyContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        let kflag = if key.is_numerical() { zmsg::flag::K } else { 0 };
        let iflag = if data_info.is_some() {
            zmsg::flag::I
        } else {
            0
        };
        let dflag = match congestion_control {
            CongestionControl::Block => 0,
            CongestionControl::Drop => zmsg::flag::D,
        };
        let header = zmsg::id::DATA | dflag | iflag | kflag;

        ZenohMessage {
            header,
            body: ZenohBody::Data(Data {
                key,
                data_info,
                payload,
            }),
            reliability,
            congestion_control,
            routing_context,
            reply_context,
            attachment,
        }
    }

    pub fn make_unit(
        reliability: Reliability,
        congestion_control: CongestionControl,
        reply_context: Option<ReplyContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        let dflag = match congestion_control {
            CongestionControl::Block => 0,
            CongestionControl::Drop => zmsg::flag::D,
        };
        let header = zmsg::id::UNIT | dflag;

        ZenohMessage {
            header,
            body: ZenohBody::Unit(Unit {}),
            reliability,
            congestion_control,
            routing_context: None,
            reply_context,
            attachment,
        }
    }

    pub fn make_pull(
        is_final: bool,
        key: ResKey,
        pull_id: ZInt,
        max_samples: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        let kflag = if key.is_numerical() { zmsg::flag::K } else { 0 };
        let nflag = if max_samples.is_some() {
            zmsg::flag::N
        } else {
            0
        };
        let fflag = if is_final { zmsg::flag::F } else { 0 };
        let header = zmsg::id::PULL | fflag | nflag | kflag;

        ZenohMessage {
            header,
            body: ZenohBody::Pull(Pull {
                key,
                pull_id,
                max_samples,
                is_final,
            }),
            reliability: zmsg::default_reliability::PULL,
            congestion_control: zmsg::default_congestion_control::PULL,
            routing_context: None,
            reply_context: None,
            attachment,
        }
    }

    pub fn make_query(
        key: ResKey,
        predicate: String,
        qid: ZInt,
        target: Option<QueryTarget>,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        let kflag = if key.is_numerical() { zmsg::flag::K } else { 0 };
        let tflag = if target.is_some() { zmsg::flag::T } else { 0 };
        let header = zmsg::id::QUERY | tflag | kflag;

        ZenohMessage {
            header,
            body: ZenohBody::Query(Query {
                key,
                predicate,
                qid,
                target,
                consolidation,
            }),
            reliability: zmsg::default_reliability::QUERY,
            congestion_control: zmsg::default_congestion_control::QUERY,
            routing_context,
            reply_context: None,
            attachment,
        }
    }

    pub fn make_link_state_list(
        link_states: Vec<LinkState>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        let header = zmsg::id::LINK_STATE_LIST;

        ZenohMessage {
            header,
            body: ZenohBody::LinkStateList(LinkStateList { link_states }),
            reliability: zmsg::default_reliability::LINK_STATE_LIST,
            congestion_control: zmsg::default_congestion_control::LINK_STATE_LIST,
            routing_context: None,
            reply_context: None,
            attachment,
        }
    }

    // -- Message Predicates
    #[inline]
    pub fn is_reliable(&self) -> bool {
        match self.reliability {
            Reliability::Reliable => true,
            Reliability::BestEffort => false,
        }
    }

    #[inline]
    pub fn is_droppable(&self) -> bool {
        match self.congestion_control {
            CongestionControl::Block => false,
            CongestionControl::Drop => true,
        }
    }

    #[inline]
    pub fn is_reply(&self) -> bool {
        self.reply_context.is_some()
    }
}

/*************************************/
/*        SESSION MESSAGES           */
/*************************************/
pub mod smsg {
    use super::imsg;

    // Session message IDs -- Re-export of some of the Inner Message IDs
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

        // Message decorators
        pub const ATTACHMENT: u8 = imsg::id::ATTACHMENT;
    }

    pub mod attachment {
        use super::imsg;

        pub const PROPERTIES: u8 = imsg::attachment::PROPERTIES;
    }

    // Session message flags
    pub mod flag {
        pub const A: u8 = 1 << 5; // 0x20 Ack           if A==1 then the message is an acknowledgment
        pub const C: u8 = 1 << 6; // 0x40 Count         if C==1 then number of unacknowledged messages is present
        pub const E: u8 = 1 << 7; // 0x80 End           if E==1 then it is the last FRAME fragment
        pub const F: u8 = 1 << 6; // 0x40 Fragment      if F==1 then the FRAME is a fragment
        pub const I: u8 = 1 << 5; // 0x20 PeerID        if I==1 then the PeerID is present
        pub const K: u8 = 1 << 6; // 0x40 CloseLink     if K==1 then close the transport link only
        pub const L: u8 = 1 << 7; // 0x80 Locators      if L==1 then Locators are present
        pub const M: u8 = 1 << 5; // 0x20 Mask          if M==1 then a Mask is present
        pub const P: u8 = 1 << 5; // 0x20 PingOrPong    if P==1 then the message is Ping, otherwise is Pong
        pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then it concerns the reliable channel, best-effort otherwise
        pub const S: u8 = 1 << 6; // 0x40 SN Resolution if S==1 then the SN Resolution is present
        pub const T: u8 = 1 << 6; // 0x40 TimeRes       if T==1 then the time resolution is in seconds
        pub const W: u8 = 1 << 6; // 0x40 WhatAmI       if W==1 then WhatAmI is indicated

        pub const X: u8 = 0; // Unused flags are set to zero
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

    // Header mask
    pub const HEADER_MASK: u8 = 0x1f;

    pub fn mid(header: u8) -> u8 {
        header & HEADER_MASK
    }
    pub fn flags(header: u8) -> u8 {
        header & !HEADER_MASK
    }
    pub fn has_flag(byte: u8, flag: u8) -> bool {
        byte & flag != 0
    }
}

#[derive(Debug, Clone)]
pub enum SessionMode {
    Push,
    Pull,
    PeriodicPush(u32),
    PeriodicPull(u32),
    PushPull,
}

// The Payload of the Frame message
#[derive(Debug, Clone, PartialEq)]
pub enum FramePayload {
    Fragment { buffer: RBuf, is_final: bool },
    Messages { messages: Vec<ZenohMessage> },
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
/// ~      what     ~ if W==1 -- Otherwise implicitly scouting for Brokers
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Scout {
    pub what: Option<WhatAmI>,
    pub pid_request: bool,
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
///     3) in a already established session to update the corresponding peer on the new capabilities
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
/// ~    whatami    ~ if W==1 -- Otherwise it is from a Broker
/// +---------------+
/// ~    Locators   ~ if L==1 -- Otherwise src-address is the locator
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Hello {
    pub pid: Option<PeerId>,
    pub whatami: Option<WhatAmI>,
    pub locators: Option<Vec<Locator>>,
}

impl fmt::Display for Hello {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let what = match self.whatami {
            Some(what) => whatami::to_string(what),
            None => whatami::to_string(whatami::ROUTER),
        };
        let locators = match &self.locators {
            Some(locators) => locators
                .iter()
                .map(|locator| locator.to_string())
                .collect::<Vec<String>>(),
            None => vec![],
        };
        f.debug_struct("Hello")
            .field("pid", &self.pid)
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
/// The INIT message is sent on a specific Locator to initiate a session with the peer associated
/// with that Locator. The initiator MUST send an INIT message with the A flag set to 0.  If the
/// corresponding peer deems appropriate to initialize a session with the initiator, the corresponding
/// peer MUST reply with an INIT message with the A flag set to 1.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|S|A|   INIT  |
/// +-+-+-+-+-------+
/// | v_maj | v_min | if A==0 -- Protocol Version VMaj.VMin
/// +-------+-------+
/// ~    whatami    ~ -- Client, Router, Peer or a combination of them
/// +---------------+
/// ~    peer_id    ~ -- PID of the sender of the INIT message
/// +---------------+
/// ~ sn_resolution ~ if S==1(*) -- Otherwise 2^28 is assumed(**)
/// +---------------+
/// ~     cookie    ~ if A==1
/// +---------------+
///
/// (*) if A==0 and S==0 then 2^28 is assumed.
///     if A==1 and S==0 then the agreed resolution is the one communicated by the initiator.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct InitSyn {
    pub version: u8,
    pub whatami: WhatAmI,
    pub pid: PeerId,
    pub sn_resolution: Option<ZInt>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct InitAck {
    pub whatami: WhatAmI,
    pub pid: PeerId,
    pub sn_resolution: Option<ZInt>,
    pub cookie: RBuf,
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
/// The OPEN message is sent on a link to finally open an initialized session with the peer.
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|T|A|   OPEN  |
/// +-+-+-+-+-------+
/// ~ lease_period  ~ -- Lease period of the sender of the OPEN message(*)
/// +---------------+
/// ~  initial_sn   ~ -- Initial SN proposed by the sender of the OPEN(**)
/// +---------------+
/// ~    cookie     ~ if A==0(*)
/// +---------------+
///
/// (*) if T==1 then the lease period is expressed in seconds, otherwise in milliseconds
/// (**) the cookie MUST be the same received in the INIT message with A==1 from the corresponding peer
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSyn {
    pub lease: ZInt,
    pub initial_sn: ZInt,
    pub cookie: RBuf,
}
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAck {
    pub lease: ZInt,
    pub initial_sn: ZInt,
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
///     2) at any time to arbitrarly close the session with the corresponding peer.
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
/// - if K==0 then close the whole zenoh session.
/// - if K==1 then close the transport link the CLOSE message was sent on (e.g., TCP socket) but
///           keep the whole session open. NOTE: the session will be automatically closed when
///           the session's lease period expires.
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub pid: Option<PeerId>,
    pub reason: u8,
    pub link_only: bool,
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
    pub ch: Channel,
    pub sn: ZInt,
    pub count: Option<ZInt>,
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

/// # KeepAlive message
///
/// ```text
/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
///
/// The KEEP_ALIVE message can be sent periodically to avoid the expiration of the session lease
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
    pub pid: Option<PeerId>,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Pong {
    pub hash: ZInt,
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
    pub ch: Channel,
    pub sn: ZInt,
    pub payload: FramePayload,
}

// Zenoh messages at zenoh-session level
#[derive(Debug, Clone, PartialEq)]
pub enum SessionBody {
    Scout(Scout),
    Hello(Hello),
    InitSyn(InitSyn),
    InitAck(InitAck),
    OpenSyn(OpenSyn),
    OpenAck(OpenAck),
    Close(Close),
    Sync(Sync),
    AckNack(AckNack),
    KeepAlive(KeepAlive),
    Ping(Ping),
    Pong(Pong),
    Frame(Frame),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionMessage {
    pub(crate) header: u8,
    pub(crate) body: SessionBody,
    pub(crate) attachment: Option<Attachment>,
}

impl SessionMessage {
    pub fn make_scout(
        what: Option<WhatAmI>,
        pid_request: bool,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let iflag = if pid_request { smsg::flag::I } else { 0 };
        let wflag = if what.is_some() { smsg::flag::W } else { 0 };
        let header = smsg::id::SCOUT | wflag | iflag;

        SessionMessage {
            header,
            body: SessionBody::Scout(Scout { what, pid_request }),
            attachment,
        }
    }

    pub fn make_hello(
        pid: Option<PeerId>,
        whatami: Option<WhatAmI>,
        locators: Option<Vec<Locator>>,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let iflag = if pid.is_some() { smsg::flag::I } else { 0 };
        let wflag = if whatami.is_some() && whatami.unwrap() != whatami::ROUTER {
            smsg::flag::W
        } else {
            0
        };
        let lflag = if locators.is_some() { smsg::flag::L } else { 0 };
        let header = smsg::id::HELLO | iflag | wflag | lflag;

        SessionMessage {
            header,
            body: SessionBody::Hello(Hello {
                pid,
                whatami,
                locators,
            }),
            attachment,
        }
    }

    pub fn make_init_syn(
        version: u8,
        whatami: WhatAmI,
        pid: PeerId,
        sn_resolution: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let sflag = if sn_resolution.is_some() {
            smsg::flag::S
        } else {
            0
        };
        let header = smsg::id::INIT | sflag;

        SessionMessage {
            header,
            body: SessionBody::InitSyn(InitSyn {
                version,
                whatami,
                pid,
                sn_resolution,
            }),
            attachment,
        }
    }

    pub fn make_init_ack(
        whatami: WhatAmI,
        pid: PeerId,
        sn_resolution: Option<ZInt>,
        cookie: RBuf,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let sflag = if sn_resolution.is_some() {
            smsg::flag::S
        } else {
            0
        };
        let header = smsg::id::INIT | smsg::flag::A | sflag;

        SessionMessage {
            header,
            body: SessionBody::InitAck(InitAck {
                whatami,
                pid,
                sn_resolution,
                cookie,
            }),
            attachment,
        }
    }

    pub fn make_open_syn(
        lease: ZInt,
        initial_sn: ZInt,
        cookie: RBuf,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let tflag = if lease % 1_000 == 0 { smsg::flag::T } else { 0 };
        let header = smsg::id::OPEN | tflag;

        SessionMessage {
            header,
            body: SessionBody::OpenSyn(OpenSyn {
                lease,
                initial_sn,
                cookie,
            }),
            attachment,
        }
    }

    pub fn make_open_ack(
        lease: ZInt,
        initial_sn: ZInt,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let tflag = if lease % 1_000 == 0 { smsg::flag::T } else { 0 };
        let header = smsg::id::OPEN | smsg::flag::A | tflag;

        SessionMessage {
            header,
            body: SessionBody::OpenAck(OpenAck { lease, initial_sn }),
            attachment,
        }
    }

    pub fn make_close(
        pid: Option<PeerId>,
        reason: u8,
        link_only: bool,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let kflag = if link_only { smsg::flag::K } else { 0 };
        let iflag = if pid.is_some() { smsg::flag::I } else { 0 };
        let header = smsg::id::CLOSE | kflag | iflag;

        SessionMessage {
            header,
            body: SessionBody::Close(Close {
                pid,
                reason,
                link_only,
            }),
            attachment,
        }
    }

    pub fn make_sync(
        ch: Channel,
        sn: ZInt,
        count: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let cflag = if count.is_some() { smsg::flag::C } else { 0 };
        let rflag = match ch {
            Channel::Reliable => smsg::flag::R,
            Channel::BestEffort => 0,
        };
        let header = smsg::id::SYNC | rflag | cflag;

        SessionMessage {
            header,
            body: SessionBody::Sync(Sync { ch, sn, count }),
            attachment,
        }
    }

    pub fn make_ack_nack(
        sn: ZInt,
        mask: Option<ZInt>,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let mflag = if mask.is_some() { smsg::flag::M } else { 0 };
        let header = smsg::id::ACK_NACK | mflag;

        SessionMessage {
            header,
            body: SessionBody::AckNack(AckNack { sn, mask }),
            attachment,
        }
    }

    pub fn make_keep_alive(pid: Option<PeerId>, attachment: Option<Attachment>) -> SessionMessage {
        let iflag = if pid.is_some() { smsg::flag::I } else { 0 };
        let header = smsg::id::KEEP_ALIVE | iflag;

        SessionMessage {
            header,
            body: SessionBody::KeepAlive(KeepAlive { pid }),
            attachment,
        }
    }

    pub fn make_ping(hash: ZInt, attachment: Option<Attachment>) -> SessionMessage {
        let pflag = smsg::flag::P;
        let header = smsg::id::PING_PONG | pflag;

        SessionMessage {
            header,
            body: SessionBody::Ping(Ping { hash }),
            attachment,
        }
    }

    pub fn make_pong(hash: ZInt, attachment: Option<Attachment>) -> SessionMessage {
        let pflag = 0;
        let header = smsg::id::PING_PONG | pflag;

        SessionMessage {
            header,
            body: SessionBody::Pong(Pong { hash }),
            attachment,
        }
    }

    pub fn make_frame(
        ch: Channel,
        sn: ZInt,
        payload: FramePayload,
        attachment: Option<Attachment>,
    ) -> SessionMessage {
        let rflag = match ch {
            Channel::Reliable => smsg::flag::R,
            Channel::BestEffort => 0,
        };
        let (eflag, fflag) = match &payload {
            FramePayload::Fragment { is_final, .. } => {
                if *is_final {
                    (smsg::flag::E, smsg::flag::F)
                } else {
                    (0, smsg::flag::F)
                }
            }
            FramePayload::Messages { .. } => (0, 0),
        };
        let header = smsg::id::FRAME | rflag | fflag | eflag;

        SessionMessage {
            header,
            body: SessionBody::Frame(Frame { ch, sn, payload }),
            attachment,
        }
    }

    pub fn make_frame_header(ch: Channel, is_fragment: Option<bool>) -> u8 {
        let rflag = match ch {
            Channel::Reliable => smsg::flag::R,
            Channel::BestEffort => 0,
        };
        let (eflag, fflag) = if let Some(is_final) = is_fragment {
            if is_final {
                (smsg::flag::E, smsg::flag::F)
            } else {
                (0, smsg::flag::F)
            }
        } else {
            (0, 0)
        };

        smsg::id::FRAME | rflag | fflag | eflag
    }

    // -- Accessor
    #[inline]
    pub fn get_body(&self) -> &SessionBody {
        &self.body
    }

    #[inline]
    pub fn get_attachment(&self) -> &Option<Attachment> {
        &self.attachment
    }

    #[inline]
    pub fn get_attachment_mut(&mut self) -> &mut Option<Attachment> {
        &mut self.attachment
    }
}
