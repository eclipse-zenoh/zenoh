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
use alloc::{borrow::ToOwned, string::String};
use core::{convert::TryFrom, fmt, hash::Hash, str::FromStr};

use zenoh_result::{Error as ZError, ZResult};

use super::endpoint::*;

/// A string that respects the [`Locator`] canon form: `<proto>/<address>[?<metadata>]`.
///
/// `<metadata>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

    pub fn protocol(&self) -> Protocol<'_> {
        self.0.protocol()
    }

    pub fn protocol_mut(&mut self) -> ProtocolMut<'_> {
        self.0.protocol_mut()
    }

    pub fn address(&self) -> Address<'_> {
        self.0.address()
    }

    pub fn address_mut(&mut self) -> AddressMut<'_> {
        self.0.address_mut()
    }

    pub fn metadata(&self) -> Metadata<'_> {
        self.0.metadata()
    }

    pub fn metadata_mut(&mut self) -> MetadataMut<'_> {
        self.0.metadata_mut()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn to_endpoint(&self) -> EndPoint {
        self.0.clone()
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

impl fmt::Debug for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Locator {
    #[cfg(feature = "test")]
    #[doc(hidden)]
    pub fn rand() -> Self {
        EndPoint::rand().into()
    }
}
