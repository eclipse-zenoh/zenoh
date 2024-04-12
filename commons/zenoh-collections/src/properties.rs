use alloc::borrow::Cow;
use core::borrow::Borrow;

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
use crate::Parameters;
use std::{collections::HashMap, fmt};

/// A map of key/value (String,String) properties.
/// It can be parsed from a String, using `;` or `<newline>` as separator between each properties
/// and `=` as separator between a key and its value. Keys and values are trimed.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Properties<'s>(Cow<'s, str>);

impl Properties<'_> {
    pub fn as_str(&self) -> &str {
        &self.0
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

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&str, &str)> {
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
}

impl<'s> From<&'s str> for Properties<'s> {
    fn from(value: &'s str) -> Self {
        if Parameters::is_sorted(Parameters::iter(value)) {
            Self(Cow::Borrowed(value))
        } else {
            Self(Cow::Owned(Parameters::from_iter(Parameters::iter(value))))
        }
    }
}

impl From<String> for Properties<'_> {
    fn from(value: String) -> Self {
        if Parameters::is_sorted(Parameters::iter(value.as_str())) {
            Self(Cow::Owned(value))
        } else {
            Self(Cow::Owned(Parameters::from_iter(Parameters::iter(
                value.as_str(),
            ))))
        }
    }
}

impl<'s, K, V> FromIterator<(&'s K, &'s V)> for Properties<'_>
where
    K: AsRef<str> + 's,
    V: AsRef<str> + 's,
{
    fn from_iter<T: IntoIterator<Item = (&'s K, &'s V)>>(iter: T) -> Self {
        let inner = Parameters::from_iter(iter.into_iter().map(|(k, v)| (k.as_ref(), v.as_ref())));
        Self(Cow::Owned(inner))
    }
}

impl<'s, K, V> FromIterator<&'s (K, V)> for Properties<'_>
where
    K: AsRef<str> + 's,
    V: AsRef<str> + 's,
{
    fn from_iter<T: IntoIterator<Item = &'s (K, V)>>(iter: T) -> Self {
        let inner = Parameters::from_iter(iter.into_iter().map(|(k, v)| (k.as_ref(), v.as_ref())));
        Self(Cow::Owned(inner))
    }
}

impl<'s, K, V> From<&'s [(K, V)]> for Properties<'_>
where
    K: AsRef<str> + 's,
    V: AsRef<str> + 's,
{
    fn from(value: &'s [(K, V)]) -> Self {
        Self::from_iter(value.iter())
    }
}

impl<K, V> From<HashMap<K, V>> for Properties<'_>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn from(map: HashMap<K, V>) -> Self {
        Self::from_iter(map.iter())
    }
}

impl From<Properties<'_>> for HashMap<String, String> {
    fn from(props: Properties) -> Self {
        HashMap::from_iter(
            Parameters::iter(props.as_str()).map(|(k, v)| (k.to_string(), v.to_string())),
        )
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
    }
}
