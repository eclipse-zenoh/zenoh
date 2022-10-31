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
use super::endpoint::*;
use std::{convert::TryFrom, fmt, hash::Hash, str::FromStr};
use zenoh_core::Error as ZError;

// Locator
/// A `String` that respects the [`Locator`] canon form: `<proto>/<address>[?<metadata>]`,
/// such that `<metadata>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub struct Locator {
    pub(super) inner: String,
}

impl Locator {
    pub fn split(
        &self,
    ) -> (
        &str,
        &str,
        impl Iterator<Item = (&str, &str)> + DoubleEndedIterator,
    ) {
        (self.protocol(), self.address(), self.metadata())
    }

    pub fn protocol(&self) -> &str {
        protocol(self.inner.as_str())
    }

    pub fn address(&self) -> &str {
        address(self.inner.as_str())
    }

    pub fn metadata(&self) -> impl Iterator<Item = (&str, &str)> + DoubleEndedIterator {
        metadata(self.inner.as_str())
    }
}

impl From<Locator> for String {
    fn from(val: Locator) -> Self {
        val.inner
    }
}

impl TryFrom<&str> for Locator {
    type Error = ZError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl TryFrom<String> for Locator {
    type Error = ZError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let ep = EndPoint::try_from(s)?;
        Ok(ep.into())
    }
}

impl FromStr for Locator {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

pub(crate) trait HasCanonForm {
    fn is_canon(&self) -> bool;

    type Output;
    fn canonicalize(self) -> Self::Output;
}

fn cmp(this: &str, than: &str) -> std::cmp::Ordering {
    let is_longer = this.len().cmp(&than.len());
    let this = this.chars();
    let than = than.chars();
    let zip = this.zip(than);
    for (this, than) in zip {
        match this.cmp(&than) {
            std::cmp::Ordering::Equal => {}
            o => return o,
        }
    }
    is_longer
}

impl<'a, T: Iterator<Item = (&'a str, V)> + Clone, V> HasCanonForm for T {
    fn is_canon(&self) -> bool {
        let mut iter = self.clone();
        let mut acc = if let Some((key, _)) = iter.next() {
            key
        } else {
            return true;
        };
        for (key, _) in iter {
            if cmp(key, acc) != std::cmp::Ordering::Greater {
                return false;
            }
            acc = key;
        }
        true
    }

    type Output = Vec<(&'a str, V)>;
    fn canonicalize(mut self) -> Self::Output {
        let mut result = Vec::new();
        if let Some(v) = self.next() {
            result.push(v);
        }
        'outer: for (k, v) in self {
            for (i, (x, _)) in result.iter().enumerate() {
                match cmp(k, x) {
                    std::cmp::Ordering::Less => {
                        result.insert(i, (k, v));
                        continue 'outer;
                    }
                    std::cmp::Ordering::Equal => {
                        result[i].1 = v;
                        continue 'outer;
                    }
                    std::cmp::Ordering::Greater => {}
                }
            }
            result.push((k, v))
        }
        result
    }
}

pub type LocatorProtocol = str;

impl Locator {
    #[doc(hidden)]
    pub fn rand() -> Self {
        EndPoint::rand().into()
    }
}
