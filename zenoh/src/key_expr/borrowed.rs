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
    borrow::Borrow,
    convert::{TryFrom, TryInto},
};
use zenoh_core::{bail, Error as ZError};

#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(PartialEq, Eq, Hash)]
pub struct keyexpr(str);

impl keyexpr {
    /// # Safety
    /// This constructs a [`keyexpr`] without ensuring that it is a valid key-expression.
    ///
    /// Much like [`std::str::from_utf8_unchecked`], this is memory-safe, but calling this without maintaining
    /// [`keyexpr`]'s invariants yourself may lead to unexpected behaviors, the Zenoh network dropping your messages.
    pub unsafe fn from_str_unchecked(s: &str) -> &Self {
        std::mem::transmute(s)
    }
    pub fn try_from<'a, T, E>(t: T) -> Result<&'a Self, E>
    where
        &'a Self: TryFrom<T, Error = E>,
    {
        t.try_into()
    }
}

impl std::fmt::Debug for keyexpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ke`{}`", self.as_ref())
    }
}

impl std::fmt::Display for keyexpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self)
    }
}

impl<'a> TryFrom<&'a str> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let mut in_big_wild = false;
        for chunk in value.split('/') {
            if chunk.is_empty() {
                bail!("Invalid Key Expr `{}`: empty chunks are forbidden, as well as leading and trailing slashes", value)
            }
            if in_big_wild {
                match chunk {
                    "**" => bail!(
                        "Invalid Key Expr `{}`: `**/**` must be replaced by `**` to reach canon-form",
                        value
                    ),
                    "*" => bail!(
                        "Invalid Key Expr `{}`: `**/*` must be replaced by `*/**` to reach canon-form",
                        value
                    ),
                    _ => {}
                }
            }
            if chunk == "**" {
                in_big_wild = true;
            } else {
                in_big_wild = false;
                if chunk.contains("**") {
                    bail!(
                        "Invalid Key Expr `{}`: `**` may only be preceded an followed by `/`",
                        value
                    )
                }
            }
        }
        if value.contains(['#', '?', '$']) {
            bail!(
                "Invalid Key Expr `{}`: `#?$` are forbidden characters",
                value
            )
        }
        Ok(unsafe { keyexpr::from_str_unchecked(value) })
    }
}

impl<'a> TryFrom<&'a mut str> for &'a keyexpr {
    type Error = ZError;
    fn try_from(mut value: &'a mut str) -> Result<Self, Self::Error> {
        super::canon::Canonizable::canonize(&mut value);
        (value as &'a str).try_into()
    }
}

impl<'a> TryFrom<&'a mut String> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a mut String) -> Result<Self, Self::Error> {
        super::canon::Canonizable::canonize(value);
        (value.as_str()).try_into()
    }
}

impl std::ops::Deref for keyexpr {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        unsafe { std::mem::transmute(self) }
    }
}
impl AsRef<str> for keyexpr {
    fn as_ref(&self) -> &str {
        &*self
    }
}

impl keyexpr {
    pub fn intersect(&self, other: &Self) -> bool {
        use super::intersect::Intersector;
        super::intersect::DEFAULT_INTERSECTOR.intersect(self, other)
    }
    pub fn as_str(&self) -> &str {
        self
    }
}

impl Borrow<keyexpr> for super::KeyExpr<'_> {
    fn borrow(&self) -> &keyexpr {
        self
    }
}
impl ToOwned for keyexpr {
    type Owned = super::KeyExpr<'static>;
    fn to_owned(&self) -> Self::Owned {
        super::KeyExpr::from(self).into_owned()
    }
}
