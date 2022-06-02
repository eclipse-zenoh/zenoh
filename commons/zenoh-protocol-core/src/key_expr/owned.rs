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

use crate::WireExpr;

use super::{canon::Canonizable, keyexpr};
use std::{convert::TryFrom, str::FromStr};

#[derive(Clone, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "String")]
#[serde(into = "Box<str>")]
pub struct OwnedKeyExpr(pub(crate) Box<str>);

impl OwnedKeyExpr {
    /// Constructs an OwnedKeyExpr without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_string_unchecked(s: String) -> Self {
        Self::from_boxed_string_unchecked(s.into_boxed_str())
    }
    /// Constructs an OwnedKeyExpr without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_boxed_string_unchecked(s: Box<str>) -> Self {
        OwnedKeyExpr(s)
    }
}
impl std::fmt::Debug for OwnedKeyExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}
impl std::fmt::Display for OwnedKeyExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl std::ops::Deref for OwnedKeyExpr {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        unsafe { keyexpr::from_str_unchecked(&self.0) }
    }
}
impl FromStr for OwnedKeyExpr {
    type Err = zenoh_core::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}
impl TryFrom<String> for OwnedKeyExpr {
    type Error = zenoh_core::Error;
    fn try_from(mut value: String) -> Result<Self, Self::Error> {
        value.canonize();
        <&keyexpr as TryFrom<&str>>::try_from(value.as_str())?;
        Ok(Self(value.into_boxed_str()))
    }
}
impl<'a> From<&'a keyexpr> for OwnedKeyExpr {
    fn from(val: &'a keyexpr) -> Self {
        OwnedKeyExpr(Box::from(val.as_str()))
    }
}
impl From<OwnedKeyExpr> for Box<str> {
    fn from(ke: OwnedKeyExpr) -> Self {
        ke.0
    }
}
impl From<OwnedKeyExpr> for String {
    fn from(ke: OwnedKeyExpr) -> Self {
        ke.0.into()
    }
}

impl<'a> From<&'a OwnedKeyExpr> for WireExpr<'a> {
    fn from(val: &'a OwnedKeyExpr) -> Self {
        WireExpr {
            scope: 0,
            suffix: std::borrow::Cow::Borrowed(val.as_str()),
        }
    }
}
