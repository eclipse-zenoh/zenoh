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
mod data;
mod declare;
mod linkstate;
mod pull;
mod query;
mod routing;
mod unit;

use crate::{
    common::Attachment,
    core::{
        Channel, CongestionControl, ConsolidationMode, QueryTarget, Reliability, WireExpr, ZInt,
    },
};
use alloc::{string::String, vec::Vec};
use core::fmt;
pub use data::*;
pub use declare::*;
pub use linkstate::*;
pub use pull::*;
pub use query::*;
pub use routing::*;
pub use unit::*;
use zenoh_buffers::ZBuf;

pub mod zmsg {
    use crate::{
        common::imsg,
        core::{Channel, CongestionControl, Priority, Reliability, ZInt},
    };

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

// Zenoh messages at zenoh level
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZenohBody {
    Data(Data),
    Unit(Unit),
    Pull(Pull),
    Query(Query),
    Declare(Declare),
    LinkStateList(LinkStateList),
}

#[derive(Clone, PartialEq, Eq)]
pub struct ZenohMessage {
    pub body: ZenohBody,
    pub channel: Channel,
    pub routing_context: Option<RoutingContext>,
    pub attachment: Option<Attachment>,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
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

impl fmt::Debug for ZenohMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?} {:?} {:?} {:?}",
            self.body, self.channel, self.routing_context, self.attachment
        )?;
        #[cfg(feature = "stats")]
        write!(f, " {:?}", self.size)?;
        Ok(())
    }
}

impl fmt::Display for ZenohMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl ZenohMessage {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::core::Priority;
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let attachment = if rng.gen_bool(0.5) {
            Some(Attachment::rand())
        } else {
            None
        };

        let routing_context = if rng.gen_bool(0.5) {
            Some(RoutingContext::rand())
        } else {
            None
        };

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
        let body = match rng.gen_range(0..6) {
            0 => ZenohBody::Data(Data::rand()),
            1 => ZenohBody::Unit(Unit::rand()),
            2 => ZenohBody::Pull(Pull::rand()),
            3 => ZenohBody::Query(Query::rand()),
            4 => ZenohBody::Declare(Declare::rand()),
            5 => ZenohBody::LinkStateList(LinkStateList::rand()),
            _ => unreachable!(),
        };

        Self {
            body,
            channel,
            routing_context,
            attachment,
        }
    }
}
