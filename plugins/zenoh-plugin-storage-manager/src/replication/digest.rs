//
// Copyright (c) 2024 ZettaScale Technology
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

use std::{
    collections::HashMap,
    ops::{BitXor, BitXorAssign, Deref},
};

use serde::{Deserialize, Serialize};

use super::classification::{IntervalIdx, SubIntervalIdx};

/// A [Fingerprint] is a 64 bits hash of the content it "represents".
///
/// The crate used to obtain this hash is <https://crates.io/crates/xxhash-rust>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Default, Deserialize, Serialize)]
#[repr(transparent)]
pub struct Fingerprint(u64);

impl Deref for Fingerprint {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl BitXor for Fingerprint {
    type Output = Fingerprint;

    fn bitxor(self, rhs: Self) -> Self::Output {
        (self.0 ^ rhs.0).into()
    }
}

impl BitXorAssign for Fingerprint {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0
    }
}

impl From<u64> for Fingerprint {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A `Digest` is a concise view of the data present in a storage.
///
/// The purpose of this structure is to be sent over the network to other storage subscribing to the
/// exact same key expression such that they can quickly assess if they are misaligned, i.e. if
/// their data differ.
///
/// To make this assessment quick and lightweight, the `Digest` is composed of a set [Fingerprint]s
/// for each "era". An "era" is a way of grouping sets of [Event]s according to their
/// [Timestamp]. There are three eras: "hot", "warm" and "cold". The closer a timestamp is to the
/// current time, the "hotter" the era it belongs to will be.
///
/// Eras are further divided into [Interval]s and [SubInterval]s — which duration and number can be
/// configured.
///
/// [Event]: super::log::Event
/// [Timestamp]: zenoh::time::Timestamp
/// [Interval]: super::classification::Interval
/// [SubInterval]: super::classification::SubInterval
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Digest {
    pub(crate) configuration_fingerprint: Fingerprint,
    pub(crate) cold_era_fingerprint: Fingerprint,
    pub(crate) warm_era_fingerprints: HashMap<IntervalIdx, Fingerprint>,
    pub(crate) hot_era_fingerprints: HashMap<IntervalIdx, HashMap<SubIntervalIdx, Fingerprint>>,
}
