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
use crate::core::split_once;
use rand::Rng;
use std::{
    collections::HashMap, convert::TryFrom, fmt, hash::Hash, iter::FromIterator, str::FromStr,
    sync::Arc,
};
use zenoh_core::bail;

// Parsing chars
pub const PROTO_SEPARATOR: char = '/';
pub const METADATA_SEPARATOR: char = '?';
pub const CONFIG_SEPARATOR: char = '#';
pub const LIST_SEPARATOR: char = ';';
pub const FIELD_SEPARATOR: char = '=';

// Propertis
#[derive(Debug, Clone, Eq)]
pub struct ArcProperties(pub Arc<HashMap<String, String>>);
impl std::ops::Deref for ArcProperties {
    type Target = Arc<HashMap<String, String>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Hash for ArcProperties {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl std::ops::DerefMut for ArcProperties {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for ArcProperties {
    fn eq(&self, other: &Self) -> bool {
        Arc::as_ptr(&self.0) == Arc::as_ptr(&other.0)
    }
}

impl From<Arc<HashMap<String, String>>> for ArcProperties {
    fn from(v: Arc<HashMap<String, String>>) -> Self {
        ArcProperties(v)
    }
}

impl From<HashMap<String, String>> for ArcProperties {
    fn from(v: HashMap<String, String>) -> Self {
        ArcProperties(Arc::new(v))
    }
}

impl ArcProperties {
    pub fn merge(&mut self, other: &Arc<HashMap<String, String>>) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            self.0 = other.clone()
        } else {
            self.extend(other.iter().map(|(k, v)| (k.clone(), v.clone())))
        }
    }
}

impl FromStr for ArcProperties {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let this = HashMap::from_iter(s.split(LIST_SEPARATOR).filter_map(|s| {
            let (k, v) = split_once(s, FIELD_SEPARATOR);
            (!k.is_empty()).then(|| (k.to_owned(), v.to_owned()))
        }));
        match this.is_empty() {
            true => Err(()),
            false => Ok(this.into()),
        }
    }
}

impl Extend<(String, String)> for ArcProperties {
    fn extend<T: IntoIterator<Item = (String, String)>>(&mut self, iter: T) {
        let mut iter = iter.into_iter();
        if let Some((k, v)) = iter.next() {
            let (min, max) = iter.size_hint();
            let extended = Arc::make_mut(&mut self.0);
            extended.reserve(max.unwrap_or(min));
            extended.insert(k, v);
            for (k, v) in iter {
                extended.insert(k, v);
            }
        };
    }
}

impl fmt::Display for ArcProperties {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut list = false;
        for (k, v) in self.0.iter() {
            if list {
                write!(f, "{}", LIST_SEPARATOR)?;
            }
            write!(f, "{}{}{}", k, FIELD_SEPARATOR, v)?;
            list = true;
        }
        Ok(())
    }
}

impl ArcProperties {
    #[doc(hidden)]
    pub fn rand() -> Self {
        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let iter = (MIN..rng.gen_range(MIN + 1..MAX)).into_iter().map(|_| {
            let k = (MIN..MAX)
                .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
                .collect::<String>();
            let v = (MIN..MAX)
                .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
                .collect::<String>();
            (k, v)
        });
        Self(Arc::new(HashMap::from_iter(iter)))
    }
}

// Locator
/// A `String` that respects the [`Locator`] canon form: `<proto>/<address>?<metadata>`, such that `<metadata>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub struct Locator {
    pub(crate) inner: String,
    #[serde(skip)]
    pub metadata: Option<ArcProperties>,
}

impl From<Locator> for String {
    fn from(val: Locator) -> Self {
        val.inner
    }
}

impl core::fmt::Display for Locator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.inner)?;
        if let Some(meta) = &self.metadata {
            write!(f, "{}{}", METADATA_SEPARATOR, meta)?;
        }
        Ok(())
    }
}

impl TryFrom<String> for Locator {
    type Error = zenoh_core::Error;
    fn try_from(mut inner: String) -> Result<Self, Self::Error> {
        let (locator, meta) = split_once(&inner, METADATA_SEPARATOR);
        if !locator.contains(PROTO_SEPARATOR) {
            bail!("Missing protocol: locators must be of the form <proto>/<address>[?<metadata>]")
        }
        let metadata = meta.parse().ok();
        let locator_len = locator.len();
        inner.truncate(locator_len);
        Ok(Locator { inner, metadata })
    }
}

impl FromStr for Locator {
    type Err = zenoh_core::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (locator, meta) = split_once(s, METADATA_SEPARATOR);
        if !locator.contains(PROTO_SEPARATOR) {
            bail!("Missing protocol: locators must be of the form <proto>/<address>[?<metadata>]")
        }
        Ok(Locator {
            inner: locator.to_owned(),
            metadata: meta.parse().ok(),
        })
    }
}

