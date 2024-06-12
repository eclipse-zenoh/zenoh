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

/// Module provides a set of utility functions whic allows to manipulate  &str` which follows the format `a=b;c=d|e;f=g`.
/// and structure `Parameters` which provides `HashMap<&str, &str>`-like view over a string of such format.
///
/// `;` is the separator between the key-value `(&str, &str)` elements.
///
/// `=` is the separator between the `&str`-key and `&str`-value
///
/// `|` is the separator between multiple elements of the values.

mod properties;
pub use properties::Parameters;

pub(super) const LIST_SEPARATOR: char = ';';
pub(super) const FIELD_SEPARATOR: char = '=';
pub(super) const VALUE_SEPARATOR: char = '|';

use alloc::{string::String, vec::Vec};

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

/// Same as [`Self::from_iter_into`] but keys are sorted in alphabetical order.
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

/// Same as [`Self::from_iter`] but it writes into a user-provided string instead of allocating a new one.
pub fn from_iter_into<'s, I>(iter: I, into: &mut String)
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    concat_into(iter, into);
}

/// Get the a `&str`-value for a `&str`-key according to the parameters format.
pub fn get<'s>(s: &'s str, k: &str) -> Option<&'s str> {
    iter(s)
        .find(|(key, _)| *key == k)
        .map(|(_, value)| value)
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

/// Same as [`Self::insert`] but keys are sorted in alphabetical order.
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
