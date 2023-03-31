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
use crate::{
    core::WireExpr,
    network::{Mapping, RequestId},
    zenoh_new::ResponseBody,
};

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
/// |Z|M|N| Response|
/// +-+-+-+---------+
/// ~ request_id:z32~  (*)
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ~  [reply_exts] ~  if Z==1
/// +---------------+
/// ~  ResponseBody ~ -- Payload
/// +---------------+
///
/// (*) The resolution of the request id is negotiated during the session establishment.
///     This implementation limits the resolution to 32bit.
/// ```
pub mod flag {
    pub const N: u8 = 1 << 5; // 0x20 Named         if N==1 then the key expr has name/suffix
    pub const M: u8 = 1 << 6; // 0x40 Mapping       if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub rid: RequestId,
    pub wire_expr: WireExpr<'static>,
    pub mapping: Mapping,
    pub payload: ResponseBody,
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
}

pub mod ext {
    pub type QoS = crate::network::ext::QoS;
    pub type QoSType = crate::network::ext::QoSType;

    pub type Timestamp = crate::network::ext::Timestamp;
    pub type TimestampType = crate::network::ext::TimestampType;
}

impl Response {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let rid: RequestId = rng.gen();
        let wire_expr = WireExpr::rand();
        let mapping = Mapping::rand();
        let payload = ResponseBody::rand();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);

        Self {
            rid,
            wire_expr,
            mapping,
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
/// - X: Reserved
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X| ResFinal|
/// +-+-+-+---------+
/// ~ request_id:z32~  (*)
/// +---------------+
/// ~  [reply_exts] ~  if Z==1
/// +---------------+
///
/// (*) The resolution of the request id is negotiated during the session establishment.
///     This implementation limits the resolution to 32bit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseFinal {
    pub rid: RequestId,
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
}

impl ResponseFinal {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let rid: RequestId = rng.gen();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);

        Self {
            rid,
            ext_qos,
            ext_tstamp,
        }
    }
}
