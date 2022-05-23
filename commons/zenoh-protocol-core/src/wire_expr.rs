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

use crate::ExprId;
use core::fmt;
use std::{borrow::Cow, convert::TryInto};
use zenoh_core::{bail, Result as ZResult};

#[inline(always)]
fn cend(s: &[u8]) -> bool {
    s.is_empty() || s[0] == b'/'
}

#[inline(always)]
fn cwild(s: &[u8]) -> bool {
    s.get(0) == Some(&b'*')
}

#[inline(always)]
fn cnext(s: &[u8]) -> &[u8] {
    &s[1..]
}

#[inline(always)]
fn cequal(s1: &[u8], s2: &[u8]) -> bool {
    s1.starts_with(&s2[0..1])
}

macro_rules! DEFINE_INCLUDE {
    ($name:ident, $end:ident, $wild:ident, $next:ident, $elem_include:ident) => {
        fn $name(this: &[u8], sub: &[u8]) -> bool {
            if ($end(this) && $end(sub)) {
                return true;
            }
            if ($wild(this) && $end(sub)) {
                return $name($next(this), sub);
            }
            if ($wild(this)) {
                if ($end($next(this))) {
                    return true;
                }
                if ($name($next(this), sub)) {
                    return true;
                } else {
                    return $name(this, $next(sub));
                }
            }
            if ($wild(sub)) {
                return false;
            }
            if ($end(this) || $end(sub)) {
                return false;
            }
            if ($elem_include(this, sub)) {
                return $name($next(this), $next(sub));
            }
            return false;
        }
    };
}

fn chunk_it_intersect(mut it1: &[u8], mut it2: &[u8]) -> bool {
    fn next(s: &[u8]) -> (u8, &[u8]) {
        (s[0], &s[1..])
    }
    while !it1.is_empty() && !it2.is_empty() {
        let (current1, advanced1) = next(it1);
        let (current2, advanced2) = next(it2);
        match (current1, current2) {
            (b'*', b'*') => {
                if advanced1.is_empty() || advanced2.is_empty() {
                    return true;
                }
                if chunk_it_intersect(advanced1, it2) {
                    return true;
                } else {
                    return chunk_it_intersect(it1, advanced2);
                };
            }
            (b'*', _) => {
                if advanced1.is_empty() {
                    return true;
                }
                if chunk_it_intersect(advanced1, it2) {
                    return true;
                } else {
                    return chunk_it_intersect(it1, advanced2);
                }
            }
            (_, b'*') => {
                if advanced2.is_empty() {
                    return true;
                }
                if chunk_it_intersect(it1, advanced2) {
                    return true;
                } else {
                    return chunk_it_intersect(advanced1, it2);
                }
            }
            (sub1, sub2) if sub1 == sub2 => {
                it1 = advanced1;
                it2 = advanced2;
            }
            (_, _) => return false,
        }
    }
    it1.is_empty() && it2.is_empty() || it1 == b"*" || it2 == b"*"
}
#[inline(always)]
fn chunk_intersect(c1: &[u8], c2: &[u8]) -> bool {
    if c1 == c2 {
        return true;
    }
    if c1.is_empty() != c2.is_empty() {
        return false;
    }
    chunk_it_intersect(c1, c2)
}

DEFINE_INCLUDE!(chunk_include, cend, cwild, cnext, cequal);

#[inline(always)]
fn end(s: &[u8]) -> bool {
    s.is_empty()
}

#[inline(always)]
fn wild(s: &[u8]) -> bool {
    s.starts_with(b"**/") || s == b"**"
}

fn next(s: &[u8]) -> (&[u8], &[u8]) {
    match s.iter().position(|c| *c == b'/') {
        Some(i) => (&s[..i], &s[(i + 1)..]),
        None => (s, b""),
    }
}
fn it_intersect<'a>(mut it1: &'a [u8], mut it2: &'a [u8]) -> bool {
    while !it1.is_empty() && !it2.is_empty() {
        let (current1, advanced1) = next(it1);
        let (current2, advanced2) = next(it2);
        match (current1, current2) {
            // ("**", "**") => {
            //     if advanced1.is_empty() || advanced2.is_empty() {
            //         return true;
            //     }
            //     if it_intersect(advanced1, it2) {
            //         return true;
            //     } else {
            //         return it_intersect(it1, advanced2);
            //     };
            // }
            (b"**", _) => {
                return advanced1.is_empty()
                    || it_intersect(advanced1, it2)
                    || it_intersect(it1, advanced2);
            }
            (_, b"**") => {
                return advanced2.is_empty()
                    || it_intersect(it1, advanced2)
                    || it_intersect(advanced1, it2);
            }
            (sub1, sub2) if chunk_intersect(sub1, sub2) => {
                it1 = advanced1;
                it2 = advanced2;
            }
            (_, _) => return false,
        }
    }
    (it1.is_empty() || it1 == b"**") && (it2.is_empty() || it2 == b"**")
}
/// Retruns `true` if the given key expressions intersect.
///
/// I.e. if it exists a resource key (with no wildcards) that matches
/// both given key expressions.
#[inline(always)]
pub fn intersect(s1: &str, s2: &str) -> bool {
    let mut s1 = s1.as_bytes();
    let mut s2 = s2.as_bytes();
    if s1.ends_with(b"/") && s1.len() > 1 {
        s1 = &s1[..(s1.len() - 1)]
    }
    if s2.ends_with(b"/") && s2.len() > 1 {
        s2 = &s2[..(s2.len() - 1)]
    }
    if s1 == s2 {
        return true;
    }
    if !s1.contains(&b'*') && !s2.contains(&b'*') {
        return false;
    }
    if s1.is_empty() || s2.is_empty() {
        return false;
    }
    it_intersect(s1, s2)
}

