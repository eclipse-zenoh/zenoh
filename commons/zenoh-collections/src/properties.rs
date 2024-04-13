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
use crate::{Parameters, FIELD_SEPARATOR, LIST_SEPARATOR, VALUE_SEPARATOR};
use alloc::borrow::Cow;
use core::{borrow::Borrow, fmt};
#[cfg(feature = "std")]
use std::collections::HashMap;

/// A map of key/value (String,String) properties.
/// It can be parsed from a String, using `;` or `<newline>` as separator between each properties
/// and `=` as separator between a key and its value. Keys and values are trimed.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Properties<'s>(Cow<'s, str>);

impl Properties<'_> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn contains_key<K>(&self, k: K) -> bool
    where
        K: Borrow<str>,
    {
        self.get(k).is_some()
    }

    pub fn get<K>(&self, k: K) -> Option<&str>
    where
        K: Borrow<str>,
    {
        Parameters::get(self.as_str(), k.borrow())
    }

    pub fn values<K>(&self, k: K) -> impl DoubleEndedIterator<Item = &str>
    where
        K: Borrow<str>,
    {
        Parameters::values(self.as_str(), k.borrow())
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&str, &str)> + Clone {
        Parameters::iter(self.as_str())
    }

    pub fn insert<K, V>(&mut self, k: K, v: V) -> Option<String>
    where
        K: Borrow<str>,
        V: Borrow<str>,
    {
        let (inner, removed) = Parameters::insert(self.iter(), k.borrow(), v.borrow());
        let removed = removed.map(|s| s.to_string());
        self.0 = Cow::Owned(inner);
        removed
    }

    pub fn remove<K>(&mut self, k: K) -> Option<String>
    where
        K: Borrow<str>,
    {
        let (inner, removed) = Parameters::remove(self.iter(), k.borrow());
        let removed = removed.map(|s| s.to_string());
        self.0 = Cow::Owned(inner);
        removed
    }

    pub fn join<'s, I, K, V>(&mut self, iter: I)
    where
        I: Iterator<Item = (&'s K, &'s V)> + Clone,
        K: Borrow<str> + 's + ?Sized,
        V: Borrow<str> + 's + ?Sized,
    {
        self.0 = Cow::Owned(Parameters::join(
            Parameters::iter(self.as_str()),
            iter.map(|(k, v)| (k.borrow(), v.borrow())),
        ));
    }

    pub fn into_owned(self) -> Properties<'static> {
        Properties(Cow::Owned(self.0.into_owned()))
    }
}

impl<'s> From<&'s str> for Properties<'s> {
    fn from(mut value: &'s str) -> Self {
        value = value.trim_end_matches(|c| {
            c == LIST_SEPARATOR || c == FIELD_SEPARATOR || c == VALUE_SEPARATOR
        });
        Self(Cow::Borrowed(value))
    }
}

impl From<String> for Properties<'_> {
    fn from(mut value: String) -> Self {
        let s = value.trim_end_matches(|c| {
            c == LIST_SEPARATOR || c == FIELD_SEPARATOR || c == VALUE_SEPARATOR
        });
        value.truncate(s.len());
        Self(Cow::Owned(value))
    }
}

impl<'s> From<Cow<'s, str>> for Properties<'s> {
    fn from(value: Cow<'s, str>) -> Self {
        match value {
            Cow::Borrowed(s) => Properties::from(s),
            Cow::Owned(s) => Properties::from(s),
        }
    }
}

impl<'s, K, V> FromIterator<(&'s K, &'s V)> for Properties<'_>
where
    K: Borrow<str> + 's + ?Sized,
    V: Borrow<str> + 's + ?Sized,
{
    fn from_iter<T: IntoIterator<Item = (&'s K, &'s V)>>(iter: T) -> Self {
        let inner = Parameters::from_iter(iter.into_iter().map(|(k, v)| (k.borrow(), v.borrow())));
        Self(Cow::Owned(inner))
    }
}

impl<'s, K, V> FromIterator<&'s (K, V)> for Properties<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from_iter<T: IntoIterator<Item = &'s (K, V)>>(iter: T) -> Self {
        let inner = Parameters::from_iter(iter.into_iter().map(|(k, v)| (k.borrow(), v.borrow())));
        Self(Cow::Owned(inner))
    }
}

impl<'s, K, V> From<&'s [(K, V)]> for Properties<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from(value: &'s [(K, V)]) -> Self {
        Self::from_iter(value.iter())
    }
}

#[cfg(feature = "std")]
impl<K, V> From<HashMap<K, V>> for Properties<'_>
where
    K: Borrow<str>,
    V: Borrow<str>,
{
    fn from(map: HashMap<K, V>) -> Self {
        Self::from_iter(map.iter())
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s Properties<'s>> for HashMap<&'s str, &'s str> {
    fn from(props: &'s Properties<'s>) -> Self {
        HashMap::from_iter(Parameters::iter(props.as_str()))
    }
}

#[cfg(feature = "std")]
impl From<&Properties<'_>> for HashMap<String, String> {
    fn from(props: &Properties<'_>) -> Self {
        HashMap::from_iter(
            Parameters::iter(props.as_str()).map(|(k, v)| (k.to_string(), v.to_string())),
        )
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s Properties<'s>> for HashMap<Cow<'s, str>, Cow<'s, str>> {
    fn from(props: &'s Properties<'s>) -> Self {
        HashMap::from_iter(
            Parameters::iter(props.as_str()).map(|(k, v)| (Cow::from(k), Cow::from(v))),
        )
    }
}

#[cfg(feature = "std")]
impl From<Properties<'_>> for HashMap<String, String> {
    fn from(props: Properties) -> Self {
        HashMap::from(&props)
    }
}

impl fmt::Display for Properties<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Properties<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_properties() {
        assert!(Properties::from("").0.is_empty());

        assert_eq!(Properties::from("p1"), Properties::from(&[("p1", "")][..]));

        assert_eq!(
            Properties::from("p1=v1"),
            Properties::from(&[("p1", "v1")][..])
        );

        assert_eq!(
            Properties::from("p1=v1;p2=v2;"),
            Properties::from(&[("p1", "v1"), ("p2", "v2")][..])
        );

        assert_eq!(
            Properties::from("p1=v1;p2=v2;|="),
            Properties::from(&[("p1", "v1"), ("p2", "v2")][..])
        );

        assert_eq!(
            Properties::from("p1=v1;p2;p3=v3"),
            Properties::from(&[("p1", "v1"), ("p2", ""), ("p3", "v3")][..])
        );

        assert_eq!(
            Properties::from("p1=v 1;p 2=v2"),
            Properties::from(&[("p1", "v 1"), ("p 2", "v2")][..])
        );

        assert_eq!(
            Properties::from("p1=x=y;p2=a==b"),
            Properties::from(&[("p1", "x=y"), ("p2", "a==b")][..])
        );

        let mut hm: HashMap<String, String> = HashMap::new();
        hm.insert("p1".to_string(), "v1".to_string());
        assert_eq!(Properties::from(hm), Properties::from("p1=v1"));

        let mut hm: HashMap<&str, &str> = HashMap::new();
        hm.insert("p1", "v1");
        assert_eq!(Properties::from(hm), Properties::from("p1=v1"));

        let mut hm: HashMap<Cow<str>, Cow<str>> = HashMap::new();
        hm.insert(Cow::from("p1"), Cow::from("v1"));
        assert_eq!(Properties::from(hm), Properties::from("p1=v1"));
    }
}
