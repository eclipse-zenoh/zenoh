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
pub use common::*;
use core::sync::atomic::AtomicU32;
pub use interest::*;
pub use keyexpr::*;
pub use queryable::*;
pub use subscriber::*;
pub use token::*;

pub mod flag {
    pub const I: u8 = 1 << 5; // 0x20 Interest      if I==1 then the declare is in a response to an Interest with future==false
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// Flags:
/// - |: Mode           The mode of the the declaration*
/// -/
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|Mod| DECLARE |
/// +-+-+-+---------+
/// ~    rid:z32    ~  if Mode != Push
/// +---------------+
/// ~  [decl_exts]  ~  if Z==1
/// +---------------+
/// ~  declaration  ~
/// +---------------+
///
/// *Mode of declaration:
/// - Mode 0b00: Push
/// - Mode 0b01: Response
/// - Mode 0b10: Request
/// - Mode 0b11: RequestContinuous

/// The resolution of a RequestId
pub type DeclareRequestId = u32;
pub type AtomicDeclareRequestId = AtomicU32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclareMode {
    Push,
    Request(DeclareRequestId),
    RequestContinuous(DeclareRequestId),
    Response(DeclareRequestId),
}

impl DeclareMode {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..4) {
            0 => DeclareMode::Push,
            1 => DeclareMode::Request(rng.gen()),
            2 => DeclareMode::RequestContinuous(rng.gen()),
            3 => DeclareMode::Response(rng.gen()),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    pub mode: DeclareMode,
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

    pub const D_FINAL: u8 = 0x1A;
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
    DeclareFinal(DeclareFinal),
}

impl DeclareBody {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..10) {
            0 => DeclareBody::DeclareKeyExpr(DeclareKeyExpr::rand()),
            1 => DeclareBody::UndeclareKeyExpr(UndeclareKeyExpr::rand()),
            2 => DeclareBody::DeclareSubscriber(DeclareSubscriber::rand()),
            3 => DeclareBody::UndeclareSubscriber(UndeclareSubscriber::rand()),
            4 => DeclareBody::DeclareQueryable(DeclareQueryable::rand()),
            5 => DeclareBody::UndeclareQueryable(UndeclareQueryable::rand()),
            6 => DeclareBody::DeclareToken(DeclareToken::rand()),
            7 => DeclareBody::UndeclareToken(UndeclareToken::rand()),
            8 => DeclareBody::DeclareInterest(DeclareInterest::rand()),
            9 => DeclareBody::DeclareFinal(DeclareFinal::rand()),
            _ => unreachable!(),
        }
    }
}

impl Declare {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let mode = DeclareMode::rand();
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_nodeid = ext::NodeIdType::rand();
        let body = DeclareBody::rand();

        Self {
            mode,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
            body,
        }
    }
}

pub mod common {
    use super::*;

    /// ```text
    /// Flags:
    /// - X: Reserved
    /// - X: Reserved
    /// - Z: Extension      If Z==1 then at least one extension is present
    ///
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|x|x| D_FINAL |
    /// +---------------+
    /// ~ [final_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareFinal;

