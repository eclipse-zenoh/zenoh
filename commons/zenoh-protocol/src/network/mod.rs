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
pub mod declare;
pub mod oam;
pub mod push;
pub mod request;
pub mod response;

pub use declare::*;
pub use oam::*;
pub use push::*;
pub use request::*;
pub use response::*;

pub mod id {
    // WARNING: it's crucial that these IDs do NOT collide with the IDs
    //          defined in `crate::transport::id`.
    pub const OAM: u8 = 0x1f;
    pub const DECLARE: u8 = 0x1e;
    pub const PUSH: u8 = 0x1d;
    pub const REQUEST: u8 = 0x1c;
    pub const RESPONSE: u8 = 0x1b;
    pub const RESPONSE_FINAL: u8 = 0x1a;
}

#[repr(u8)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Mapping {
    #[default]
    Receiver = 0,
    Sender = 1,
}

impl Mapping {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        if rng.gen_bool(0.5) {
            Mapping::Sender
        } else {
            Mapping::Receiver
        }
    }
}

// Zenoh messages at zenoh-network level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkBody {
    Declare(Declare),
    Push(Push),
    Request(Request),
    Response(Response),
    ResponseFinal(ResponseFinal),
    OAM(Oam),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMessage {
    pub body: NetworkBody,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl NetworkMessage {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let body = match rng.gen_range(0..6) {
            0 => NetworkBody::Declare(Declare::rand()),
            1 => NetworkBody::Push(Push::rand()),
            2 => NetworkBody::Request(Request::rand()),
            3 => NetworkBody::Response(Response::rand()),
            4 => NetworkBody::ResponseFinal(ResponseFinal::rand()),
            5 => NetworkBody::OAM(Oam::rand()),
            _ => unreachable!(),
        };

        Self { body }
    }
}

impl From<NetworkBody> for NetworkMessage {
    fn from(body: NetworkBody) -> Self {
        Self {
            body,
            #[cfg(feature = "stats")]
            size: None,
        }
    }
}

impl From<Declare> for NetworkMessage {
    fn from(declare: Declare) -> Self {
        NetworkBody::Declare(declare).into()
    }
}

impl From<Push> for NetworkMessage {
    fn from(push: Push) -> Self {
        NetworkBody::Push(push).into()
    }
}

impl From<Request> for NetworkMessage {
    fn from(request: Request) -> Self {
        NetworkBody::Request(request).into()
    }
}

impl From<Response> for NetworkMessage {
    fn from(response: Response) -> Self {
        NetworkBody::Response(response).into()
    }
}

impl From<ResponseFinal> for NetworkMessage {
    fn from(final_response: ResponseFinal) -> Self {
        NetworkBody::ResponseFinal(final_response).into()
    }
}

// Extensions
pub mod ext {
    use crate::{
        common::{imsg, ZExtZ64},
        core::{CongestionControl, Priority},
    };
    use core::fmt;

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|0_1|    ID   |
    /// +-+-+-+---------+
    /// %0|rsv|E|D|prio %
    /// +---------------+
    ///
    /// - prio: Priority class
    /// - D:    Don't drop. Don't drop the message for congestion control.
    /// - E:    Express. Don't batch this message.
    /// - rsv:  Reserved
    /// ```
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct QoSType<const ID: u8> {
        inner: u8,
    }

    impl<const ID: u8> QoSType<{ ID }> {
        const P_MASK: u8 = 0b00000111;
        const D_FLAG: u8 = 0b00001000;
        const E_FLAG: u8 = 0b00010000;

        pub const fn new(
            priority: Priority,
            congestion_control: CongestionControl,
            is_express: bool,
        ) -> Self {
            let mut inner = priority as u8;
            if let CongestionControl::Block = congestion_control {
                inner |= Self::D_FLAG;
            }
            if is_express {
                inner |= Self::E_FLAG;
            }
            Self { inner }
        }

        pub const fn priority(&self) -> Priority {
            unsafe { core::mem::transmute(self.inner & Self::P_MASK) }
        }

        pub const fn congestion_control(&self) -> CongestionControl {
            match imsg::has_flag(self.inner, Self::D_FLAG) {
                true => CongestionControl::Block,
                false => CongestionControl::Drop,
            }
        }

        pub const fn is_express(&self) -> bool {
            imsg::has_flag(self.inner, Self::E_FLAG)
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
            Self::new(Priority::default(), CongestionControl::default(), false)
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

    impl<const ID: u8> fmt::Debug for QoSType<{ ID }> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.debug_struct("QoS")
                .field("priority", &self.priority())
                .field("congestion", &self.congestion_control())
                .field("express", &self.is_express())
                .finish()
        }
    }

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|1_0|    ID   |
    /// +-+-+-+---------+
    /// ~ ts: <u8;z16>  ~
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TimestampType<const ID: u8> {
        pub timestamp: uhlc::Timestamp,
    }

    impl<const ID: u8> TimestampType<{ ID }> {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use crate::core::ZenohId;
            use core::convert::TryFrom;
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let time = uhlc::NTP64(rng.gen());
            let id = uhlc::ID::try_from(ZenohId::rand().as_slice()).unwrap();
            let timestamp = uhlc::Timestamp::new(time, id);
            Self { timestamp }
        }
    }

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|0_1|    ID   |
    /// +-+-+-+---------+
    /// % source_id     %
    /// +---------------+
    /// ```
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodeIdType<const ID: u8> {
        pub node_id: u16,
    }

    impl<const ID: u8> NodeIdType<{ ID }> {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let node_id = rng.gen_range(1..20);
            Self { node_id }
        }
    }

    impl<const ID: u8> From<ZExtZ64<{ ID }>> for NodeIdType<{ ID }> {
        fn from(ext: ZExtZ64<{ ID }>) -> Self {
            Self {
                node_id: ext.value as u16,
            }
        }
    }

    impl<const ID: u8> From<NodeIdType<{ ID }>> for ZExtZ64<{ ID }> {
        fn from(ext: NodeIdType<{ ID }>) -> Self {
            ZExtZ64::new(ext.node_id as u64)
        }
    }
}
