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
pub mod fragment;
pub mod frame;
pub mod init;
// pub mod join;
pub mod keepalive;
pub mod open;

use crate::core::{Priority, ZInt};
use alloc::boxed::Box;
pub use close::Close;
use core::{convert::TryInto, fmt};
pub use fragment::{Fragment, FragmentHeader};
pub use frame::{Frame, FrameHeader};
pub use init::{InitAck, InitSyn};
// pub use join::Join;
pub use keepalive::KeepAlive;
pub use open::{OpenAck, OpenSyn};

pub mod id {
    // pub const JOIN: u8 = 0x01; // For multicast communications only
    pub const INIT: u8 = 0x02; // For unicast communications only
    pub const OPEN: u8 = 0x03; // For unicast communications only
    pub const CLOSE: u8 = 0x04;
    pub const KEEP_ALIVE: u8 = 0x05;
    pub const FRAME: u8 = 0x06;
    pub const FRAGMENT: u8 = 0x07;
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct ConduitSn {
    pub reliable: ZInt,
    pub best_effort: ZInt,
}

// Zenoh messages at zenoh-transport level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportBody {
    InitSyn(InitSyn),
    InitAck(InitAck),
    OpenSyn(OpenSyn),
    OpenAck(OpenAck),
    // Join(Join),
    Close(Close),
    KeepAlive(KeepAlive),
    Frame(Frame),
    Fragment(Fragment),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportMessage {
    pub body: TransportBody,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl TransportMessage {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let body = match rng.gen_range(0..8) {
            0 => TransportBody::InitSyn(InitSyn::rand()),
            1 => TransportBody::InitAck(InitAck::rand()),
            2 => TransportBody::OpenSyn(OpenSyn::rand()),
            3 => TransportBody::OpenAck(OpenAck::rand()),
            // 4 => TransportBody::Join(Join::rand()),
            4 => TransportBody::Close(Close::rand()),
            5 => TransportBody::KeepAlive(KeepAlive::rand()),
            6 => TransportBody::Frame(Frame::rand()),
            7 => TransportBody::Fragment(Fragment::rand()),
            _ => unreachable!(),
        };

        Self { body }
    }
}

impl From<TransportBody> for TransportMessage {
    fn from(body: TransportBody) -> Self {
        Self {
            body,
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}

impl From<InitSyn> for TransportMessage {
    fn from(init_syn: InitSyn) -> Self {
        TransportBody::InitSyn(init_syn).into()
    }
}

impl From<InitAck> for TransportMessage {
    fn from(init_ack: InitAck) -> Self {
        TransportBody::InitAck(init_ack).into()
    }
}

impl From<OpenSyn> for TransportMessage {
    fn from(open_syn: OpenSyn) -> Self {
        TransportBody::OpenSyn(open_syn).into()
    }
}

impl From<OpenAck> for TransportMessage {
    fn from(open_ack: OpenAck) -> Self {
        TransportBody::OpenAck(open_ack).into()
    }
}

// impl From<Join> for TransportMessage {
//     fn from(join: Join) -> Self {
//         TransportBody::Join(join).into()
//     }
// }

impl From<Close> for TransportMessage {
    fn from(close: Close) -> Self {
        TransportBody::Close(close).into()
    }
}

impl From<KeepAlive> for TransportMessage {
    fn from(keep_alive: KeepAlive) -> Self {
        TransportBody::KeepAlive(keep_alive).into()
    }
}

impl From<Frame> for TransportMessage {
    fn from(frame: Frame) -> Self {
        TransportBody::Frame(frame).into()
    }
}

impl From<Fragment> for TransportMessage {
    fn from(fragment: Fragment) -> Self {
        TransportBody::Fragment(fragment).into()
    }
}
