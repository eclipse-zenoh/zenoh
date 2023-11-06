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
use std::{
    collections::HashMap,
    convert::{From, TryFrom},
    fmt,
    ops::{Deref, DerefMut},
};

const PROP_SEPS: &[&str] = &["\r\n", "\n", ";"];
const DEFAULT_PROP_SEP: char = ';';
const KV_SEP: char = '=';
const COMMENT_PREFIX: char = '#';

/// A map of key/value (String,String) properties.
/// It can be parsed from a String, using `;` or `<newline>` as separator between each properties
/// and `=` as separator between a key and its value. Keys and values are trimed.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Properties(HashMap<String, String>);

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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut it = self.0.iter();
        if let Some((k, v)) = it.next() {
            if v.is_empty() {
                write!(f, "{k}")?
            } else {
                write!(f, "{k}{KV_SEP}{v}")?
            }
            for (k, v) in it {
                if v.is_empty() {
                    write!(f, "{DEFAULT_PROP_SEP}{k}")?
                } else {
                    write!(f, "{DEFAULT_PROP_SEP}{k}{KV_SEP}{v}")?
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Properties {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self}")
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
        props = props.into_iter().map(str::trim).collect::<Vec<&str>>();
        let inner = props
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
            .collect();
        Self(inner)
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

impl From<&[(&str, &str)]> for Properties {
    fn from(kvs: &[(&str, &str)]) -> Self {
        let inner = kvs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        Self(inner)
    }
}

impl TryFrom<&std::path::Path> for Properties {
    type Error = std::io::Error;

    fn try_from(p: &std::path::Path) -> Result<Self, Self::Error> {
        Ok(Self::from(std::fs::read_to_string(p)?))
    }
}

impl From<Properties> for HashMap<String, String> {
    fn from(props: Properties) -> Self {
        props.0
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
