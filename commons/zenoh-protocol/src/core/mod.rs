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
};
use core::{
    convert::{From, TryFrom, TryInto},
    fmt::{self, Display},
    hash::Hash,
    ops::{Deref, RangeInclusive},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
pub use uhlc::{Timestamp, NTP64};
use zenoh_keyexpr::OwnedKeyExpr;
use zenoh_result::{bail, zerror};

/// The unique Id of the [`HLC`](uhlc::HLC) that generated the concerned [`Timestamp`].
pub type TimestampId = uhlc::ID;

/// Constants and helpers for zenoh `whatami` flags.
pub mod whatami;
pub use whatami::*;
pub use zenoh_keyexpr::key_expr;

pub mod wire_expr;
pub use wire_expr::*;

mod cowstr;
pub use cowstr::CowStr;
pub mod encoding;
pub use encoding::{Encoding, EncodingId};

pub mod locator;
pub use locator::*;

pub mod endpoint;
pub use endpoint::*;

pub mod resolution;
pub use resolution::*;

pub mod parameters;
pub use parameters::Parameters;

/// The global unique id of a zenoh peer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ZenohIdProto(uhlc::ID);

impl ZenohIdProto {
    pub const MAX_SIZE: usize = 16;

    #[inline]
    pub fn size(&self) -> usize {
        self.0.size()
    }

    #[inline]
    pub fn to_le_bytes(&self) -> [u8; uhlc::ID::MAX_SIZE] {
        self.0.to_le_bytes()
    }

    #[doc(hidden)]
    pub fn rand() -> ZenohIdProto {
        ZenohIdProto(uhlc::ID::rand())
    }

    pub fn into_keyexpr(self) -> OwnedKeyExpr {
        self.into()
    }
}

impl Default for ZenohIdProto {
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
        impl TryFrom<$T> for ZenohIdProto {
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

impl FromStr for ZenohIdProto {
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
        Ok(ZenohIdProto(u))
    }
}

impl fmt::Debug for ZenohIdProto {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ZenohIdProto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// A PeerID can be converted into a Timestamp's ID
impl From<&ZenohIdProto> for uhlc::ID {
    fn from(zid: &ZenohIdProto) -> Self {
        zid.0
    }
}

impl From<ZenohIdProto> for uhlc::ID {
    fn from(zid: ZenohIdProto) -> Self {
        zid.0
    }
}

impl From<ZenohIdProto> for OwnedKeyExpr {
    fn from(zid: ZenohIdProto) -> Self {
        // SAFETY: zid.to_string() returns an stringified hexadecimal
        // representation of the zid. Therefore, building a OwnedKeyExpr
        // by calling from_string_unchecked() is safe because it is
        // guaranteed that no wildcards nor reserved chars will be present.
        unsafe { OwnedKeyExpr::from_string_unchecked(zid.to_string()) }
    }
}

impl From<&ZenohIdProto> for OwnedKeyExpr {
    fn from(zid: &ZenohIdProto) -> Self {
        (*zid).into()
    }
}

impl serde::Serialize for ZenohIdProto {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ZenohIdProto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ZenohIdVisitor;

        impl<'de> serde::de::Visitor<'de> for ZenohIdVisitor {
            type Value = ZenohIdProto;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(&format!(
                    "An hex string of 1-{} bytes",
                    ZenohIdProto::MAX_SIZE
                ))
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

/// The unique id of a zenoh entity inside it's parent `Session`.
pub type EntityId = u32;

/// The global unique id of a zenoh entity.
#[derive(Debug, Default, Copy, Clone, Eq, Hash, PartialEq)]
pub struct EntityGlobalIdProto {
    pub zid: ZenohIdProto,
    pub eid: EntityId,
}

impl EntityGlobalIdProto {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        Self {
            zid: ZenohIdProto::rand(),
            eid: rand::thread_rng().gen(),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Default, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize)]
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

#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize)]
/// A [`Priority`] range bounded inclusively below and above.
pub struct PriorityRange(RangeInclusive<Priority>);

impl Deref for PriorityRange {
    type Target = RangeInclusive<Priority>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PriorityRange {
    pub fn new(range: RangeInclusive<Priority>) -> Self {
        Self(range)
    }

    /// Returns `true` if `self` is a superset of `other`.
    pub fn includes(&self, other: &PriorityRange) -> bool {
        self.start() <= other.start() && other.end() <= self.end()
    }

    pub fn len(&self) -> usize {
        *self.end() as usize - *self.start() as usize + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let start = rng.gen_range(Priority::MAX as u8..Priority::MIN as u8);
        let end = rng.gen_range((start + 1)..=Priority::MIN as u8);

        Self(Priority::try_from(start).unwrap()..=Priority::try_from(end).unwrap())
    }
}

impl Display for PriorityRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", *self.start() as u8, *self.end() as u8)
    }
}

#[derive(Debug, PartialEq)]
pub enum InvalidPriorityRange {
    InvalidSyntax { found: String },
    InvalidBound { message: String },
}

impl Display for InvalidPriorityRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidPriorityRange::InvalidSyntax { found } => write!(f, "invalid priority range string, expected an range of the form `start-end` but found {found}"),
            InvalidPriorityRange::InvalidBound { message } => write!(f, "invalid priority range bound: {message}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidPriorityRange {}

impl FromStr for PriorityRange {
    type Err = InvalidPriorityRange;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const SEPARATOR: &str = "-";
        let mut metadata = s.split(SEPARATOR);

        let start = metadata
            .next()
            .ok_or_else(|| InvalidPriorityRange::InvalidSyntax {
                found: s.to_string(),
            })?
            .parse::<u8>()
            .map(Priority::try_from)
            .map_err(|err| InvalidPriorityRange::InvalidBound {
                message: err.to_string(),
            })?
            .map_err(|err| InvalidPriorityRange::InvalidBound {
                message: err.to_string(),
            })?;

        match metadata.next() {
            Some(slice) => {
                let end = slice
                    .parse::<u8>()
                    .map(Priority::try_from)
                    .map_err(|err| InvalidPriorityRange::InvalidBound {
                        message: err.to_string(),
                    })?
                    .map_err(|err| InvalidPriorityRange::InvalidBound {
                        message: err.to_string(),
                    })?;

                if metadata.next().is_some() {
                    return Err(InvalidPriorityRange::InvalidSyntax {
                        found: s.to_string(),
                    });
                };

                Ok(PriorityRange::new(start..=end))
            }
            None => Ok(PriorityRange::new(start..=start)),
        }
    }
}

impl Priority {
    /// Default
    pub const DEFAULT: Self = Self::Data;
    /// The lowest Priority
    pub const MIN: Self = Self::Background;
    /// The highest Priority
    pub const MAX: Self = Self::Control;
    /// The number of available priorities
    pub const NUM: usize = 1 + Self::MIN as usize - Self::MAX as usize;
}

impl TryFrom<u8> for Priority {
    type Error = zenoh_result::Error;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
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

/// Reliability guarantees for message delivery.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Reliability {
    /// Messages may be lost.
    BestEffort = 0,
    /// Messages are guaranteed to be delivered.
    #[default]
    Reliable = 1,
}

impl Reliability {
    pub const DEFAULT: Self = Self::Reliable;

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        if rng.gen_bool(0.5) {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        }
    }
}

impl From<bool> for Reliability {
    fn from(value: bool) -> Self {
        if value {
            Reliability::Reliable
        } else {
            Reliability::BestEffort
        }
    }
}

impl From<Reliability> for bool {
    fn from(value: Reliability) -> Self {
        match value {
            Reliability::BestEffort => false,
            Reliability::Reliable => true,
        }
    }
}

impl Display for Reliability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

#[derive(Debug)]
pub struct InvalidReliability {
    found: String,
}

impl Display for InvalidReliability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid Reliability string, expected `{}` or `{}` but found {}",
            Reliability::Reliable as u8,
            Reliability::BestEffort as u8,
            self.found
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidReliability {}

impl FromStr for Reliability {
    type Err = InvalidReliability;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Ok(desc) = s.parse::<u8>() else {
            return Err(InvalidReliability {
                found: s.to_string(),
            });
        };

        if desc == Reliability::BestEffort as u8 {
            Ok(Reliability::BestEffort)
        } else if desc == Reliability::Reliable as u8 {
            Ok(Reliability::Reliable)
        } else {
            Err(InvalidReliability {
                found: s.to_string(),
            })
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Channel {
    pub priority: Priority,
    pub reliability: Reliability,
}

impl Channel {
    pub const DEFAULT: Self = Self {
        priority: Priority::DEFAULT,
        reliability: Reliability::DEFAULT,
    };
}

/// Congestion control strategy.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Deserialize)]
#[repr(u8)]
pub enum CongestionControl {
    #[default]
    /// When transmitting a message in a node with a full queue, the node may drop the message.
    Drop = 0,
    /// When transmitting a message in a node with a full queue, the node will wait for queue to
    /// progress.
    Block = 1,
    #[cfg(feature = "unstable")]
    /// When transmitting a message in a node with a full queue, the node will wait for queue to
    /// progress, but only for the first message sent with this strategy; other messages will be
    /// dropped.
    BlockFirst = 2,
}

impl CongestionControl {
    pub const DEFAULT: Self = Self::Drop;

    #[cfg(feature = "internal")]
    pub const DEFAULT_PUSH: Self = Self::Drop;
    #[cfg(not(feature = "internal"))]
    pub(crate) const DEFAULT_PUSH: Self = Self::Drop;

    #[cfg(feature = "internal")]
    pub const DEFAULT_REQUEST: Self = Self::Block;
    #[cfg(not(feature = "internal"))]
    pub(crate) const DEFAULT_REQUEST: Self = Self::Block;

    #[cfg(feature = "internal")]
    pub const DEFAULT_RESPONSE: Self = Self::Block;
    #[cfg(not(feature = "internal"))]
    pub(crate) const DEFAULT_RESPONSE: Self = Self::Block;

    #[cfg(feature = "internal")]
    pub const DEFAULT_DECLARE: Self = Self::Block;
    #[cfg(not(feature = "internal"))]
    pub(crate) const DEFAULT_DECLARE: Self = Self::Block;

    #[cfg(feature = "internal")]
    pub const DEFAULT_OAM: Self = Self::Block;
    #[cfg(not(feature = "internal"))]
    pub(crate) const DEFAULT_OAM: Self = Self::Block;
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use crate::core::{Priority, PriorityRange};

    #[test]
    fn test_priority_range() {
        assert_eq!(
            PriorityRange::from_str("2-3"),
            Ok(PriorityRange::new(
                Priority::InteractiveHigh..=Priority::InteractiveLow
            ))
        );

        assert_eq!(
            PriorityRange::from_str("7"),
            Ok(PriorityRange::new(
                Priority::Background..=Priority::Background
            ))
        );

        assert!(PriorityRange::from_str("1-").is_err());
        assert!(PriorityRange::from_str("-5").is_err());
    }
}
