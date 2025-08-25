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

#[cfg(feature = "internal")]
use alloc::vec::Vec;
use alloc::{
    borrow::{Borrow, ToOwned},
    format,
    string::String,
};
use core::{
    convert::{TryFrom, TryInto},
    fmt,
    ops::{Deref, Div},
};

use zenoh_result::{anyhow, bail, zerror, Error as ZError, ZResult};

use super::{canon::Canonize, OwnedKeyExpr, OwnedNonWildKeyExpr};

/// A [`str`] newtype that is statically known to be a valid key expression.
///
/// The exact key expression specification can be found [here](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Key%20Expressions.md). Here are the major lines:
/// * Key expressions are conceptually a `/`-separated list of UTF-8 string typed chunks. These chunks are not allowed to be empty.
/// * Key expressions must be valid UTF-8 strings.
///   Be aware that Zenoh does not perform UTF normalization for you, so get familiar with that concept if your key expression contains glyphs that may have several unicode representation, such as accented characters.
/// * Key expressions may never start or end with `'/'`, nor contain `"//"` or any of the following characters: `#$?`
/// * Key expression must be in canon-form (this ensure that key expressions representing the same set are always the same string).
///   Note that safe constructors will perform canonization for you if this can be done without extraneous allocations.
///
/// Since Key Expressions define sets of keys, you may want to be aware of the hierarchy of [relations](keyexpr::relation_to) between such sets:
/// * Trivially, two sets can have no elements in common: `a/**` and `b/**` for example define two disjoint sets of keys.
/// * Two sets [intersect](keyexpr::intersects()) if they have at least one element in common. `a/*` intersects `*/a` on `a/a` for example.
/// * One set A [includes](keyexpr::includes()) the other set B if all of B's elements are in A: `a/*/**` includes `a/b/**`
/// * Two sets A and B are equal if all A includes B and B includes A. The Key Expression language is designed so that string equality is equivalent to set equality.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct keyexpr(str);

