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
extern crate alloc;

// use crate::core::WireExpr;
use alloc::{borrow::ToOwned, boxed::Box, string::String, sync::Arc};
use core::{
    convert::TryFrom,
    fmt,
    ops::{Deref, Div},
    str::FromStr,
};

use super::{canon::Canonize, keyexpr, nonwild_keyexpr};

/// A [`Arc<str>`] newtype that is statically known to be a valid key expression.
///
/// See [`keyexpr`](super::borrowed::keyexpr).
#[derive(Clone, PartialEq, Eq, Hash, serde::Deserialize)]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
#[serde(try_from = "String")]
pub struct OwnedKeyExpr(pub(crate) Arc<str>);
impl serde::Serialize for OwnedKeyExpr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl OwnedKeyExpr {
    /// Equivalent to `<OwnedKeyExpr as TryFrom>::try_from(t)`.
    ///
    /// Will return an Err if `t` isn't a valid key expression.
    /// Note that to be considered a valid key expression, a string MUST be canon.
    ///
    /// [`OwnedKeyExpr::autocanonize`] is an alternative constructor that will canonize the passed expression before constructing it.
    pub fn new<T, E>(t: T) -> Result<Self, E>
    where
        Self: TryFrom<T, Error = E>,
    {
        Self::try_from(t)
    }

    /// Canonizes the passed value before returning it as an `OwnedKeyExpr`.
    ///
    /// Will return Err if the passed value isn't a valid key expression despite canonization.
    pub fn autocanonize<T, E>(mut t: T) -> Result<Self, E>
    where
        Self: TryFrom<T, Error = E>,
        T: Canonize,
    {
        t.canonize();
        Self::new(t)
    }

    /// Constructs an OwnedKeyExpr without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_string_unchecked(s: String) -> Self {
        Self::from_boxed_str_unchecked(s.into_boxed_str())
    }
    /// Constructs an OwnedKeyExpr without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_boxed_str_unchecked(s: Box<str>) -> Self {
        OwnedKeyExpr(s.into())
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
impl Div<&keyexpr> for OwnedKeyExpr {
    type Output = Self;
    fn div(self, rhs: &keyexpr) -> Self::Output {
        &self / rhs
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
impl Div<&keyexpr> for &OwnedKeyExpr {
    type Output = OwnedKeyExpr;
    fn div(self, rhs: &keyexpr) -> Self::Output {
        let s: String = [self.as_str(), "/", rhs.as_str()].concat();
        OwnedKeyExpr::autocanonize(s).unwrap() // Joining 2 key expressions should always result in a canonizable string.
    }
}

#[test]
fn div() {
    let a = OwnedKeyExpr::new("a").unwrap();
    let b = OwnedKeyExpr::new("b").unwrap();
    let k = a / &b;
    assert_eq!(k.as_str(), "a/b")
}

impl fmt::Debug for OwnedKeyExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Display for OwnedKeyExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl Deref for OwnedKeyExpr {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        unsafe { keyexpr::from_str_unchecked(&self.0) }
    }
}

impl AsRef<str> for OwnedKeyExpr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl FromStr for OwnedKeyExpr {
    type Err = zenoh_result::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}
impl TryFrom<&str> for OwnedKeyExpr {
    type Error = zenoh_result::Error;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}
impl TryFrom<String> for OwnedKeyExpr {
    type Error = zenoh_result::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        <&keyexpr as TryFrom<&str>>::try_from(value.as_str())?;
        Ok(Self(value.into()))
    }
}
impl<'a> From<&'a keyexpr> for OwnedKeyExpr {
    fn from(val: &'a keyexpr) -> Self {
        OwnedKeyExpr(Arc::from(val.as_str()))
    }
}
impl From<OwnedKeyExpr> for Arc<str> {
    fn from(ke: OwnedKeyExpr) -> Self {
        ke.0
    }
}
impl From<OwnedKeyExpr> for String {
    fn from(ke: OwnedKeyExpr) -> Self {
        ke.as_str().to_owned()
    }
}

/// A [`Arc<str>`] newtype that is statically known to be a valid nonwild key expression.
///
/// See [`nonwild_keyexpr`](super::borrowed::nonwild_keyexpr).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
#[cfg_attr(feature = "std", derive(schemars::JsonSchema))]
#[serde(try_from = "String")]
pub struct OwnedNonWildKeyExpr(pub(crate) Arc<str>);
impl serde::Serialize for OwnedNonWildKeyExpr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl TryFrom<String> for OwnedNonWildKeyExpr {
    type Error = zenoh_result::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let ke = <&keyexpr as TryFrom<&str>>::try_from(value.as_str())?;
        <&nonwild_keyexpr as TryFrom<&keyexpr>>::try_from(ke)?;
        Ok(Self(value.into()))
    }
}
impl<'a> From<&'a nonwild_keyexpr> for OwnedNonWildKeyExpr {
    fn from(val: &'a nonwild_keyexpr) -> Self {
        OwnedNonWildKeyExpr(Arc::from(val.as_str()))
    }
}

impl Deref for OwnedNonWildKeyExpr {
    type Target = nonwild_keyexpr;
    fn deref(&self) -> &Self::Target {
        unsafe { nonwild_keyexpr::from_str_unchecked(&self.0) }
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
impl Div<&keyexpr> for &OwnedNonWildKeyExpr {
    type Output = OwnedKeyExpr;
    fn div(self, rhs: &keyexpr) -> Self::Output {
        let s: String = [self.as_str(), "/", rhs.as_str()].concat();
        OwnedKeyExpr::autocanonize(s).unwrap() // Joining 2 key expressions should always result in a canonizable string.
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
impl Div<&nonwild_keyexpr> for &OwnedNonWildKeyExpr {
    type Output = OwnedKeyExpr;
    fn div(self, rhs: &nonwild_keyexpr) -> Self::Output {
        let s: String = [self.as_str(), "/", rhs.as_str()].concat();
        s.try_into().unwrap() // Joining 2 non wild key expressions should always result in a non wild string.
    }
}