fn advance(s: &[u8]) -> &[u8] {
    next(s).1
}
DEFINE_INCLUDE!(res_include, end, wild, advance, chunk_include);

/// Retruns `true` if the first key expression (`this`) includes the second key expression (`sub`).
///
/// I.e. if there exists no resource key (with no wildcards) that matches
/// `sub` but does not match `this`.
#[inline(always)]
pub fn include(this: &str, sub: &str) -> bool {
    res_include(this.as_bytes(), sub.as_bytes())
}

pub const ADMIN_PREFIX: &str = "/@/";

#[inline(always)]
pub fn matches(s1: &str, s2: &str) -> bool {
    if s1.starts_with(ADMIN_PREFIX) == s2.starts_with(ADMIN_PREFIX) {
        intersect(s1, s2)
    } else {
        false
    }
}

/// A zenoh **resource** is represented by a pair composed by a **key** and a
/// **value**, such as, ```(/car/telemetry/speed, 320)```.  A **resource key**
/// is an arbitrary array of characters, with the exclusion of the symbols
/// ```*```, ```**```, ```?```, ```[```, ```]```, and ```#```,
/// which have special meaning in the context of zenoh.
///
/// A key including any number of the wildcard symbols, ```*``` and ```**```,
/// such as, ```/car/telemetry/*```, is called a **key expression** as it
/// denotes a set of keys. The wildcard character ```*``` expands to an
/// arbitrary string not including zenoh's reserved characters and the ```/```
/// character, while the ```**``` expands to  strings that may also include the
/// ```/``` character.  
///
/// Finally, it is worth mentioning that for time and space efficiency matters,
/// zenoh will automatically map key expressions to small integers. The mapping is automatic,
/// but it can be triggered excplicily by with `zenoh::Session::declare_expr()`.
///
//
//  7 6 5 4 3 2 1 0
// +-+-+-+-+-+-+-+-+
// ~      id       — if Expr : id=0
// +-+-+-+-+-+-+-+-+
// ~    suffix     ~ if flag K==1 in Message's header
// +---------------+
//
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct WireExpr<'a> {
    pub scope: ExprId, // 0 marks global scope
    pub suffix: Cow<'a, str>,
}

impl<'a> WireExpr<'a> {
    pub fn as_str(&'a self) -> &'a str {
        if self.scope == 0 {
            self.suffix.as_ref()
        } else {
            "<encoded_expr>"
        }
    }

    pub fn try_as_str(&'a self) -> ZResult<&'a str> {
        if self.scope == 0 {
            Ok(self.suffix.as_ref())
        } else {
            bail!("Scoped key expression")
        }
    }

    pub fn as_id(&'a self) -> ExprId {
        self.scope
    }

    pub fn try_as_id(&'a self) -> ZResult<ExprId> {
        if self.has_suffix() {
            bail!("Suffixed key expression")
        } else {
            Ok(self.scope)
        }
    }

    pub fn as_id_and_suffix(&'a self) -> (ExprId, &'a str) {
        (self.scope, self.suffix.as_ref())
    }

    pub fn has_suffix(&self) -> bool {
        !self.suffix.as_ref().is_empty()
    }

    pub fn to_owned(&self) -> WireExpr<'static> {
        WireExpr {
            scope: self.scope,
            suffix: self.suffix.to_string().into(),
        }
    }

    pub fn with_suffix(mut self, suffix: &'a str) -> Self {
        if self.suffix.is_empty() {
            self.suffix = suffix.into();
        } else {
            self.suffix += suffix;
        }
        self
    }
}

impl TryInto<String> for WireExpr<'_> {
    type Error = zenoh_core::Error;
    fn try_into(self) -> Result<String, Self::Error> {
        if self.scope == 0 {
            Ok(self.suffix.into_owned())
        } else {
            bail!("Scoped key expression")
        }
    }
}

impl TryInto<ExprId> for WireExpr<'_> {
    type Error = zenoh_core::Error;
    fn try_into(self) -> Result<ExprId, Self::Error> {
        self.try_as_id()
    }
}

impl fmt::Debug for WireExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.scope == 0 {
            write!(f, "{}", self.suffix)
        } else {
            write!(f, "{}:{}", self.scope, self.suffix)
        }
    }
}

impl fmt::Display for WireExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.scope == 0 {
            write!(f, "{}", self.suffix)
        } else {
            write!(f, "{}:{}", self.scope, self.suffix)
        }
    }
}

impl<'a> From<&WireExpr<'a>> for WireExpr<'a> {
    #[inline]
    fn from(key: &WireExpr<'a>) -> WireExpr<'a> {
        key.clone()
    }
}

impl From<ExprId> for WireExpr<'_> {
    #[inline]
    fn from(rid: ExprId) -> WireExpr<'static> {
        WireExpr {
            scope: rid,
            suffix: "".into(),
        }
    }
}

impl From<&ExprId> for WireExpr<'_> {
    #[inline]
    fn from(rid: &ExprId) -> WireExpr<'static> {
        WireExpr {
            scope: *rid,
            suffix: "".into(),
        }
    }
}

impl<'a> From<&'a str> for WireExpr<'a> {
    #[inline]
    fn from(name: &'a str) -> WireExpr<'a> {
        WireExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

impl From<String> for WireExpr<'_> {
    #[inline]
    fn from(name: String) -> WireExpr<'static> {
        WireExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}

impl<'a> From<&'a String> for WireExpr<'a> {
    #[inline]
    fn from(name: &'a String) -> WireExpr<'a> {
        WireExpr {
            scope: 0,
            suffix: name.into(),
        }
    }
}