impl Locator {
    #[inline(always)]
    pub fn new<Addr: std::fmt::Display>(protocol: &str, addr: &Addr) -> Self {
        Locator::try_from(format!("{}{}{}", protocol, PROTO_SEPARATOR, addr)).unwrap()
    }

    #[must_use = "returns true if successful"]
    pub fn set_addr(&mut self, addr: &str) -> bool {
        let addr_start = self.inner.find(PROTO_SEPARATOR).unwrap() + 1;
        let addr_end = self
            .inner
            .find(METADATA_SEPARATOR)
            .unwrap_or(self.inner.len());
        self.inner.replace_range(addr_start..addr_end, addr);
        true
    }
}

impl Locator {
    pub fn split(
        &self,
    ) -> (
        &str,
        &str,
        impl Iterator<Item = (&str, &str)> + DoubleEndedIterator + Clone,
    ) {
        let (protocol, rest) = split_once(&self.inner, PROTO_SEPARATOR);
        let (address, properties) = split_once(rest, METADATA_SEPARATOR);
        (
            protocol,
            address,
            properties
                .split(LIST_SEPARATOR)
                .map(|prop| split_once(prop, FIELD_SEPARATOR)),
        )
    }

    pub fn protocol(&self) -> &str {
        let index = self.inner.find(PROTO_SEPARATOR).unwrap_or(self.inner.len());
        &self.inner[..index]
    }

    pub fn address(&self) -> &str {
        let index = self.inner.find(PROTO_SEPARATOR).unwrap_or(self.inner.len());
        let rest = &self.inner[index + 1..];
        let index = rest.find(METADATA_SEPARATOR).unwrap_or(rest.len());
        &rest[..index]
    }

    pub fn clone_without_meta(&self) -> Self {
        Locator {
            inner: self.inner.clone(),
            metadata: None,
        }
    }

    pub fn metadata(&self) -> Option<&ArcProperties> {
        self.metadata.as_ref()
    }
}

pub(crate) trait HasCanonForm {
    fn is_canon(&self) -> bool;
    type Output;
    fn canonicalize(self) -> Self::Output;
}
fn cmp(this: &str, than: &str) -> std::cmp::Ordering {
    let is_longer = this.len().cmp(&than.len());
    let this = this.chars();
    let than = than.chars();
    let zip = this.zip(than);
    for (this, than) in zip {
        match this.cmp(&than) {
            std::cmp::Ordering::Equal => {}
            o => return o,
        }
    }
    is_longer
}
impl<'a, T: Iterator<Item = (&'a str, V)> + Clone, V> HasCanonForm for T {
    fn is_canon(&self) -> bool {
        let mut iter = self.clone();
        let mut acc = if let Some((key, _)) = iter.next() {
            key
        } else {
            return true;
        };
        for (key, _) in iter {
            if cmp(key, acc) != std::cmp::Ordering::Greater {
                return false;
            }
            acc = key;
        }
        true
    }
    type Output = Vec<(&'a str, V)>;
    fn canonicalize(mut self) -> Self::Output {
        let mut result = Vec::new();
        if let Some(v) = self.next() {
            result.push(v);
        }
        'outer: for (k, v) in self {
            for (i, (x, _)) in result.iter().enumerate() {
                match cmp(k, x) {
                    std::cmp::Ordering::Less => {
                        result.insert(i, (k, v));
                        continue 'outer;
                    }
                    std::cmp::Ordering::Equal => {
                        result[i].1 = v;
                        continue 'outer;
                    }
                    std::cmp::Ordering::Greater => {}
                }
            }
            result.push((k, v))
        }
        result
    }
}

pub type LocatorProtocol = str;

impl Locator {
    #[doc(hidden)]
    pub fn rand() -> Self {
        const MIN: usize = 2;
        const MAX: usize = 16;

        let mut rng = rand::thread_rng();

        let mut locator = String::new();

        let proto = (MIN..MAX)
            .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
            .collect::<String>();

        locator.push_str(proto.as_str());
        locator.push(PROTO_SEPARATOR);

        let address = (MIN..MAX)
            .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
            .collect::<String>();

        locator.push_str(address.as_str());

        // @TODO: equality for ArcProperties needs to be fixed
        // if rng.gen_bool(0.5) {
        //     locator.push(METADATA_SEPARATOR);
        //     locator.push_str(ArcProperties::rand().to_string().as_str());
        // }

        Locator {
            inner: locator,
            metadata: None,
        }
    }
}

#[test]
fn locators() {
    Locator::from_str("udp/127.0.0.1:7447").unwrap();
    let locator = Locator::from_str("udp/127.0.0.1:7447?hello=there;general=kenobi").unwrap();
    assert_eq!(locator.protocol(), "udp");
    assert_eq!(locator.address(), "127.0.0.1:7447");
    assert_eq!(
        ***locator.metadata().unwrap(),
        [("general", "kenobi"), ("hello", "there")]
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<HashMap<String, String>>()
    );
}
