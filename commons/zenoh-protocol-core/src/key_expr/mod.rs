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
use zenoh_core::{bail, Error as ZError};

pub mod canon;
#[cfg(test)]
mod test;
mod utils;

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
        if value.contains(['#', '?', '$', '{', '}', '[', ']']) {
            bail!(
                "Invalid Key Expr `{}`: `#?${{}}[]` are forbidden characters",
                value
            )
        }
        Ok(unsafe { keyexpr::from_str_unchecked(value) })
    }
}

impl<'a> TryFrom<&'a mut str> for &'a keyexpr {
    type Error = ZError;
    fn try_from(mut value: &'a mut str) -> Result<Self, Self::Error> {
        canon::Canonizable::canonize(&mut value);
        (value as &'a str).try_into()
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

mod intersect;

impl keyexpr {
    fn intersect(&self, other: &Self) -> bool {
        use intersect::Intersector;
        intersect::DEFAULT_INTERSECTOR.intersect(self, other)
    }
}

pub fn intersect<
    'a,
    L: TryInto<&'a keyexpr, Error = ZError>,
    R: TryInto<&'a keyexpr, Error = ZError>,
>(
    left: L,
    right: R,
) -> Result<bool, ZError> {
    let left: &'a keyexpr = left.try_into()?;
    let right: &'a keyexpr = right.try_into()?;
    Ok(left.intersect(right))
}
