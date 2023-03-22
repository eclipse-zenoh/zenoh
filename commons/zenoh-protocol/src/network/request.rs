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
use crate::{core::WireExpr, network::Mapping};
use core::sync::atomic::AtomicU32;

/// The resolution of a RequestId
pub type RequestId = u32;
pub type AtomicRequestId = AtomicU32;

pub mod flag {
    pub const N: u8 = 1 << 5; // 0x20 Named         if N==1 then the key expr has name/suffix
    pub const M: u8 = 1 << 6; // 0x40 Mapping       if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// # Request message
///
/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N| Request |
/// +-+-+-+---------+
/// ~ request_id:z32~  (*)
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ~   [req_exts]  ~  if Z==1
/// +---------------+
/// ~  ZenohMessage ~ -- Payload
/// +---------------+
///
/// (*) The resolution of the request id is negotiated during the session establishment.
///     This implementation limits the resolution to 32bit.
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: RequestId,
    pub wire_expr: WireExpr<'static>,
    pub mapping: Mapping,
    pub payload: u8, // @TODO
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
    pub ext_dst: ext::DestinationType,
    pub ext_target: ext::TargetType,
}

pub mod ext {
    use crate::{
        common::{ZExtUnit, ZExtZ64},
        zextunit, zextz64,
    };

    pub type QoS = crate::network::ext::QoS;
    pub type QoSType = crate::network::ext::QoSType;

    pub type Timestamp = crate::network::ext::Timestamp;
    pub type TimestampType = crate::network::ext::TimestampType;

    pub type Destination = zextunit!(0x03, true);

    #[repr(u8)]
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub enum DestinationType {
        Subscribers = 0x00,
        #[default]
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

    pub type Target = zextz64!(0x4, true);

    /// - Target (0x03)
    ///     7 6 5 4 3 2 1 0
    ///    +-+-+-+-+-+-+-+-+
    ///    %     target    %
    ///    +---------------+
    ///
    /// The `zenoh::queryable::Queryable`s that should be target of a `zenoh::Session::get()`.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub enum TargetType {
        #[default]
        BestMatching,
        All,
        AllComplete,
        #[cfg(feature = "complete_n")]
        Complete(u64),
    }

    impl TargetType {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::prelude::SliceRandom;
            let mut rng = rand::thread_rng();

            *[
                TargetType::All,
                TargetType::AllComplete,
                TargetType::BestMatching,
                #[cfg(feature = "complete_n")]
                TargetType::Complete(rng.gen()),
            ]
            .choose(&mut rng)
            .unwrap()
        }
    }
}

impl Request {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let wire_expr = WireExpr::rand();
        let mapping = Mapping::rand();
        let id: RequestId = rng.gen();
        // let payload = ZenohMessage::rand();
        let payload: u8 = rng.gen(); // @TODO
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_dst = ext::DestinationType::rand();
        let ext_target = ext::TargetType::rand();

        Self {
            wire_expr,
            mapping,
            id,
            payload,
            ext_qos,
            ext_tstamp,
            ext_dst,
            ext_target,
        }
    }
}