impl keyexpr {
    /// Equivalent to `<&keyexpr as TryFrom>::try_from(t)`.
    ///
    /// Will return an Err if `t` isn't a valid key expression.
    /// Note that to be considered a valid key expression, a string MUST be canon.
    ///
    /// [`keyexpr::autocanonize`] is an alternative constructor that will canonize the passed expression before constructing it.
    pub fn new<'a, T, E>(t: &'a T) -> Result<&'a Self, E>
    where
        &'a Self: TryFrom<&'a T, Error = E>,
        T: ?Sized,
    {
        t.try_into()
    }

    /// Canonizes the passed value before returning it as a `&keyexpr`.
    ///
    /// Will return Err if the passed value isn't a valid key expression despite canonization.
    ///
    /// Note that this function does not allocate, and will instead mutate the passed value in place during canonization.
    pub fn autocanonize<'a, T, E>(t: &'a mut T) -> Result<&'a Self, E>
    where
        &'a Self: TryFrom<&'a T, Error = E>,
        T: Canonize + ?Sized,
    {
        t.canonize();
        Self::new(t)
    }

    /// Returns `true` if the `keyexpr`s intersect, i.e. there exists at least one key which is contained in both of the sets defined by `self` and `other`.
    pub fn intersects(&self, other: &Self) -> bool {
        use super::intersect::Intersector;
        super::intersect::DEFAULT_INTERSECTOR.intersect(self, other)
    }

    /// Returns `true` if `self` includes `other`, i.e. the set defined by `self` contains every key belonging to the set defined by `other`.
    pub fn includes(&self, other: &Self) -> bool {
        use super::include::Includer;
        super::include::DEFAULT_INCLUDER.includes(self, other)
    }

    /// Returns the relation between `self` and `other` from `self`'s point of view ([`SetIntersectionLevel::Includes`] signifies that `self` includes `other`).
    ///
    /// Note that this is slower than [`keyexpr::intersects`] and [`keyexpr::includes`], so you should favor these methods for most applications.
    #[cfg(feature = "unstable")]
    pub fn relation_to(&self, other: &Self) -> SetIntersectionLevel {
        use SetIntersectionLevel::*;
        if self.intersects(other) {
            if self == other {
                Equals
            } else if self.includes(other) {
                Includes
            } else {
                Intersects
            }
        } else {
            Disjoint
        }
    }

    /// Joins both sides, inserting a `/` in between them.
    ///
    /// This should be your preferred method when concatenating path segments.
    ///
    /// This is notably useful for workspaces:
    /// ```rust
    /// # use core::convert::TryFrom;
    /// # use zenoh_keyexpr::OwnedKeyExpr;
    /// # let get_workspace = || OwnedKeyExpr::try_from("some/workspace").unwrap();
    /// let workspace: OwnedKeyExpr = get_workspace();
    /// let topic = workspace.join("some/topic").unwrap();
    /// ```
    ///
    /// If `other` is of type `&keyexpr`, you may use `self / other` instead, as the joining becomes infallible.
    pub fn join<S: AsRef<str> + ?Sized>(&self, other: &S) -> ZResult<OwnedKeyExpr> {
        OwnedKeyExpr::autocanonize(format!("{}/{}", self, other.as_ref()))
    }

    /// Returns `true` if `self` contains any wildcard character (`**` or `$*`).
    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn is_wild(&self) -> bool {
        self.is_wild_impl()
    }
    pub(crate) fn is_wild_impl(&self) -> bool {
        self.0.contains(super::SINGLE_WILD as char)
    }

    pub(crate) const fn is_double_wild(&self) -> bool {
        let bytes = self.0.as_bytes();
        bytes.len() == 2 && bytes[0] == b'*'
    }

    /// Returns the longest prefix of `self` that doesn't contain any wildcard character (`**` or `$*`).
    ///
    /// NOTE: this operation can typically be used in a backend implementation, at creation of a Storage to get the keys prefix,
    /// and then in `zenoh_backend_traits::Storage::on_sample()` this prefix has to be stripped from all received
    /// `Sample::key_expr` to retrieve the corresponding key.
    ///
    /// # Examples:
    /// ```
    /// # use zenoh_keyexpr::keyexpr;
    /// assert_eq!(
    ///     Some(keyexpr::new("demo/example").unwrap()),
    ///     keyexpr::new("demo/example/**").unwrap().get_nonwild_prefix());
    /// assert_eq!(
    ///     Some(keyexpr::new("demo").unwrap()),
    ///     keyexpr::new("demo/**/test/**").unwrap().get_nonwild_prefix());
    /// assert_eq!(
    ///     Some(keyexpr::new("demo/example/test").unwrap()),
    ///     keyexpr::new("demo/example/test").unwrap().get_nonwild_prefix());
    /// assert_eq!(
    ///     Some(keyexpr::new("demo").unwrap()),
    ///     keyexpr::new("demo/ex$*/**").unwrap().get_nonwild_prefix());
    /// assert_eq!(
    ///     None,
    ///     keyexpr::new("**").unwrap().get_nonwild_prefix());
    /// assert_eq!(
    ///     None,
    ///     keyexpr::new("dem$*").unwrap().get_nonwild_prefix());
    /// ```
    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn get_nonwild_prefix(&self) -> Option<&keyexpr> {
        match self.0.find('*') {
            Some(i) => match self.0[..i].rfind('/') {
                Some(j) => unsafe { Some(keyexpr::from_str_unchecked(&self.0[..j])) },
                None => None, // wildcard in the first segment => no invariant prefix
            },
            None => Some(self), // no wildcard => return self
        }
    }

    /// Remove the specified `prefix` from `self`.
    /// The result is a list of `keyexpr`, since there might be several ways for the prefix to match the beginning of the `self` key expression.
    /// For instance, if `self` is `"a/**/c/*" and `prefix` is `a/b/c` then:
    ///   - the `prefix` matches `"a/**/c"` leading to a result of `"*"` when stripped from `self`
    ///   - the `prefix` matches `"a/**"` leading to a result of `"**/c/*"` when stripped from `self`
    ///
    /// So the result is `["*", "**/c/*"]`.
    /// If `prefix` cannot match the beginning of `self`, an empty list is reuturned.
    ///
    /// See below more examples.
    ///
    /// NOTE: this operation can typically used in a backend implementation, within the `zenoh_backend_traits::Storage::on_query()` implementation,
    /// to transform the received `Query::selector()`'s `key_expr` into a list of key selectors
    /// that will match all the relevant stored keys (that correspond to keys stripped from the prefix).
    ///
    /// # Examples:
    /// ```
    /// # use core::convert::{TryFrom, TryInto};
    /// # use zenoh_keyexpr::keyexpr;
    /// assert_eq!(
    ///     ["abc"],
    ///     keyexpr::new("demo/example/test/abc").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["**"],
    ///     keyexpr::new("demo/example/test/**").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["**"],
    ///     keyexpr::new("demo/example/**").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["**"],
    ///     keyexpr::new("**").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["**/xyz"],
    ///     keyexpr::new("demo/**/xyz").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["**"],
    ///     keyexpr::new("demo/**/test/**").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["xyz", "**/ex$*/*/xyz"],
    ///     keyexpr::new("demo/**/ex$*/*/xyz").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert_eq!(
    ///     ["*", "**/test/*"],
    ///     keyexpr::new("demo/**/test/*").unwrap().strip_prefix(keyexpr::new("demo/example/test").unwrap()).as_slice()
    /// );
    /// assert!(
    ///     keyexpr::new("demo/example/test/**").unwrap().strip_prefix(keyexpr::new("not/a/prefix").unwrap()).is_empty()
    /// );
    /// ```
    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn strip_prefix(&self, prefix: &Self) -> Vec<&keyexpr> {
        let mut result = alloc::vec![];
        'chunks: for i in (0..=self.len()).rev() {
            if if i == self.len() {
                self.ends_with("**")
            } else {
                self.as_bytes()[i] == b'/'
            } {
                let sub_part = keyexpr::new(&self[..i]).unwrap();
                if sub_part.intersects(prefix) {
                    // if sub_part ends with "**", keep those in remaining part
                    let remaining = if sub_part.ends_with("**") {
                        &self[i - 2..]
                    } else {
                        &self[i + 1..]
                    };
                    let remaining: &keyexpr = if remaining.is_empty() {
                        continue 'chunks;
                    } else {
                        remaining
                    }
                    .try_into()
                    .unwrap();
                    // if remaining is "**" return only this since it covers all
                    if remaining.as_bytes() == b"**" {
                        result.clear();
                        result.push(unsafe { keyexpr::from_str_unchecked(remaining) });
                        return result;
                    }
                    for i in (0..(result.len())).rev() {
                        if result[i].includes(remaining) {
                            continue 'chunks;
                        }
                        if remaining.includes(result[i]) {
                            result.swap_remove(i);
                        }
                    }
                    result.push(remaining);
                }
            }
        }
        result
    }

    /// Remove the specified namespace `prefix` from `self`.
    ///
    /// This method works essentially like [`keyexpr::strip_prefix()`], but returns only the longest possible suffix.
    /// Prefix can not contain '*' character.
    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn strip_nonwild_prefix(&self, prefix: &nonwild_keyexpr) -> Option<&keyexpr> {
        fn is_chunk_matching(target: &[u8], prefix: &[u8]) -> bool {
            let mut target_idx: usize = 0;
            let mut prefix_idx: usize = 0;
            let mut target_prev: u8 = b'/';
            if prefix.first() == Some(&b'@') && target.first() != Some(&b'@') {
                // verbatim chunk can only be matched by verbatim chunk
                return false;
            }

            while target_idx < target.len() && prefix_idx < prefix.len() {
                if target[target_idx] == b'*' {
                    if target_prev == b'*' || target_idx + 1 == target.len() {
                        // either a ** wild chunk or a single * chunk at the end of the string - this matches anything
                        return true;
                    } else if target_prev == b'$' {
                        for i in prefix_idx..prefix.len() - 1 {
                            if is_chunk_matching(&target[target_idx + 1..], &prefix[i..]) {
                                return true;
                            }
                        }
                    }
                } else if target[target_idx] == prefix[prefix_idx] {
                    prefix_idx += 1;
                } else if target[target_idx] != b'$' {
                    // non-special character, which do not match the one in prefix
                    return false;
                }
                target_prev = target[target_idx];
                target_idx += 1;
            }
            if prefix_idx != prefix.len() {
                // prefix was not matched entirely
                return false;
            }
            target_idx == target.len()
                || (target_idx + 2 == target.len() && target[target_idx] == b'$')
        }

        fn strip_nonwild_prefix_inner<'a>(
            target_bytes: &'a [u8],
            prefix_bytes: &[u8],
        ) -> Option<&'a keyexpr> {
            let mut target_idx = 0;
            let mut prefix_idx = 0;

            while target_idx < target_bytes.len() && prefix_idx < prefix_bytes.len() {
                let target_end = target_idx
                    + target_bytes[target_idx..]
                        .iter()
                        .position(|&i| i == b'/')
                        .unwrap_or(target_bytes.len() - target_idx);
                let prefix_end = prefix_idx
                    + prefix_bytes[prefix_idx..]
                        .iter()
                        .position(|&i| i == b'/')
                        .unwrap_or(prefix_bytes.len() - prefix_idx);
                let target_chunk = &target_bytes[target_idx..target_end];
                if target_chunk.len() == 2 && target_chunk[0] == b'*' {
                    let remaining_prefix = &prefix_bytes[prefix_idx..];
                    return match remaining_prefix.iter().position(|&x| x == b'@') {
                        Some(mut p) => {
                            if target_end + 1 >= target_bytes.len() {
                                // "**" is the last chunk, and it is not allowed to match @verbatim chunks, so we stop here
                                return None;
                            } else {
                                loop {
                                    // try to use "**" to match as many non-verbatim chunks as possible
                                    if let Some(ke) = strip_nonwild_prefix_inner(
                                        &target_bytes[(target_end + 1)..],
                                        &remaining_prefix[p..],
                                    ) {
                                        return Some(ke);
                                    } else if p == 0 {
                                        // "**" is not allowed to match @verbatim chunks
                                        return None;
                                    } else {
                                        // search for the beginning of the next chunk from the end
                                        p -= 2;
                                        while p > 0 && remaining_prefix[p - 1] != b'/' {
                                            p -= 1;
                                        }
                                    }
                                }
                            }
                        }
                        None => unsafe {
                            // "**" can match all remaining non-verbatim chunks
                            Some(keyexpr::from_str_unchecked(core::str::from_utf8_unchecked(
                                &target_bytes[target_idx..],
                            )))
                        },
                    };
                }
                if target_end == target_bytes.len() {
                    // target contains no more chunks than prefix and the last one is non double-wild - so it can not match
                    return None;
                }
                let prefix_chunk = &prefix_bytes[prefix_idx..prefix_end];
                if !is_chunk_matching(target_chunk, prefix_chunk) {
                    return None;
                }
                if prefix_end == prefix_bytes.len() {
                    // Safety: every chunk of keyexpr is also a valid keyexpr
                    return unsafe {
                        Some(keyexpr::from_str_unchecked(core::str::from_utf8_unchecked(
                            &target_bytes[(target_end + 1)..],
                        )))
                    };
                }
                target_idx = target_end + 1;
                prefix_idx = prefix_end + 1;
            }
            None
        }

        let target_bytes = self.0.as_bytes();
        let prefix_bytes = prefix.0.as_bytes();

        strip_nonwild_prefix_inner(target_bytes, prefix_bytes)
    }

    pub const fn as_str(&self) -> &str {
        &self.0
    }

    /// # Safety
    /// This constructs a [`keyexpr`] without ensuring that it is a valid key-expression.
    ///
    /// Much like [`core::str::from_utf8_unchecked`], this is memory-safe, but calling this without maintaining
    /// [`keyexpr`]'s invariants yourself may lead to unexpected behaviors, the Zenoh network dropping your messages.
    pub const unsafe fn from_str_unchecked(s: &str) -> &Self {
        core::mem::transmute(s)
    }

    /// # Safety
    /// This constructs a [`keyexpr`] without ensuring that it is a valid key-expression.
    ///
    /// Much like [`core::str::from_utf8_unchecked`], this is memory-safe, but calling this without maintaining
    /// [`keyexpr`]'s invariants yourself may lead to unexpected behaviors, the Zenoh network dropping your messages.
    pub unsafe fn from_slice_unchecked(s: &[u8]) -> &Self {
        core::mem::transmute(s)
    }

    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub const fn chunks(&self) -> Chunks<'_> {
        self.chunks_impl()
    }
    pub(crate) const fn chunks_impl(&self) -> Chunks<'_> {
        Chunks {
            inner: self.as_str(),
        }
    }
    pub(crate) fn next_delimiter(&self, i: usize) -> Option<usize> {
        self.as_str()
            .get(i + 1..)
            .and_then(|s| s.find('/').map(|j| i + 1 + j))
    }
    pub(crate) fn previous_delimiter(&self, i: usize) -> Option<usize> {
        self.as_str().get(..i).and_then(|s| s.rfind('/'))
    }
    pub(crate) fn first_byte(&self) -> u8 {
        unsafe { *self.as_bytes().get_unchecked(0) }
    }
    pub(crate) fn iter_splits_ltr(&self) -> SplitsLeftToRight<'_> {
        SplitsLeftToRight {
            inner: self,
            index: 0,
        }
    }
    pub(crate) fn iter_splits_rtl(&self) -> SplitsRightToLeft<'_> {
        SplitsRightToLeft {
            inner: self,
            index: self.len(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SplitsLeftToRight<'a> {
    inner: &'a keyexpr,
    index: usize,
}
impl<'a> SplitsLeftToRight<'a> {
    fn right(&self) -> &'a str {
        &self.inner[self.index + ((self.index != 0) as usize)..]
    }
    fn left(&self, followed_by_double: bool) -> &'a str {
        &self.inner[..(self.index + ((self.index != 0) as usize + 2) * followed_by_double as usize)]
    }
}
impl<'a> Iterator for SplitsLeftToRight<'a> {
    type Item = (&'a keyexpr, &'a keyexpr);
    fn next(&mut self) -> Option<Self::Item> {
        match self.index < self.inner.len() {
            false => None,
            true => {
                let right = self.right();
                let double_wild = right.starts_with("**");
                let left = self.left(double_wild);
                self.index = if left.is_empty() {
                    self.inner.next_delimiter(0).unwrap_or(self.inner.len())
                } else {
                    self.inner
                        .next_delimiter(left.len())
                        .unwrap_or(self.inner.len() + (left.len() == self.inner.len()) as usize)
                };
                if left.is_empty() {
                    self.next()
                } else {
                    // SAFETY: because any keyexpr split at `/` becomes 2 valid keyexprs by design, it's safe to assume the constraint is valid once both sides have been validated to not be empty.
                    (!right.is_empty()).then(|| unsafe {
                        (
                            keyexpr::from_str_unchecked(left),
                            keyexpr::from_str_unchecked(right),
                        )
                    })
                }
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SplitsRightToLeft<'a> {
    inner: &'a keyexpr,
    index: usize,
}
impl<'a> SplitsRightToLeft<'a> {
    fn right(&self, followed_by_double: bool) -> &'a str {
        &self.inner[(self.index
            - ((self.index != self.inner.len()) as usize + 2) * followed_by_double as usize)..]
    }
    fn left(&self) -> &'a str {
        &self.inner[..(self.index - ((self.index != self.inner.len()) as usize))]
    }
}
impl<'a> Iterator for SplitsRightToLeft<'a> {
    type Item = (&'a keyexpr, &'a keyexpr);
    fn next(&mut self) -> Option<Self::Item> {
        match self.index {
            0 => None,
            _ => {
                let left = self.left();
                let double_wild = left.ends_with("**");
                let right = self.right(double_wild);
                self.index = if right.is_empty() {
                    self.inner
                        .previous_delimiter(self.inner.len())
                        .map_or(0, |n| n + 1)
                } else {
                    self.inner
                        .previous_delimiter(
                            self.inner.len()
                                - right.len()
                                - (self.inner.len() != right.len()) as usize,
                        )
                        .map_or(0, |n| n + 1)
                };
                if right.is_empty() {
                    self.next()
                } else {
                    // SAFETY: because any keyexpr split at `/` becomes 2 valid keyexprs by design, it's safe to assume the constraint is valid once both sides have been validated to not be empty.
                    (!left.is_empty()).then(|| unsafe {
                        (
                            keyexpr::from_str_unchecked(left),
                            keyexpr::from_str_unchecked(right),
                        )
                    })
                }
            }
        }
    }
}
#[test]
fn splits() {
    let ke = keyexpr::new("a/**/b/c").unwrap();
    let mut splits = ke.iter_splits_ltr();
    assert_eq!(
        splits.next(),
        Some((
            keyexpr::new("a/**").unwrap(),
            keyexpr::new("**/b/c").unwrap()
        ))
    );
    assert_eq!(
        splits.next(),
        Some((keyexpr::new("a/**/b").unwrap(), keyexpr::new("c").unwrap()))
    );
    assert_eq!(splits.next(), None);
    let mut splits = ke.iter_splits_rtl();
    assert_eq!(
        splits.next(),
        Some((keyexpr::new("a/**/b").unwrap(), keyexpr::new("c").unwrap()))
    );
    assert_eq!(
        splits.next(),
        Some((
            keyexpr::new("a/**").unwrap(),
            keyexpr::new("**/b/c").unwrap()
        ))
    );
    assert_eq!(splits.next(), None);
    let ke = keyexpr::new("**").unwrap();
    let mut splits = ke.iter_splits_ltr();
    assert_eq!(
        splits.next(),
        Some((keyexpr::new("**").unwrap(), keyexpr::new("**").unwrap()))
    );
    assert_eq!(splits.next(), None);
    let ke = keyexpr::new("ab").unwrap();
    let mut splits = ke.iter_splits_ltr();
    assert_eq!(splits.next(), None);
    let ke = keyexpr::new("ab/cd").unwrap();
    let mut splits = ke.iter_splits_ltr();
    assert_eq!(
        splits.next(),
        Some((keyexpr::new("ab").unwrap(), keyexpr::new("cd").unwrap()))
    );
    assert_eq!(splits.next(), None);
    for (i, ke) in crate::fuzzer::KeyExprFuzzer(rand::thread_rng())
        .take(100)
        .enumerate()
    {
        dbg!(i, &ke);
        let splits = ke.iter_splits_ltr().collect::<Vec<_>>();
        assert_eq!(splits, {
            let mut rtl_rev = ke.iter_splits_rtl().collect::<Vec<_>>();
            rtl_rev.reverse();
            rtl_rev
        });
        assert!(!splits
            .iter()
            .any(|s| s.0.ends_with('/') || s.1.starts_with('/')));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chunks<'a> {
    inner: &'a str,
}
impl<'a> Chunks<'a> {
    /// Convert the remaining part of the iterator to a keyexpr if it is not empty.
    pub const fn as_keyexpr(self) -> Option<&'a keyexpr> {
        match self.inner.is_empty() {
            true => None,
            _ => Some(unsafe { keyexpr::from_str_unchecked(self.inner) }),
        }
    }
    /// Peek at the next chunk without consuming it.
    pub fn peek(&self) -> Option<&keyexpr> {
        if self.inner.is_empty() {
            None
        } else {
            Some(unsafe {
                keyexpr::from_str_unchecked(
                    &self.inner[..self.inner.find('/').unwrap_or(self.inner.len())],
                )
            })
        }
    }
    /// Peek at the last chunk without consuming it.
    pub fn peek_back(&self) -> Option<&keyexpr> {
        if self.inner.is_empty() {
            None
        } else {
            Some(unsafe {
                keyexpr::from_str_unchecked(
                    &self.inner[self.inner.rfind('/').map_or(0, |i| i + 1)..],
                )
            })
        }
    }
}
impl<'a> Iterator for Chunks<'a> {
    type Item = &'a keyexpr;
    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.is_empty() {
            return None;
        }
        let (next, inner) = self.inner.split_once('/').unwrap_or((self.inner, ""));
        self.inner = inner;
        Some(unsafe { keyexpr::from_str_unchecked(next) })
    }
}
impl DoubleEndedIterator for Chunks<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.inner.is_empty() {
            return None;
        }
        let (inner, next) = self.inner.rsplit_once('/').unwrap_or(("", self.inner));
        self.inner = inner;
        Some(unsafe { keyexpr::from_str_unchecked(next) })
    }
}

