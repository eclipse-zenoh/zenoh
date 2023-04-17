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
mod close;
mod frame;
mod init;
mod join;
mod keepalive;
mod open;

use crate::{
    common::Attachment,
    core::{Channel, ConduitSnList, WhatAmI, ZInt, ZenohId},
};
pub use close::*;
use core::time::Duration;
pub use frame::*;
pub use init::*;
pub use join::*;
pub use keepalive::*;
pub use open::*;
use zenoh_buffers::ZSlice;

pub mod tmsg {
    use crate::common::imsg;
    use crate::core::{Priority, ZInt};

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

// Zenoh messages at zenoh-transport level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportBody {
    InitSyn(InitSyn),
    InitAck(InitAck),
    OpenSyn(OpenSyn),
    OpenAck(OpenAck),
    Join(Join),
    Close(Close),
    KeepAlive(KeepAlive),
    Frame(Frame),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportMessage {
    pub body: TransportBody,
    pub attachment: Option<Attachment>,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl TransportMessage {
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

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let attachment = if rng.gen_bool(0.5) {
            Some(Attachment::rand())
        } else {
            None
        };

        let body = match rng.gen_range(0..8) {
            0 => TransportBody::InitSyn(InitSyn::rand()),
            1 => TransportBody::InitAck(InitAck::rand()),
            2 => TransportBody::OpenSyn(OpenSyn::rand()),
            3 => TransportBody::OpenAck(OpenAck::rand()),
            4 => TransportBody::Join(Join::rand()),
            5 => TransportBody::Close(Close::rand()),
            6 => TransportBody::KeepAlive(KeepAlive::rand()),
            7 => TransportBody::Frame(Frame::rand()),
            _ => unreachable!(),
        };

        Self { body, attachment }
    }
}
