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

/// Module provides a set of utility functions which allows to manipulate  &str` which follows the format `a=b;c=d|e;f=g`.
/// and structure `Parameters` which provides `HashMap<&str, &str>`-like view over a string of such format.
///
/// `;` is the separator between the key-value `(&str, &str)` elements.
///
/// `=` is the separator between the `&str`-key and `&str`-value
///
/// `|` is the separator between multiple elements of the values.
use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};
use core::{borrow::Borrow, fmt};
#[cfg(feature = "std")]
use std::collections::HashMap;

pub(super) const LIST_SEPARATOR: char = ';';
pub(super) const FIELD_SEPARATOR: char = '=';
pub(super) const VALUE_SEPARATOR: char = '|';

fn split_once(s: &str, c: char) -> (&str, &str) {
    match s.find(c) {
        Some(index) => {
            let (l, r) = s.split_at(index);
            (l, &r[1..])
        }
        None => (s, ""),
    }
}

/// Returns an iterator of key-value `(&str, &str)` pairs according to the parameters format.
pub fn iter(s: &str) -> impl DoubleEndedIterator<Item = (&str, &str)> + Clone {
    s.split(LIST_SEPARATOR)
        .filter(|p| !p.is_empty())
        .map(|p| split_once(p, FIELD_SEPARATOR))
}

/// Same as [`from_iter_into`] but keys are sorted in alphabetical order.
pub fn sort<'s, I>(iter: I) -> impl Iterator<Item = (&'s str, &'s str)>
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let mut from = iter.collect::<Vec<(&str, &str)>>();
    from.sort_unstable_by(|(k1, _), (k2, _)| k1.cmp(k2));
    from.into_iter()
}

/// Joins two key-value `(&str, &str)` iterators removing from `current` any element whose key is present in `new`.
pub fn join<'s, C, N>(current: C, new: N) -> impl Iterator<Item = (&'s str, &'s str)> + Clone
where
    C: Iterator<Item = (&'s str, &'s str)> + Clone,
    N: Iterator<Item = (&'s str, &'s str)> + Clone + 's,
{
    let n = new.clone();
    let current = current
        .clone()
        .filter(move |(kc, _)| !n.clone().any(|(kn, _)| kn == *kc));
    current.chain(new)
}

/// Builds a string from an iterator preserving the order.
#[allow(clippy::should_implement_trait)]
pub fn from_iter<'s, I>(iter: I) -> String
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let mut into = String::new();
    from_iter_into(iter, &mut into);
    into
}

/// Same as [`from_iter`] but it writes into a user-provided string instead of allocating a new one.
pub fn from_iter_into<'s, I>(iter: I, into: &mut String)
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    concat_into(iter, into);
}

/// Get the a `&str`-value for a `&str`-key according to the parameters format.
pub fn get<'s>(s: &'s str, k: &str) -> Option<&'s str> {
    iter(s).find(|(key, _)| *key == k).map(|(_, value)| value)
}

/// Get the a `&str`-value iterator for a `&str`-key according to the parameters format.
pub fn values<'s>(s: &'s str, k: &str) -> impl DoubleEndedIterator<Item = &'s str> {
    match get(s, k) {
        Some(v) => v.split(VALUE_SEPARATOR),
        None => {
            let mut i = "".split(VALUE_SEPARATOR);
            i.next();
            i
        }
    }
}

fn _insert<'s, I>(
    i: I,
    k: &'s str,
    v: &'s str,
) -> (impl Iterator<Item = (&'s str, &'s str)>, Option<&'s str>)
where
    I: Iterator<Item = (&'s str, &'s str)> + Clone,
{
    let mut iter = i.clone();
    let item = iter.find(|(key, _)| *key == k).map(|(_, v)| v);

    let current = i.filter(move |x| x.0 != k);
    let new = Some((k, v)).into_iter();
    (current.chain(new), item)
}

/// Insert a key-value `(&str, &str)` pair by appending it at the end of `s` preserving the insertion order.
pub fn insert<'s>(s: &'s str, k: &'s str, v: &'s str) -> (String, Option<&'s str>) {
    let (iter, item) = _insert(iter(s), k, v);
    (from_iter(iter), item)
}