impl Div for &keyexpr {
    type Output = OwnedKeyExpr;
    fn div(self, rhs: Self) -> Self::Output {
        self.join(rhs).unwrap() // Joining 2 key expressions should always result in a canonizable string.
    }
}

/// The possible relations between two sets.
///
/// Note that [`Equals`](SetIntersectionLevel::Equals) implies [`Includes`](SetIntersectionLevel::Includes), which itself implies [`Intersects`](SetIntersectionLevel::Intersects).
///
/// You can check for intersection with `level >= SetIntersecionLevel::Intersection` and for inclusion with `level >= SetIntersectionLevel::Includes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg(feature = "unstable")]
pub enum SetIntersectionLevel {
    Disjoint,
    Intersects,
    Includes,
    Equals,
}

#[cfg(feature = "unstable")]
#[test]
fn intersection_level_cmp() {
    use SetIntersectionLevel::*;
    assert!(Disjoint < Intersects);
    assert!(Intersects < Includes);
    assert!(Includes < Equals);
}

impl fmt::Debug for keyexpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ke`{}`", self.as_ref())
    }
}

impl fmt::Display for keyexpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

#[repr(i8)]
enum KeyExprError {
    LoneDollarStar = -1,
    SingleStarAfterDoubleStar = -2,
    DoubleStarAfterDoubleStar = -3,
    EmptyChunk = -4,
    StarInChunk = -5,
    DollarAfterDollar = -6,
    SharpOrQMark = -7,
    UnboundDollar = -8,
}

