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

//! ⚠️ WARNING ⚠️
//!
//! This crate is depecrated and it will be completely removed in the future.
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
pub mod config;

use std::collections::HashMap;
use std::convert::{From, TryFrom};
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub trait KeyTranscoder {
    fn encode(key: &str) -> Option<u64>;
    fn decode(key: u64) -> Option<String>;
}

/// A set of Key/Value (`u64`/`String`) pairs.
#[non_exhaustive]
#[derive(PartialEq, Eq)]
pub struct IntKeyProperties<T>(pub HashMap<u64, String>, PhantomData<T>)
where
    T: KeyTranscoder;

impl<T: KeyTranscoder> IntKeyProperties<T> {
    #[inline]
    pub fn get_or<'a>(&'a self, key: &u64, default: &'a str) -> &'a str {
        self.get(key).map(|s| &s[..]).unwrap_or(default)
    }
}

impl<T: KeyTranscoder> Clone for IntKeyProperties<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<T: KeyTranscoder> Default for IntKeyProperties<T> {
    fn default() -> Self {
        Self(HashMap::new(), PhantomData)
    }
}

impl<T: KeyTranscoder> Deref for IntKeyProperties<T> {
    type Target = HashMap<u64, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: KeyTranscoder> DerefMut for IntKeyProperties<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: KeyTranscoder> fmt::Display for IntKeyProperties<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&Properties::from(self.clone()), f)
    }
}

impl<T: KeyTranscoder> fmt::Debug for IntKeyProperties<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&Properties::from(self.clone()), f)
    }
}

impl<T: KeyTranscoder> From<IntKeyProperties<T>> for HashMap<String, String> {
    fn from(props: IntKeyProperties<T>) -> Self {
        props
            .0
            .into_iter()
            .filter_map(|(k, v)| T::decode(k).map(|k| (k, v)))
            .collect()
    }
}

impl<T: KeyTranscoder> From<HashMap<u64, String>> for IntKeyProperties<T> {
    fn from(map: HashMap<u64, String>) -> Self {
        Self(map, PhantomData)
    }
}

impl<T: KeyTranscoder> From<HashMap<String, String>> for IntKeyProperties<T> {
    fn from(map: HashMap<String, String>) -> Self {
        Self(
            map.into_iter()
                .filter_map(|(k, v)| T::encode(&k).map(|k| (k, v)))
                .collect(),
            PhantomData,
        )
    }
}

impl<T: KeyTranscoder> From<Properties> for IntKeyProperties<T> {
    fn from(props: Properties) -> Self {
        props.0.into()
    }
}

impl<T: KeyTranscoder> From<&str> for IntKeyProperties<T> {
    fn from(s: &str) -> Self {
        Properties::from(s).into()
    }
}

impl<T: KeyTranscoder> From<String> for IntKeyProperties<T> {
    fn from(s: String) -> Self {
        Properties::from(s).into()
    }
}

impl<T: KeyTranscoder> From<&[(&str, &str)]> for IntKeyProperties<T> {
    fn from(kvs: &[(&str, &str)]) -> Self {
        Self(
            kvs.iter()
                .filter_map(|(k, v)| T::encode(k).map(|k| (k, v.to_string())))
                .collect(),
            PhantomData,
        )
    }
}

impl<T: KeyTranscoder> From<&[(u64, &str)]> for IntKeyProperties<T> {
    fn from(kvs: &[(u64, &str)]) -> Self {
        Self(
            kvs.iter()
                .cloned()
                .map(|(k, v)| (k, v.to_string()))
                .collect(),
            PhantomData,
        )
    }
}

static PROP_SEPS: &[&str] = &["\r\n", "\n", ";"];
const DEFAULT_PROP_SEP: char = ';';
const KV_SEP: &[char] = &['=', ':'];
const COMMENT_PREFIX: char = '#';

/// A map of key/value (String,String) properties.
///
/// It can be parsed from a String, using `;` or `<newline>` as separator between each properties
/// and `=` as separator between a key and its value. Keys and values are trimed.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Properties(pub HashMap<String, String>);

