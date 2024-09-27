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
pub mod interest;
pub mod oam;
pub mod push;
pub mod request;
pub mod response;

use core::fmt;

pub use declare::{
    Declare, DeclareBody, DeclareFinal, DeclareKeyExpr, DeclareQueryable, DeclareSubscriber,
    DeclareToken, UndeclareKeyExpr, UndeclareQueryable, UndeclareSubscriber, UndeclareToken,
};
pub use interest::Interest;
pub use oam::Oam;
pub use push::Push;
pub use request::{AtomicRequestId, Request, RequestId};
pub use response::{Response, ResponseFinal};

use crate::core::{CongestionControl, Priority, Reliability};

pub mod id {
    // WARNING: it's crucial that these IDs do NOT collide with the IDs
    //          defined in `crate::transport::id`.
    pub const OAM: u8 = 0x1f;
    pub const DECLARE: u8 = 0x1e;
    pub const PUSH: u8 = 0x1d;
    pub const REQUEST: u8 = 0x1c;
    pub const RESPONSE: u8 = 0x1b;
    pub const RESPONSE_FINAL: u8 = 0x1a;
    pub const INTEREST: u8 = 0x19;
}

#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Mapping {
    #[default]
    Receiver = 0,
    Sender = 1,
}

impl Mapping {
    pub const DEFAULT: Self = Self::Receiver;

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
    Push(Push),
    Request(Request),
    Response(Response),
    ResponseFinal(ResponseFinal),
    Interest(Interest),
    Declare(Declare),
    OAM(Oam),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMessage {
    pub body: NetworkBody,
    pub reliability: Reliability,
    #[cfg(feature = "stats")]
    pub size: Option<core::num::NonZeroUsize>,
}

impl NetworkMessage {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let body = match rng.gen_range(0..6) {
            0 => NetworkBody::Push(Push::rand()),
            1 => NetworkBody::Request(Request::rand()),
            2 => NetworkBody::Response(Response::rand()),
            3 => NetworkBody::ResponseFinal(ResponseFinal::rand()),
            4 => NetworkBody::Declare(Declare::rand()),
            5 => NetworkBody::OAM(Oam::rand()),
            _ => unreachable!(),
        };

        body.into()
    }

    #[inline]
    pub fn is_reliable(&self) -> bool {
        self.reliability == Reliability::Reliable
    }

    #[inline]
    pub fn is_express(&self) -> bool {
        match &self.body {
            NetworkBody::Push(msg) => msg.ext_qos.is_express(),
            NetworkBody::Request(msg) => msg.ext_qos.is_express(),
            NetworkBody::Response(msg) => msg.ext_qos.is_express(),
            NetworkBody::ResponseFinal(msg) => msg.ext_qos.is_express(),
            NetworkBody::Interest(msg) => msg.ext_qos.is_express(),
            NetworkBody::Declare(msg) => msg.ext_qos.is_express(),
            NetworkBody::OAM(msg) => msg.ext_qos.is_express(),
        }
    }

