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

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    convert::{From, TryFrom, TryInto},
    fmt,
    hash::Hash,
    num::NonZeroU64,
    str::FromStr,
};
pub use uhlc::{Timestamp, NTP64};
use zenoh_keyexpr::OwnedKeyExpr;
use zenoh_result::{bail, zerror};

/// The unique Id of the [`HLC`](uhlc::HLC) that generated the concerned [`Timestamp`].
pub type TimestampId = uhlc::ID;

/// A zenoh integer.
pub type ZInt = u64;
pub type ZiInt = i64;
pub type NonZeroZInt = NonZeroU64;
pub const ZINT_MAX_BYTES: usize = 10;

// WhatAmI values
pub type WhatAmI = whatami::WhatAmI;

/// Constants and helpers for zenoh `whatami` flags.
pub mod whatami;

/// A numerical Id mapped to a key expression.
pub type ExprId = ZInt;

pub const EMPTY_EXPR_ID: ExprId = 0;

pub use zenoh_keyexpr::key_expr;

pub mod wire_expr;
pub use wire_expr::WireExpr;

mod cowstr;
pub use cowstr::CowStr;
mod encoding;
pub use encoding::{Encoding, KnownEncoding};

pub mod locator;
pub use locator::Locator;
pub mod endpoint;
pub use endpoint::EndPoint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: ZInt,
    pub value: Vec<u8>,
}

/// The kind of a `Sample`.
#[repr(u8)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum SampleKind {
    /// if the `Sample` was issued by a `put` operation.
    #[default]
    Put = 0,
    /// if the `Sample` was issued by a `delete` operation.
    Delete = 1,
}

impl fmt::Display for SampleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleKind::Put => write!(f, "PUT"),
            SampleKind::Delete => write!(f, "DELETE"),
        }
    }
}

impl TryFrom<ZInt> for SampleKind {
    type Error = ZInt;
    fn try_from(kind: ZInt) -> Result<Self, ZInt> {
        match kind {
            0 => Ok(SampleKind::Put),
            1 => Ok(SampleKind::Delete),
            _ => Err(kind),
        }
    }
}

/// The global unique id of a zenoh peer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ZenohId(uhlc::ID);

impl ZenohId {
    pub const MAX_SIZE: usize = 16;

    #[inline]
    pub fn size(&self) -> usize {
        self.0.size()
    }

    #[inline]
    pub fn to_le_bytes(&self) -> [u8; uhlc::ID::MAX_SIZE] {
        self.0.to_le_bytes()
    }

    pub fn rand() -> ZenohId {
        ZenohId(uhlc::ID::rand())
    }

    pub fn into_keyexpr(self) -> OwnedKeyExpr {
        self.into()
    }
}

impl Default for ZenohId {
    fn default() -> Self {
        Self::rand()
    }
}

// Mimics uhlc::SizeError,
#[derive(Debug, Clone, Copy)]
pub struct SizeError(usize);

#[cfg(feature = "std")]
impl std::error::Error for SizeError {}
#[cfg(not(feature = "std"))]
impl zenoh_result::IError for SizeError {}

impl From<uhlc::SizeError> for SizeError {
    fn from(val: uhlc::SizeError) -> Self {
        Self(val.0)
    }
}

// Taken from uhlc::SizeError
impl fmt::Display for SizeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Maximum ID size ({} bytes) exceeded: {}",
            uhlc::ID::MAX_SIZE,
            self.0
        )
    }
}

