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
    common::{imsg, ZExtZ64, ZExtZBuf},
    core::{ExprId, Reliability, WireExpr},
    network::Mapping,
    zextz64, zextzbuf,
};
use alloc::borrow::Cow;
use core::ops::BitOr;
pub use interest::*;
pub use keyexpr::*;
pub use queryable::*;
pub use subscriber::*;
pub use token::*;

pub mod flag {
    // pub const X: u8 = 1 << 5; // 0x20 Reserved
    // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// Flags:
/// - X: Reserved
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|X| DECLARE |
/// +-+-+-+---------+
/// ~  [decl_exts]  ~  if Z==1
/// +---------------+
/// ~  declaration  ~
/// +---------------+
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
    pub ext_nodeid: ext::NodeIdType,
    pub body: DeclareBody,
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

pub mod id {
    pub const D_KEYEXPR: u8 = 0x00;
    pub const U_KEYEXPR: u8 = 0x01;

    pub const D_SUBSCRIBER: u8 = 0x02;
    pub const U_SUBSCRIBER: u8 = 0x03;

    pub const D_QUERYABLE: u8 = 0x04;
    pub const U_QUERYABLE: u8 = 0x05;

    pub const D_TOKEN: u8 = 0x06;
    pub const U_TOKEN: u8 = 0x07;

    pub const D_INTEREST: u8 = 0x08;
    pub const F_INTEREST: u8 = 0x09;
    pub const U_INTEREST: u8 = 0x0A;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclareBody {
    DeclareKeyExpr(DeclareKeyExpr),
    UndeclareKeyExpr(UndeclareKeyExpr),
    DeclareSubscriber(DeclareSubscriber),
    UndeclareSubscriber(UndeclareSubscriber),
    DeclareQueryable(DeclareQueryable),
    UndeclareQueryable(UndeclareQueryable),
    DeclareToken(DeclareToken),
    UndeclareToken(UndeclareToken),
    DeclareInterest(DeclareInterest),
    FinalInterest(FinalInterest),
    UndeclareInterest(UndeclareInterest),
}

impl DeclareBody {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..11) {
            0 => DeclareBody::DeclareKeyExpr(DeclareKeyExpr::rand()),
            1 => DeclareBody::UndeclareKeyExpr(UndeclareKeyExpr::rand()),
            2 => DeclareBody::DeclareSubscriber(DeclareSubscriber::rand()),
            3 => DeclareBody::UndeclareSubscriber(UndeclareSubscriber::rand()),
            4 => DeclareBody::DeclareQueryable(DeclareQueryable::rand()),
            5 => DeclareBody::UndeclareQueryable(UndeclareQueryable::rand()),
            6 => DeclareBody::DeclareToken(DeclareToken::rand()),
            7 => DeclareBody::UndeclareToken(UndeclareToken::rand()),
            8 => DeclareBody::DeclareInterest(DeclareInterest::rand()),
            9 => DeclareBody::FinalInterest(FinalInterest::rand()),
            10 => DeclareBody::UndeclareInterest(UndeclareInterest::rand()),
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
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_nodeid = ext::NodeIdType::rand();

