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
use super::{canon::Canonizable, OwnedKeyExpr, FORBIDDEN_CHARS};
// use crate::core::WireExpr;
use alloc::{
    borrow::{Borrow, ToOwned},
    format,
    string::String,
    vec,
    vec::Vec,
};
use core::{
    convert::{TryFrom, TryInto},
    fmt,
    ops::{Deref, Div},
};
use zenoh_result::{bail, Error as ZError, ZResult};

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
#[derive(PartialEq, Eq, Hash)]
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
        T: Canonizable + ?Sized,
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
    /// This should be your prefered method when concatenating path segments.
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
    pub fn is_wild(&self) -> bool {
        self.0.contains(super::SINGLE_WILD as char)
    }

    /// Returns the longest prefix of `self` that doesn't contain any wildcard character (`**` or `$*`).
    ///
    /// NOTE: this operation can typically be used in a backend implementation, at creation of a Storage to get the keys prefix,
    /// and then in `zenoh_backend_traits::Storage::on_sample()` this prefix has to be stripped from all received
    /// `Sample::key_expr` to retrieve the corrsponding key.
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
    /// The result is a list of `keyexpr`, since there might be several ways for the prefix to match the begining of the `self` key expression.  
    /// For instance, if `self` is `"a/**/c/*" and `prefix` is `a/b/c` then:  
    ///   - the `prefix` matches `"a/**/c"` leading to a result of `"*"` when stripped from `self`
    ///   - the `prefix` matches `"a/**"` leading to a result of `"**/c/*"` when stripped from `self`
    /// So the result is `["*", "**/c/*"]`.  
    /// If `prefix` cannot match the begining of `self`, an empty list is reuturned.
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
    pub fn strip_prefix(&self, prefix: &Self) -> Vec<&keyexpr> {
        let mut result = vec![];
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

    pub fn as_str(&self) -> &str {
        self
    }

    /// # Safety
    /// This constructs a [`keyexpr`] without ensuring that it is a valid key-expression.
    ///
    /// Much like [`core::str::from_utf8_unchecked`], this is memory-safe, but calling this without maintaining
    /// [`keyexpr`]'s invariants yourself may lead to unexpected behaviors, the Zenoh network dropping your messages.
    pub unsafe fn from_str_unchecked(s: &str) -> &Self {
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
    pub fn chunks(&self) -> impl Iterator<Item = &Self> + DoubleEndedIterator {
        self.split('/').map(|c| unsafe {
            // Any chunk of a valid KE is itself a valid KE => we can safely call the unchecked constructor.
            Self::from_str_unchecked(c)
        })
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
pub enum SetIntersectionLevel {
    Disjoint,
    Intersects,
    Includes,
    Equals,
}

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
enum KeyExprConstructionError {
    LoneDollarStar = -1,
    SingleStarAfterDoubleStar = -2,
    DoubleStarAfterDoubleStar = -3,
    EmpyChunk = -4,
    StarsInChunk = -5,
    DollarAfterDollarOrStar = -6,
    ContainsSharpOrQMark = -7,
    ContainsUnboundDollar = -8,
}

impl<'a> TryFrom<&'a str> for &'a keyexpr {
    type Error = ZError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let mut in_big_wild = false;
        for chunk in value.split('/') {
            if chunk.is_empty() {
                bail!((KeyExprConstructionError::EmpyChunk) "Invalid Key Expr `{}`: empty chunks are forbidden, as well as leading and trailing slashes", value)
            }
            if chunk == "$*" {
                bail!((KeyExprConstructionError::LoneDollarStar)
                    "Invalid Key Expr `{}`: lone `$*`s must be replaced by `*` to reach canon-form",
                    value
                )
            }
            if in_big_wild {
                match chunk {
                    "**" => bail!((KeyExprConstructionError::DoubleStarAfterDoubleStar)
                        "Invalid Key Expr `{}`: `**/**` must be replaced by `**` to reach canon-form",
                        value
                    ),
                    "*" => bail!((KeyExprConstructionError::SingleStarAfterDoubleStar)
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
                if chunk != "*" {
                    let mut split = chunk.split('*');
                    split.next_back();
                    if split.any(|s| !s.ends_with('$')) {
                        bail!((KeyExprConstructionError::StarsInChunk)
                            "Invalid Key Expr `{}`: `*` and `**` may only be preceded an followed by `/`",
                            value
                        )
                    }
                }
            }
        }

        for (index, forbidden) in value.bytes().enumerate().filter_map(|(i, c)| {
            if FORBIDDEN_CHARS.contains(&c) {
                Some((i, c))
            } else {
                None
            }
        }) {
            let bytes = value.as_bytes();
            if forbidden == b'$' {
                if let Some(b'*') = bytes.get(index + 1) {
                    if let Some(b'$') = bytes.get(index + 2) {
                        bail!((KeyExprConstructionError::DollarAfterDollarOrStar)
                            "Invalid Key Expr `{}`: `$` is not allowed after `$*`",
                            value
                        )
                    }
                } else {
                    bail!((KeyExprConstructionError::ContainsUnboundDollar)"Invalid Key Expr `{}`: `$` is only allowed in `$*`", value)
                }
            } else {
                bail!((KeyExprConstructionError::ContainsSharpOrQMark)
                    "Invalid Key Expr `{}`: `#` and `?` are forbidden characters",
                    value
                )
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
