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
use crate::{core::WireExpr, network::RequestId, zenoh::ZenohMessage};

/// # Response message
///
/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N| ResMore |
/// +-+-+-+---------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ~      rid      ~
/// +---------------+
/// ~  [reply_exts] ~  if Z==1
/// +---------------+
/// ~  ZenohMessage ~ -- Payload
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub wire_expr: WireExpr<'static>,
    pub rid: RequestId,
    pub payload: ZenohMessage,
    pub ext_qos: ext::QoS,
    pub ext_tstamp: Option<ext::Timestamp>,
}

pub mod ext {
    pub const QOS: u8 = crate::network::ext::QOS;
    pub const TSTAMP: u8 = crate::network::ext::TSTAMP;
    pub const TARGET: u8 = 0x03;

    pub type QoS = crate::network::ext::QoS;
    pub type Timestamp = crate::network::ext::Timestamp;
}

impl Response {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let wire_expr = WireExpr::rand();
        let rid: RequestId = rng.gen();
        let payload = ZenohMessage::rand();
        let ext_qos = ext::QoS::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::Timestamp::rand);

        Self {
            wire_expr,
            rid,
            payload,
            ext_qos,
            ext_tstamp,
        }
    }
}

/// # ResponseFinal message
///
/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N| ResFinal|
/// +-+-+-+---------+
/// ~      rid      ~
/// +---------------+
/// ~  [reply_exts] ~  if Z==1
/// +---------------+
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseFinal {
    pub rid: RequestId,
    pub ext_qos: ext::QoS,
    pub ext_tstamp: Option<ext::Timestamp>,
}

impl ResponseFinal {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let rid: RequestId = rng.gen();
        let ext_qos = ext::QoS::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::Timestamp::rand);

        Self {
            rid,
            ext_qos,
            ext_tstamp,
        }
    }
}