/// Same as [`insert`] but keys are sorted in alphabetical order.
pub fn insert_sort<'s>(s: &'s str, k: &'s str, v: &'s str) -> (String, Option<&'s str>) {
    let (iter, item) = _insert(iter(s), k, v);
    (from_iter(sort(iter)), item)
}

/// Remove a key-value `(&str, &str)` pair from `s` preserving the insertion order.
pub fn remove<'s>(s: &'s str, k: &str) -> (String, Option<&'s str>) {
    let mut iter = iter(s);
    let item = iter.find(|(key, _)| *key == k).map(|(_, v)| v);
    let iter = iter.filter(|x| x.0 != k);
    (concat(iter), item)
}

/// Returns `true` if all keys are sorted in alphabetical order
pub fn is_ordered(s: &str) -> bool {
    let mut prev = None;
    for (k, _) in iter(s) {
        match prev.take() {
            Some(p) if k < p => return false,
            _ => prev = Some(k),
        }
    }
    true
}

fn concat<'s, I>(iter: I) -> String
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let mut into = String::new();
    concat_into(iter, &mut into);
    into
}

fn concat_into<'s, I>(iter: I, into: &mut String)
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let mut first = true;
    for (k, v) in iter.filter(|(k, _)| !k.is_empty()) {
        if !first {
            into.push(LIST_SEPARATOR);
        }
        into.push_str(k);
        if !v.is_empty() {
            into.push(FIELD_SEPARATOR);
            into.push_str(v);
        }
        first = false;
    }
}

#[cfg(feature = "test")]
#[doc(hidden)]
pub fn rand(into: &mut String) {
    use rand::{
        distributions::{Alphanumeric, DistString},
        Rng,
    };

    const MIN: usize = 2;
    const MAX: usize = 8;

    let mut rng = rand::thread_rng();

    let num = rng.gen_range(MIN..MAX);
    for i in 0..num {
        if i != 0 {
            into.push(LIST_SEPARATOR);
        }
        let len = rng.gen_range(MIN..MAX);
        let key = Alphanumeric.sample_string(&mut rng, len);
        into.push_str(key.as_str());

        into.push(FIELD_SEPARATOR);

        let len = rng.gen_range(MIN..MAX);
        let value = Alphanumeric.sample_string(&mut rng, len);
        into.push_str(value.as_str());
    }
}

/// A map of key/value (String,String) parameters.
/// It can be parsed from a String, using `;` or `<newline>` as separator between each parameters
/// and `=` as separator between a key and its value. Keys and values are trimmed.
///
/// Example:
/// ```
/// use zenoh_protocol::core::Parameters;
///
/// let a = "a=1;b=2;c=3|4|5;d=6";
/// let p = Parameters::from(a);
///
/// // Retrieve values
/// assert!(!p.is_empty());
/// assert_eq!(p.get("a").unwrap(), "1");
/// assert_eq!(p.get("b").unwrap(), "2");
/// assert_eq!(p.get("c").unwrap(), "3|4|5");
/// assert_eq!(p.get("d").unwrap(), "6");
/// assert_eq!(p.values("c").collect::<Vec<&str>>(), vec!["3", "4", "5"]);
///
/// // Iterate over parameters
/// let mut iter = p.iter();
/// assert_eq!(iter.next().unwrap(), ("a", "1"));
/// assert_eq!(iter.next().unwrap(), ("b", "2"));
/// assert_eq!(iter.next().unwrap(), ("c", "3|4|5"));
/// assert_eq!(iter.next().unwrap(), ("d", "6"));
/// assert!(iter.next().is_none());
///
/// // Create parameters from iterators
/// let pi = Parameters::from_iter(vec![("a", "1"), ("b", "2"), ("c", "3|4|5"), ("d", "6")]);
/// assert_eq!(p, pi);
/// ```
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Parameters<'s>(Cow<'s, str>);

impl<'s> Parameters<'s> {
    /// Create empty parameters.
    pub const fn empty() -> Self {
        Self(Cow::Borrowed(""))
    }

