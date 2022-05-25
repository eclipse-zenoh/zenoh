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

use std::convert::{TryFrom, TryInto};
use zenoh_core::Result as ZResult;

pub(crate) mod owned;
use owned::*;

pub(crate) mod borrowed;
pub use borrowed::*;

pub mod canon;
pub(crate) mod intersect;
pub(crate) mod utils;

#[cfg(test)]
pub(crate) mod test;

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
        Ok(Self::from(keyexpr::try_from(value)?))
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
    pub(crate) unsafe fn from_string_unchecked(s: String) -> Self {
        Self::from_boxed_string_unchecked(s.into_boxed_str())
    }
    pub(crate) unsafe fn from_boxed_string_unchecked(s: Box<str>) -> Self {
        Self(KeyExprInner::Owned(OwnedKeyExpr(s)))
    }
}
impl<'a> KeyExpr<'a> {
    pub fn keyexpr(&self) -> &keyexpr {
        self
    }
    pub fn borrowing_clone(&'a self) -> Self {
        Self::from(self.as_ref())
    }
    pub fn into_owned(self) -> KeyExpr<'static> {
        match self.0 {
            KeyExprInner::Borrowed(s) => {
                KeyExpr(KeyExprInner::Owned(OwnedKeyExpr(s.as_ref().into())))
            }
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
    pub fn concat<S: AsRef<str>>(&self, s: &S) -> ZResult<KeyExpr<'static>> {
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
        match &self.0 {
            KeyExprInner::Wire { expr_id, .. } if *expr_id != 0 => true,
            _ => false,
        }
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
