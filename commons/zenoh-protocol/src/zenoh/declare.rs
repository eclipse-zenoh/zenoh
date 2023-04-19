//
// Copyright (c) 2023 ZettaScale Technology
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
use crate::core::{QueryableInfo, SubInfo, WireExpr, ZInt};
use alloc::vec::Vec;

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X| DECLARE |
/// +-+-+-+---------+
/// ~  Num of Decl  ~
/// +---------------+
/// ~ [Declaration] ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declare {
    pub declarations: Vec<Declaration>,
}

impl Declare {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let n: usize = rng.gen_range(1..16);
        let declarations = (0..n)
            .map(|_| Declaration::rand())
            .collect::<Vec<Declaration>>();

        Self { declarations }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Resource(Resource),
    ForgetResource(ForgetResource),
    Publisher(Publisher),
    ForgetPublisher(ForgetPublisher),
    Subscriber(Subscriber),
    ForgetSubscriber(ForgetSubscriber),
    Queryable(Queryable),
    ForgetQueryable(ForgetQueryable),
}

impl Declaration {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..8) {
            0 => Declaration::Resource(Resource::rand()),
            1 => Declaration::ForgetResource(ForgetResource::rand()),
            2 => Declaration::Publisher(Publisher::rand()),
            3 => Declaration::ForgetPublisher(ForgetPublisher::rand()),
            4 => Declaration::Subscriber(Subscriber::rand()),
            5 => Declaration::ForgetSubscriber(ForgetSubscriber::rand()),
            6 => Declaration::Queryable(Queryable::rand()),
            7 => Declaration::ForgetQueryable(ForgetQueryable::rand()),
            _ => unreachable!(),
        }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X| RESOURCE|
/// +---------------+
/// ~      RID      ~
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub expr_id: ZInt,
    pub key: WireExpr<'static>,
}

impl Resource {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let expr_id: ZInt = rng.gen();
        let key = WireExpr::rand();

        Self { expr_id, key }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |X|X|X|  F_RES  |
/// +---------------+
/// ~      RID      ~
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetResource {
    pub expr_id: ZInt,
}

impl ForgetResource {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let expr_id: ZInt = rng.gen();

        Self { expr_id }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X|   PUB   |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publisher {
    pub key: WireExpr<'static>,
}

impl Publisher {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let key = WireExpr::rand();

        Self { key }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X|  F_PUB  |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetPublisher {
    pub key: WireExpr<'static>,
}

impl ForgetPublisher {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let key = WireExpr::rand();

        Self { key }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|S|R|   SUB   |  R for Reliable
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// |    SubMode    | if S==1. Otherwise: SubMode=Push
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscriber {
    pub key: WireExpr<'static>,
    pub info: SubInfo,
}

impl Subscriber {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use crate::core::{Reliability, SubMode};
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let key = WireExpr::rand();
        let reliability = if rng.gen_bool(0.5) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        };
        let mode = if rng.gen_bool(0.5) {
            SubMode::Push
        } else {
            SubMode::Pull
        };
        let info = SubInfo { reliability, mode };

        Self { key, info }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X|  F_SUB  |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetSubscriber {
    pub key: WireExpr<'static>,
}

impl ForgetSubscriber {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let key = WireExpr::rand();

        Self { key }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|Q|X|  QABLE  |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ~   QablInfo    ~ if Q==1
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queryable {
    pub key: WireExpr<'static>,
    pub info: QueryableInfo,
}

impl Queryable {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        let key = WireExpr::rand();
        let complete: ZInt = rng.gen();
        let distance: ZInt = rng.gen();
        let info = QueryableInfo { complete, distance };

        Self { key, info }
    }
}

/// ```text
///  7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |K|X|X| F_QABLE |
/// +---------------+
/// ~    KeyExpr     ~ if K==1 then key_expr has suffix
/// +---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetQueryable {
    pub key: WireExpr<'static>,
}

impl ForgetQueryable {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        let key = WireExpr::rand();

        Self { key }
    }
}
