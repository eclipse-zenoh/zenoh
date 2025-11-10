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
use core::{
    fmt::{self, Debug},
    ops::{Add, AddAssign, Sub, SubAssign},
    sync::atomic::AtomicU32,
};

use crate::{common::imsg, core::WireExpr, network::Mapping};

pub type InterestId = u32;

pub mod flag {
    pub const Z: u8 = 1 << 7; // 0x80 Extensions    if Z==1 then an extension will follow
}

/// The INTEREST message is sent to request the transmission of current and optionally future
/// declarations of a given kind matching a target keyexpr. E.g., an interest could be
/// sent to request the transmission of all current subscriptions matching `a/*`.
///
/// The behaviour of a INTEREST depends on the INTEREST MODE.
///
/// E.g., the message flow is the following for an [`Interest`] with mode [`InterestMode::Current`]:
///
/// ```text
///     A                   B
///     |     INTEREST      |
///     |------------------>| -- Mode: Current
///     |                   |    This is an Interest e.g. for subscriber declarations.
///     |                   |
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field set
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field set
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field set
///     |                   |
///     |     DECL FINAL    |
///     |<------------------| -- With interest_id field set
///     |                   |
/// ```
///
/// And the message flow is the following for an [`Interest`] with mode [`InterestMode::CurrentFuture`]:
///
/// ```text
///     A                   B
///     |     INTEREST      |
///     |------------------>| -- This is a DeclareInterest e.g. for subscriber declarations/undeclarations.
///     |                   |
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field set
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field set
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field set
///     |                   |
///     |     DECL FINAL    |
///     |<------------------| -- With interest_id field set
///     |                   |
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field not set
///     | UNDECL SUBSCRIBER |
///     |<------------------| -- With interest_id field not set
///     |                   |
///     |        ...        |
///     |                   |
///     | INTEREST FINAL    |
///     |------------------>| -- Mode: Final
///     |                   |    This stops the transmission of subscriber declarations/undeclarations.
///     |                   |
/// ```
///
/// And the message flow is the following for an [`Interest`] with mode [`InterestMode::Future`]:
///
/// ```text
///     A                   B
///     |     INTEREST      |
///     |------------------>| -- This is a DeclareInterest e.g. for subscriber declarations/undeclarations.
///     |                   |
///     |  DECL SUBSCRIBER  |
///     |<------------------| -- With interest_id field not set
///     | UNDECL SUBSCRIBER |
///     |<------------------| -- With interest_id field not set
///     |                   |
///     |        ...        |
///     |                   |
///     | INTEREST FINAL    |
///     |------------------>| -- Mode: Final
///     |                   |    This stops the transmission of subscriber declarations/undeclarations.
///     |                   |
/// ```
///
/// ```text
/// Flags:
/// - |: Mode           The mode of the interest*
/// -/
/// - Z: Extension      If Z==1 then at least one extension is present
///
/// 7 6 5 4 3 2 1 0
/// +-+-+-+-+-+-+-+-+
/// |Z|Mod|INTEREST |
/// +-+-+-+---------+
/// ~    id:z32     ~
/// +---------------+
/// |A|M|N|R|T|Q|S|K|  if Mod!=Final (*)
/// +---------------+
/// ~ key_scope:z16 ~  if Mod!=Final && R==1
/// +---------------+
/// ~  key_suffix   ~  if Mod!=Final && R==1 && N==1 -- <u8;z16>
/// +---------------+
/// ~  [int_exts]   ~  if Z==1
/// +---------------+
///
/// Mode of declaration:
/// - Mode 0b00: Final
/// - Mode 0b01: Current
/// - Mode 0b10: Future
/// - Mode 0b11: CurrentFuture
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
pub struct Interest {
    pub id: InterestId,
    pub mode: InterestMode,
    pub options: InterestOptions,
    pub wire_expr: Option<WireExpr<'static>>,
    pub ext_qos: ext::QoSType,
    pub ext_tstamp: Option<ext::TimestampType>,
    pub ext_nodeid: ext::NodeIdType,
}

/// The resolution of a RequestId
pub type DeclareRequestId = u32;
pub type AtomicDeclareRequestId = AtomicU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterestMode {
    Final,
    Current,
    Future,
    CurrentFuture,
}

