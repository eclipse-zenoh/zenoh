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

use crate::WireExpr;

use super::{OwnedKeyExpr, FORBIDDEN_CHARS};

/// A [`str`] newtype that is statically known to be a valid key expression.
///
/// The exact key expression specification can be found [here](https://github.com/eclipse-zenoh/roadmap/discussions/24#discussioncomment-2766713). Here are the major lines:
/// * Key expressions must be valid UTF8 strings.  
///   Be aware that Zenoh does not perform UTF normalization for you, so get familiar with that concept if your key expression contains glyphs that may have several unicode representation, such as accented characters.
/// * Key expressions may never start or end with `'/'`, nor contain `"//"` or any of the following characters: `#$?`
/// * Key expression must be in canon-form (this ensure that key expressions representing the same set are always the same string).  
///   Note that safe constructors will perform canonization for you if this can be done without extraneous allocations.
///
/// Since Key Expressions define sets of keys, you may want to be aware of the hierarchy of intersection between such sets:
/// * Trivially, two sets can have no elements in common: `a/**` and `b/**` for example define two disjoint sets of keys.
/// * Two sets [`keyexpr::intersect()`] if they have at least one element in common. `a/*` intersects `*/a` on `a/a` for example.
/// * One set A includes the other set B if all of B's elements are in A: `a/*/**` includes `a/b/**`
/// * Two sets A and B are equal if all A includes B and B includes A. The Key Expression language is designed so that string equality is equivalent to set equality.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(PartialEq, Eq, Hash)]
pub struct keyexpr(str);

impl keyexpr {
    /// Returns `true` if the `keyexpr`s intersect, i.e. there exists at least one key which is contained in both of the sets defined by `self` and `other`.
    pub fn intersects(&self, other: &Self) -> bool {
        use super::intersect::Intersector;
        super::intersect::DEFAULT_INTERSECTOR.intersect(self, other)
    }

    pub fn as_str(&self) -> &str {
        self
    }

    /// # Safety
    /// This constructs a [`keyexpr`] without ensuring that it is a valid key-expression.
    ///
    /// Much like [`std::str::from_utf8_unchecked`], this is memory-safe, but calling this without maintaining
    /// [`keyexpr`]'s invariants yourself may lead to unexpected behaviors, the Zenoh network dropping your messages.
    pub unsafe fn from_str_unchecked(s: &str) -> &Self {
        std::mem::transmute(s)
    }
    pub fn new<'a, T, E>(t: T) -> Result<&'a Self, E>
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
        if value.contains(FORBIDDEN_CHARS) {
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

impl Borrow<keyexpr> for OwnedKeyExpr {
    fn borrow(&self) -> &keyexpr {
        self
    }
}
impl ToOwned for keyexpr {
    type Owned = OwnedKeyExpr;
    fn to_owned(&self) -> Self::Owned {
        OwnedKeyExpr::from(self)
    }
}

impl<'a> From<&'a keyexpr> for WireExpr<'a> {
    fn from(val: &'a keyexpr) -> Self {
        WireExpr {
            scope: 0,
            suffix: std::borrow::Cow::Borrowed(val.as_str()),
        }
    }
}
