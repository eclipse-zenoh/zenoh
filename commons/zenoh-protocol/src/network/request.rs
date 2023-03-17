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
use crate::{core::WireExpr, zenoh::ZenohMessage};
use core::sync::atomic::AtomicU32;

/// The resolution of a RequestId
pub type RequestId = u32;
pub type AtomicRequestId = AtomicU32;

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
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ~      rid      ~
/// +---------------+
/// ~   [req_exts]  ~  if Z==1
/// +---------------+
/// ~  ZenohMessage ~ -- Payload
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub wire_expr: WireExpr<'static>,
    pub rid: RequestId,
    pub payload: ZenohMessage,
    pub ext_qos: ext::QoS,
    pub ext_tstamp: Option<ext::Timestamp>,
    pub ext_target: ext::RequestTarget,
}

pub mod ext {
    pub const QOS: u8 = crate::network::ext::QOS;
    pub const TSTAMP: u8 = crate::network::ext::TSTAMP;
    pub const TARGET: u8 = 0x03;

    pub type QoS = crate::network::ext::QoS;
    pub type Timestamp = crate::network::ext::Timestamp;

    /// - Target (0x03)
    ///     7 6 5 4 3 2 1 0
    ///    +-+-+-+-+-+-+-+-+
    ///    |Z|ENC|    ID   |
    ///    +-+-+-+---------+
    ///    %     target    % -> Queryable or Subscriber? + Query targeting (ALL, BEST_MATCHING, COMPLETE_ALL, ?COMPLETE_N?)?
    ///    +---------------+
    ///
    /// The `zenoh::queryable::Queryable`s that should be target of a `zenoh::Session::get()`.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub enum RequestTarget {
        #[default]
        BestMatching,
        All,
        AllComplete,
        #[cfg(feature = "complete_n")]
        Complete(u64),
    }

    impl RequestTarget {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::prelude::SliceRandom;
            let mut rng = rand::thread_rng();

            *[
                RequestTarget::All,
                RequestTarget::AllComplete,
                RequestTarget::BestMatching,
                #[cfg(feature = "complete_n")]
                RequestTarget::Complete(rng.gen()),
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
        let rid: RequestId = rng.gen();
        let payload = ZenohMessage::rand();
        let ext_qos = ext::QoS::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::Timestamp::rand);
        let ext_target = ext::RequestTarget::rand();

        Self {
            wire_expr,
            rid,
            payload,
            ext_qos,
            ext_tstamp,
            ext_target,
        }
    }
}