    impl DeclareFinal {
        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            Self
        }
    }

    pub mod ext {
        use super::*;

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

            pub fn is_null(&self) -> bool {
                self.wire_expr.is_empty()
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
    use crate::core::EntityId;

    use super::*;

    pub type SubscriberId = EntityId;

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
    /// - if R==1 then the subscription is reliable, else it is best effort    ///
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
        /// %  reserved   |R%
        /// +---------------+
        ///
        /// - if R==1 then the subscription is reliable, else it is best effort
        /// - rsv:  Reserved
        /// ```        
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct SubscriberInfo {
            pub reliability: Reliability,
        }

        impl SubscriberInfo {
            pub const R: u64 = 1;

            pub const DEFAULT: Self = Self {
                reliability: Reliability::DEFAULT,
            };

            #[cfg(feature = "test")]
            pub fn rand() -> Self {
                let reliability = Reliability::rand();

                Self { reliability }
            }
        }

        impl Default for SubscriberInfo {
            fn default() -> Self {
                Self::DEFAULT
            }
        }

        impl From<Info> for SubscriberInfo {
            fn from(ext: Info) -> Self {
                let reliability = if imsg::has_option(ext.value, SubscriberInfo::R) {
                    Reliability::Reliable
                } else {
                    Reliability::BestEffort
                };
                Self { reliability }
            }
        }

        impl From<SubscriberInfo> for Info {
            fn from(ext: SubscriberInfo) -> Self {
                let mut v: u64 = 0;
                if ext.reliability == Reliability::Reliable {
                    v |= SubscriberInfo::R;
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
    use crate::core::EntityId;

    use super::*;

    pub type QueryableId = EntityId;

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
        pub ext_info: ext::QueryableInfoType,
    }

    pub mod ext {
        use super::*;

        pub type QueryableInfo = zextz64!(0x01, false);

        pub mod flag {
            pub const C: u8 = 1; // 0x01 Complete      if C==1 then the queryable is complete
        }
        ///
        ///  7 6 5 4 3 2 1 0
        /// +-+-+-+-+-+-+-+-+
        /// |Z|0_1|    ID   |
        /// +-+-+-+---------+
        /// |x|x|x|x|x|x|x|C|
        /// +---------------+
        /// ~ distance <z16>~
        /// +---------------+
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct QueryableInfoType {
            pub complete: bool, // Default false: incomplete
            pub distance: u16,  // Default 0: no distance
        }

        impl QueryableInfoType {
            pub const DEFAULT: Self = Self {
                complete: false,
                distance: 0,
            };

            #[cfg(feature = "test")]
            pub fn rand() -> Self {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let complete: bool = rng.gen_bool(0.5);
                let distance: u16 = rng.gen();

                Self { complete, distance }
            }
        }

        impl Default for QueryableInfoType {
            fn default() -> Self {
                Self::DEFAULT
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
            let ext_info = ext::QueryableInfoType::rand();

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
    use core::{
        fmt::{self, Debug},
        ops::{Add, AddAssign, Sub, SubAssign},
    };

    use super::*;

    pub type InterestId = u32;

    pub mod flag {
        // pub const X: u8 = 1 << 5; // 0x20 Reserved
        // pub const X: u8 = 1 << 6; // 0x40 Reserved
        pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
    }

    /// # DeclareInterest message
    ///
    /// The DECLARE INTEREST message is sent to request the transmission of current and/or future
    /// declarations of a given kind matching a target keyexpr. E.g., a declare interest could be
    /// sent to request the transmisison of all current subscriptions matching `a/*`.
    ///
    /// The behaviour of a DECLARE INTEREST depends on the DECLARE MODE in the DECLARE MESSAGE:
    /// - Push: only future declarations
    /// - Request: only current declarations
    /// - RequestContinous: current and future declarations
    /// - Response: invalid
    ///
    /// E.g., the [`DeclareInterest`] message flow is the following:
    ///
    /// ```text
    ///     A                   B
    ///     |   DECL INTEREST   |
    ///     |------------------>| -- Sent in Declare::RequestContinuous.
    ///     |                   |    This is a DeclareInterest e.g. for subscriber declarations/undeclarations.
    ///     |                   |
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------| -- Sent in Declare::Response
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------| -- Sent in Declare::Response
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------| -- Sent in Declare::Response
    ///     |                   |
    ///     |       FINAL       |
    ///     |<------------------| -- Sent in Declare::Response
    ///     |                   |
    ///     |  DECL SUBSCRIBER  |
    ///     |<------------------| -- Sent in Declare::Push. This is a new subscriber declaration.
    ///     | UNDECL SUBSCRIBER |
    ///     |<------------------| -- Sent in Declare::Push. This is a new subscriber undeclaration.
    ///     |                   |
    ///     |        ...        |
    ///     |                   |
    ///     |       FINAL       |
    ///     |------------------>| -- Sent in Declare::RequestContinuous.
    ///     |                   |    This stops the transmission of subscriber declarations/undeclarations.
    ///     |                   |
    /// ```
    ///
    /// The DECLARE INTEREST message structure is defined as follows:
    ///
    /// ```text
    /// Flags:
    /// - X: Reserved
    /// - X: Reserved
    /// - Z: Extension      If Z==1 then at least one extension is present
    ///
    /// 7 6 5 4 3 2 1 0
    /// +-+-+-+-+-+-+-+-+
    /// |Z|F|X|  D_INT  |
    /// +---------------+
    /// ~ intst_id:z32  ~
    /// +---------------+
    /// |A|M|N|R|T|Q|S|K|  (*)
    /// +---------------+
    /// ~ key_scope:z16 ~  if R==1
    /// +---------------+
    /// ~  key_suffix   ~  if R==1 && N==1 -- <u8;z16>
    /// +---------------+
    /// ~  [decl_exts]  ~  if Z==1
    /// +---------------+
    ///
    /// (*) - if K==1 then the interest refers to key expressions
    ///     - if S==1 then the interest refers to subscribers
    ///     - if Q==1 then the interest refers to queryables
    ///     - if T==1 then the interest refers to tokens
    ///     - if R==1 then the interest is restricted to the matching key expression, else it is for all key expressions.
    ///     - if N==1 then the key expr has name/suffix. If R==0 then N should be set to 0.
    ///     - if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver.
    ///               If R==0 then M should be set to 0.
    ///     - if A==1 then the replies SHOULD be aggregated
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareInterest {
        pub id: InterestId,
        pub interest: Interest,
        pub wire_expr: Option<WireExpr<'static>>,
    }

    impl DeclareInterest {
        pub fn options(&self) -> u8 {
            let mut interest = self.interest;
            if let Some(we) = self.wire_expr.as_ref() {
                interest += Interest::RESTRICTED;
                if we.has_suffix() {
                    interest += Interest::NAMED;
                }
                if let Mapping::Sender = we.mapping {
                    interest += Interest::MAPPING;
                }
            }
            interest.options
        }

        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: InterestId = rng.gen();
            let wire_expr = rng.gen_bool(0.5).then_some(WireExpr::rand());
            let interest = Interest::rand();

            Self {
                id,
                wire_expr,
                interest,
            }
        }
    }

    #[derive(Clone, Copy)]
    pub struct Interest {
        options: u8,
    }

    impl Interest {
        // Flags
        pub const KEYEXPRS: Interest = Interest::options(1);
        pub const SUBSCRIBERS: Interest = Interest::options(1 << 1);
        pub const QUERYABLES: Interest = Interest::options(1 << 2);
        pub const TOKENS: Interest = Interest::options(1 << 3);
        const RESTRICTED: Interest = Interest::options(1 << 4);
        const NAMED: Interest = Interest::options(1 << 5);
        const MAPPING: Interest = Interest::options(1 << 6);
        pub const AGGREGATE: Interest = Interest::options(1 << 7);
        pub const ALL: Interest = Interest::options(
            Interest::KEYEXPRS.options
                | Interest::SUBSCRIBERS.options
                | Interest::QUERYABLES.options
                | Interest::TOKENS.options,
        );

        const fn options(options: u8) -> Self {
            Self { options }
        }

        pub const fn empty() -> Self {
            Self { options: 0 }
        }

        pub const fn keyexprs(&self) -> bool {
            imsg::has_flag(self.options, Self::KEYEXPRS.options)
        }

        pub const fn subscribers(&self) -> bool {
            imsg::has_flag(self.options, Self::SUBSCRIBERS.options)
        }

        pub const fn queryables(&self) -> bool {
            imsg::has_flag(self.options, Self::QUERYABLES.options)
        }

        pub const fn tokens(&self) -> bool {
            imsg::has_flag(self.options, Self::TOKENS.options)
        }

        pub const fn restricted(&self) -> bool {
            imsg::has_flag(self.options, Self::RESTRICTED.options)
        }

        pub const fn named(&self) -> bool {
            imsg::has_flag(self.options, Self::NAMED.options)
        }

        pub const fn mapping(&self) -> bool {
            imsg::has_flag(self.options, Self::MAPPING.options)
        }

        pub const fn aggregate(&self) -> bool {
            imsg::has_flag(self.options, Self::AGGREGATE.options)
        }

        #[cfg(feature = "test")]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let mut s = Self::empty();
            if rng.gen_bool(0.5) {
                s += Interest::KEYEXPRS;
            }
            if rng.gen_bool(0.5) {
                s += Interest::SUBSCRIBERS;
            }
            if rng.gen_bool(0.5) {
                s += Interest::TOKENS;
            }
            if rng.gen_bool(0.5) {
                s += Interest::AGGREGATE;
            }
            s
        }
    }

    impl PartialEq for Interest {
        fn eq(&self, other: &Self) -> bool {
            self.keyexprs() == other.keyexprs()
                && self.subscribers() == other.subscribers()
                && self.queryables() == other.queryables()
                && self.tokens() == other.tokens()
                && self.aggregate() == other.aggregate()
        }
    }

    impl Debug for Interest {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Interest {{ ")?;
            if self.keyexprs() {
                write!(f, "K:Y, ")?;
            } else {
                write!(f, "K:N, ")?;
            }
            if self.subscribers() {
                write!(f, "S:Y, ")?;
            } else {
                write!(f, "S:N, ")?;
            }
            if self.queryables() {
                write!(f, "Q:Y, ")?;
            } else {
                write!(f, "Q:N, ")?;
            }
            if self.tokens() {
                write!(f, "T:Y, ")?;
            } else {
                write!(f, "T:N, ")?;
            }
            if self.aggregate() {
                write!(f, "A:Y")?;
            } else {
                write!(f, "A:N")?;
            }
            write!(f, " }}")?;
            Ok(())
        }
    }

    impl Eq for Interest {}

    impl Add for Interest {
        type Output = Self;

        #[allow(clippy::suspicious_arithmetic_impl)] // Allows to implement Add & Sub for Interest
        fn add(self, rhs: Self) -> Self::Output {
            Self {
                options: self.options | rhs.options,
            }
        }
    }

    impl AddAssign for Interest {
        #[allow(clippy::suspicious_op_assign_impl)] // Allows to implement Add & Sub for Interest
        fn add_assign(&mut self, rhs: Self) {
            self.options |= rhs.options;
        }
    }

    impl Sub for Interest {
        type Output = Self;

        fn sub(self, rhs: Self) -> Self::Output {
            Self {
                options: self.options & !rhs.options,
            }
        }
    }

    impl SubAssign for Interest {
        fn sub_assign(&mut self, rhs: Self) {
            self.options &= !rhs.options;
        }
    }

    impl From<u8> for Interest {
        fn from(options: u8) -> Self {
            Self { options }
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
