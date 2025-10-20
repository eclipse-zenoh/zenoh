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
use alloc::borrow::Cow;

pub use common::*;
pub use keyexpr::*;
pub use queryable::*;
pub use subscriber::*;
pub use token::*;

use crate::{
    common::{ZExtZ64, ZExtZBuf},
    core::{ExprId, WireExpr},
    network::Mapping,
    zextz64, zextzbuf,
};

pub mod flag {
    pub const I: u8 = 1 << 5; // 0x20 Interest      if I==1 then the declare is in a response to an Interest with future==false
                              // pub const X: u8 = 1 << 6; // 0x40 Reserved
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// ```text
/// Flags:
/// - I: Interest       If I==1 then interest_id is present
/// - X: Reserved
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|X|I| DECLARE |
/// +-+-+-+---------+
/// ~interest_id:z32~  if I==1
/// +---------------+
/// ~  [decl_exts]  ~  if Z==1
/// +---------------+
/// ~  declaration  ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    pub interest_id: Option<super::interest::InterestId>,
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
    DeclareFinal(DeclareFinal),
}

impl DeclareBody {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..9) {
            0 => DeclareBody::DeclareKeyExpr(DeclareKeyExpr::rand()),
            1 => DeclareBody::UndeclareKeyExpr(UndeclareKeyExpr::rand()),
            2 => DeclareBody::DeclareSubscriber(DeclareSubscriber::rand()),
            3 => DeclareBody::UndeclareSubscriber(UndeclareSubscriber::rand()),
            4 => DeclareBody::DeclareQueryable(DeclareQueryable::rand()),
            5 => DeclareBody::UndeclareQueryable(UndeclareQueryable::rand()),
            6 => DeclareBody::DeclareToken(DeclareToken::rand()),
            7 => DeclareBody::UndeclareToken(UndeclareToken::rand()),
            8 => DeclareBody::DeclareFinal(DeclareFinal::rand()),
            _ => unreachable!(),
        }
    }
}

impl Declare {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let interest_id = rng
            .gen_bool(0.5)
            .then_some(rng.gen::<super::interest::InterestId>());
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_nodeid = ext::NodeIdType::rand();
        let body = DeclareBody::rand();

        Self {
            interest_id,
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
    /// |Z|X|X| D_FINAL |
    /// +---------------+
    /// ~ [final_exts]  ~  if Z==1
    /// +---------------+
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeclareFinal;

    impl DeclareFinal {
        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            Self
        }
    }

    pub mod ext {
        use super::*;

        /// ```text
        /// Flags:
        /// - N: Named          If N==1 then the key expr has name/suffix
        /// - M: Mapping        if M==1 then key expr mapping is the one declared by the sender, else it is the one declared by the receiver
        ///
        ///  7 6 5 4 3 2 1 0
        /// +-+-+-+-+-+-+-+-+
        /// |X|X|X|X|X|X|M|N|
        /// +-+-+-+---------+
        /// ~ key_scope:z16 ~
        /// +---------------+
        /// ~  key_suffix   ~  if N==1 -- <u8;z16>
        /// +---------------+
        /// ```
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
            #[doc(hidden)]
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
        #[doc(hidden)]
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
        #[doc(hidden)]
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
    use crate::core::EntityId;

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
    }

    impl DeclareSubscriber {
        #[cfg(feature = "test")]
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: SubscriberId = rng.gen();
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
        #[doc(hidden)]
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
    use crate::core::EntityId;

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
        /// ```text
        ///  7 6 5 4 3 2 1 0
        /// +-+-+-+-+-+-+-+-+
        /// |Z|0_1|    ID   |
        /// +-+-+-+---------+
        /// |x|x|x|x|x|x|x|C|
        /// +---------------+
        /// ~ distance <z16>~
        /// +---------------+
        /// ```
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct QueryableInfoType {
            pub complete: bool, // Default false: incomplete
            pub distance: u16,  // Default 0: distance is null (e.g. intra-process communication)
        }

        impl QueryableInfoType {
            pub const DEFAULT: Self = Self {
                complete: false,
                distance: 0,
            };

            #[cfg(feature = "test")]
            #[doc(hidden)]
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
        #[doc(hidden)]
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
        #[doc(hidden)]
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
        #[doc(hidden)]
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
        #[doc(hidden)]
        pub fn rand() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let id: TokenId = rng.gen();
            let ext_wire_expr = common::ext::WireExprType::rand();

            Self { id, ext_wire_expr }
        }
    }
}
