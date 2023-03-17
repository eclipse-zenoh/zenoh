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
use crate::core::{ExprId, Reliability, WireExpr};

/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X| DECLARE |
/// +-+-+-+---------+
/// ~ declare_exts  ~  if Z==1
/// +---------------+
/// ~  declaration  ~
/// +---------------+
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    pub ext_qos: ext::QoS,
    pub ext_tstamp: Option<ext::Timestamp>,
    pub body: DeclareBody,
}

pub mod ext {
    pub const QOS: u8 = crate::network::ext::QOS;
    pub const TSTAMP: u8 = crate::network::ext::TSTAMP;

    pub type QoS = crate::network::ext::QoS;
    pub type Timestamp = crate::network::ext::Timestamp;
}

pub mod id {
    pub const D_KEYEXPR: u8 = 0x01;
    pub const F_KEYEXPR: u8 = 0x02;

    pub const D_PUBLISHER: u8 = 0x03;
    pub const F_PUBLISHER: u8 = 0x04;

    pub const D_SUBSCRIBER: u8 = 0x05;
    pub const F_SUBSCRIBER: u8 = 0x06;

    pub const D_QUERYABLE: u8 = 0x07;
    pub const F_QUERYABLE: u8 = 0x08;

    // SubModes
    pub const MODE_PUSH: u8 = 0x00;
    pub const MODE_PULL: u8 = 0x01;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclareBody {
    DeclareKeyExpr(DeclareKeyExpr),
    ForgetKeyExpr(ForgetKeyExpr),
    DeclarePublisher(DeclarePublisher),
    ForgetPublisher(ForgetPublisher),
    DeclareSubscriber(DeclareSubscriber),
    ForgetSubscriber(ForgetSubscriber),
    DeclareQueryable(DeclareQueryable),
    ForgetQueryable(ForgetQueryable),
}

impl DeclareBody {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..8) {
            0 => DeclareBody::DeclareKeyExpr(DeclareKeyExpr::rand()),
            1 => DeclareBody::ForgetKeyExpr(ForgetKeyExpr::rand()),
            2 => DeclareBody::DeclarePublisher(DeclarePublisher::rand()),
            3 => DeclareBody::ForgetPublisher(ForgetPublisher::rand()),
            4 => DeclareBody::DeclareSubscriber(DeclareSubscriber::rand()),
            5 => DeclareBody::ForgetSubscriber(ForgetSubscriber::rand()),
            6 => DeclareBody::DeclareQueryable(DeclareQueryable::rand()),
            7 => DeclareBody::ForgetQueryable(ForgetQueryable::rand()),
            _ => unreachable!(),
        }
    }
}

impl Declare {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let body = DeclareBody::rand();
        let ext_qos = ext::QoS::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::Timestamp::rand);

        Self {
            body,
            ext_qos,
            ext_tstamp,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    #[default]
    Push,
    Pull,
}

impl Mode {
    #[cfg(feature = "test")]
    fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        if rng.gen_bool(0.5) {
            Mode::Push
        } else {
            Mode::Pull
        }
    }
}

/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|N| D_KEXPR |
/// +---------------+
/// ~  expr_id:z16  ~
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareKeyExpr {
    pub expr_id: ExprId,
    pub wire_expr: WireExpr<'static>,
}

impl DeclareKeyExpr {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let expr_id: ExprId = rng.gen();
        let wire_expr = WireExpr::rand();

        Self { expr_id, wire_expr }
    }
}

/// ```text
/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X|  F_RES  |
/// +---------------+
/// ~  expr_id:z16  ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetKeyExpr {
    pub expr_id: ExprId,
}

impl ForgetKeyExpr {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let expr_id: ExprId = rng.gen();

        Self { expr_id }
    }
}

/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  D_PUB  |
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarePublisher {
    pub key: WireExpr<'static>,
}

impl DeclarePublisher {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let key = WireExpr::rand();

