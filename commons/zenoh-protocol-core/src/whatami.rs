use super::{NonZeroZInt, ZInt};
use zenoh_core::{bail, zresult::ZError};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WhatAmI {
    Router = 1,
    Peer = 1 << 1,
    Client = 1 << 2,
}

impl std::str::FromStr for WhatAmI {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "router" => Ok(WhatAmI::Router),
            "peer" => Ok(WhatAmI::Peer),
            "client" => Ok(WhatAmI::Client),
            _ => bail!("{} is not a valid WhatAmI value. Valid values are: [\"router\", \"peer\", \"client\"].", s),
        }
    }
}

impl WhatAmI {
    pub fn to_str(self) -> &'static str {
        match self {
            WhatAmI::Router => "router",
            WhatAmI::Peer => "peer",
            WhatAmI::Client => "client",
        }
    }

    pub fn try_from(value: ZInt) -> Option<Self> {
        const CLIENT: ZInt = WhatAmI::Client as ZInt;
        const ROUTER: ZInt = WhatAmI::Router as ZInt;
        const PEER: ZInt = WhatAmI::Peer as ZInt;
        match value {
            CLIENT => Some(WhatAmI::Client),
            ROUTER => Some(WhatAmI::Router),
            PEER => Some(WhatAmI::Peer),
            _ => None,
        }
    }
}

impl std::fmt::Display for WhatAmI {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_str())
    }
}

impl serde::Serialize for WhatAmI {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_str())
    }
}

pub struct WhatAmIVisitor;

impl<'de> serde::de::Visitor<'de> for WhatAmIVisitor {
    type Value = WhatAmI;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("either 'router', 'client' or 'peer'")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse()
            .map_err(|_| serde::de::Error::unknown_variant(v, &["router", "client", "peer"]))
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

impl<'de> serde::Deserialize<'de> for WhatAmI {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(WhatAmIVisitor)
    }
}

impl From<WhatAmI> for ZInt {
    fn from(w: WhatAmI) -> Self {
        w as ZInt
    }
}

use std::ops::BitOr;
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WhatAmIMatcher(pub NonZeroZInt);

impl WhatAmIMatcher {
    pub fn try_from<T: std::convert::TryInto<ZInt>>(i: T) -> Option<Self> {
        let i = i.try_into().ok()?;
        if 0 < i && i < 8 {
            Some(WhatAmIMatcher(unsafe { NonZeroZInt::new_unchecked(i) }))
        } else {
            None
        }
    }

    pub fn matches(self, w: WhatAmI) -> bool {
        (self.0.get() & w as ZInt) != 0
    }

    pub fn to_str(self) -> &'static str {
        match self.0.get() {
            2 => "peer",
            4 => "client",
            1 => "router",
            3 => "router|peer",
            6 => "client|peer",
            5 => "client|router",
            7 => "client|router|peer",
            _ => "invalid_matcher",
        }
    }
}

impl std::str::FromStr for WhatAmIMatcher {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut inner = 0;
        for s in s.split('|') {
            match s.trim() {
                "router" => inner |= WhatAmI::Router as ZInt,
                "client" => inner |= WhatAmI::Client as ZInt,
                "peer" => inner |= WhatAmI::Peer as ZInt,
                _ => return Err(()),
            }
        }
        Self::try_from(inner).ok_or(())
    }
}

impl serde::Serialize for WhatAmIMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_str())
    }
}
struct WhatAmIMatcherVisitor;
impl<'de> serde::de::Visitor<'de> for WhatAmIMatcherVisitor {
    type Value = WhatAmIMatcher;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a | separated list of whatami variants ('peer', 'client' or 'router')")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(|_| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(v),
                &"a | separated list of whatami variants ('peer', 'client' or 'router')",
            )
        })
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

impl<'de> serde::Deserialize<'de> for WhatAmIMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(WhatAmIMatcherVisitor)
    }
}

impl std::fmt::Display for WhatAmIMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_str())
    }
}

impl From<WhatAmIMatcher> for ZInt {
    fn from(w: WhatAmIMatcher) -> ZInt {
        w.0.get() as ZInt
    }
}

impl<T> BitOr<T> for WhatAmIMatcher
where
    NonZeroZInt: BitOr<T, Output = NonZeroZInt>,
{
    type Output = Self;
    fn bitor(self, rhs: T) -> Self::Output {
        WhatAmIMatcher(self.0 | rhs)
    }
}

impl BitOr<WhatAmI> for WhatAmIMatcher {
    type Output = Self;
    fn bitor(self, rhs: WhatAmI) -> Self::Output {
        self | rhs as ZInt
    }
}

impl BitOr for WhatAmIMatcher {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        self | rhs.0
    }
}

impl BitOr for WhatAmI {
    type Output = WhatAmIMatcher;
    fn bitor(self, rhs: Self) -> Self::Output {
        WhatAmIMatcher(unsafe { NonZeroZInt::new_unchecked(self as ZInt | rhs as ZInt) })
    }
}

impl From<WhatAmI> for WhatAmIMatcher {
    fn from(w: WhatAmI) -> Self {
        WhatAmIMatcher(unsafe { NonZeroZInt::new_unchecked(w as ZInt) })
    }
}