    /// Returns `true` if parameters does not contain anything.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns parameters as [`str`].
    pub fn as_str(&'s self) -> &'s str {
        &self.0
    }

    /// Returns `true` if parameters contains the specified key.
    pub fn contains_key<K>(&self, k: K) -> bool
    where
        K: Borrow<str>,
    {
        super::parameters::get(self.as_str(), k.borrow()).is_some()
    }

    /// Returns a reference to the `&str`-value corresponding to the key.
    pub fn get<K>(&'s self, k: K) -> Option<&'s str>
    where
        K: Borrow<str>,
    {
        super::parameters::get(self.as_str(), k.borrow())
    }

    /// Returns an iterator to the `&str`-values corresponding to the key.
    pub fn values<K>(&'s self, k: K) -> impl DoubleEndedIterator<Item = &'s str>
    where
        K: Borrow<str>,
    {
        super::parameters::values(self.as_str(), k.borrow())
    }

    /// Returns an iterator on the key-value pairs as `(&str, &str)`.
    pub fn iter(&'s self) -> impl DoubleEndedIterator<Item = (&'s str, &'s str)> + Clone {
        super::parameters::iter(self.as_str())
    }

    /// Inserts a key-value pair into the map.
    /// If the map did not have this key present, [`None`]` is returned.
    /// If the map did have this key present, the value is updated, and the old value is returned.
    pub fn insert<K, V>(&mut self, k: K, v: V) -> Option<String>
    where
        K: Borrow<str>,
        V: Borrow<str>,
    {
        let (inner, item) = super::parameters::insert(self.as_str(), k.borrow(), v.borrow());
        let item = item.map(|i| i.to_string());
        self.0 = Cow::Owned(inner);
        item
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the parameters.
    pub fn remove<K>(&mut self, k: K) -> Option<String>
    where
        K: Borrow<str>,
    {
        let (inner, item) = super::parameters::remove(self.as_str(), k.borrow());
        let item = item.map(|i| i.to_string());
        self.0 = Cow::Owned(inner);
        item
    }

    /// Extend these parameters with other parameters.
    pub fn extend(&mut self, other: &Parameters) {
        self.extend_from_iter(other.iter());
    }

    /// Extend these parameters from an iterator.
    pub fn extend_from_iter<'e, I, K, V>(&mut self, iter: I)
    where
        I: Iterator<Item = (&'e K, &'e V)> + Clone,
        K: Borrow<str> + 'e + ?Sized,
        V: Borrow<str> + 'e + ?Sized,
    {
        let inner = super::parameters::from_iter(super::parameters::join(
            self.iter(),
            iter.map(|(k, v)| (k.borrow(), v.borrow())),
        ));
        self.0 = Cow::Owned(inner);
    }

    /// Convert these parameters into owned parameters.
    pub fn into_owned(self) -> Parameters<'static> {
        Parameters(Cow::Owned(self.0.into_owned()))
    }

    /// Returns `true`` if all keys are sorted in alphabetical order.
    pub fn is_ordered(&self) -> bool {
        super::parameters::is_ordered(self.as_str())
    }
}

impl<'s> From<&'s str> for Parameters<'s> {
    fn from(mut value: &'s str) -> Self {
        value = value.trim_end_matches(|c| {
            c == LIST_SEPARATOR || c == FIELD_SEPARATOR || c == VALUE_SEPARATOR
        });
        Self(Cow::Borrowed(value))
    }
}

impl From<String> for Parameters<'_> {
    fn from(mut value: String) -> Self {
        let s = value.trim_end_matches(|c| {
            c == LIST_SEPARATOR || c == FIELD_SEPARATOR || c == VALUE_SEPARATOR
        });
        value.truncate(s.len());
        Self(Cow::Owned(value))
    }
}

impl<'s> From<Cow<'s, str>> for Parameters<'s> {
    fn from(value: Cow<'s, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Parameters::from(s),
            Cow::Owned(s) => Parameters::from(s),
        }
    }
}

impl<'a> From<Parameters<'a>> for Cow<'_, Parameters<'a>> {
    fn from(props: Parameters<'a>) -> Self {
        Cow::Owned(props)
    }
}

