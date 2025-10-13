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

use crate::core::{CongestionControl, Priority, Reliability, WireExpr};

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
    #[doc(hidden)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NetworkBodyRef<'a> {
    Push(&'a Push),
    Request(&'a Request),
    Response(&'a Response),
    ResponseFinal(&'a ResponseFinal),
    Interest(&'a Interest),
    Declare(&'a Declare),
    OAM(&'a Oam),
}

#[derive(Debug, PartialEq, Eq)]
pub enum NetworkBodyMut<'a> {
    Push(&'a mut Push),
    Request(&'a mut Request),
    Response(&'a mut Response),
    ResponseFinal(&'a mut ResponseFinal),
    Interest(&'a mut Interest),
    Declare(&'a mut Declare),
    OAM(&'a mut Oam),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMessage {
    pub body: NetworkBody,
    pub reliability: Reliability,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct NetworkMessageRef<'a> {
    pub body: NetworkBodyRef<'a>,
    pub reliability: Reliability,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NetworkMessageMut<'a> {
    pub body: NetworkBodyMut<'a>,
    pub reliability: Reliability,
}

pub trait NetworkMessageExt {
    #[doc(hidden)]
    fn body(&self) -> NetworkBodyRef<'_>;

    #[doc(hidden)]
    fn reliability(&self) -> Reliability;

    #[inline]
    fn is_reliable(&self) -> bool {
        self.reliability() == Reliability::Reliable
    }

    #[inline]
    fn is_express(&self) -> bool {
        match self.body() {
            NetworkBodyRef::Push(msg) => msg.ext_qos.is_express(),
            NetworkBodyRef::Request(msg) => msg.ext_qos.is_express(),
            NetworkBodyRef::Response(msg) => msg.ext_qos.is_express(),
            NetworkBodyRef::ResponseFinal(msg) => msg.ext_qos.is_express(),
            NetworkBodyRef::Interest(msg) => msg.ext_qos.is_express(),
            NetworkBodyRef::Declare(msg) => msg.ext_qos.is_express(),
            NetworkBodyRef::OAM(msg) => msg.ext_qos.is_express(),
        }
    }

    #[inline]
    fn congestion_control(&self) -> CongestionControl {
        match self.body() {
            NetworkBodyRef::Push(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBodyRef::Request(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBodyRef::Response(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBodyRef::ResponseFinal(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBodyRef::Interest(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBodyRef::Declare(msg) => msg.ext_qos.get_congestion_control(),
            NetworkBodyRef::OAM(msg) => msg.ext_qos.get_congestion_control(),
        }
    }

    #[inline]
    #[cfg(feature = "shared-memory")]
    fn is_shm(&self) -> bool {
        use crate::zenoh::{PushBody, RequestBody, ResponseBody};

        match self.body() {
            NetworkBodyRef::Push(Push { payload, .. }) => match payload {
                PushBody::Put(p) => p.ext_shm.is_some(),
                PushBody::Del(_) => false,
            },
            NetworkBodyRef::Request(Request { payload, .. }) => match payload {
                RequestBody::Query(b) => b.ext_body.as_ref().is_some_and(|b| b.ext_shm.is_some()),
            },
            NetworkBodyRef::Response(Response { payload, .. }) => match payload {
                ResponseBody::Reply(b) => match &b.payload {
                    PushBody::Put(p) => p.ext_shm.is_some(),
                    PushBody::Del(_) => false,
                },
                ResponseBody::Err(e) => e.ext_shm.is_some(),
            },
            NetworkBodyRef::ResponseFinal(_)
            | NetworkBodyRef::Interest(_)
            | NetworkBodyRef::Declare(_)
            | NetworkBodyRef::OAM(_) => false,
        }
    }

    #[inline]
    fn is_droppable(&self) -> bool {
        !self.is_reliable() || self.congestion_control() == CongestionControl::Drop
    }

    #[inline]
    fn priority(&self) -> Priority {
        match self.body() {
            NetworkBodyRef::Push(msg) => msg.ext_qos.get_priority(),
            NetworkBodyRef::Request(msg) => msg.ext_qos.get_priority(),
            NetworkBodyRef::Response(msg) => msg.ext_qos.get_priority(),
            NetworkBodyRef::ResponseFinal(msg) => msg.ext_qos.get_priority(),
            NetworkBodyRef::Interest(msg) => msg.ext_qos.get_priority(),
            NetworkBodyRef::Declare(msg) => msg.ext_qos.get_priority(),
            NetworkBodyRef::OAM(msg) => msg.ext_qos.get_priority(),
        }
    }

    #[inline]
    fn wire_expr(&self) -> Option<&WireExpr<'_>> {
        match &self.body() {
            NetworkBodyRef::Push(m) => Some(&m.wire_expr),
            NetworkBodyRef::Request(m) => Some(&m.wire_expr),
            NetworkBodyRef::Response(m) => Some(&m.wire_expr),
            NetworkBodyRef::ResponseFinal(_) => None,
            NetworkBodyRef::Interest(m) => m.wire_expr.as_ref(),
            NetworkBodyRef::Declare(m) => match &m.body {
                DeclareBody::DeclareKeyExpr(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareKeyExpr(_) => None,
                DeclareBody::DeclareSubscriber(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareSubscriber(m) => Some(&m.ext_wire_expr.wire_expr),
                DeclareBody::DeclareQueryable(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareQueryable(m) => Some(&m.ext_wire_expr.wire_expr),
                DeclareBody::DeclareToken(m) => Some(&m.wire_expr),
                DeclareBody::UndeclareToken(m) => Some(&m.ext_wire_expr.wire_expr),
                DeclareBody::DeclareFinal(_) => None,
            },
            NetworkBodyRef::OAM(_) => None,
        }
    }

    #[inline]
    fn as_ref(&self) -> NetworkMessageRef<'_> {
        NetworkMessageRef {
            body: self.body(),
            reliability: self.reliability(),
        }
    }

    #[inline]
    fn to_owned(&self) -> NetworkMessage {
        NetworkMessage {
            body: match self.body() {
                NetworkBodyRef::Push(msg) => NetworkBody::Push(msg.clone()),
                NetworkBodyRef::Request(msg) => NetworkBody::Request(msg.clone()),
                NetworkBodyRef::Response(msg) => NetworkBody::Response(msg.clone()),
                NetworkBodyRef::ResponseFinal(msg) => NetworkBody::ResponseFinal(msg.clone()),
                NetworkBodyRef::Interest(msg) => NetworkBody::Interest(msg.clone()),
                NetworkBodyRef::Declare(msg) => NetworkBody::Declare(msg.clone()),
                NetworkBodyRef::OAM(msg) => NetworkBody::OAM(msg.clone()),
            },
            reliability: self.reliability(),
        }
    }
}

impl NetworkMessageExt for NetworkMessage {
    fn body(&self) -> NetworkBodyRef<'_> {
        match &self.body {
            NetworkBody::Push(body) => NetworkBodyRef::Push(body),
            NetworkBody::Request(body) => NetworkBodyRef::Request(body),
            NetworkBody::Response(body) => NetworkBodyRef::Response(body),
            NetworkBody::ResponseFinal(body) => NetworkBodyRef::ResponseFinal(body),
            NetworkBody::Interest(body) => NetworkBodyRef::Interest(body),
            NetworkBody::Declare(body) => NetworkBodyRef::Declare(body),
            NetworkBody::OAM(body) => NetworkBodyRef::OAM(body),
        }
    }

    fn reliability(&self) -> Reliability {
        self.reliability
    }
}

impl NetworkMessageExt for NetworkMessageRef<'_> {
    fn body(&self) -> NetworkBodyRef<'_> {
        self.body
    }

    fn reliability(&self) -> Reliability {
        self.reliability
    }
}

impl NetworkMessageExt for NetworkMessageMut<'_> {
    fn body(&self) -> NetworkBodyRef<'_> {
        match &self.body {
            NetworkBodyMut::Push(body) => NetworkBodyRef::Push(body),
            NetworkBodyMut::Request(body) => NetworkBodyRef::Request(body),
            NetworkBodyMut::Response(body) => NetworkBodyRef::Response(body),
            NetworkBodyMut::ResponseFinal(body) => NetworkBodyRef::ResponseFinal(body),
            NetworkBodyMut::Interest(body) => NetworkBodyRef::Interest(body),
            NetworkBodyMut::Declare(body) => NetworkBodyRef::Declare(body),
            NetworkBodyMut::OAM(body) => NetworkBodyRef::OAM(body),
        }
    }

    fn reliability(&self) -> Reliability {
        self.reliability
    }
}

impl NetworkMessage {
    #[cfg(feature = "test")]
    #[doc(hidden)]
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
    pub fn as_mut(&mut self) -> NetworkMessageMut<'_> {
        let body = match &mut self.body {
            NetworkBody::Push(body) => NetworkBodyMut::Push(body),
            NetworkBody::Request(body) => NetworkBodyMut::Request(body),
            NetworkBody::Response(body) => NetworkBodyMut::Response(body),
            NetworkBody::ResponseFinal(body) => NetworkBodyMut::ResponseFinal(body),
            NetworkBody::Interest(body) => NetworkBodyMut::Interest(body),
            NetworkBody::Declare(body) => NetworkBodyMut::Declare(body),
            NetworkBody::OAM(body) => NetworkBodyMut::OAM(body),
        };
        NetworkMessageMut {
            body,
            reliability: self.reliability,
        }
    }
}

impl NetworkMessageMut<'_> {
    #[inline]
    pub fn as_mut(&mut self) -> NetworkMessageMut<'_> {
        let body = match &mut self.body {
            NetworkBodyMut::Push(body) => NetworkBodyMut::Push(body),
            NetworkBodyMut::Request(body) => NetworkBodyMut::Request(body),
            NetworkBodyMut::Response(body) => NetworkBodyMut::Response(body),
            NetworkBodyMut::ResponseFinal(body) => NetworkBodyMut::ResponseFinal(body),
            NetworkBodyMut::Interest(body) => NetworkBodyMut::Interest(body),
            NetworkBodyMut::Declare(body) => NetworkBodyMut::Declare(body),
            NetworkBodyMut::OAM(body) => NetworkBodyMut::OAM(body),
        };
        NetworkMessageMut {
            body,
            reliability: self.reliability,
        }
    }
}

impl fmt::Display for NetworkMessageRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.body {
            NetworkBodyRef::OAM(_) => write!(f, "OAM"),
            NetworkBodyRef::Push(_) => write!(f, "Push"),
            NetworkBodyRef::Request(_) => write!(f, "Request"),
            NetworkBodyRef::Response(_) => write!(f, "Response"),
            NetworkBodyRef::ResponseFinal(_) => write!(f, "ResponseFinal"),
            NetworkBodyRef::Interest(_) => write!(f, "Interest"),
            NetworkBodyRef::Declare(_) => write!(f, "Declare"),
        }
    }
}

impl fmt::Display for NetworkMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Display for NetworkMessageMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl From<NetworkBody> for NetworkMessage {
    #[inline]
    fn from(body: NetworkBody) -> Self {
        Self {
            body,
            reliability: Reliability::DEFAULT,
        }
    }
}

#[cfg(feature = "test")]
impl From<Push> for NetworkMessage {
    fn from(push: Push) -> Self {
        NetworkBody::Push(push).into()
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
    /// %0|r|F|E|D|prio %
    /// +---------------+
    ///
    /// - prio: Priority class
    /// - D:    Don't drop. Don't drop the message for congestion control.
    /// - E:    Express. Don't batch this message.
    /// - F:    Don't drop the first message for congestion control.
    /// - r:  Reserved
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
        const F_FLAG: u8 = 0b00100000;

        pub const DEFAULT: Self = Self::new(Priority::DEFAULT, CongestionControl::DEFAULT, false);

        pub const DECLARE: Self =
            Self::new(Priority::Control, CongestionControl::DEFAULT_DECLARE, false);
        pub const PUSH: Self = Self::new(Priority::DEFAULT, CongestionControl::DEFAULT_PUSH, false);
        pub const REQUEST: Self =
            Self::new(Priority::DEFAULT, CongestionControl::DEFAULT_REQUEST, false);
        pub const RESPONSE: Self = Self::new(
            Priority::DEFAULT,
            CongestionControl::DEFAULT_RESPONSE,
            false,
        );
        pub const RESPONSE_FINAL: Self = Self::new(
            Priority::DEFAULT,
            CongestionControl::DEFAULT_RESPONSE,
            false,
        );
        pub const OAM: Self = Self::new(Priority::Control, CongestionControl::DEFAULT_OAM, false);

        pub const fn new(
            priority: Priority,
            congestion_control: CongestionControl,
            is_express: bool,
        ) -> Self {
            let mut inner = priority as u8;
            match congestion_control {
                CongestionControl::Block => inner |= Self::D_FLAG,
                #[cfg(feature = "unstable")]
                CongestionControl::BlockFirst => inner |= Self::F_FLAG,
                _ => {}
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
                CongestionControl::Block => {
                    self.inner = imsg::set_flag(self.inner, Self::D_FLAG);
                    self.inner = imsg::unset_flag(self.inner, Self::F_FLAG);
                }
                CongestionControl::Drop => {
                    self.inner = imsg::unset_flag(self.inner, Self::D_FLAG);
                    self.inner = imsg::unset_flag(self.inner, Self::F_FLAG);
                }
                #[cfg(feature = "unstable")]
                CongestionControl::BlockFirst => {
                    self.inner = imsg::unset_flag(self.inner, Self::D_FLAG);
                    self.inner = imsg::set_flag(self.inner, Self::F_FLAG);
                }
            }
        }

        pub const fn get_congestion_control(&self) -> CongestionControl {
            match (
                imsg::has_flag(self.inner, Self::D_FLAG),
                imsg::has_flag(self.inner, Self::F_FLAG),
            ) {
                (false, false) => CongestionControl::Drop,
                #[cfg(feature = "unstable")]
                (false, true) => CongestionControl::BlockFirst,
                #[cfg(not(feature = "unstable"))]
                (false, true) => CongestionControl::Drop,
                (true, _) => CongestionControl::Block,
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
        #[doc(hidden)]
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
        #[doc(hidden)]
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
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let zid = ZenohIdProto::rand();
            let eid: EntityId = rng.gen();
            Self { zid, eid }
        }
    }
}
