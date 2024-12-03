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
use alloc::{borrow::ToOwned, boxed::Box, string::String, vec::Vec};
use core::{
    fmt::{Debug, Display, Formatter},
    num::NonZeroUsize,
};

enum CowStrInner<'a> {
    Borrowed(&'a str),
    Owned { s: Box<str>, capacity: NonZeroUsize },
}
pub struct CowStr<'a>(CowStrInner<'a>);
impl<'a> CowStr<'a> {
    pub(crate) const fn borrowed(s: &'a str) -> Self {
        Self(CowStrInner::Borrowed(s))
    }
    pub fn as_str(&self) -> &str {
        self
    }
}
impl<'a> From<alloc::borrow::Cow<'a, str>> for CowStr<'a> {
    fn from(value: alloc::borrow::Cow<'a, str>) -> Self {
        match value {
            alloc::borrow::Cow::Borrowed(s) => CowStr::borrowed(s),
            alloc::borrow::Cow::Owned(s) => s.into(),
        }
    }
}
impl<'a> From<&'a str> for CowStr<'a> {
    fn from(value: &'a str) -> Self {
        CowStr::borrowed(value)
    }
}
impl From<String> for CowStr<'_> {
    fn from(s: String) -> Self {
        if s.is_empty() {
            CowStr::borrowed("")
        } else {
            let capacity = unsafe { NonZeroUsize::new_unchecked(s.capacity()) };
            Self(CowStrInner::Owned {
                s: s.into_boxed_str(),
                capacity,
            })
        }
    }
}
impl AsRef<str> for CowStr<'_> {
    fn as_ref(&self) -> &str {
        self
    }
}
impl core::ops::Deref for CowStr<'_> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            CowStrInner::Borrowed(s) => s,
            CowStrInner::Owned { s, .. } => s,
        }
    }
}
impl Clone for CowStr<'_> {
    fn clone(&self) -> Self {
        self.as_str().to_owned().into()
    }
}
impl Debug for CowStr<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(self)
    }
}
impl Display for CowStr<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(self)
    }
}
impl PartialEq for CowStr<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for CowStr<'_> {}
impl core::ops::Add<&str> for CowStr<'_> {
    type Output = String;
    fn add(self, rhs: &str) -> Self::Output {
        match self.0 {
            CowStrInner::Borrowed(s) => {
                let mut ans = String::with_capacity(s.len() + rhs.len());
                ans.push_str(s);
                ans.push_str(rhs);
                ans
            }
            CowStrInner::Owned { mut s, capacity } => unsafe {
                let mut s = String::from_utf8_unchecked(Vec::from_raw_parts(
                    s.as_mut_ptr(),
                    s.len(),
                    capacity.get(),
                ));
                s += rhs;
                s
            },
        }
    }
}
