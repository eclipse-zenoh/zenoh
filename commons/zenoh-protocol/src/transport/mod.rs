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
pub mod close;
pub mod frame;
pub mod init;
pub mod join;
pub mod keepalive;
pub mod open;

use crate::{
    common::Attachment,
    core::{Channel, Priority, WhatAmI, ZInt, ZenohId},
};
pub use close::Close;
use core::{convert::TryInto, fmt, time::Duration};
pub use frame::*;
pub use init::{InitAck, InitSyn};
pub use join::*;
pub use keepalive::*;
pub use open::*;
use zenoh_buffers::ZSlice;

pub mod id {
    pub const JOIN: u8 = 0x01; // For multicast communications only
    pub const INIT: u8 = 0x02; // For unicast communications only
    pub const OPEN: u8 = 0x03; // For unicast communications only
    pub const CLOSE: u8 = 0x04;
    pub const KEEP_ALIVE: u8 = 0x05;
    pub const FRAME: u8 = 0x06;
    pub const FRAGMENT: u8 = 0x07;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ConduitSnList {
    Plain(ConduitSn),
    QoS(Box<[ConduitSn; Priority::NUM]>),
}

impl fmt::Display for ConduitSnList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[ ")?;
        match self {
            ConduitSnList::Plain(sn) => {
                write!(
                    f,
                    "{:?} {{ reliable: {}, best effort: {} }}",
                    Priority::default(),
                    sn.reliable,
                    sn.best_effort
                )?;
            }
            ConduitSnList::QoS(ref sns) => {
                for (prio, sn) in sns.iter().enumerate() {
                    let p: Priority = (prio as u8).try_into().unwrap();
                    write!(
                        f,
                        "{:?} {{ reliable: {}, best effort: {} }}",
                        p, sn.reliable, sn.best_effort
                    )?;
                    if p != Priority::Background {
                        write!(f, ", ")?;
                    }
                }
            }
        }
        write!(f, " ]")
    }
}

/// The kind of reliability.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConduitSn {
    pub reliable: ZInt,
    pub best_effort: ZInt,
}

pub mod tmsg {
    use super::Priority;
    use crate::common::imsg;

    // Transport message IDs -- Re-export of some of the Inner Message IDs
    pub mod id {
        use super::imsg;

        // Message decorators
        pub const PRIORITY: u8 = imsg::id::PRIORITY;
        pub const ATTACHMENT: u8 = imsg::id::ATTACHMENT;
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TransportMessage {
    pub body: TransportBody,
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
            body: TransportBody::InitSyn(InitSyn::rand()), // @TODO
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
            body: TransportBody::InitAck(InitAck::rand()), // @TODO
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
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    pub fn make_close(
        zid: Option<ZenohId>,
        reason: u8,
        session: bool,
        attachment: Option<Attachment>,
    ) -> TransportMessage {
        TransportMessage {
            body: TransportBody::Close(Close { reason, session }),
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
            #[cfg(feature = "stats")]
            size: None,
        }
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

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

        Self { body }
    }
}