        Self {
            body,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
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

pub mod common {
    use super::*;

    pub mod ext {
        use super::*;

        // WARNING: this is a temporary and mandatory extension used for undeclarations
        pub type WireExprExt = zextzbuf!(0x0f, true);
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct WireExprType {
            pub wire_expr: WireExpr<'static>,
        }

        impl WireExprType {
            pub fn null() -> Self {
                Self {
                    wire_expr: WireExpr {
                        scope: ExprId::MIN,
                        suffix: Cow::from(""),
                        mapping: Mapping::Receiver,
                    },
                }
            }

            #[cfg(feature = "test")]
            pub fn rand() -> Self {
                Self {
                    wire_expr: WireExpr::rand(),
                }
            }
        }
    }
}

pub mod keyexpr {
    use super::*;

    pub mod flag {
        pub const N: u8 = 1 << 5; // 0x20 Named         if N==1 then the key expr has name/suffix
                                  // pub const X: u8 = 1 << 6; // 0x40 Reserved
        pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
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
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareKeyExpr {
        pub id: ExprId,
        pub wire_expr: WireExpr<'static>,
    }

    impl DeclareKeyExpr {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: ExprId = rng.gen();
            let wire_expr = WireExpr::rand();

            Self { id, wire_expr }
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
    /// |Z|X|X| U_KEXPR |
    /// +---------------+
    /// ~  expr_id:z16  ~
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UndeclareKeyExpr {
        pub id: ExprId,
    }

    impl UndeclareKeyExpr {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: ExprId = rng.gen();

            Self { id }
        }
    }
}

pub mod subscriber {
    use super::*;

    pub type SubscriberId = u32;

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
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|M|N|  D_SUB  |
    /// +---------------+
    /// ~  subs_id:z32  ~
    /// +---------------+
    /// ~ key_scope:z16 ~
    /// +---------------+
    /// ~  key_suffix   ~  if N==1 -- <u8;z16>
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    ///
    /// - if R==1 then the subscription is reliable, else it is best effort
    /// - if P==1 then the subscription is pull, else it is push
    ///
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareSubscriber {
        pub id: SubscriberId,
        pub wire_expr: WireExpr<'static>,
        pub ext_info: ext::SubscriberInfo,
    }

    pub mod ext {
        use super::*;

        pub type Info = zextz64!(0x01, false);

        /// # The subscription mode.
        ///
        /// ```text
        ///  7 6 5 4 3 2 1 0
        /// +-+-+-+-+-+-+-+-+
        /// |Z|0_1|    ID   |
        /// +-+-+-+---------+
        /// % reserved  |P|R%
        /// +---------------+
        ///
        /// - if R==1 then the subscription is reliable, else it is best effort
        /// - if P==1 then the subscription is pull, else it is push
        /// - rsv:  Reserved
        /// ```        
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        pub struct SubscriberInfo {
            pub reliability: Reliability,
            pub mode: Mode,
        }

        impl SubscriberInfo {
            pub const R: u64 = 1;
            pub const P: u64 = 1 << 1;

            #[cfg(feature = "test")]
            pub fn rand() -> Self {
                let reliability = Reliability::rand();
                let mode = Mode::rand();

                Self { reliability, mode }
            }
        }

        impl From<Info> for SubscriberInfo {
            fn from(ext: Info) -> Self {
                let reliability = if imsg::has_option(ext.value, SubscriberInfo::R) {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                };
                let mode = if imsg::has_option(ext.value, SubscriberInfo::P) {
                    Mode::Pull
                } else {
                    Mode::Push
                };
                Self { reliability, mode }
            }
        }

        impl From<SubscriberInfo> for Info {
            fn from(ext: SubscriberInfo) -> Self {
                let mut v: u64 = 0;
                if ext.reliability == Reliability::Reliable {
                    v |= SubscriberInfo::R;
                }
                if ext.mode == Mode::Pull {
                    v |= SubscriberInfo::P;
                }
                Info::new(v)
            }
        }
    }

    impl DeclareSubscriber {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: SubscriberId = rng.gen();
            let wire_expr = WireExpr::rand();
            let ext_info = ext::SubscriberInfo::rand();

            Self {
                id,
                wire_expr,
                ext_info,
            }
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
    /// |Z|X|X|  U_SUB  |
    /// +---------------+
    /// ~  subs_id:z32  ~
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UndeclareSubscriber {
        pub id: SubscriberId,
        // WARNING: this is a temporary and mandatory extension used for undeclarations
        pub ext_wire_expr: common::ext::WireExprType,
    }

    impl UndeclareSubscriber {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: SubscriberId = rng.gen();
            let ext_wire_expr = common::ext::WireExprType::rand();

            Self { id, ext_wire_expr }
        }
    }
}

pub mod queryable {
    use super::*;

    pub type QueryableId = u32;

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
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|M|N|  D_QBL  |
    /// +---------------+
    /// ~  qbls_id:z32  ~
    /// +---------------+
    /// ~ key_scope:z16 ~
    /// +---------------+
    /// ~  key_suffix   ~  if N==1 -- <u8;z16>
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    ///
    /// - if R==1 then the queryable is reliable, else it is best effort
    /// - if P==1 then the queryable is pull, else it is push
    /// - if C==1 then the queryable is complete and the N parameter is present
    /// - if D==1 then the queryable distance is present
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareQueryable {
        pub id: QueryableId,
        pub wire_expr: WireExpr<'static>,
        pub ext_info: ext::QueryableInfo,
    }

    pub mod ext {
        use super::*;

        pub type Info = zextz64!(0x01, false);

        ///  7 6 5 4 3 2 1 0
        /// +-+-+-+-+-+-+-+-+
        /// |Z|0_1|    ID   |
        /// +-+-+-+---------+
        /// ~  complete_n   ~
        /// +---------------+
        /// ~   distance    ~
        /// +---------------+
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        pub struct QueryableInfo {
            pub complete: u8,  // Default 0: incomplete // @TODO: maybe a bitflag
            pub distance: u32, // Default 0: no distance
        }

        impl QueryableInfo {
            #[cfg(feature = "test")]
            pub fn rand() -> Self {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let complete: u8 = rng.gen();
                let distance: u32 = rng.gen();

                Self { complete, distance }
            }
        }

        impl From<Info> for QueryableInfo {
            fn from(ext: Info) -> Self {
                let complete = ext.value as u8;
                let distance = (ext.value >> 8) as u32;

                Self { complete, distance }
            }
        }

        impl From<QueryableInfo> for Info {
            fn from(ext: QueryableInfo) -> Self {
                let mut v: u64 = ext.complete as u64;
                v |= (ext.distance as u64) << 8;
                Info::new(v)
            }
        }
    }

    impl DeclareQueryable {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: QueryableId = rng.gen();
            let wire_expr = WireExpr::rand();
            let ext_info = ext::QueryableInfo::rand();

            Self {
                id,
                wire_expr,
                ext_info,
            }
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
    /// |Z|X|X|  U_QBL  |
    /// +---------------+
    /// ~  qbls_id:z32  ~
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UndeclareQueryable {
        pub id: QueryableId,
        // WARNING: this is a temporary and mandatory extension used for undeclarations
        pub ext_wire_expr: common::ext::WireExprType,
    }

    impl UndeclareQueryable {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: QueryableId = rng.gen();
            let ext_wire_expr = common::ext::WireExprType::rand();

            Self { id, ext_wire_expr }
        }
    }
}

pub mod token {
    use super::*;

    pub type TokenId = u32;

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
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|M|N|  D_TKN  |
    /// +---------------+
    /// ~ token_id:z32  ~  
    /// +---------------+
    /// ~ key_scope:z16 ~
    /// +---------------+
    /// ~  key_suffix   ~  if N==1 -- <u8;z16>
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    ///
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareToken {
        pub id: TokenId,
        pub wire_expr: WireExpr<'static>,
    }

    impl DeclareToken {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: TokenId = rng.gen();
            let wire_expr = WireExpr::rand();

            Self { id, wire_expr }
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
    /// |Z|X|X|  U_TKN  |
    /// +---------------+
    /// ~ token_id:z32  ~  
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UndeclareToken {
        pub id: TokenId,
        // WARNING: this is a temporary and mandatory extension used for undeclarations
        pub ext_wire_expr: common::ext::WireExprType,
    }

    impl UndeclareToken {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: TokenId = rng.gen();
            let ext_wire_expr = common::ext::WireExprType::rand();

            Self { id, ext_wire_expr }
        }
    }
}

pub mod interest {
    use super::*;

    pub type InterestId = u32;

    pub mod flag {
        pub const N: u8 = 1 << 5; // 0x20 Named         if N==1 then the key expr has name/suffix
        pub const M: u8 = 1 << 6; // 0x40 Mapping       if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
        pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
    }

    /// # DeclareInterest message
    ///
    /// The DECLARE INTEREST message is sent to request the transmission of existing and future
    /// declarations of a given kind matching a target keyexpr. E.g., a declare interest could be sent to
    /// request the transmisison of all existing subscriptions matching `a/*`. A FINAL INTEREST is used to
    /// mark the end of the transmission of exisiting matching declarations.
    ///
    /// E.g., the [`DeclareInterest`]/[`FinalInterest`]/[`UndeclareInterest`] message flow is the following:
    ///
    /// ```text
    ///     A                   B
    ///     |   DECL INTEREST   |
    ///     |------------------>| -- This is a DeclareInterest e.g. for subscriber declarations/undeclarations.
    ///     |                   |
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------|
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------|
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------|
    ///     |                   |
    ///     |   FINAL INTEREST  |
    ///     |<------------------|  -- The FinalInterest signals that all known subscribers have been transmitted.
    ///     |                   |
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------|  -- This is a new subscriber declaration.
    ///     | UNDECL SUBSCRIBER |
    ///     |<------------------|  -- This is a new subscriber undeclaration.
    ///     |                   |
    ///     |        ...        |
    ///     |                   |
    ///     |  UNDECL INTEREST  |
    ///     |------------------>|  -- This is an UndeclareInterest to stop receiving subscriber declarations/undeclarations.
    ///     |                   |
    /// ```
    ///
    /// The DECLARE INTEREST message structure is defined as follows:
    ///
    /// ```text
    /// Flags:
    /// - N: Named          If N==1 then the key expr has name/suffix
    /// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
    /// - Z: Extension      If Z==1 then at least one extension is present
    ///
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|M|N|  D_INT  |
    /// +---------------+
    /// ~ intst_id:z32  ~
    /// +---------------+
    /// ~ key_scope:z16 ~
    /// +---------------+
    /// ~  key_suffix   ~  if N==1 -- <u8;z16>
    /// +---------------+
    /// |A|F|C|X|T|Q|S|K|  (*)
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    ///
    /// (*) - if K==1 then the interest refers to key expressions
    ///     - if S==1 then the interest refers to subscribers
    ///     - if Q==1 then the interest refers to queryables
    ///     - if T==1 then the interest refers to tokens
    ///     - if C==1 then the interest refers to the current declarations.
    ///     - if F==1 then the interest refers to the future declarations. Note that if F==0 then:
    ///               - replies SHOULD NOT be sent after the FinalInterest;
    ///               - UndeclareInterest SHOULD NOT be sent after the FinalInterest.
    ///     - if A==1 then the replies SHOULD be aggregated
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareInterest {
        pub id: InterestId,
        pub wire_expr: WireExpr<'static>,
        pub interest: Interest,
    }

    #[repr(transparent)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Interest(u8);

    impl Interest {
        pub const KEYEXPRS: Interest = Interest(1);
        pub const SUBSCRIBERS: Interest = Interest(1 << 1);
        pub const QUERYABLES: Interest = Interest(1 << 2);
        pub const TOKENS: Interest = Interest(1 << 3);
        // pub const X: Interest = Interest(1 << 4);
        pub const CURRENT: Interest = Interest(1 << 5);
        pub const FUTURE: Interest = Interest(1 << 6);
        pub const AGGREGATE: Interest = Interest(1 << 7);

        pub const fn keyexprs(&self) -> bool {
            imsg::has_flag(self.0, Self::KEYEXPRS.0)
        }

        pub const fn subscribers(&self) -> bool {
            imsg::has_flag(self.0, Self::SUBSCRIBERS.0)
        }

        pub const fn queryables(&self) -> bool {
            imsg::has_flag(self.0, Self::QUERYABLES.0)
        }

        pub const fn tokens(&self) -> bool {
            imsg::has_flag(self.0, Self::TOKENS.0)
        }

        pub const fn current(&self) -> bool {
            imsg::has_flag(self.0, Self::CURRENT.0)
        }

        pub const fn future(&self) -> bool {
            imsg::has_flag(self.0, Self::FUTURE.0)
        }

        pub const fn aggregate(&self) -> bool {
            imsg::has_flag(self.0, Self::AGGREGATE.0)
        }

        pub const fn as_u8(&self) -> u8 {
            self.0
        }

        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let inner: u8 = rng.gen();

            Self(inner)
        }
    }

    impl BitOr for Interest {
        type Output = Self;

        fn bitor(self, rhs: Self) -> Self::Output {
            Self(self.0 | rhs.0)
        }
    }

    impl From<u8> for Interest {
        fn from(v: u8) -> Self {
            Self(v)
        }
    }

    impl DeclareInterest {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: InterestId = rng.gen();
            let wire_expr = WireExpr::rand();
            let interest = Interest::rand();

            Self {
                id,
                wire_expr,
                interest,
            }
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
    /// |Z|X|X|  F_INT  |
    /// +---------------+
    /// ~ intst_id:z32  ~  
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FinalInterest {
        pub id: InterestId,
    }

    impl FinalInterest {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: InterestId = rng.gen();

            Self { id }
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
    /// |Z|X|X|  U_INT  |
    /// +---------------+
    /// ~ intst_id:z32  ~  
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UndeclareInterest {
        pub id: InterestId,
        // WARNING: this is a temporary and mandatory extension used for undeclarations
        pub ext_wire_expr: common::ext::WireExprType,
    }

    impl UndeclareInterest {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: InterestId = rng.gen();
            let ext_wire_expr = common::ext::WireExprType::rand();

            Self { id, ext_wire_expr }
        }
    }
}