impl KeyExprError {
    #[cold]
    fn into_err(self, s: &str) -> ZError {
        let error = match &self {
            Self::LoneDollarStar => anyhow!("Invalid Key Expr `{s}`: empty chunks are forbidden, as well as leading and trailing slashes"),
            Self::SingleStarAfterDoubleStar => anyhow!("Invalid Key Expr `{s}`: `**/*` must be replaced by `*/**` to reach canon-form"),
            Self::DoubleStarAfterDoubleStar => anyhow!("Invalid Key Expr `{s}`: `**/**` must be replaced by `**` to reach canon-form"),
            Self::EmptyChunk => anyhow!("Invalid Key Expr `{s}`: empty chunks are forbidden, as well as leading and trailing slashes"),
            Self::StarInChunk => anyhow!("Invalid Key Expr `{s}`: `*` may only be preceded by `/` or `$`"),
            Self::DollarAfterDollar => anyhow!("Invalid Key Expr `{s}`: `$` is not allowed after `$*`"),
            Self::SharpOrQMark => anyhow!("Invalid Key Expr `{s}`: `#` and `?` are forbidden characters"),
            Self::UnboundDollar => anyhow!("Invalid Key Expr `{s}`: `$` is only allowed in `$*`")
        };
        zerror!((self) error).into()
    }
}

