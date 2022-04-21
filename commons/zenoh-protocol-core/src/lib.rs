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

use std::convert::{From, TryFrom, TryInto};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
pub use uhlc::{Timestamp, NTP64};
use uuid::Uuid;
use zenoh_core::{bail, zerror};

/// The unique Id of the [`HLC`](uhlc::HLC) that generated the concerned [`Timestamp`].
pub type TimestampId = uhlc::ID;

/// A zenoh integer.
pub type ZInt = u64;
pub type ZiInt = i64;
pub type AtomicZInt = AtomicU64;
pub type NonZeroZInt = NonZeroU64;
pub const ZINT_MAX_BYTES: usize = 10;

// WhatAmI values
pub type WhatAmI = whatami::WhatAmI;

/// Constants and helpers for zenoh `whatami` flags.
pub mod whatami;

/// A numerical Id mapped to a key expression with `zenoh::Session::declare_expr()`.
pub type ExprId = ZInt;

pub const EMPTY_EXPR_ID: ExprId = 0;

pub mod key_expr;
pub use crate::key_expr::KeyExpr;

mod encoding;
pub use encoding::{Encoding, KnownEncoding};

pub mod locators;
pub use locators::Locator;
pub mod endpoints;
pub use endpoints::EndPoint;

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: ZInt,
    pub value: Vec<u8>,
}

/// The global unique id of a zenoh peer.
#[derive(Clone, Copy, Eq)]
pub struct ZenohId {
    size: usize,
    id: [u8; ZenohId::MAX_SIZE],
}

impl ZenohId {
    pub const MAX_SIZE: usize = 16;

    pub fn new(size: usize, id: [u8; ZenohId::MAX_SIZE]) -> ZenohId {
        ZenohId { size, id }
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.id[..self.size]
    }

    pub fn rand() -> ZenohId {
        ZenohId::from(Uuid::new_v4())
    }
}

impl From<uuid::Uuid> for ZenohId {
    #[inline]
    fn from(uuid: uuid::Uuid) -> Self {
        ZenohId {
            size: 16,
            id: *uuid.as_bytes(),
        }
    }
}

impl FromStr for ZenohId {
    type Err = zenoh_core::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // filter-out '-' characters (in case s has UUID format)
        let s = s.replace('-', "");
        let vec = hex::decode(&s).map_err(|e| zerror!("Invalid id: {} - {}", s, e))?;
        let size = vec.len();
        if size > ZenohId::MAX_SIZE {
            bail!(
                "Invalid id size: {} ({} bytes max)",
                size,
                ZenohId::MAX_SIZE
            )
        }
        let mut id = [0_u8; ZenohId::MAX_SIZE];
        id[..size].copy_from_slice(vec.as_slice());
        Ok(ZenohId::new(size, id))
    }
}

impl PartialEq for ZenohId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size && self.as_slice() == other.as_slice()
    }
}

impl Hash for ZenohId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl fmt::Debug for ZenohId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode_upper(self.as_slice()))
    }
}

impl fmt::Display for ZenohId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// A PeerID can be converted into a Timestamp's ID
impl From<&ZenohId> for uhlc::ID {
    fn from(pid: &ZenohId) -> Self {
        uhlc::ID::new(pid.size, pid.id)
    }
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Control = 0,
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl Priority {
    /// The lowest Priority
    pub const MIN: Self = Self::Background;
    /// The highest Priority
    pub const MAX: Self = Self::Control;
    /// The number of available priorities
    pub const NUM: usize = 1 + Self::MIN as usize - Self::MAX as usize;
}

impl Default for Priority {
    fn default() -> Priority {
        Priority::Data
    }
}

impl TryFrom<u8> for Priority {
    type Error = zenoh_core::Error;

