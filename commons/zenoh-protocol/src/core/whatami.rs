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
use super::ZInt;
use alloc::string::String;
use core::{convert::TryInto, fmt, num::NonZeroU8, ops::BitOr, str::FromStr};
use zenoh_result::{bail, ZError};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WhatAmI {
    Router = 1,
    Peer = 1 << 1,
    Client = 1 << 2,
}

impl FromStr for WhatAmI {
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
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::prelude::SliceRandom;

        let mut rng = rand::thread_rng();
        *[WhatAmI::Router, WhatAmI::Peer, WhatAmI::Client]
            .choose(&mut rng)
            .unwrap()
    }

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

impl fmt::Display for WhatAmI {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WhatAmIMatcher(pub NonZeroU8);

impl WhatAmIMatcher {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        WhatAmIMatcher(unsafe { NonZeroU8::new_unchecked(rng.gen_range(128..136)) })
    }

    pub fn try_from<T: TryInto<u8>>(i: T) -> Option<Self> {
        let i = i.try_into().ok()?;
        if 127 < i && i < 136 {
            Some(WhatAmIMatcher(unsafe { NonZeroU8::new_unchecked(i) }))
        } else {
            None
        }
    }

    pub fn is_empty(self) -> bool {
        self.0.get() == 128
    }

    pub fn empty() -> Self {
        WhatAmIMatcher(unsafe { NonZeroU8::new_unchecked(128) })
    }

    pub fn matches(self, w: WhatAmI) -> bool {
        (self.0.get() & w as u8) != 0
    }

    pub fn to_str(self) -> &'static str {
        match self.0.get() {
            128 => "",
            129 => "router",
            130 => "peer",
            132 => "client",
            131 => "router|peer",
            134 => "client|peer",
            133 => "client|router",
            135 => "client|router|peer",
            _ => "invalid_matcher",
        }
    }
}

impl FromStr for WhatAmIMatcher {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut inner = 128;
        for s in s.split('|') {
            match s.trim() {
                "" => {}
                "router" => inner |= WhatAmI::Router as u8,
                "client" => inner |= WhatAmI::Client as u8,
                "peer" => inner |= WhatAmI::Peer as u8,
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

pub struct WhatAmIMatcherVisitor;
impl<'de> serde::de::Visitor<'de> for WhatAmIMatcherVisitor {
    type Value = WhatAmIMatcher;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
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

impl fmt::Display for WhatAmIMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    NonZeroU8: BitOr<T, Output = NonZeroU8>,
{
    type Output = Self;
    fn bitor(self, rhs: T) -> Self::Output {
        WhatAmIMatcher(self.0 | rhs)
    }
}

impl BitOr<WhatAmI> for WhatAmIMatcher {
    type Output = Self;
    fn bitor(self, rhs: WhatAmI) -> Self::Output {
        self | rhs as u8
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
        WhatAmIMatcher(unsafe { NonZeroU8::new_unchecked(self as u8 | rhs as u8 | 128) })
    }
}

impl From<WhatAmI> for WhatAmIMatcher {
    fn from(w: WhatAmI) -> Self {
        WhatAmIMatcher(unsafe { NonZeroU8::new_unchecked(w as u8 | 128) })
    }
}