impl<'a> TryFrom<&'a str> for &'a keyexpr {
    type Error = ZError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        use KeyExprError::*;
        // Check for emptiness or trailing slash, as they are not caught after.
        if value.is_empty() || value.ends_with('/') {
            return Err(EmptyChunk.into_err(value));
        }
        let bytes = value.as_bytes();
        // The start of the chunk, i.e. the index of the char after the '/'.
        let mut chunk_start = 0;
        // Use a while loop to scan the string because it requires to advance the iteration
        // manually for some characters, e.g. '$'.
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                // In UTF-8, all keyexpr special characters are lesser or equal to '/', except '?'
                // This shortcut greatly reduce the number of operations for alphanumeric
                // characters, which are the most common in keyexprs.
                c if c > b'/' && c != b'?' => i += 1,
                // A chunk cannot start with '/'
                b'/' if i == chunk_start => return Err(EmptyChunk.into_err(value)),
                // `chunk_start` is updated when starting a new chunk.
                b'/' => {
                    i += 1;
                    chunk_start = i;
                }
                // The first encountered '*' must be at the beginning of a chunk
                b'*' if i != chunk_start => return Err(StarInChunk.into_err(value)),
                // When a '*' is match, it means this is a wildcard chunk, possibly with two "**",
                // which must be followed by a '/' or the end of the string.
                // So the next character is checked, and the cursor
                b'*' => match bytes.get(i + 1) {
                    // Break if end of string is reached.
                    None => break,
                    // If a '/' is found, start a new chunk, and advance the cursor to take in
                    // previous check.
                    Some(&b'/') => {
                        i += 2;
                        chunk_start = i;
                    }
                    // If a second '*' is found, the next character must be a slash.
                    Some(&b'*') => match bytes.get(i + 2) {
                        // Break if end of string is reached.
                        None => break,
                        // Because a "**" chunk cannot be followed by "*" or "**", the next char is
                        // checked to not be a '*'.
                        Some(&b'/') if matches!(bytes.get(i + 3), Some(b'*')) => {
                            // If there are two consecutive wildcard chunks, raise the appropriate
                            // error.
                            #[cold]
                            fn double_star_err(value: &str, i: usize) -> ZError {
                                match (value.as_bytes().get(i + 4), value.as_bytes().get(i + 5)) {
                                    (None | Some(&b'/'), _) => SingleStarAfterDoubleStar,
                                    (Some(&b'*'), None | Some(&b'/')) => DoubleStarAfterDoubleStar,
                                    _ => StarInChunk,
                                }
                                .into_err(value)
                            }
                            return Err(double_star_err(value, i));
                        }
                        // If a '/' is found, start a new chunk, and advance the cursor to take in
                        // previous checks.
                        Some(&b'/') => {
                            i += 3;
                            chunk_start = i;
                        }
                        // This is not a "**" chunk, raise an error.
                        _ => return Err(StarInChunk.into_err(value)),
                    },
                    // This is not a "*" chunk, raise an error.
                    _ => return Err(StarInChunk.into_err(value)),
                },
                // A '$' must be followed by '*'.
                b'$' if bytes.get(i + 1) != Some(&b'*') => {
                    return Err(UnboundDollar.into_err(value))
                }
                // "$*" has some additional rules to check.
                b'$' => match bytes.get(i + 2) {
                    // "$*" cannot be followed by '$'.
                    Some(&b'$') => return Err(DollarAfterDollar.into_err(value)),
                    // "$*" cannot be alone in a chunk
                    Some(&b'/') | None if i == chunk_start => {
                        return Err(LoneDollarStar.into_err(value))
                    }
                    // Break if end of string is reached.
                    None => break,
                    // Everything is fine, advance the cursor taking the '*' check in account.
                    _ => i += 2,
                },
                // '#' and '?' are forbidden.
                b'#' | b'?' => return Err(SharpOrQMark.into_err(value)),
                // Fallback for unmatched characters
                _ => i += 1,
            }
        }
        Ok(unsafe { keyexpr::from_str_unchecked(value) })
    }
}

