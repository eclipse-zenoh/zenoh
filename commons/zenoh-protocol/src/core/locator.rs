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
use super::endpoint::*;
use alloc::{borrow::ToOwned, string::String};
use core::{convert::TryFrom, fmt, hash::Hash, str::FromStr};
use zenoh_result::{Error as ZError, ZResult};

// Locator
/// A `String` that respects the [`Locator`] canon form: `<proto>/<address>[?<metadata>]`,
/// such that `<metadata>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub struct Locator(pub(super) EndPoint);

impl Locator {
    pub fn new<A, B, C>(protocol: A, address: B, metadata: C) -> ZResult<Self>
    where
        A: AsRef<str>,
        B: AsRef<str>,
        C: AsRef<str>,
    {
        let ep = EndPoint::new(protocol, address, metadata, "")?;
        Ok(Self(ep))
    }

    pub fn protocol(&self) -> Protocol {
        self.0.protocol()
    }

    pub fn protocol_mut(&mut self) -> ProtocolMut {
        self.0.protocol_mut()
    }

    pub fn address(&self) -> Address {
        self.0.address()
    }

    pub fn address_mut(&mut self) -> AddressMut {
        self.0.address_mut()
    }

    pub fn metadata(&self) -> Metadata {
        self.0.metadata()
    }

    pub fn metadata_mut(&mut self) -> MetadataMut {
        self.0.metadata_mut()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<EndPoint> for Locator {
    fn from(mut val: EndPoint) -> Self {
        if let Some(cidx) = val.inner.find(CONFIG_SEPARATOR) {
            val.inner.truncate(cidx);
        }
        Locator(val)
    }
}

impl TryFrom<&str> for Locator {
    type Error = ZError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl From<Locator> for String {
    fn from(val: Locator) -> Self {
        val.0.into()
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
        f.write_str(self.0.as_str())
    }
}

impl Locator {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        EndPoint::rand().into()
    }
}

// pub(crate) trait HasCanonForm {
//     fn is_canon(&self) -> bool;

//     type Output;
//     fn canonicalize(self) -> Self::Output;
// }

// fn cmp(this: &str, than: &str) -> core::cmp::Ordering {
//     let is_longer = this.len().cmp(&than.len());
//     let this = this.chars();
//     let than = than.chars();
//     let zip = this.zip(than);
//     for (this, than) in zip {
//         match this.cmp(&than) {
//             core::cmp::Ordering::Equal => {}
//             o => return o,
//         }
//     }
//     is_longer
// }

// impl<'a, T: Iterator<Item = (&'a str, V)> + Clone, V> HasCanonForm for T {
//     fn is_canon(&self) -> bool {
//         let mut iter = self.clone();
//         let mut acc = if let Some((key, _)) = iter.next() {
//             key
//         } else {
//             return true;
//         };
//         for (key, _) in iter {
//             if cmp(key, acc) != core::cmp::Ordering::Greater {
//                 return false;
//             }
//             acc = key;
//         }
//         true
//     }

//     type Output = Vec<(&'a str, V)>;
//     fn canonicalize(mut self) -> Self::Output {
//         let mut result = Vec::new();
//         if let Some(v) = self.next() {
//             result.push(v);
//         }
//         'outer: for (k, v) in self {
//             for (i, (x, _)) in result.iter().enumerate() {
//                 match cmp(k, x) {
//                     core::cmp::Ordering::Less => {
//                         result.insert(i, (k, v));
//                         continue 'outer;
//                     }
//                     core::cmp::Ordering::Equal => {
//                         result[i].1 = v;
//                         continue 'outer;
//                     }
//                     core::cmp::Ordering::Greater => {}
//                 }
//             }
//             result.push((k, v))
//         }
//         result
//     }
// }
