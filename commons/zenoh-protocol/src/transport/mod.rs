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
pub mod close;
pub mod fragment;
pub mod frame;
pub mod init;
pub mod join;
pub mod keepalive;
pub mod oam;
pub mod open;

pub use close::Close;
pub use fragment::{Fragment, FragmentHeader};
pub use frame::{Frame, FrameHeader};
pub use init::{InitAck, InitSyn};
pub use join::Join;
pub use keepalive::KeepAlive;
pub use oam::Oam;
pub use open::{OpenAck, OpenSyn};

use crate::network::NetworkMessage;

/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
pub type BatchSize = u16;

pub mod batch_size {
    use super::BatchSize;

    pub const UNICAST: BatchSize = BatchSize::MAX;
    pub const MULTICAST: BatchSize = 8_192;
}

pub mod id {
    // WARNING: it's crucial that these IDs do NOT collide with the IDs
    //          defined in `crate::network::id`.
    pub const OAM: u8 = 0x00;
    pub const INIT: u8 = 0x01; // For unicast communications only
    pub const OPEN: u8 = 0x02; // For unicast communications only
    pub const CLOSE: u8 = 0x03;
    pub const KEEP_ALIVE: u8 = 0x04;
    pub const FRAME: u8 = 0x05;
    pub const FRAGMENT: u8 = 0x06;
    pub const JOIN: u8 = 0x07; // For multicast communications only
}

#[derive(Debug)]
pub struct TransportMessageLowLatency {
    pub body: TransportBodyLowLatency,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum TransportBodyLowLatency {
    Close(Close),
    KeepAlive(KeepAlive),
    Network(NetworkMessage),
}

pub type TransportSn = u32;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct PrioritySn {
    pub reliable: TransportSn,
    pub best_effort: TransportSn,
}

impl PrioritySn {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        Self {
            reliable: rng.gen(),
            best_effort: rng.gen(),
        }
    }
}

// Zenoh messages at zenoh-transport level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportBody {
    InitSyn(InitSyn),
    InitAck(InitAck),
    OpenSyn(OpenSyn),
    OpenAck(OpenAck),
    Close(Close),
    KeepAlive(KeepAlive),
    Frame(Frame),
    Fragment(Fragment),
    OAM(Oam),
    Join(Join),
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

        let body = match rng.gen_range(0..10) {
            0 => TransportBody::InitSyn(InitSyn::rand()),
            1 => TransportBody::InitAck(InitAck::rand()),
            2 => TransportBody::OpenSyn(OpenSyn::rand()),
            3 => TransportBody::OpenAck(OpenAck::rand()),
            4 => TransportBody::Close(Close::rand()),
            5 => TransportBody::KeepAlive(KeepAlive::rand()),
            6 => TransportBody::Frame(Frame::rand()),
            7 => TransportBody::Fragment(Fragment::rand()),
            8 => TransportBody::OAM(Oam::rand()),
            9 => TransportBody::Join(Join::rand()),
            _ => unreachable!(),
        };

        Self {
            body,
            #[cfg(feature = "stats")]
            size: None,
        }
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

impl From<Join> for TransportMessage {
    fn from(join: Join) -> Self {
        TransportBody::Join(join).into()
    }
}

pub mod ext {
    use crate::{common::ZExtZ64, core::Priority};

    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// %0|  rsv  |prio %
    /// +---------------+
    /// - prio: Priority class
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct QoSType<const ID: u8> {
        inner: u8,
    }

    impl<const ID: u8> QoSType<{ ID }> {
        pub const P_MASK: u8 = 0b00000111;

        pub const fn new(priority: Priority) -> Self {
            Self {
                inner: priority as u8,
            }
        }

        pub const fn priority(&self) -> Priority {
            unsafe { core::mem::transmute(self.inner & Self::P_MASK) }
        }

        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let inner: u8 = rng.gen();
            Self { inner }
        }
    }

    impl<const ID: u8> Default for QoSType<{ ID }> {
        fn default() -> Self {
            Self::new(Priority::default())
        }
    }

    impl<const ID: u8> From<ZExtZ64<{ ID }>> for QoSType<{ ID }> {
        fn from(ext: ZExtZ64<{ ID }>) -> Self {
            Self {
                inner: ext.value as u8,
            }
        }
    }

    impl<const ID: u8> From<QoSType<{ ID }>> for ZExtZ64<{ ID }> {
        fn from(ext: QoSType<{ ID }>) -> Self {
            ZExtZ64::new(ext.inner as u64)
        }
    }
}
