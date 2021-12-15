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
use super::core::Encoding;
use super::core::*;
use super::id as iid;
use super::io::ZBuf;
use super::{Attachment, Header, Options};
use crate::net::link::Locator;
use std::fmt;

/*************************************/
/*               IDS                 */
/*************************************/
// Zenoh message IDs -- Re-export of some of the Inner Message IDs
pub mod id {
    // Messages
    pub const DECLARE: u8 = super::iid::DECLARE;
    pub const DATA: u8 = super::iid::DATA;
    pub const QUERY: u8 = super::iid::QUERY;
    pub const PULL: u8 = super::iid::PULL;
    pub const UNIT: u8 = super::iid::UNIT;
    pub const LINK_STATE_LIST: u8 = super::iid::LINK_STATE_LIST;

    // Message decorators
    pub const PRIORITY: u8 = super::iid::PRIORITY;
    pub const REPLY_CONTEXT: u8 = super::iid::REPLY_CONTEXT;
    pub const ATTACHMENT: u8 = super::iid::ATTACHMENT;
    pub const ROUTING_CONTEXT: u8 = super::iid::ROUTING_CONTEXT;
}

// Zenoh message flags
pub mod flag {
    pub const D: u8 = 1 << 5; // 0x20 Drop          if D==1 then the message can be dropped
    pub const F: u8 = 1 << 5; // 0x20 Final         if F==1 then this is the final message (e.g., ReplyContext, Pull)
    pub const I: u8 = 1 << 6; // 0x40 DataInfo      if I==1 then DataInfo is present
    pub const K: u8 = 1 << 7; // 0x80 KeySuffix     if K==1 then key_expr has suffix
    pub const N: u8 = 1 << 6; // 0x40 MaxSamples    if N==1 then the MaxSamples is indicated
    pub const Q: u8 = 1 << 6; // 0x40 QueryableInfo if Q==1 then the queryable info is present
    pub const R: u8 = 1 << 5; // 0x20 Reliable      if R==1 then it concerns the reliable channel, best-effort otherwise
    pub const S: u8 = 1 << 6; // 0x40 SubMode       if S==1 then the declaration SubMode is indicated
    pub const T: u8 = 1 << 5; // 0x20 QueryTarget   if T==1 then the query target is present

    // pub const X: u8 = 0; // Unused flags are set to zero
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
    }
}

// Default reliability for each Zenoh Message
pub mod default_channel {
    use super::{Channel, Priority, Reliability};

