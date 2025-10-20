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
#[cfg(feature = "test")]
use crate::zenoh::Put;
use crate::{core::WireExpr, zenoh::PushBody};

pub mod flag {
    pub const N: u8 = 1 << 5; // 0x20 Named         if N==1 then the key expr has name/suffix
    pub const M: u8 = 1 << 6; // 0x40 Mapping       if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  PUSH   |
/// +-+-+-+---------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ~  [push_exts]  ~  if Z==1
/// +---------------+
/// ~   PushBody    ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    pub wire_expr: WireExpr<'static>,
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
    pub ext_nodeid: ext::NodeIdType,
    pub payload: PushBody,
}

pub mod ext {
    use crate::{
        common::{ZExtZ64, ZExtZBuf},
        zextz64, zextzbuf,
    };

    pub type QoS = zextz64!(0x1, false);
    pub type QoSType = crate::network::ext::QoSType<{ QoS::ID }>;

    pub type Timestamp = zextzbuf!(0x2, false);
    pub type TimestampType = crate::network::ext::TimestampType<{ Timestamp::ID }>;

    pub type NodeId = zextz64!(0x3, true);
    pub type NodeIdType = crate::network::ext::NodeIdType<{ NodeId::ID }>;
}

impl Push {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let wire_expr = WireExpr::rand();
        let payload = PushBody::rand();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_nodeid = ext::NodeIdType::rand();

        Self {
            wire_expr,
            payload,
            ext_tstamp,
            ext_qos,
            ext_nodeid,
        }
    }
}

impl From<PushBody> for Push {
    fn from(value: PushBody) -> Self {
        Self {
            wire_expr: WireExpr::empty(),
            ext_qos: ext::QoSType::DEFAULT,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            payload: value,
        }
    }
}

#[cfg(feature = "test")]
impl From<Put> for Push {
    fn from(value: Put) -> Self {
        PushBody::from(value).into()
    }
}

#[cfg(feature = "test")]
impl From<Vec<u8>> for Push {
    fn from(value: Vec<u8>) -> Self {
        Self::from(Put {
            payload: value.into(),
            ..Put::default()
        })
    }
}