impl<'a> TryFrom<&'a mut str> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a mut str) -> Result<Self, Self::Error> {
        (value as &'a str).try_into()
    }
}

impl<'a> TryFrom<&'a mut String> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a mut String) -> Result<Self, Self::Error> {
        (value.as_str()).try_into()
    }
}

impl<'a> TryFrom<&'a String> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a String) -> Result<Self, Self::Error> {
        (value.as_str()).try_into()
    }
}
impl<'a> TryFrom<&'a &'a str> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a &'a str) -> Result<Self, Self::Error> {
        (*value).try_into()
    }
}
impl<'a> TryFrom<&'a &'a mut str> for &'a keyexpr {
    type Error = ZError;
    fn try_from(value: &'a &'a mut str) -> Result<Self, Self::Error> {
        keyexpr::new(*value)
    }
}
#[test]
fn autocanon() {
    let mut s: Box<str> = Box::from("hello/**/*");
    let mut s: &mut str = &mut s;
    assert_eq!(keyexpr::autocanonize(&mut s).unwrap(), "hello/*/**");
}

impl Deref for keyexpr {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}
impl AsRef<str> for keyexpr {
    fn as_ref(&self) -> &str {
        self
    }
}

impl PartialEq<str> for keyexpr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<keyexpr> for str {
    fn eq(&self, other: &keyexpr) -> bool {
        self == other.as_str()
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

/// A keyexpr that is statically known not to contain any wild chunks.
#[allow(non_camel_case_types)]
#[repr(transparent)]
#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct nonwild_keyexpr(keyexpr);

impl nonwild_keyexpr {
    /// Attempts to construct a non-wild key expression from anything convertible to keyexpression.
    ///
    /// Will return an Err if `t` isn't a valid key expression.
    pub fn new<'a, T, E>(t: &'a T) -> Result<&'a Self, ZError>
    where
        &'a keyexpr: TryFrom<&'a T, Error = E>,
        E: Into<ZError>,
        T: ?Sized,
    {
        let ke: &'a keyexpr = t.try_into().map_err(|e: E| e.into())?;
        ke.try_into()
    }

    /// # Safety
    /// This constructs a [`nonwild_keyexpr`] without ensuring that it is a valid key-expression without wild chunks.
    ///
    /// Much like [`core::str::from_utf8_unchecked`], this is memory-safe, but calling this without maintaining
    /// [`nonwild_keyexpr`]'s invariants yourself may lead to unexpected behaviors, the Zenoh network dropping your messages.
    pub const unsafe fn from_str_unchecked(s: &str) -> &Self {
        core::mem::transmute(s)
    }
}

impl Deref for nonwild_keyexpr {
    type Target = keyexpr;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> TryFrom<&'a keyexpr> for &'a nonwild_keyexpr {
    type Error = ZError;
    fn try_from(value: &'a keyexpr) -> Result<Self, Self::Error> {
        if value.is_wild_impl() {
            bail!("nonwild_keyexpr can not contain any wild chunks")
        }
        Ok(unsafe { core::mem::transmute::<&keyexpr, &nonwild_keyexpr>(value) })
    }
}

impl Borrow<nonwild_keyexpr> for OwnedNonWildKeyExpr {
    fn borrow(&self) -> &nonwild_keyexpr {
        self
    }
}

impl ToOwned for nonwild_keyexpr {
    type Owned = OwnedNonWildKeyExpr;
    fn to_owned(&self) -> Self::Owned {
        OwnedNonWildKeyExpr::from(self)
    }
}

#[cfg(feature = "internal")]
#[test]
fn test_keyexpr_strip_prefix() {
    let expectations = [
        (("demo/example/test/**", "demo/example/test"), &["**"][..]),
        (("demo/example/**", "demo/example/test"), &["**"]),
        (("**", "demo/example/test"), &["**"]),
        (
            ("demo/example/test/**/x$*/**", "demo/example/test"),
            &["**/x$*/**"],
        ),
        (("demo/**/xyz", "demo/example/test"), &["**/xyz"]),
        (("demo/**/test/**", "demo/example/test"), &["**"]),
        (
            ("demo/**/ex$*/*/xyz", "demo/example/test"),
            ["xyz", "**/ex$*/*/xyz"].as_ref(),
        ),
        (
            ("demo/**/ex$*/t$*/xyz", "demo/example/test"),
            ["xyz", "**/ex$*/t$*/xyz"].as_ref(),
        ),
        (
            ("demo/**/te$*/*/xyz", "demo/example/test"),
            ["*/xyz", "**/te$*/*/xyz"].as_ref(),
        ),
        (("demo/example/test", "demo/example/test"), [].as_ref()),
    ]
    .map(|((a, b), expected)| {
        (
            (keyexpr::new(a).unwrap(), keyexpr::new(b).unwrap()),
            expected
                .iter()
                .map(|s| keyexpr::new(*s).unwrap())
                .collect::<Vec<_>>(),
        )
    });
    for ((ke, prefix), expected) in expectations {
        dbg!(ke, prefix);
        assert_eq!(ke.strip_prefix(prefix), expected)
    }
}

#[cfg(feature = "internal")]
#[test]
fn test_keyexpr_strip_nonwild_prefix() {
    let expectations = [
        (("demo/example/test/**", "demo/example/test"), Some("**")),
        (("demo/example/**", "demo/example/test"), Some("**")),
        (("**", "demo/example/test"), Some("**")),
        (("*/example/test/1", "demo/example/test"), Some("1")),
        (("demo/*/test/1", "demo/example/test"), Some("1")),
        (("*/*/test/1", "demo/example/test"), Some("1")),
        (("*/*/*/1", "demo/example/test"), Some("1")),
        (("*/test/1", "demo/example/test"), None),
        (("*/*/1", "demo/example/test"), None),
        (("*/*/**", "demo/example/test"), Some("**")),
        (
            ("demo/example/test/**/x$*/**", "demo/example/test"),
            Some("**/x$*/**"),
        ),
        (("demo/**/xyz", "demo/example/test"), Some("**/xyz")),
        (("demo/**/test/**", "demo/example/test"), Some("**/test/**")),
        (
            ("demo/**/ex$*/*/xyz", "demo/example/test"),
            Some("**/ex$*/*/xyz"),
        ),
        (
            ("demo/**/ex$*/t$*/xyz", "demo/example/test"),
            Some("**/ex$*/t$*/xyz"),
        ),
        (
            ("demo/**/te$*/*/xyz", "demo/example/test"),
            Some("**/te$*/*/xyz"),
        ),
        (("demo/example/test", "demo/example/test"), None),
        (("demo/example/test1/something", "demo/example/test"), None),
        (
            ("demo/example/test$*/something", "demo/example/test"),
            Some("something"),
        ),
        (("*/example/test/something", "@demo/example/test"), None),
        (("**/test/something", "@demo/example/test"), None),
        (("**/test/something", "demo/example/@test"), None),
        (
            ("@demo/*/test/something", "@demo/example/test"),
            Some("something"),
        ),
        (("@demo/*/test/something", "@demo/@example/test"), None),
        (("**/@demo/test/something", "@demo/test"), Some("something")),
        (("**/@test/something", "demo/@test"), Some("something")),
        (
            ("@demo/**/@test/something", "@demo/example/@test"),
            Some("something"),
        ),
    ]
    .map(|((a, b), expected)| {
        (
            (keyexpr::new(a).unwrap(), nonwild_keyexpr::new(b).unwrap()),
            expected.map(|t| keyexpr::new(t).unwrap()),
        )
    });
    for ((ke, prefix), expected) in expectations {
        dbg!(ke, prefix);
        assert_eq!(ke.strip_nonwild_prefix(prefix), expected)
    }
}

#[cfg(test)]
mod tests {
    use test_case::test_case;
    use zenoh_result::ErrNo;

