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

use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};
use zenoh_core::Result as ZResult;

pub use zenoh_protocol_core::key_expr::*;

#[derive(Clone)]
pub(crate) enum KeyExprInner<'a> {
    Borrowed(&'a keyexpr),
    Owned(OwnedKeyExpr),
    Wire {
        key_expr: OwnedKeyExpr,
        expr_id: u64,
        prefix_len: u32,
    },
}

/// A possibly-owned, possibly pre-optimized version of [`keyexpr`].
/// Check [`keyexpr`]'s documentation for detailed explainations.
#[repr(transparent)]
#[derive(Clone)]
pub struct KeyExpr<'a>(pub(crate) KeyExprInner<'a>);
impl std::ops::Deref for KeyExpr<'_> {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            KeyExprInner::Borrowed(s) => *s,
            KeyExprInner::Owned(s) => s,
            KeyExprInner::Wire { key_expr, .. } => key_expr,
        }
    }
}
impl FromStr for KeyExpr<'static> {
    type Err = zenoh_core::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(KeyExprInner::Owned(s.parse()?)))
    }
}
impl<'a> From<super::KeyExpr<'a>> for OwnedKeyExpr {
    fn from(val: super::KeyExpr<'a>) -> Self {
        match val.0 {
            KeyExprInner::Borrowed(s) => s.into(),
            KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => key_expr,
        }
    }
}
impl AsRef<keyexpr> for KeyExpr<'_> {
    fn as_ref(&self) -> &keyexpr {
        self
    }
}
impl<'a> From<&'a keyexpr> for KeyExpr<'a> {
    fn from(ke: &'a keyexpr) -> Self {
        Self(KeyExprInner::Borrowed(ke))
    }
}
impl From<OwnedKeyExpr> for KeyExpr<'_> {
    fn from(v: OwnedKeyExpr) -> Self {
        Self(KeyExprInner::Owned(v))
    }
}
impl TryFrom<String> for KeyExpr<'static> {
    type Error = zenoh_core::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Owned(value.try_into()?)))
    }
}
impl<'a> TryFrom<&'a String> for KeyExpr<'a> {
    type Error = zenoh_core::Error;
    fn try_from(value: &'a String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}
impl<'a> TryFrom<&'a mut String> for KeyExpr<'a> {
    type Error = zenoh_core::Error;
    fn try_from(value: &'a mut String) -> Result<Self, Self::Error> {
        Ok(Self::from(keyexpr::new(value)?))
    }
}
impl<'a> From<&'a KeyExpr<'a>> for KeyExpr<'a> {
    fn from(val: &'a KeyExpr<'a>) -> Self {
        Self::from(val.as_ref())
    }
}
impl<'a> TryFrom<&'a str> for KeyExpr<'a> {
    type Error = zenoh_core::Error;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed(value.try_into()?)))
    }
}
impl<'a> TryFrom<&'a mut str> for KeyExpr<'a> {
    type Error = zenoh_core::Error;
    fn try_from(value: &'a mut str) -> Result<Self, Self::Error> {
        Ok(Self(KeyExprInner::Borrowed(value.try_into()?)))
    }
}
impl std::fmt::Debug for KeyExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.keyexpr(), f)
    }
}
impl std::fmt::Display for KeyExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.keyexpr(), f)
    }
}
impl PartialEq for KeyExpr<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.keyexpr() == other.keyexpr()
    }
}
impl Eq for KeyExpr<'_> {}
impl std::hash::Hash for KeyExpr<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.keyexpr().hash(state);
    }
}

impl KeyExpr<'static> {
    /// Constructs an [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_string_unchecked(s: String) -> Self {
        Self(KeyExprInner::Owned(OwnedKeyExpr::from_string_unchecked(s)))
    }

    /// Constructs an [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_boxed_string_unchecked(s: Box<str>) -> Self {
        Self(KeyExprInner::Owned(
            OwnedKeyExpr::from_boxed_string_unchecked(s),
        ))
    }
}
impl<'a> KeyExpr<'a> {
    /// Constructs an [`KeyExpr`] without checking [`keyexpr`]'s invariants
    /// # Safety
    /// Key Expressions must follow some rules to be accepted by a Zenoh network.
    /// Messages addressed with invalid key expressions will be dropped.
    pub unsafe fn from_str_uncheckend(s: &'a str) -> Self {
        keyexpr::from_str_unchecked(s).into()
    }
    pub fn keyexpr(&self) -> &keyexpr {
        self
    }
    pub fn borrowing_clone(&'a self) -> Self {
        Self::from(self.as_ref())
    }
    pub fn into_owned(self) -> KeyExpr<'static> {
        match self.0 {
            KeyExprInner::Borrowed(s) => KeyExpr(KeyExprInner::Owned(s.into())),
            KeyExprInner::Owned(s) => KeyExpr(KeyExprInner::Owned(s)),
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                prefix_len,
            } => KeyExpr(KeyExprInner::Wire {
                key_expr,
                expr_id,
                prefix_len,
            }),
        }
    }
    pub fn concat<S: AsRef<str> + ?Sized>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        let s = s.as_ref();
        if self.ends_with('*') && s.starts_with('*') {
            bail!("Tried to concatenate {} (ends with *) and {} (starts with *), which would likely have caused bugs. If you're sure you want to do this, concatenate these into a string and then try to convert.", self, s)
        }
        format!("{}{}", self, s).try_into()
    }
    pub fn join<S: AsRef<str>>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
        format!("{}/{}", self, s.as_ref()).try_into()
    }
    pub(crate) fn is_optimized(&self) -> bool {
        matches!(&self.0, KeyExprInner::Wire { expr_id, .. } if *expr_id != 0)
    }
}

impl<'a> From<&'a KeyExpr<'a>> for zenoh_protocol_core::WireExpr<'a> {
    fn from(val: &'a KeyExpr<'a>) -> Self {
        match &val.0 {
            KeyExprInner::Borrowed(s) => zenoh_protocol_core::WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Borrowed(s),
            },
            KeyExprInner::Owned(s) => zenoh_protocol_core::WireExpr {
                scope: 0,
                suffix: std::borrow::Cow::Borrowed(s),
            },
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                prefix_len,
            } => zenoh_protocol_core::WireExpr {
                scope: *expr_id as u64,
                suffix: std::borrow::Cow::Borrowed(&key_expr.as_str()[((*prefix_len) as usize)..]),
            },
        }
    }
}
