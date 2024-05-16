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
// filetag{rust.internal.protocol}
use alloc::{
    borrow::Cow,
    string::{String, ToString},
};
use core::{borrow::Borrow, fmt};
#[cfg(feature = "std")]
use std::collections::HashMap;

use super::parameters::{Parameters, FIELD_SEPARATOR, LIST_SEPARATOR, VALUE_SEPARATOR};

/// A map of key/value (String,String) properties.
/// It can be parsed from a String, using `;` or `<newline>` as separator between each properties
/// and `=` as separator between a key and its value. Keys and values are trimed.
///
/// Example:
/// ```
/// use zenoh_protocol::core::Properties;
///
/// let a = "a=1;b=2;c=3|4|5;d=6";
/// let p = Properties::from(a);
///
/// // Retrieve values
/// assert!(!p.is_empty());
/// assert_eq!(p.get("a").unwrap(), "1");
/// assert_eq!(p.get("b").unwrap(), "2");
/// assert_eq!(p.get("c").unwrap(), "3|4|5");
/// assert_eq!(p.get("d").unwrap(), "6");
/// assert_eq!(p.values("c").collect::<Vec<&str>>(), vec!["3", "4", "5"]);
///
/// // Iterate over properties
/// let mut iter = p.iter();
/// assert_eq!(iter.next().unwrap(), ("a", "1"));
/// assert_eq!(iter.next().unwrap(), ("b", "2"));
/// assert_eq!(iter.next().unwrap(), ("c", "3|4|5"));
/// assert_eq!(iter.next().unwrap(), ("d", "6"));
/// assert!(iter.next().is_none());
///
/// // Create properties from iterators
/// let pi = Properties::from_iter(vec![("a", "1"), ("b", "2"), ("c", "3|4|5"), ("d", "6")]);
/// assert_eq!(p, pi);
/// ```
/// filetag{rust.selector}
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Properties<'s>(Cow<'s, str>);