        Self { key }
    }
}

/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  F_PUB  |
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetPublisher {
    pub wire_expr: WireExpr<'static>,
}

impl ForgetPublisher {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let key = WireExpr::rand();

        Self { wire_expr: key }
    }
}

/// The subscription mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubscriberInfo {
    pub reliability: Reliability,
    pub mode: Mode,
}

/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  D_SUB  |
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// |X|X|X|X|X|X|P|R|  (*)
/// +---------------+
///
/// - if R==1 then the subscription is reliable, else it is best effort
/// - if P==1 then the subscription is pull, else it is push
///
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareSubscriber {
    pub key: WireExpr<'static>,
    pub info: SubscriberInfo,
}

impl DeclareSubscriber {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let wire_expr = WireExpr::rand();
        let reliability = Reliability::rand();
        let mode = Mode::rand();
        let info = SubscriberInfo { reliability, mode };

        Self {
            key: wire_expr,
            info,
        }
    }
}

/// ```text
/// Flags:
/// - N: Named          If N==1 then the key expr has name/suffix
/// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  F_SUB  |
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// |X|X|X|X|X|X|P|R|  (*)
/// +---------------+
///
/// - if R==1 then the subscription is reliable, else it is best effort
/// - if P==1 then the subscription is pull, else it is push
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetSubscriber {
    pub wire_expr: WireExpr<'static>,
    pub info: SubscriberInfo,
}

impl ForgetSubscriber {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let wire_expr = WireExpr::rand();
        let reliability = Reliability::rand();
        let mode = Mode::rand();
        let info = SubscriberInfo { reliability, mode };

        Self { wire_expr, info }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QueryableInfo {
    pub reliability: Reliability,
    pub mode: Mode,
    pub complete: u32, // Default 0: incomplete
    pub distance: u32, // Default 0: no distance
}

/// ```text
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  D_QBL  |
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// |X|X|X|X|D|C|P|R|  (*)
/// +---------------+
/// ~  complete_n   ~  if C==1
/// +---------------+
/// ~   distance    ~  if D==1
/// +---------------+
///
/// - if R==1 then the queryable is reliable, else it is best effort
/// - if P==1 then the queryable is pull, else it is push
/// - if C==1 then the queryable is complete and the N parameter is present
/// - if D==1 then the queryable distance is present
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareQueryable {
    pub wire_expr: WireExpr<'static>,
    pub info: QueryableInfo,
}

impl DeclareQueryable {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let wire_expr = WireExpr::rand();
        let mode = Mode::rand();
        let reliability = Reliability::rand();
        let complete: u32 = rng.gen();
        let distance: u32 = rng.gen();
        let info = QueryableInfo {
            reliability,
            mode,
            complete,
            distance,
        };

        Self { wire_expr, info }
    }
}

/// ```text
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|M|N|  F_QBL  |
/// +---------------+
/// ~ key_scope:z16 ~
/// +---------------+
/// ~  key_suffix   ~  if N==1 -- <u8;z16>
/// +---------------+
/// |X|X|X|X|D|C|P|R|  (*)
/// +---------------+
/// ~  complete_n   ~  if C==1
/// +---------------+
/// ~   distance    ~  if D==1
/// +---------------+
///
/// - if R==1 then the queryable is reliable, else it is best effort
/// - if P==1 then the queryable is pull, else it is push
/// - if C==1 then the queryable is complete and the N parameter is present
/// - if D==1 then the queryable distance is present
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetQueryable {
    pub wire_expr: WireExpr<'static>,
    pub info: QueryableInfo,
}

impl ForgetQueryable {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let wire_expr = WireExpr::rand();
        let mode = Mode::rand();
        let reliability = Reliability::rand();
        let complete: u32 = rng.gen();
        let distance: u32 = rng.gen();
        let info = QueryableInfo {
            reliability,
            mode,
            complete,
            distance,
        };

        Self { wire_expr, info }
    }
}
