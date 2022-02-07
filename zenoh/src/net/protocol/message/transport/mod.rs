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
mod acknack;
mod close;
mod fragment;
mod frame;
mod init;
mod join;
mod keepalive;
mod open;
mod sync;

use super::flag as iflag;
use super::ZMessage;
use crate::net::protocol::io::{WBuf, ZBuf};
pub use acknack::*;
pub use close::*;
pub use fragment::*;
pub use frame::*;
pub use init::*;
pub use join::*;
pub use keepalive::*;
pub use open::*;
use std::convert::TryFrom;
#[cfg(feature = "stats")]
use std::num::NonZeroUsize;
pub use sync::*;

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
    Close = 0x04,
    KeepAlive = 0x05,
    Sync = 0x06,
    AckNack = 0x07,
    Frame = 0x08,
    Fragment = 0x09,
    // 0x05: Reserved
    // ..  : Reserved
    // 0x1f: Reserved
}

impl TransportId {
    const MIN: u8 = TransportId::Init.id();
    const MAX: u8 = TransportId::Fragment.id();

    const fn id(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for TransportId {
    type Error = ();

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        if (Self::MIN..=Self::MAX).contains(&b) {
            Ok(unsafe { std::mem::transmute(b) })
        } else {
            Err(())
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TransportProto;

// Transport message flags
pub mod flag {
    pub const Z: u8 = super::iflag::Z; // 0x20 MixedSlices   if Z==1 then the payload contains a mix of raw and shm_info payload

    // pub const X: u8 = 0; // Unused flags are set to zero
}

/*************************************/
/*       TRANSPORT MESSAGES          */
/*************************************/
#[derive(Clone, PartialEq, Debug)]
pub enum TransportBody {
    InitSyn(InitSyn),
    InitAck(InitAck),
    OpenSyn(OpenSyn),
    OpenAck(OpenAck),
    Join(Join),
    Close(Close),
    KeepAlive(KeepAlive),
    Sync(Sync),
    AckNack(AckNack),
    Frame(Frame),
    Fragment(Fragment),
}

impl From<InitSyn> for TransportBody {
    fn from(body: InitSyn) -> Self {
        Self::InitSyn(body)
    }
}

impl From<InitAck> for TransportBody {
    fn from(body: InitAck) -> Self {
        Self::InitAck(body)
    }
}

impl From<OpenSyn> for TransportBody {
    fn from(body: OpenSyn) -> Self {
        Self::OpenSyn(body)
    }
}

impl From<OpenAck> for TransportBody {
    fn from(body: OpenAck) -> Self {
        Self::OpenAck(body)
    }
}

impl From<Join> for TransportBody {
    fn from(body: Join) -> Self {
        Self::Join(body)
    }
}

impl From<Close> for TransportBody {
    fn from(body: Close) -> Self {
        Self::Close(body)
    }
}

impl From<KeepAlive> for TransportBody {
    fn from(body: KeepAlive) -> Self {
        Self::KeepAlive(body)
    }
}

impl From<Sync> for TransportBody {
    fn from(body: Sync) -> Self {
        Self::Sync(body)
    }
}

impl From<AckNack> for TransportBody {
    fn from(body: AckNack) -> Self {
        Self::AckNack(body)
    }
}

impl From<Frame> for TransportBody {
    fn from(body: Frame) -> Self {
        Self::Frame(body)
    }
}

impl From<Fragment> for TransportBody {
    fn from(body: Fragment) -> Self {
        Self::Fragment(body)
    }
}

// Zenoh messages at zenoh-transport level
#[derive(Clone, PartialEq, Debug)]
pub struct TransportMessage {
    pub body: TransportBody,
    #[cfg(feature = "stats")]
    pub size: Option<NonZeroUsize>,
}

impl TransportMessage {
    pub fn read(zbuf: &mut ZBuf) -> Option<Self> {
        #[cfg(feature = "stats")]
        let start = self.readable();

        let header = zbuf.read()?;
        let id = TransportId::try_from(super::mid(header)).ok()?;
        let body: TransportBody = match id {
            TransportId::Init => {
                if super::has_flag(header, InitAck::FLAG_A) {
                    InitAck::read(zbuf, header)?.into()
                } else {
                    InitSyn::read(zbuf, header)?.into()
                }
            }
            TransportId::Open => {
                if super::has_flag(header, InitAck::FLAG_A) {
                    OpenAck::read(zbuf, header)?.into()
                } else {
                    OpenSyn::read(zbuf, header)?.into()
                }
            }
            TransportId::Join => Join::read(zbuf, header)?.into(),
            TransportId::Close => Close::read(zbuf, header)?.into(),
            TransportId::KeepAlive => KeepAlive::read(zbuf, header)?.into(),
            TransportId::Sync => Sync::read(zbuf, header)?.into(),
            TransportId::AckNack => AckNack::read(zbuf, header)?.into(),
            TransportId::Frame => Frame::read(zbuf, header)?.into(),
            TransportId::Fragment => Fragment::read(zbuf, header)?.into(),
        };

        #[cfg(feature = "stats")]
        let end = self.readable();

        Some(Self {
            body,
            #[cfg(feature = "stats")]
            size: NonZeroUsize::new(start - end),
        })
    }

    #[allow(clippy::let_and_return)] // Necessary When "stats" feature is disabled
    pub fn write(&mut self, wbuf: &mut WBuf) -> bool {
        #[cfg(feature = "stats")]
        let start = wbuf.len();

        let res = match &self.body {
            TransportBody::InitSyn(msg) => msg.write(wbuf),
            TransportBody::InitAck(msg) => msg.write(wbuf),
            TransportBody::OpenSyn(msg) => msg.write(wbuf),
            TransportBody::OpenAck(msg) => msg.write(wbuf),
            TransportBody::Join(msg) => msg.write(wbuf),
            TransportBody::Close(msg) => msg.write(wbuf),
            TransportBody::KeepAlive(msg) => msg.write(wbuf),
            TransportBody::Sync(msg) => msg.write(wbuf),
            TransportBody::AckNack(msg) => msg.write(wbuf),
            TransportBody::Frame(msg) => msg.write(wbuf),
            TransportBody::Fragment(msg) => msg.write(wbuf),
        };

        #[cfg(feature = "stats")]
        {
            let end = self.len();
            msg.size = NonZeroUsize::new(end - start);
        }

        res
    }
}

impl<T: Into<TransportBody>> From<T> for TransportMessage {
    fn from(body: T) -> Self {
        Self {
            body: body.into(),
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}