impl<'s> Properties<'s> {
    /// Returns `true` if properties does not contain anything.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns properties as [`str`].
    pub fn as_str(&'s self) -> &'s str {
        &self.0
    }

    /// Returns `true` if properties contains the specified key.
    pub fn contains_key<K>(&self, k: K) -> bool
    where
        K: Borrow<str>,
    {
        Parameters::get(self.as_str(), k.borrow()).is_some()
    }

    /// Returns a reference to the `&str`-value corresponding to the key.
    pub fn get<K>(&'s self, k: K) -> Option<&'s str>
    where
        K: Borrow<str>,
    {
        Parameters::get(self.as_str(), k.borrow())
    }

    /// Returns an iterator to the `&str`-values corresponding to the key.
    pub fn values<K>(&'s self, k: K) -> impl DoubleEndedIterator<Item = &'s str>
    where
        K: Borrow<str>,
    {
        Parameters::values(self.as_str(), k.borrow())
    }

    /// Returns an iterator on the key-value pairs as `(&str, &str)`.
    pub fn iter(&'s self) -> impl DoubleEndedIterator<Item = (&'s str, &'s str)> + Clone {
        Parameters::iter(self.as_str())
    }

    /// Inserts a key-value pair into the map.
    /// If the map did not have this key present, [`None`]` is returned.
    /// If the map did have this key present, the value is updated, and the old value is returned.
    pub fn insert<K, V>(&mut self, k: K, v: V) -> Option<String>
    where
        K: Borrow<str>,
        V: Borrow<str>,
    {
        let (inner, item) = Parameters::insert(self.as_str(), k.borrow(), v.borrow());
        let item = item.map(|i| i.to_string());
        self.0 = Cow::Owned(inner);
        item
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the properties.    
    pub fn remove<K>(&mut self, k: K) -> Option<String>
    where
        K: Borrow<str>,
    {
        let (inner, item) = Parameters::remove(self.as_str(), k.borrow());
        let item = item.map(|i| i.to_string());
        self.0 = Cow::Owned(inner);
        item
    }

    /// Extend these properties with other properties.
    pub fn extend(&mut self, other: &Properties) {
        self.extend_from_iter(other.iter());
    }

    /// Extend these properties from an iterator.
    pub fn extend_from_iter<'e, I, K, V>(&mut self, iter: I)
    where
        I: Iterator<Item = (&'e K, &'e V)> + Clone,
        K: Borrow<str> + 'e + ?Sized,
        V: Borrow<str> + 'e + ?Sized,
    {
        let inner = Parameters::from_iter(Parameters::join(
            self.iter(),
            iter.map(|(k, v)| (k.borrow(), v.borrow())),
        ));
        self.0 = Cow::Owned(inner);
    }

    /// Convert these properties into owned properties.
    pub fn into_owned(self) -> Properties<'static> {
        Properties(Cow::Owned(self.0.into_owned()))
    }

    /// Returns `true`` if all keys are sorted in alphabetical order.
    pub fn is_ordered(&self) -> bool {
        Parameters::is_ordered(self.as_str())
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
        let iter = iter.into_iter();
        let inner = Parameters::from_iter(iter.map(|(k, v)| (k.borrow(), v.borrow())));
        Self(Cow::Owned(inner))
    }
}

impl<'s, K, V> FromIterator<&'s (K, V)> for Properties<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from_iter<T: IntoIterator<Item = &'s (K, V)>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().map(|(k, v)| (k.borrow(), v.borrow())))
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
        HashMap::from_iter(props.iter())
    }
}

#[cfg(feature = "std")]
impl From<&Properties<'_>> for HashMap<String, String> {
    fn from(props: &Properties<'_>) -> Self {
        HashMap::from_iter(props.iter().map(|(k, v)| (k.to_string(), v.to_string())))
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s Properties<'s>> for HashMap<Cow<'s, str>, Cow<'s, str>> {
    fn from(props: &'s Properties<'s>) -> Self {
        HashMap::from_iter(props.iter().map(|(k, v)| (Cow::from(k), Cow::from(v))))
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

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct OrderedProperties<'s>(Properties<'s>);

impl<'s> OrderedProperties<'s> {
    /// Returns `true` if properties does not contain anything.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns properties as [`str`].
    pub fn as_str(&'s self) -> &'s str {
        self.0.as_str()
    }

    /// Returns `true` if properties contains the specified key.
    pub fn contains_key<K>(&self, k: K) -> bool
    where
        K: Borrow<str>,
    {
        self.0.contains_key(k)
    }

    /// Returns a reference to the `&str`-value corresponding to the key.
    pub fn get<K>(&'s self, k: K) -> Option<&'s str>
    where
        K: Borrow<str>,
    {
        self.0.get(k)
    }

    /// Returns an iterator to the `&str`-values corresponding to the key.
    pub fn values<K>(&'s self, k: K) -> impl DoubleEndedIterator<Item = &'s str>
    where
        K: Borrow<str>,
    {
        self.0.values(k)
    }

    /// Returns an iterator on the key-value pairs as `(&str, &str)`.
    pub fn iter(&'s self) -> impl DoubleEndedIterator<Item = (&'s str, &'s str)> + Clone {
        self.0.iter()
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the properties.    
    pub fn remove<K>(&mut self, k: K) -> Option<String>
    where
        K: Borrow<str>,
    {
        self.0.remove(k)
    }

    /// Inserts a key-value pair into the map.
    /// If the map did not have this key present, [`None`]` is returned.
    /// If the map did have this key present, the value is updated, and the old value is returned.
    pub fn insert<K, V>(&mut self, k: K, v: V) -> Option<String>
    where
        K: Borrow<str>,
        V: Borrow<str>,
    {
        let item = self.0.insert(k, v);
        self.order();
        item
    }

    /// Extend these properties with other properties.
    pub fn extend(&mut self, other: &Properties) {
        self.extend_from_iter(other.iter());
    }

    /// Extend these properties from an iterator.
    pub fn extend_from_iter<'e, I, K, V>(&mut self, iter: I)
    where
        I: Iterator<Item = (&'e K, &'e V)> + Clone,
        K: Borrow<str> + 'e + ?Sized,
        V: Borrow<str> + 'e + ?Sized,
    {
        self.0.extend_from_iter(iter);
        self.order();
    }

    /// Convert these properties into owned properties.
    pub fn into_owned(self) -> OrderedProperties<'static> {
        OrderedProperties(self.0.into_owned())
    }

    fn order(&mut self) {
        if !self.0.is_ordered() {
            self.0 = Properties(Cow::Owned(Parameters::from_iter(Parameters::sort(
                self.iter(),
            ))));
        }
    }
}

impl<'s> From<Properties<'s>> for OrderedProperties<'s> {
    fn from(value: Properties<'s>) -> Self {
        let mut props = Self(value);
        props.order();
        props
    }
}

impl<'s> From<&'s str> for OrderedProperties<'s> {
    fn from(value: &'s str) -> Self {
        Self::from(Properties::from(value))
    }
}

impl From<String> for OrderedProperties<'_> {
    fn from(value: String) -> Self {
        Self::from(Properties::from(value))
    }
}

impl<'s> From<Cow<'s, str>> for OrderedProperties<'s> {
    fn from(value: Cow<'s, str>) -> Self {
        Self::from(Properties::from(value))
    }
}

impl<'s, K, V> FromIterator<(&'s K, &'s V)> for OrderedProperties<'_>
where
    K: Borrow<str> + 's + ?Sized,
    V: Borrow<str> + 's + ?Sized,
{
    fn from_iter<T: IntoIterator<Item = (&'s K, &'s V)>>(iter: T) -> Self {
        Self::from(Properties::from_iter(iter))
    }
}

impl<'s, K, V> FromIterator<&'s (K, V)> for OrderedProperties<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from_iter<T: IntoIterator<Item = &'s (K, V)>>(iter: T) -> Self {
        Self::from(Properties::from_iter(iter))
    }
}

impl<'s, K, V> From<&'s [(K, V)]> for OrderedProperties<'_>
where
    K: Borrow<str> + 's,
    V: Borrow<str> + 's,
{
    fn from(value: &'s [(K, V)]) -> Self {
        Self::from_iter(value.iter())
    }
}

#[cfg(feature = "std")]
impl<K, V> From<HashMap<K, V>> for OrderedProperties<'_>
where
    K: Borrow<str>,
    V: Borrow<str>,
{
    fn from(map: HashMap<K, V>) -> Self {
        Self::from_iter(map.iter())
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s OrderedProperties<'s>> for HashMap<&'s str, &'s str> {
    fn from(props: &'s OrderedProperties<'s>) -> Self {
        HashMap::from(&props.0)
    }
}

#[cfg(feature = "std")]
impl From<&OrderedProperties<'_>> for HashMap<String, String> {
    fn from(props: &OrderedProperties<'_>) -> Self {
        HashMap::from(&props.0)
    }
}

#[cfg(feature = "std")]
impl<'s> From<&'s OrderedProperties<'s>> for HashMap<Cow<'s, str>, Cow<'s, str>> {
    fn from(props: &'s OrderedProperties<'s>) -> Self {
        HashMap::from(&props.0)
    }
}

#[cfg(feature = "std")]
impl From<OrderedProperties<'_>> for HashMap<String, String> {
    fn from(props: OrderedProperties) -> Self {
        HashMap::from(&props)
    }
}

impl fmt::Display for OrderedProperties<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for OrderedProperties<'_> {
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