    pub const DECLARE: Channel = Channel {
        priority: Priority::Data,
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

/*************************************/
/*            DECORATORS             */
/*************************************/
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
/// ~  replier_kind ~ if F==0
/// +---------------+
/// ~   replier_id  ~ if F==0
/// +---------------+
///
/// - if F==1 then the message is a REPLY_FINAL
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ReplierInfo {
    pub kind: ZInt,
    pub id: PeerId,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyContext {
    pub qid: ZInt,
    pub replier: Option<ReplierInfo>,
}

impl Header for ReplyContext {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::REPLY_CONTEXT;
        if self.is_final() {
            header |= flag::F;
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingContext {
    pub tree_id: ZInt,
}

impl Header for RoutingContext {
    #[inline(always)]
    fn header(&self) -> u8 {
        id::ROUTING_CONTEXT
    }
}

impl RoutingContext {
    #[inline(always)]
    pub fn new(tree_id: ZInt) -> RoutingContext {
        RoutingContext { tree_id }
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
/// ~first_router_id~ if options & (1 << 9)
/// +---------------+
/// ~first_router_sn~ if options & (1 << 10)
/// +---------------+
///
/// - if options & (1 << 5) then the payload is sliced
///
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataInfo {
    #[cfg(feature = "shared-memory")]
    pub sliced: bool,
    pub kind: Option<ZInt>,
    pub encoding: Option<Encoding>,
    pub timestamp: Option<Timestamp>,
    pub source_id: Option<PeerId>,
    pub source_sn: Option<ZInt>,
    pub first_router_id: Option<PeerId>,
    pub first_router_sn: Option<ZInt>,
}

impl DataInfo {
    #[cfg(feature = "shared-memory")]
    pub const OPT_SLICED: ZInt = 1 << 0; // 0x01
    pub const OPT_KIND: ZInt = 1 << 1; // 0x02
    pub const OPT_ENCODING: ZInt = 1 << 2; // 0x04
    pub const OPT_TIMESTAMP: ZInt = 1 << 3; // 0x08
                                            // 0x10: Reserved
                                            // 0x20: Reserved
                                            // 0x40: Reserved
    pub const OPT_SRCID: ZInt = 1 << 7; // 0x80
    pub const OPT_SRCSN: ZInt = 1 << 8; // 0x100
    pub const OPT_RTRID: ZInt = 1 << 9; // 0x200
    pub const OPT_RTRSN: ZInt = 1 << 10; // 0x400

    pub fn new() -> DataInfo {
        DataInfo::default()
    }
}

impl Options for DataInfo {
    fn options(&self) -> ZInt {
        let mut options = 0;
        #[cfg(feature = "shared-memory")]
        if self.sliced {
            options |= Self::OPT_SLICED;
        }
        if self.kind.is_some() {
            options |= Self::OPT_KIND;
        }
        if self.encoding.is_some() {
            options |= Self::OPT_ENCODING;
        }
        if self.timestamp.is_some() {
            options |= Self::OPT_TIMESTAMP;
        }
        if self.source_id.is_some() {
            options |= Self::OPT_SRCID;
        }
        if self.source_sn.is_some() {
            options |= Self::OPT_SRCSN;
        }
        if self.first_router_id.is_some() {
            options |= Self::OPT_RTRID;
        }
        if self.first_router_sn.is_some() {
            options |= Self::OPT_RTRSN;
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
            || self.kind.is_some()
            || self.encoding.is_some()
            || self.timestamp.is_some()
            || self.source_id.is_some()
            || self.source_sn.is_some()
            || self.first_router_id.is_some()
            || self.first_router_sn.is_some()
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
    pub key: KeyExpr<'static>,
    pub data_info: Option<DataInfo>,
    pub payload: ZBuf,
    pub congestion_control: CongestionControl,
    pub reply_context: Option<ReplyContext>,
}

impl Header for Data {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::DATA;
        if self.data_info.is_some() {
            header |= flag::I;
        }
        if self.key.has_suffix() {
            header |= flag::K;
        }
        if self.congestion_control == CongestionControl::Drop {
            header |= flag::D;
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
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub congestion_control: CongestionControl,
    pub reply_context: Option<ReplyContext>,
}

impl Header for Unit {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::UNIT;
        if self.congestion_control == CongestionControl::Drop {
            header |= flag::D;
        }
        header
    }
}

#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub expr_id: ZInt,
    pub key: KeyExpr<'static>,
}

impl Header for Resource {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::RESOURCE;
        if self.key.has_suffix() {
            header |= flag::K;
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
#[derive(Debug, Clone, PartialEq)]
pub struct ForgetResource {
    pub expr_id: ZInt,
}

impl Header for ForgetResource {
    #[inline(always)]
    fn header(&self) -> u8 {
        declaration::id::FORGET_RESOURCE
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
#[derive(Debug, Clone, PartialEq)]
pub struct Publisher {
    pub key: KeyExpr<'static>,
}

impl Header for Publisher {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::PUBLISHER;
        if self.key.has_suffix() {
            header |= flag::K;
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
#[derive(Debug, Clone, PartialEq)]
pub struct ForgetPublisher {
    pub key: KeyExpr<'static>,
}

impl Header for ForgetPublisher {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::FORGET_PUBLISHER;
        if self.key.has_suffix() {
            header |= flag::K;
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
#[derive(Debug, Clone, PartialEq)]
pub struct Subscriber {
    pub key: KeyExpr<'static>,
    pub info: SubInfo,
}

impl Subscriber {
    pub const PERIOD: u8 = 0x80;
}

impl Header for Subscriber {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::SUBSCRIBER;
        if self.info.reliability == Reliability::Reliable {
            header |= flag::R;
        }
        if !(self.info.mode == SubMode::Push && self.info.period.is_none()) {
            header |= flag::S;
        }
        if self.key.has_suffix() {
            header |= flag::K;
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
#[derive(Debug, Clone, PartialEq)]
pub struct ForgetSubscriber {
    pub key: KeyExpr<'static>,
}

impl Header for ForgetSubscriber {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::FORGET_SUBSCRIBER;
        if self.key.has_suffix() {
            header |= flag::K;
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
/// ~     Kind      ~
/// +---------------+
/// ~   QablInfo    ~ if Q==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Queryable {
    pub key: KeyExpr<'static>,
    pub kind: ZInt,
    pub info: QueryableInfo,
}

impl Header for Queryable {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::QUERYABLE;
        if self.info != QueryableInfo::default() {
            header |= flag::Q;
        }
        if self.key.has_suffix() {
            header |= flag::K;
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
/// ~     Kind      ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ForgetQueryable {
    pub key: KeyExpr<'static>,
    pub kind: ZInt,
}

impl Header for ForgetQueryable {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = declaration::id::FORGET_QUERYABLE;
        if self.kind != queryable::EVAL {
            header |= flag::Q
        }
        if self.key.has_suffix() {
            header |= flag::K
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
#[derive(Debug, Clone, PartialEq)]
pub struct Declare {
    pub declarations: Vec<Declaration>,
}

impl Header for Declare {
    #[inline(always)]
    fn header(&self) -> u8 {
        id::DECLARE
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
#[derive(Debug, Clone, PartialEq)]
pub struct Pull {
    pub key: KeyExpr<'static>,
    pub pull_id: ZInt,
    pub max_samples: Option<ZInt>,
    pub is_final: bool,
}

impl Header for Pull {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::PULL;
        if self.is_final {
            header |= flag::F;
        }
        if self.max_samples.is_some() {
            header |= flag::N;
        }
        if self.key.has_suffix() {
            header |= flag::K;
        }
        header
    }
}

/// # Query message
///
/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|T|  QUERY  |
/// +-+-+-+---------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ~ value_selector~
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
    pub key: KeyExpr<'static>,
    pub value_selector: String,
    pub qid: ZInt,
    pub target: Option<QueryTarget>,
    pub consolidation: QueryConsolidation,
}

impl Header for Query {
    #[inline(always)]
    fn header(&self) -> u8 {
        let mut header = id::QUERY;
        if self.target.is_some() {
            header |= flag::T;
        }
        if self.key.has_suffix() {
            header |= flag::K;
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

impl LinkState {
    pub const OPT_PID: ZInt = 1; // 0x01
    pub const OPT_WAI: ZInt = 1 << 1; // 0x02
    pub const OPT_LOC: ZInt = 1 << 2; // 0x04
}

impl Options for LinkState {
    fn options(&self) -> ZInt {
        let mut opts = 0;
        if self.pid.is_some() {
            opts |= Self::OPT_PID;
        }
        if self.whatami.is_some() {
            opts |= Self::OPT_WAI;
        }
        if self.locators.is_some() {
            opts |= Self::OPT_LOC;
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
#[derive(Debug, Clone, PartialEq)]
pub struct LinkStateList {
    pub link_states: Vec<LinkState>,
}

impl Header for LinkStateList {
    #[inline(always)]
    fn header(&self) -> u8 {
        id::LINK_STATE_LIST
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
            channel: default_channel::DECLARE,
            routing_context,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn make_data(
        key: KeyExpr<'static>,
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
        key: KeyExpr<'static>,
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
            channel: default_channel::PULL,
            routing_context: None,
            attachment,
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[inline(always)]
    pub fn make_query(
        key: KeyExpr<'static>,
        value_selector: String,
        qid: ZInt,
        target: Option<QueryTarget>,
        consolidation: QueryConsolidation,
        routing_context: Option<RoutingContext>,
        attachment: Option<Attachment>,
    ) -> ZenohMessage {
        ZenohMessage {
            body: ZenohBody::Query(Query {
                key,
                value_selector,
                qid,
                target,
                consolidation,
            }),
            channel: default_channel::QUERY,
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
            channel: default_channel::LINK_STATE_LIST,
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
            ZenohBody::Declare(_) => default_congestion_control::DECLARE,
            ZenohBody::Pull(_) => default_congestion_control::PULL,
            ZenohBody::Query(_) => default_congestion_control::QUERY,
            ZenohBody::LinkStateList(_) => default_congestion_control::LINK_STATE_LIST,
        };

        cc == CongestionControl::Drop
    }
}
