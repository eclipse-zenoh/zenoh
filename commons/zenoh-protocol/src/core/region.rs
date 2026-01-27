//
// Copyright (c) 2026 ZettaScale Technology
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
use alloc::string::{String, ToString};
use core::{fmt::Display, str::FromStr};

use serde::{de, Deserialize, Serialize};

use crate::core::WhatAmI;

/// Gateway bound.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bound {
    #[default]
    North,
    South,
}

impl Bound {
    pub fn is_north(&self) -> bool {
        *self == Bound::North
    }

    pub fn is_south(&self) -> bool {
        *self == Bound::South
    }
}

impl Display for Bound {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Bound::North => f.write_str("north"),
            Bound::South => f.write_str("south"),
        }
    }
}

impl TryFrom<u8> for Bound {
    type Error = InvalidBoundError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            v if v == Bound::North as u8 => Ok(Bound::North),
            v if v == Bound::South as u8 => Ok(Bound::South),
            _ => Err(InvalidBoundError),
        }
    }
}

#[derive(Debug)]
pub struct InvalidBoundError;

impl Display for InvalidBoundError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "a u8-encoded bound should either be {} (for '{}') or {} (for '{}')",
            Bound::North as u8,
            Bound::North,
            Bound::South as u8,
            Bound::South
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidBoundError {}

/// Region identifier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Region {
    /// Main region.
    #[default]
    North,
    /// Subregion of local sessions.
    Local,
    /// User-defined subregion(s).
    South { id: usize, mode: WhatAmI },
}

impl Region {
    pub fn bound(&self) -> Bound {
        match self {
            Region::North => Bound::North,
            Region::South { .. } | Region::Local => Bound::South,
        }
    }

    pub fn mode(&self) -> Option<WhatAmI> {
        match self {
            Region::North => None,
            Region::Local => Some(WhatAmI::Client),
            Region::South { mode, .. } => Some(*mode),
        }
    }
}

impl FromStr for Region {
    type Err = InvalidRegionIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "north" => Ok(Region::North),
            "local" => Ok(Region::Local),
            _ => {
                let mut substrings = s.splitn(3, ":");

                let Some("south") = substrings.next() else {
                    return Err(InvalidRegionIdError::ExpectedSouth);
                };

                let number_str = substrings
                    .next()
                    .ok_or(InvalidRegionIdError::ExpectedNumber)?;

                let number = number_str
                    .parse()
                    .map_err(InvalidRegionIdError::BadNumber)?;

                let mode_str = substrings
                    .next()
                    .ok_or(InvalidRegionIdError::ExpectedWhatAmI)?;

                let mode = mode_str.parse().map_err(InvalidRegionIdError::BadWhatAmI)?;

                if substrings.next().is_some() {
                    return Err(InvalidRegionIdError::ExpectedEof);
                }

                Ok(Region::South { id: number, mode })
            }
        }
    }
}

pub enum InvalidRegionIdError {
    ExpectedSouth,
    ExpectedNumber,
    ExpectedWhatAmI,
    ExpectedEof,
    BadNumber(core::num::ParseIntError),
    BadWhatAmI(zenoh_result::ZError),
}

impl Display for InvalidRegionIdError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InvalidRegionIdError::ExpectedSouth => f.write_str("expected 'south' literal"),
            InvalidRegionIdError::ExpectedNumber => f.write_str("expected u16 number"),
            InvalidRegionIdError::ExpectedWhatAmI => f.write_str("expected mode"),
            InvalidRegionIdError::ExpectedEof => f.write_str("expected EOF"),
            InvalidRegionIdError::BadNumber(err) => write!(f, "error parsing number: {err}"),
            InvalidRegionIdError::BadWhatAmI(err) => write!(f, "error parsing mode: {err}"),
        }
    }
}

impl Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Region::North => f.write_str("north"),
            Region::Local => f.write_str("local"),
            Region::South { id, mode } => write!(f, "south:{id}:{mode}"),
        }
    }
}

impl Serialize for Region {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Region {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// Region name.
///
/// A region name is a non-empty UTF-8 string limited to [`Self::MAX_LEN`] bytes. It is used to
/// communicate (north) regions names in establishment as well as to match against said names in
/// `gateway` configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionName(String);

impl RegionName {
    pub const MAX_LEN: usize = 32;

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    fn validate<S>(s: S) -> Result<S, InvalidRegionNameError>
    where
        S: AsRef<str>,
    {
        if s.as_ref().is_empty() {
            return Err(InvalidRegionNameError::Empty);
        }

        if s.as_ref().len() > Self::MAX_LEN {
            return Err(InvalidRegionNameError::TooLong);
        }

        Ok(s)
    }

    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        use rand::distributions::{Alphanumeric, DistString};

        Alphanumeric
            .sample_string(&mut rand::thread_rng(), Self::MAX_LEN)
            .try_into()
            .unwrap()
    }
}

impl FromStr for RegionName {
    type Err = InvalidRegionNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::validate(s).map(|s| Self(s.to_string()))
    }
}

impl TryFrom<String> for RegionName {
    type Error = InvalidRegionNameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::validate(s).map(Self)
    }
}

#[derive(Debug)]
pub enum InvalidRegionNameError {
    Empty,
    TooLong,
}

impl Display for InvalidRegionNameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InvalidRegionNameError::Empty => f.write_str("region names should be non-empty"),
            InvalidRegionNameError::TooLong => write!(
                f,
                "region names should be at most {} bytes",
                RegionName::MAX_LEN
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidRegionNameError {}

impl Serialize for RegionName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RegionName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