impl Deref for Properties {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Properties {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for Properties {
    /// Format the Properties as a string, using `'='` for key/value separator
    /// and `';'` for separator between each keys/values.
    ///
    /// **WARNING**: the passwords are displayed in clear. This is required for the result
    /// of the [`trait@ToString`] automatic implementation that must preserve all the properties.
    /// To display the properties, hidding the passwords, rather use the [`Debug`](core::fmt::Debug) trait implementation.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut it = self.0.iter();
        if let Some((k, v)) = it.next() {
            if v.is_empty() {
                write!(f, "{k}")?
            } else {
                write!(f, "{}{}{}", k, KV_SEP[0], v)?
            }
            for (k, v) in it {
                if v.is_empty() {
                    write!(f, "{DEFAULT_PROP_SEP}{k}")?
                } else {
                    write!(f, "{}{}{}{}", DEFAULT_PROP_SEP, k, KV_SEP[0], v)?
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Properties {
    /// Format the Properties as a string, using `'='` for key/value separator
    /// and `';'` for separator between each keys/values.
    ///
    /// **NOTE**: for each key containing `"password"` as sub-string,
    /// the value is replaced by `"*****"`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut it = self.0.iter();
        if let Some((k, v)) = it.next() {
            if v.is_empty() {
                write!(f, "{k}")?
            } else if k.contains("password") {
                write!(f, "{}{}*****", k, KV_SEP[0])?
            } else {
                write!(f, "{}{}{}", k, KV_SEP[0], v)?
            }
            for (k, v) in it {
                if v.is_empty() {
                    write!(f, "{DEFAULT_PROP_SEP}{k}")?
                } else if k.contains("password") {
                    write!(f, "{}{}{}*****", DEFAULT_PROP_SEP, k, KV_SEP[0])?
                } else {
                    write!(f, "{}{}{}{}", DEFAULT_PROP_SEP, k, KV_SEP[0], v)?
                }
            }
        }
        Ok(())
    }
}

impl From<&str> for Properties {
    fn from(s: &str) -> Self {
        let mut props = vec![s];
        for sep in PROP_SEPS {
            props = props
                .into_iter()
                .flat_map(|s| s.split(sep))
                .collect::<Vec<&str>>();
        }
        props = props.into_iter().map(|s| s.trim()).collect::<Vec<&str>>();

        Properties(
            props
                .iter()
                .filter_map(|prop| {
                    if prop.is_empty() || prop.starts_with(COMMENT_PREFIX) {
                        None
                    } else {
                        let mut it = prop.splitn(2, KV_SEP);
                        Some((
                            it.next().unwrap().trim().to_string(),
                            it.next().unwrap_or("").trim().to_string(),
                        ))
                    }
                })
                .collect(),
        )
    }
}

impl From<String> for Properties {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<HashMap<String, String>> for Properties {
    fn from(map: HashMap<String, String>) -> Self {
        Self(map)
    }
}

impl<T: KeyTranscoder> From<IntKeyProperties<T>> for Properties {
    fn from(props: IntKeyProperties<T>) -> Self {
        Self(
            props
                .0
                .into_iter()
                .filter_map(|(k, v)| T::decode(k).map(|k| (k, v)))
                .collect(),
        )
    }
}

impl From<&[(&str, &str)]> for Properties {
    fn from(kvs: &[(&str, &str)]) -> Self {
        Properties(
            kvs.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
}

impl TryFrom<&std::path::Path> for Properties {
    type Error = zenoh_result::Error;
    fn try_from(p: &std::path::Path) -> Result<Self, Self::Error> {
        Ok(Self::from(std::fs::read_to_string(p)?))
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

#[non_exhaustive]
pub struct DummyTranscoder;

impl KeyTranscoder for DummyTranscoder {
    fn encode(_key: &str) -> Option<u64> {
        None
    }

    fn decode(_key: u64) -> Option<String> {
        None
    }
}
