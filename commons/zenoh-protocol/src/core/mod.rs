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
pub mod key_expr;

use core::{
    convert::{From, TryFrom, TryInto},
    fmt,
    hash::{Hash, Hasher},
    num::NonZeroU64,
    str::FromStr,
    sync::atomic::AtomicU64,
};
use key_expr::OwnedKeyExpr;
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

/// Constants and helpers for zenoh `whatami` flags.
pub mod whatami;
pub use whatami::*;

pub mod wire_expr;
pub use wire_expr::*;

mod encoding;
pub use encoding::{Encoding, KnownEncoding};

pub mod locator;
pub use locator::*;

pub mod endpoint;
pub use endpoint::*;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Property {
    pub key: ZInt,
    pub value: Vec<u8>,
}

/// The global unique id of a zenoh peer.
#[derive(Clone, Copy, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ZenohId(uhlc::ID);

impl ZenohId {
    pub const MAX_SIZE: usize = 16;

    #[inline]
    pub fn size(&self) -> usize {
        self.0.size()
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn rand() -> ZenohId {
        ZenohId::from(Uuid::new_v4())
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

impl From<uuid::Uuid> for ZenohId {
    #[inline]
    fn from(uuid: uuid::Uuid) -> Self {
        ZenohId(uuid.into())
    }
}

macro_rules! derive_tryfrom {
    ($T: ty) => {
        impl TryFrom<$T> for ZenohId {
            type Error = zenoh_core::Error;
            fn try_from(val: $T) -> Result<Self, Self::Error> {
                Ok(Self(val.try_into()?))
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
    type Err = zenoh_core::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // filter-out '-' characters (in case s has UUID format)
        let s = s.replace('-', "");
        let vec = hex::decode(&s).map_err(|e| zerror!("Invalid id: {} - {}", s, e))?;
        vec.as_slice().try_into()
    }
}

impl PartialEq for ZenohId {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
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

/// The kind of congestion control.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Channel {
    pub priority: Priority,
    pub reliability: Reliability,
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