    fn try_from(conduit: u8) -> Result<Self, Self::Error> {
        match conduit {
            0 => Ok(Priority::Control),
            1 => Ok(Priority::RealTime),
            2 => Ok(Priority::InteractiveHigh),
            3 => Ok(Priority::InteractiveLow),
            4 => Ok(Priority::DataHigh),
            5 => Ok(Priority::Data),
            6 => Ok(Priority::DataLow),
            7 => Ok(Priority::Background),
            unknown => bail!(
                "{} is not a valid priority value. Admitted values are: [{}-{}].",
                unknown,
                Self::MAX as u8,
                Self::MIN as u8
            ),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum Reliability {
    BestEffort,
    Reliable,
}

impl Default for Reliability {
    fn default() -> Reliability {
        Reliability::BestEffort
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct Channel {
    pub priority: Priority,
    pub reliability: Reliability,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConduitSnList {
    Plain(ConduitSn),
    QoS(Box<[ConduitSn; Priority::NUM]>),
}

impl fmt::Display for ConduitSnList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[ ")?;
        match self {
            ConduitSnList::Plain(sn) => {
                write!(
                    f,
                    "{:?} {{ reliable: {}, best effort: {} }}",
                    Priority::default(),
                    sn.reliable,
                    sn.best_effort
                )?;
            }
            ConduitSnList::QoS(ref sns) => {
                for (prio, sn) in sns.iter().enumerate() {
                    let p: Priority = (prio as u8).try_into().unwrap();
                    write!(
                        f,
                        "{:?} {{ reliable: {}, best effort: {} }}",
                        p, sn.reliable, sn.best_effort
                    )?;
                    if p != Priority::Background {
                        write!(f, ", ")?;
                    }
                }
            }
        }
        write!(f, " ]")
    }
}

/// The kind of reliability.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct ConduitSn {
    pub reliable: ZInt,
    pub best_effort: ZInt,
}

/// The kind of congestion control.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum CongestionControl {
    Block,
    Drop,
}

impl Default for CongestionControl {
    fn default() -> CongestionControl {
        CongestionControl::Drop
    }
}

/// The subscription mode.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum SubMode {
    Push,
    Pull,
}

impl Default for SubMode {
    #[inline]
    fn default() -> Self {
        SubMode::Push
    }
}

/// A time period.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Period {
    pub origin: ZInt,
    pub period: ZInt,
    pub duration: ZInt,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubInfo {
    pub reliability: Reliability,
    pub mode: SubMode,
    pub period: Option<Period>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryableInfo {
    pub complete: ZInt,
    pub distance: ZInt,
}

impl Default for QueryableInfo {
    fn default() -> QueryableInfo {
        QueryableInfo {
            complete: 1,
            distance: 0,
        }
    }
}

pub mod queryable {
    pub const ALL_KINDS: super::ZInt = 0x01;
    pub const STORAGE: super::ZInt = 0x02;
    pub const EVAL: super::ZInt = 0x04;
}

/// The kind of consolidation.
#[derive(Debug, Clone, PartialEq, Copy)]
#[repr(u8)]
pub enum ConsolidationMode {
    None,
    Lazy,
    Full,
}

/// The kind of consolidation that should be applied on replies to a`zenoh::Session::get()`
/// at different stages of the reply process.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationStrategy {
    pub first_routers: ConsolidationMode,
    pub last_router: ConsolidationMode,
    pub reception: ConsolidationMode,
}

impl ConsolidationStrategy {
    /// No consolidation performed.
    ///
    /// This is usefull when querying timeseries data bases or
    /// when using quorums.
    #[inline]
    pub fn none() -> Self {
        Self {
            first_routers: ConsolidationMode::None,
            last_router: ConsolidationMode::None,
            reception: ConsolidationMode::None,
        }
    }

    /// Lazy consolidation performed at all stages.
    ///
    /// This strategy offers the best latency. Replies are directly
    /// transmitted to the application when received without needing
    /// to wait for all replies.
    ///
    /// This mode does not garantie that there will be no duplicates.
    #[inline]
    pub fn lazy() -> Self {
        Self {
            first_routers: ConsolidationMode::Lazy,
            last_router: ConsolidationMode::Lazy,
            reception: ConsolidationMode::Lazy,
        }
    }

    /// Full consolidation performed at reception.
    ///
    /// This is the default strategy. It offers the best latency while
    /// garantying that there will be no duplicates.
    #[inline]
    pub fn reception() -> Self {
        Self {
            first_routers: ConsolidationMode::Lazy,
            last_router: ConsolidationMode::Lazy,
            reception: ConsolidationMode::Full,
        }
    }

    /// Full consolidation performed on last router and at reception.
    ///
    /// This mode offers a good latency while optimizing bandwidth on
    /// the last transport link between the router and the application.
    #[inline]
    pub fn last_router() -> Self {
        Self {
            first_routers: ConsolidationMode::Lazy,
            last_router: ConsolidationMode::Full,
            reception: ConsolidationMode::Full,
        }
    }

    /// Full consolidation performed everywhere.
    ///
    /// This mode optimizes bandwidth on all links in the system
    /// but will provide a very poor latency.
    #[inline]
    pub fn full() -> Self {
        Self {
            first_routers: ConsolidationMode::Full,
            last_router: ConsolidationMode::Full,
            reception: ConsolidationMode::Full,
        }
    }
}

impl Default for ConsolidationStrategy {
    #[inline]
    fn default() -> Self {
        ConsolidationStrategy::reception()
    }
}

/// The `zenoh::queryable::Queryable`s that should be target of a `zenoh::Session::get()`.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryTarget {
    BestMatching,
    All,
    AllComplete,
    None,
    #[cfg(feature = "complete_n")]
    Complete(ZInt),
}

impl Default for QueryTarget {
    fn default() -> Self {
        QueryTarget::BestMatching
    }
}

/// The `zenoh::queryable::Queryable`s that should be target of a `zenoh::Session::get()`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryTAK {
    pub kind: ZInt,
    pub target: QueryTarget,
}

impl Default for QueryTAK {
    fn default() -> Self {
        QueryTAK {
            kind: queryable::ALL_KINDS,
            target: QueryTarget::default(),
        }
    }
}

pub(crate) fn split_once(s: &str, c: char) -> (&str, &str) {
    match s.find(c) {
        Some(index) => {
            let (l, r) = s.split_at(index);
            (l, &r[1..])
        }
        None => (s, ""),
    }
}