    use crate::{
        key_expr::borrowed::{KeyExprError, KeyExprError::*},
        keyexpr,
    };

    #[test_case("", EmptyChunk; "Empty")]
    #[test_case("demo/example/test", None; "Normal key_expr")]
    #[test_case("demo/*", None; "Single star at the end")]
    #[test_case("demo/**", None; "Double star at the end")]
    #[test_case("demo/*/*/test", None; "Single star after single star")]
    #[test_case("demo/*/**/test", None; "Double star after single star")]
    #[test_case("demo/example$*/test", None; "Dollar with star")]
    #[test_case("demo/example$*-$*/test", None; "Multiple dollar with star")]
    #[test_case("/demo/example/test", EmptyChunk; "Leading /")]
    #[test_case("demo/example/test/", EmptyChunk; "Trailing /")]
    #[test_case("demo/$*/test", LoneDollarStar; "Lone $*")]
    #[test_case("demo/$*", LoneDollarStar; "Lone $* at the end")]
    #[test_case("demo/example$*", None; "Trailing lone $*")]
    #[test_case("demo/**/*/test", SingleStarAfterDoubleStar; "Single star after double star")]
    #[test_case("demo/**/**/test", DoubleStarAfterDoubleStar; "Double star after double star")]
    #[test_case("demo//test", EmptyChunk; "Empty Chunk")]
    #[test_case("demo/exam*ple/test", StarInChunk; "Star in chunk")]
    #[test_case("demo/example$*$/test", DollarAfterDollar; "Dollar after dollar or star")]
    #[test_case("demo/example$*$*/test", DollarAfterDollar; "Dollar star after dollar or star")]
    #[test_case("demo/example#/test", SharpOrQMark; "Contain sharp")]
    #[test_case("demo/example?/test", SharpOrQMark; "Contain mark")]
    #[test_case("demo/$/test", UnboundDollar; "Contain unbounded dollar")]
    fn test_keyexpr_new(key_str: &str, error: impl Into<Option<KeyExprError>>) {
        assert_eq!(
            keyexpr::new(key_str).err().map(|err| {
                err.downcast_ref::<zenoh_result::ZError>()
                    .unwrap()
                    .errno()
                    .get()
            }),
            error.into().map(|err| err as i8)
        );
    }
}