impl InterestMode {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        match rng.gen_range(0..4) {
            0 => InterestMode::Final,
            1 => InterestMode::Current,
            2 => InterestMode::Future,
            3 => InterestMode::CurrentFuture,
            _ => unreachable!(),
        }
    }
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

impl Interest {
    pub fn options(&self) -> u8 {
        let mut interest = self.options;
        if let Some(we) = self.wire_expr.as_ref() {
            interest += InterestOptions::RESTRICTED;
            if we.has_suffix() {
                interest += InterestOptions::NAMED;
            } else {
                interest -= InterestOptions::NAMED;
            }
            if let Mapping::Sender = we.mapping {
                interest += InterestOptions::MAPPING;
            } else {
                interest -= InterestOptions::MAPPING;
            }
        } else {
            interest -= InterestOptions::RESTRICTED;
        }
        interest.options
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let id = rng.gen::<InterestId>();
        let mode = InterestMode::rand();
        let options = if mode == InterestMode::Final {
            InterestOptions::empty()
        } else {
            InterestOptions::rand()
        };
        let wire_expr = options.restricted().then_some(WireExpr::rand());
        let ext_qos = ext::QoSType::rand();
        let ext_tstamp = rng.gen_bool(0.5).then(ext::TimestampType::rand);
        let ext_nodeid = ext::NodeIdType::rand();

        Self {
            id,
            mode,
            wire_expr,
            options,
            ext_qos,
            ext_tstamp,
            ext_nodeid,
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct InterestOptions {
    options: u8,
}

impl InterestOptions {
    // Flags
    pub const KEYEXPRS: InterestOptions = InterestOptions::options(1);
    pub const SUBSCRIBERS: InterestOptions = InterestOptions::options(1 << 1);
    pub const QUERYABLES: InterestOptions = InterestOptions::options(1 << 2);
    pub const TOKENS: InterestOptions = InterestOptions::options(1 << 3);
    const RESTRICTED: InterestOptions = InterestOptions::options(1 << 4);
    const NAMED: InterestOptions = InterestOptions::options(1 << 5);
    const MAPPING: InterestOptions = InterestOptions::options(1 << 6);
    pub const AGGREGATE: InterestOptions = InterestOptions::options(1 << 7);
    pub const ALL: InterestOptions = InterestOptions::options(
        InterestOptions::KEYEXPRS.options
            | InterestOptions::SUBSCRIBERS.options
            | InterestOptions::QUERYABLES.options
            | InterestOptions::TOKENS.options,
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
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let mut s = Self::empty();
        if rng.gen_bool(0.5) {
            s += InterestOptions::KEYEXPRS;
        }
        if rng.gen_bool(0.5) {
            s += InterestOptions::SUBSCRIBERS;
        }
        if rng.gen_bool(0.5) {
            s += InterestOptions::TOKENS;
        }
        if rng.gen_bool(0.5) {
            s += InterestOptions::AGGREGATE;
        }
        s
    }
}

impl PartialEq for InterestOptions {
    fn eq(&self, other: &Self) -> bool {
        self.keyexprs() == other.keyexprs()
            && self.subscribers() == other.subscribers()
            && self.queryables() == other.queryables()
            && self.tokens() == other.tokens()
            && self.aggregate() == other.aggregate()
    }
}

impl Debug for InterestOptions {
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

impl Eq for InterestOptions {}

impl Add for InterestOptions {
    type Output = Self;

    #[allow(clippy::suspicious_arithmetic_impl)] // Allows to implement Add & Sub for Interest
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            options: self.options | rhs.options,
        }
    }
}

impl AddAssign for InterestOptions {
    #[allow(clippy::suspicious_op_assign_impl)] // Allows to implement Add & Sub for Interest
    fn add_assign(&mut self, rhs: Self) {
        self.options |= rhs.options;
    }
}

impl Sub for InterestOptions {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            options: self.options & !rhs.options,
        }
    }
}

impl SubAssign for InterestOptions {
    fn sub_assign(&mut self, rhs: Self) {
        self.options &= !rhs.options;
    }
}

impl From<u8> for InterestOptions {
    fn from(options: u8) -> Self {
        Self { options }
    }
}
