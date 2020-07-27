//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use std::collections::HashMap;
use std::convert::From;
use std::fmt;
use std::ops::{Deref, DerefMut};

const PROP_SEP: char = ';';
const KV_SEP: char = '=';

#[derive(Clone, Debug, PartialEq)]
pub struct Properties(pub(crate) HashMap<String, String>);

impl Default for Properties {
    fn default() -> Self {
        Properties(HashMap::new())
    }
}

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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut it = self.0.iter();
        if let Some((k, v)) = it.next() {
            if v.is_empty() {
                write!(f, "{}", k)?
            } else {
                write!(f, "{}{}{}", k, KV_SEP, v)?
            }
            for (k, v) in it {
                if v.is_empty() {
                    write!(f, "{}{}", PROP_SEP, k)?
                } else {
                    write!(f, "{}{}{}{}", PROP_SEP, k, KV_SEP, v)?
                }
            }
        }
        Ok(())
    }
}

impl From<&str> for Properties {
    fn from(s: &str) -> Self {
        let p: HashMap<String, String> = if !s.is_empty() {
            s.split(PROP_SEP)
                .filter_map(|prop| {
                    if prop.is_empty() {
                        None
                    } else {
                        let mut it = prop.splitn(2, KV_SEP);
                        Some((
                            it.next().unwrap().to_string(),
                            it.next().unwrap_or("").to_string(),
                        ))
                    }
                })
                .collect()
        } else {
            HashMap::new()
        };
        Properties(p)
    }
}

impl From<String> for Properties {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&[(&str, &str)]> for Properties {
    fn from(kvs: &[(&str, &str)]) -> Properties {
        let p: HashMap<String, String> = kvs
            .iter()
            .cloned()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Properties(p)
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