impl<'a> From<&'a Parameters<'a>> for Cow<'a, Parameters<'a>> {
    fn from(props: &'a Parameters<'a>) -> Self {
        Cow::Borrowed(props)
    }
}

impl<'s, K, V> FromIterator<(&'s K, &'s V)> for Parameters<'_>
where
    K: Borrow<str> + 's + ?Sized,
    V: Borrow<str> + 's + ?Sized,
{
    fn from_iter<T: IntoIterator<Item = (&'s K, &'s V)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let inner = super::parameters::from_iter(iter.map(|(k, v)| (k.borrow(), v.borrow())));
        Self(Cow::Owned(inner))
    }
}

impl<'s, K, V> FromIterator<&'s (K, V)> for Parameters<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from_iter<T: IntoIterator<Item = &'s (K, V)>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().map(|(k, v)| (k.borrow(), v.borrow())))
    }
}

impl<'s, K, V> From<&'s [(K, V)]> for Parameters<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from(value: &'s [(K, V)]) -> Self {
        Self::from_iter(value.iter())
    }
}

#[cfg(feature = "std")]
impl<K, V> From<HashMap<K, V>> for Parameters<'_>
where
    K: Borrow<str>,
    V: Borrow<str>,
{
    fn from(map: HashMap<K, V>) -> Self {
        Self::from_iter(map.iter())
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s Parameters<'s>> for HashMap<&'s str, &'s str> {
    fn from(props: &'s Parameters<'s>) -> Self {
        HashMap::from_iter(props.iter())
    }
}

#[cfg(feature = "std")]
impl From<&Parameters<'_>> for HashMap<String, String> {
    fn from(props: &Parameters<'_>) -> Self {
        HashMap::from_iter(props.iter().map(|(k, v)| (k.to_string(), v.to_string())))
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s Parameters<'s>> for HashMap<Cow<'s, str>, Cow<'s, str>> {
    fn from(props: &'s Parameters<'s>) -> Self {
        HashMap::from_iter(props.iter().map(|(k, v)| (Cow::from(k), Cow::from(v))))
    }
}

#[cfg(feature = "std")]
impl From<Parameters<'_>> for HashMap<String, String> {
    fn from(props: Parameters) -> Self {
        HashMap::from(&props)
    }
}

impl fmt::Display for Parameters<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Parameters<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters() {
        assert!(Parameters::from("").0.is_empty());

        assert_eq!(Parameters::from("p1"), Parameters::from(&[("p1", "")][..]));

        assert_eq!(
            Parameters::from("p1=v1"),
            Parameters::from(&[("p1", "v1")][..])
        );

        assert_eq!(
            Parameters::from("p1=v1;p2=v2;"),
            Parameters::from(&[("p1", "v1"), ("p2", "v2")][..])
        );

        assert_eq!(
            Parameters::from("p1=v1;p2=v2;|="),
            Parameters::from(&[("p1", "v1"), ("p2", "v2")][..])
        );

        assert_eq!(
            Parameters::from("p1=v1;p2;p3=v3"),
            Parameters::from(&[("p1", "v1"), ("p2", ""), ("p3", "v3")][..])
        );

        assert_eq!(
            Parameters::from("p1=v 1;p 2=v2"),
            Parameters::from(&[("p1", "v 1"), ("p 2", "v2")][..])
        );

        assert_eq!(
            Parameters::from("p1=x=y;p2=a==b"),
            Parameters::from(&[("p1", "x=y"), ("p2", "a==b")][..])
        );

        let mut hm: HashMap<String, String> = HashMap::new();
        hm.insert("p1".to_string(), "v1".to_string());
        assert_eq!(Parameters::from(hm), Parameters::from("p1=v1"));

        let mut hm: HashMap<&str, &str> = HashMap::new();
        hm.insert("p1", "v1");
        assert_eq!(Parameters::from(hm), Parameters::from("p1=v1"));

        let mut hm: HashMap<Cow<str>, Cow<str>> = HashMap::new();
        hm.insert(Cow::from("p1"), Cow::from("v1"));
        assert_eq!(Parameters::from(hm), Parameters::from("p1=v1"));
    }
}