    #[inline]
    pub fn is_droppable(&self) -> bool {
        if !self.is_reliable() {
            return true;
        }

        let cc = match &self.body {
            NetworkBody::Push(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBody::Request(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBody::Response(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBody::ResponseFinal(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBody::Interest(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBody::Declare(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBody::OAM(msg) => msg.ext_qos.get_congestion_control(),
        };

        cc == CongestionControl::Drop
    }

    #[inline]
    pub fn priority(&self) -> Priority {
        match &self.body {
            NetworkBody::Push(msg) => msg.ext_qos.get_priority(),
            NetworkBody::Request(msg) => msg.ext_qos.get_priority(),
            NetworkBody::Response(msg) => msg.ext_qos.get_priority(),
            NetworkBody::ResponseFinal(msg) => msg.ext_qos.get_priority(),
            NetworkBody::Interest(msg) => msg.ext_qos.get_priority(),
            NetworkBody::Declare(msg) => msg.ext_qos.get_priority(),
            NetworkBody::OAM(msg) => msg.ext_qos.get_priority(),
        }
    }
}

impl fmt::Display for NetworkMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use NetworkBody::*;
        match &self.body {
            OAM(_) => write!(f, "OAM"),
            Push(_) => write!(f, "Push"),
            Request(_) => write!(f, "Request"),
            Response(_) => write!(f, "Response"),
            ResponseFinal(_) => write!(f, "ResponseFinal"),
            Interest(_) => write!(f, "Interest"),
            Declare(_) => write!(f, "Declare"),
        }
    }
}

impl From<NetworkBody> for NetworkMessage {
    #[inline]
    fn from(body: NetworkBody) -> Self {
        Self {
            body,
            reliability: Reliability::DEFAULT,
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
    use core::fmt;

    use crate::{
        common::{imsg, ZExtZ64},
        core::{CongestionControl, EntityId, Priority, ZenohIdProto},
    };

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

        pub const DEFAULT: Self = Self::new(Priority::DEFAULT, CongestionControl::DEFAULT, false);

        pub const DECLARE: Self = Self::new(Priority::Control, CongestionControl::Block, false);
        pub const PUSH: Self = Self::new(Priority::DEFAULT, CongestionControl::Drop, false);
        pub const REQUEST: Self = Self::new(Priority::DEFAULT, CongestionControl::Block, false);
        pub const RESPONSE: Self = Self::new(Priority::DEFAULT, CongestionControl::Block, false);
        pub const RESPONSE_FINAL: Self =
            Self::new(Priority::DEFAULT, CongestionControl::Block, false);
        pub const OAM: Self = Self::new(Priority::Control, CongestionControl::Block, false);

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

        pub fn set_priority(&mut self, priority: Priority) {
            self.inner = imsg::set_bitfield(self.inner, priority as u8, Self::P_MASK);
        }

        pub const fn get_priority(&self) -> Priority {
            unsafe { core::mem::transmute(self.inner & Self::P_MASK) }
        }

        pub fn set_congestion_control(&mut self, cctrl: CongestionControl) {
            match cctrl {
                CongestionControl::Block => self.inner = imsg::set_flag(self.inner, Self::D_FLAG),
                CongestionControl::Drop => self.inner = imsg::unset_flag(self.inner, Self::D_FLAG),
            }
        }

        pub const fn get_congestion_control(&self) -> CongestionControl {
            match imsg::has_flag(self.inner, Self::D_FLAG) {
                true => CongestionControl::Block,
                false => CongestionControl::Drop,
            }
        }

        pub fn set_is_express(&mut self, is_express: bool) {
            match is_express {
                true => self.inner = imsg::set_flag(self.inner, Self::E_FLAG),
                false => self.inner = imsg::unset_flag(self.inner, Self::E_FLAG),
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
            Self::new(Priority::DEFAULT, CongestionControl::DEFAULT, false)
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
                .field("priority", &self.get_priority())
                .field("congestion", &self.get_congestion_control())
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
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let time = uhlc::NTP64(rng.gen());
            let id = uhlc::ID::try_from(ZenohIdProto::rand().to_le_bytes()).unwrap();
            let timestamp = uhlc::Timestamp::new(time, id);
            Self { timestamp }
        }
    }

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|0_1|    ID   |
    /// +-+-+-+---------+
    /// %    node_id    %
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodeIdType<const ID: u8> {
        pub node_id: u16,
    }

    impl<const ID: u8> NodeIdType<{ ID }> {
        // node_id == 0 means the message has been generated by the node itself
        pub const DEFAULT: Self = Self { node_id: 0 };

        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let node_id = rng.gen();
            Self { node_id }
        }
    }

    impl<const ID: u8> Default for NodeIdType<{ ID }> {
        fn default() -> Self {
            Self::DEFAULT
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

    /// ```text
    ///  7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |zid_len|X|X|X|X|
    /// +-------+-+-+---+
    /// ~      zid      ~
    /// +---------------+
    /// %      eid      %
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EntityGlobalIdType<const ID: u8> {
        pub zid: ZenohIdProto,
        pub eid: EntityId,
    }

    impl<const ID: u8> EntityGlobalIdType<{ ID }> {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let zid = ZenohIdProto::rand();
            let eid: EntityId = rng.gen();
            Self { zid, eid }
        }
    }
}
