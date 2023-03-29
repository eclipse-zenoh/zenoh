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
use crate::{core::WireExpr, network::Mapping, zenoh_new::PushBody};

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
/// ~ ZenohMessage  ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    pub wire_expr: WireExpr<'static>,
    pub mapping: Mapping,
    pub payload: PushBody, // @TODO
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
    pub ext_dst: ext::DestinationType,
}

pub mod ext {
    use crate::{common::ZExtUnit, zextunit};

    pub type QoS = crate::network::ext::QoS;
    pub type QoSType = crate::network::ext::QoSType;

    pub type Timestamp = crate::network::ext::Timestamp;
    pub type TimestampType = crate::network::ext::TimestampType;

    pub type Destination = zextunit!(0x03, true);

    #[repr(u8)]
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub enum DestinationType {
        #[default]
        Subscribers = 0x00,
        Queryables = 0x01,
    }

    impl DestinationType {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            match rng.gen_range(0..2) {
                0 => DestinationType::Subscribers,
                1 => DestinationType::Queryables,
                _ => unreachable!(),
            }
        }
    }
}

impl Push {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let wire_expr = WireExpr::rand();
        let mapping = Mapping::rand();
        let payload = PushBody::rand();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_dst = ext::DestinationType::rand();

        Self {
            wire_expr,
            mapping,
            payload,
            ext_tstamp,
            ext_qos,
            ext_dst,
        }
    }
}
