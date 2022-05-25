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

use zenoh_protocol_core::WireExpr;

use super::{canon::Canonizable, keyexpr};
use std::convert::TryFrom;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OwnedKeyExpr(pub(crate) Box<str>);
impl std::ops::Deref for OwnedKeyExpr {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        unsafe { keyexpr::from_str_unchecked(&self.0) }
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
impl<'a> From<super::KeyExpr<'a>> for OwnedKeyExpr {
    fn from(val: super::KeyExpr<'a>) -> Self {
        match val.0 {
            KeyExprInner::Borrowed(s) => OwnedKeyExpr(s.as_str().into()),
            KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => key_expr,
        }
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

#[derive(Clone)]
pub(crate) enum KeyExprInner<'a> {
    Borrowed(&'a keyexpr),
    Owned(OwnedKeyExpr),
    Wire {
        key_expr: OwnedKeyExpr,
        expr_id: u32,
        prefix_len: u32,
    },
}
