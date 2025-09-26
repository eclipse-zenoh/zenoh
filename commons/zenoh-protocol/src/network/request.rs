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
use core::sync::atomic::AtomicU32;

use crate::{core::WireExpr, zenoh::RequestBody};

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
/// - N: Named          if N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      if Z==1 then at least one extension is present
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
/// ~  RequestBody  ~  -- Payload
/// +---------------+
///
/// (*) The resolution of the request id is negotiated during the session establishment.
///     This implementation limits the resolution to 32bit.
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: RequestId,
    pub wire_expr: WireExpr<'static>,
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
    pub ext_nodeid: ext::NodeIdType,
    pub ext_target: ext::QueryTarget,
    pub ext_budget: Option<ext::BudgetType>,
    pub ext_timeout: Option<ext::TimeoutType>,
    pub payload: RequestBody,
}

pub mod ext {
    use core::{num::NonZeroU32, time::Duration};

    use serde::Deserialize;

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

    pub type Target = zextz64!(0x4, true);
    // ```text
    // - Target (0x03)
    //  7 6 5 4 3 2 1 0
    // +-+-+-+-+-+-+-+-+
    // %     target    %
    // +---------------+
    // ```
    // The `zenoh::queryable::Queryable`s that should be target of a `zenoh::Session::get()`.
    #[repr(u8)]
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
    pub enum QueryTarget {
        /// Let Zenoh find the BestMatching queryable capabale of serving the query.
        #[default]
        BestMatching,
        /// Deliver the query to all queryables matching the query's key expression.
        All,
        /// Deliver the query to all queryables matching the query's key expression that are declared as complete.
        AllComplete,
    }

    impl QueryTarget {
        pub const DEFAULT: Self = Self::BestMatching;

        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::prelude::*;
            let mut rng = rand::thread_rng();

            *[
                QueryTarget::All,
                QueryTarget::AllComplete,
                QueryTarget::BestMatching,
            ]
            .choose(&mut rng)
            .unwrap()
        }
    }

    // The maximum number of responses
    pub type Budget = zextz64!(0x5, false);
    pub type BudgetType = NonZeroU32;

    // The timeout of the request
    pub type Timeout = zextz64!(0x6, false);
    pub type TimeoutType = Duration;
}

impl Request {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use core::num::NonZeroU32;

        use rand::Rng;

        let mut rng = rand::thread_rng();
        let wire_expr = WireExpr::rand();
        let id: RequestId = rng.gen();
        let payload = RequestBody::rand();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_nodeid = ext::NodeIdType::rand();
        let ext_target = ext::QueryTarget::rand();
        let ext_budget = if rng.gen_bool(0.5) {
            NonZeroU32::new(rng.gen())
        } else {
            None
        };
        let ext_timeout = if rng.gen_bool(0.5) {
            Some(ext::TimeoutType::from_millis(rng.gen()))
        } else {
            None
        };

        Self {
            wire_expr,
            id,
            payload,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
            ext_target,
            ext_budget,
            ext_timeout,
        }
    }
}