macro_rules! derive_tryfrom {
    ($T: ty) => {
        impl TryFrom<$T> for ZenohId {
            type Error = zenoh_result::Error;
            fn try_from(val: $T) -> Result<Self, Self::Error> {
                match val.try_into() {
                    Ok(ok) => Ok(Self(ok)),
                    Err(err) => Err(Box::<SizeError>::new(err.into())),
                }
            }
        }
    };
}
derive_tryfrom!([u8; 1]);
derive_tryfrom!(&[u8; 1]);
derive_tryfrom!([u8; 2]);
derive_tryfrom!(&[u8; 2]);
derive_tryfrom!([u8; 3]);
derive_tryfrom!(&[u8; 3]);
derive_tryfrom!([u8; 4]);
derive_tryfrom!(&[u8; 4]);
derive_tryfrom!([u8; 5]);
derive_tryfrom!(&[u8; 5]);
derive_tryfrom!([u8; 6]);
derive_tryfrom!(&[u8; 6]);
derive_tryfrom!([u8; 7]);
derive_tryfrom!(&[u8; 7]);
derive_tryfrom!([u8; 8]);
derive_tryfrom!(&[u8; 8]);
derive_tryfrom!([u8; 9]);
derive_tryfrom!(&[u8; 9]);
derive_tryfrom!([u8; 10]);
derive_tryfrom!(&[u8; 10]);
derive_tryfrom!([u8; 11]);
derive_tryfrom!(&[u8; 11]);
derive_tryfrom!([u8; 12]);
derive_tryfrom!(&[u8; 12]);
derive_tryfrom!([u8; 13]);
derive_tryfrom!(&[u8; 13]);
derive_tryfrom!([u8; 14]);
derive_tryfrom!(&[u8; 14]);
derive_tryfrom!([u8; 15]);
derive_tryfrom!(&[u8; 15]);
derive_tryfrom!([u8; 16]);
derive_tryfrom!(&[u8; 16]);
derive_tryfrom!(&[u8]);

impl FromStr for ZenohId {
    type Err = zenoh_result::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains(|c: char| c.is_ascii_uppercase()) {
            bail!(
                "Invalid id: {} - uppercase hexadecimal is not accepted, use lowercase",
                s
            );
        }
        let u: uhlc::ID = s
            .parse()
            .map_err(|e: uhlc::ParseIDError| zerror!("Invalid id: {} - {}", s, e.cause))?;
        Ok(ZenohId(u))
    }
}

impl fmt::Debug for ZenohId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ZenohId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// A PeerID can be converted into a Timestamp's ID
impl From<&ZenohId> for uhlc::ID {
    fn from(zid: &ZenohId) -> Self {
        zid.0
    }
}

impl From<ZenohId> for OwnedKeyExpr {
    fn from(zid: ZenohId) -> Self {
        // Safety: zid.to_string() returns an stringified hexadecimal
        // representation of the zid. Therefore, building a OwnedKeyExpr
        // by calling from_string_unchecked() is safe because it is
        // guaranteed that no wildcards nor reserved chars will be present.
        unsafe { OwnedKeyExpr::from_string_unchecked(zid.to_string()) }
    }
}

impl From<&ZenohId> for OwnedKeyExpr {
    fn from(zid: &ZenohId) -> Self {
        (*zid).into()
    }
}

impl serde::Serialize for ZenohId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ZenohId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ZenohIdVisitor;

        impl<'de> serde::de::Visitor<'de> for ZenohIdVisitor {
            type Value = ZenohId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(&format!("An hex string of 1-{} bytes", ZenohId::MAX_SIZE))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse().map_err(serde::de::Error::custom)
            }

            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(v)
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_str(ZenohIdVisitor)
    }
}

#[derive(Debug, Default, Copy, Clone, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Control = 0,
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    #[default]
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

impl TryFrom<u8> for Priority {
    type Error = zenoh_result::Error;

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

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Reliability {
    #[default]
    BestEffort,
    Reliable,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Channel {
    pub priority: Priority,
    pub reliability: Reliability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct ConduitSn {
    pub reliable: ZInt,
    pub best_effort: ZInt,
}

/// The kind of congestion control.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum CongestionControl {
    Block,
    #[default]
    Drop,
}

/// The subscription mode.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum SubMode {
    #[default]
    Push,
    Pull,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubInfo {
    pub reliability: Reliability,
    pub mode: SubMode,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct QueryableInfo {
    pub complete: ZInt, // Default 0: incomplete
    pub distance: ZInt, // Default 0: no distance
}

/// The kind of consolidation.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum ConsolidationMode {
    /// No consolidation applied: multiple samples may be received for the same key-timestamp.
    None,
    /// Monotonic consolidation immediately forwards samples, except if one with an equal or more recent timestamp
    /// has already been sent with the same key.
    ///
    /// This optimizes latency while potentially reducing bandwidth.
    ///
    /// Note that this doesn't cause re-ordering, but drops the samples for which a more recent timestamp has already
    /// been observed with the same key.
    Monotonic,
    /// Holds back samples to only send the set of samples that had the highest timestamp for their key.
    Latest,
}

/// The `zenoh::queryable::Queryable`s that should be target of a `zenoh::Session::get()`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum QueryTarget {
    #[default]
    BestMatching,
    All,
    AllComplete,
    #[cfg(feature = "complete_n")]
    Complete(ZInt),
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
