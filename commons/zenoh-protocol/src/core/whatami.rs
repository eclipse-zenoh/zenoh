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
use alloc::string::String;
use const_format::formatcp;
use core::{convert::TryFrom, fmt, num::NonZeroU8, ops::BitOr, str::FromStr};
use zenoh_result::{bail, ZError};

const WAI_STR_R: &str = "router";
const WAI_STR_P: &str = "peer";
const WAI_STR_C: &str = "client";

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WhatAmI {
    Router = 0b001,
    Peer = 0b010,
    Client = 0b100,
}

impl WhatAmI {
    const U8_R: u8 = WhatAmI::Router as u8;
    const U8_P: u8 = WhatAmI::Peer as u8;
    const U8_C: u8 = WhatAmI::Client as u8;

    pub const fn to_str(self) -> &'static str {
        match self {
            WhatAmI::Router => WAI_STR_R,
            WhatAmI::Peer => WAI_STR_P,
            WhatAmI::Client => WAI_STR_C,
        }
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::prelude::SliceRandom;

        let mut rng = rand::thread_rng();
        *[WhatAmI::Router, WhatAmI::Peer, WhatAmI::Client]
            .choose(&mut rng)
            .unwrap()
    }
}

impl TryFrom<u8> for WhatAmI {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            Self::U8_R => Ok(WhatAmI::Router),
            Self::U8_P => Ok(WhatAmI::Peer),
            Self::U8_C => Ok(WhatAmI::Client),
            _ => Err(()),
        }
    }
}

impl FromStr for WhatAmI {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            WAI_STR_R => Ok(WhatAmI::Router),
            WAI_STR_P => Ok(WhatAmI::Peer),
            WAI_STR_C => Ok(WhatAmI::Client),
            _ => bail!(
                "{s} is not a valid WhatAmI value. Valid values are: {WAI_STR_R}, {WAI_STR_P}, {WAI_STR_C}."
            ),
        }
    }
}

impl fmt::Display for WhatAmI {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl From<WhatAmI> for u8 {
    fn from(w: WhatAmI) -> Self {
        w as u8
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WhatAmIMatcher(NonZeroU8);

impl WhatAmIMatcher {
    // We use the 7th bit for detecting whether the WhatAmIMatcher is non-zero
    const U8_0: u8 = 1 << 7;
    const U8_R: u8 = Self::U8_0 | WhatAmI::U8_R;
    const U8_P: u8 = Self::U8_0 | WhatAmI::U8_P;
    const U8_C: u8 = Self::U8_0 | WhatAmI::U8_C;
    const U8_R_P: u8 = Self::U8_0 | WhatAmI::U8_R | WhatAmI::U8_P;
    const U8_P_C: u8 = Self::U8_0 | WhatAmI::U8_P | WhatAmI::U8_C;
    const U8_R_C: u8 = Self::U8_0 | WhatAmI::U8_R | WhatAmI::U8_C;
    const U8_R_P_C: u8 = Self::U8_0 | WhatAmI::U8_R | WhatAmI::U8_P | WhatAmI::U8_C;

    pub const fn empty() -> Self {
        Self(unsafe { NonZeroU8::new_unchecked(Self::U8_0) })
    }

    pub const fn router(self) -> Self {
        Self(unsafe { NonZeroU8::new_unchecked(self.0.get() | Self::U8_R) })
    }

    pub const fn peer(self) -> Self {
        Self(unsafe { NonZeroU8::new_unchecked(self.0.get() | Self::U8_P) })
    }

    pub const fn client(self) -> Self {
        Self(unsafe { NonZeroU8::new_unchecked(self.0.get() | Self::U8_C) })
    }

    pub const fn is_empty(&self) -> bool {
        self.0.get() == Self::U8_0
    }

    pub const fn matches(&self, w: WhatAmI) -> bool {
        (self.0.get() & w as u8) != 0
    }

    pub const fn to_str(self) -> &'static str {
        match self.0.get() {
            Self::U8_0 => "",
            Self::U8_R => WAI_STR_R,
            Self::U8_P => WAI_STR_P,
            Self::U8_C => WAI_STR_C,
            Self::U8_R_P => formatcp!("{}|{}", WAI_STR_R, WAI_STR_P),
            Self::U8_R_C => formatcp!("{}|{}", WAI_STR_R, WAI_STR_C),
            Self::U8_P_C => formatcp!("{}|{}", WAI_STR_P, WAI_STR_C),
            Self::U8_R_P_C => formatcp!("{}|{}|{}", WAI_STR_R, WAI_STR_P, WAI_STR_C),
            _ => unreachable!(),
        }
    }

    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::Rng;

        let mut rng = rand::thread_rng();
        let mut waim = WhatAmIMatcher::empty();
        if rng.gen_bool(0.5) {
            waim = waim.router();
        }
        if rng.gen_bool(0.5) {
            waim = waim.peer();
        }
        if rng.gen_bool(0.5) {
            waim = waim.client();
        }
        waim
    }
}

impl TryFrom<u8> for WhatAmIMatcher {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        const MIN: u8 = 0;
        const MAX: u8 = WhatAmI::U8_R | WhatAmI::U8_P | WhatAmI::U8_C;

        if (MIN..=MAX).contains(&v) {
            Ok(WhatAmIMatcher(unsafe {
                NonZeroU8::new_unchecked(Self::U8_0 | v)
            }))
        } else {
            Err(())
        }
    }
}

impl FromStr for WhatAmIMatcher {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut inner = 0;
        for s in s.split('|') {
            match s.trim() {
                "" => {}
                WAI_STR_R => inner |= WhatAmI::U8_R,
                WAI_STR_P => inner |= WhatAmI::U8_P,
                WAI_STR_C => inner |= WhatAmI::U8_C,
                _ => return Err(()),
            }
        }
        Self::try_from(inner)
    }
}

impl fmt::Display for WhatAmIMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl From<WhatAmIMatcher> for u8 {
    fn from(w: WhatAmIMatcher) -> u8 {
        w.0.get()
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
        WhatAmIMatcher(unsafe {
            NonZeroU8::new_unchecked(self as u8 | rhs as u8 | WhatAmIMatcher::U8_0)
        })
    }
}

impl From<WhatAmI> for WhatAmIMatcher {
    fn from(w: WhatAmI) -> Self {
        WhatAmIMatcher(unsafe { NonZeroU8::new_unchecked(w as u8 | WhatAmIMatcher::U8_0) })
    }
}

// Serde
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
