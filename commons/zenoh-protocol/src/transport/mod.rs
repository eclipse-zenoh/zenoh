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

use core::fmt;

pub use close::Close;
pub use fragment::{Fragment, FragmentHeader};
pub use frame::{Frame, FrameHeader};
pub use init::{InitAck, InitSyn};
pub use join::Join;
pub use keepalive::KeepAlive;
pub use oam::Oam;
pub use open::{OpenAck, OpenSyn};

use crate::network::{NetworkMessage, NetworkMessageRef};

/// NOTE: 16 bits (2 bytes) may be prepended to the serialized message indicating the total length
///       in bytes of the message, resulting in the maximum length of a message being 65_535 bytes.
///       This is necessary in those stream-oriented transports (e.g., TCP) that do not preserve
///       the boundary of the serialized messages. The length is encoded as little-endian.
///       In any case, the length of a message must not exceed 65_535 bytes.
pub type BatchSize = u16;
pub type AtomicBatchSize = core::sync::atomic::AtomicU16;

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

#[derive(Debug, Clone, Copy)]
pub struct TransportMessageLowLatencyRef<'a> {
    pub body: TransportBodyLowLatencyRef<'a>,
}

impl TryFrom<NetworkMessage> for TransportMessageLowLatency {
    type Error = zenoh_result::Error;
    fn try_from(msg: NetworkMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            body: TransportBodyLowLatency::Network(msg),
        })
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum TransportBodyLowLatency {
    Close(Close),
    KeepAlive(KeepAlive),
    Network(NetworkMessage),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Copy)]
pub enum TransportBodyLowLatencyRef<'a> {
    Close(Close),
    KeepAlive(KeepAlive),
    Network(NetworkMessageRef<'a>),
}

pub type TransportSn = u32;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PrioritySn {
    pub reliable: TransportSn,
    pub best_effort: TransportSn,
}

impl PrioritySn {
    pub const DEFAULT: Self = Self {
        reliable: TransportSn::MIN,
        best_effort: TransportSn::MIN,
    };

    #[cfg(feature = "test")]
    #[doc(hidden)]
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
}

impl TransportMessage {
    #[cfg(feature = "test")]
    #[doc(hidden)]
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

        Self { body }
    }
}

impl From<TransportBody> for TransportMessage {
    fn from(body: TransportBody) -> Self {
        Self { body }
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

impl fmt::Display for TransportMessage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use TransportBody::*;
        match &self.body {
            OAM(_) => write!(f, "OAM"),
            InitSyn(_) => write!(f, "InitSyn"),
            InitAck(_) => write!(f, "InitAck"),
            OpenSyn(_) => write!(f, "OpenSyn"),
            OpenAck(_) => write!(f, "OpenAck"),
            Close(_) => write!(f, "Close"),
            KeepAlive(_) => write!(f, "KeepAlive"),
            Frame(m) => {
                write!(f, "Frame[")?;
                let mut netmsgs = m.payload.iter().peekable();
                while let Some(m) = netmsgs.next() {
                    m.fmt(f)?;
                    if netmsgs.peek().is_some() {
                        write!(f, ", ")?;
                    }
                }
                write!(f, "]")
            }
            Fragment(_) => write!(f, "Fragment"),
            Join(_) => write!(f, "Join"),
        }
    }
}

pub mod ext {
    use crate::{common::ZExtZ64, core::Priority};

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// %0|  rsv  |prio %
    /// +---------------+
    /// - prio: Priority class
    /// ```
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct QoSType<const ID: u8> {
        inner: u8,
    }

    impl<const ID: u8> QoSType<{ ID }> {
        const P_MASK: u8 = 0b00000111;
        pub const DEFAULT: Self = Self::new(Priority::DEFAULT);

        pub const fn new(priority: Priority) -> Self {
            Self {
                inner: priority as u8,
            }
        }

        pub const fn priority(&self) -> Priority {
            unsafe { core::mem::transmute(self.inner & Self::P_MASK) }
        }

        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let inner: u8 = rng.gen();
            Self { inner }
        }
    }

    impl<const ID: u8> Default for QoSType<{ ID }> {
        fn default() -> Self {
            Self::DEFAULT
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct PatchType<const ID: u8>(u8);

    impl<const ID: u8> PatchType<ID> {
        pub const NONE: Self = Self(0);
        pub const CURRENT: Self = Self(1);

        pub fn new(int: u8) -> Self {
            Self(int)
        }

        pub fn raw(self) -> u8 {
            self.0
        }

        pub fn has_fragmentation_markers(&self) -> bool {
            self.0 >= 1
        }

        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            Self(rand::thread_rng().gen())
        }
    }

    impl<const ID: u8> From<ZExtZ64<ID>> for PatchType<ID> {
        fn from(ext: ZExtZ64<ID>) -> Self {
            Self(ext.value as u8)
        }
    }

    impl<const ID: u8> From<PatchType<ID>> for ZExtZ64<ID> {
        fn from(ext: PatchType<ID>) -> Self {
            ZExtZ64::new(ext.0 as u64)
        }
    }
}
