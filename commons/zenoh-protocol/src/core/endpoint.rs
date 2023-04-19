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
use super::locator::*;
use crate::core::split_once;
use alloc::{borrow::ToOwned, format, string::String, vec::Vec};
use core::{convert::TryFrom, fmt, str::FromStr};
use zenoh_result::{zerror, Error as ZError, ZResult};

// Parsing chars
pub const PROTO_SEPARATOR: char = '/';
pub const METADATA_SEPARATOR: char = '?';
pub const LIST_SEPARATOR: char = ';';
pub const FIELD_SEPARATOR: char = '=';
pub const CONFIG_SEPARATOR: char = '#';

// Parsing functions
pub(super) fn protocol(s: &str) -> &str {
    let pdix = s.find(PROTO_SEPARATOR).unwrap_or(s.len());
    &s[..pdix]
}

pub(super) fn address(s: &str) -> &str {
    let pdix = s.find(PROTO_SEPARATOR).unwrap_or(s.len());
    let midx = s.find(METADATA_SEPARATOR).unwrap_or(s.len());
    let cidx = s.find(CONFIG_SEPARATOR).unwrap_or(s.len());
    &s[pdix + 1..midx.min(cidx)]
}

pub(super) fn metadata(s: &str) -> &str {
    match s.find(METADATA_SEPARATOR) {
        Some(midx) => {
            let cidx = s.find(CONFIG_SEPARATOR).unwrap_or(s.len());
            &s[midx + 1..cidx]
        }
        None => "",
    }
}

pub(super) fn config(s: &str) -> &str {
    match s.find(CONFIG_SEPARATOR) {
        Some(cidx) => &s[cidx + 1..],
        None => "",
    }
}

pub(super) fn read_properties(s: &str) -> impl Iterator<Item = (&str, &str)> + DoubleEndedIterator {
    s.split(LIST_SEPARATOR).filter_map(|prop| {
        if prop.is_empty() {
            None
        } else {
            Some(split_once(prop, FIELD_SEPARATOR))
        }
    })
}

pub(super) fn write_properties<'s, I>(iter: I, into: &mut String)
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let mut first = true;
    for (k, v) in iter {
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

pub(super) fn extend_properties<'s, I>(iter: I, k: &'s str, v: &'s str) -> String
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let current = iter.filter(|x| x.0 != k);
    let new = Some((k, v)).into_iter();
    let iter = current.chain(new);

    let mut into = String::new();
    write_properties(iter, &mut into);
    into
}

pub(super) fn remove_properties<'s, I>(iter: I, k: &'s str) -> String
where
    I: Iterator<Item = (&'s str, &'s str)>,
{
    let iter = iter.filter(|x| x.0 != k);

    let mut into = String::new();
    write_properties(iter, &mut into);
    into
}

// Protocol
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Protocol<'a>(pub(super) &'a str);

impl<'a> Protocol<'a> {
    pub fn as_str(&'a self) -> &'a str {
        self.0
    }
}

impl AsRef<str> for Protocol<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Protocol<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ProtocolMut<'a>(&'a mut EndPoint);

impl<'a> ProtocolMut<'a> {
    pub fn as_str(&'a self) -> &'a str {
        address(self.0.as_str())
    }

    pub fn set(&mut self, p: &str) -> ZResult<()> {
        let ep = EndPoint::new(p, self.0.address(), self.0.metadata(), self.0.config())?;

        self.0.inner = ep.inner;
        Ok(())
    }
}

impl AsRef<str> for ProtocolMut<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ProtocolMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Address
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Address<'a>(pub(super) &'a str);

impl<'a> Address<'a> {
    pub fn as_str(&'a self) -> &'a str {
        self.0
    }
}

impl AsRef<str> for Address<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Address<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct AddressMut<'a>(&'a mut EndPoint);

impl<'a> AddressMut<'a> {
    pub fn as_str(&'a self) -> &'a str {
        address(self.0.as_str())
    }

    pub fn set(&'a mut self, a: &str) -> ZResult<()> {
        let ep = EndPoint::new(self.0.protocol(), a, self.0.metadata(), self.0.config())?;

        self.0.inner = ep.inner;
        Ok(())
    }
}

impl AsRef<str> for AddressMut<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AddressMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Metadata
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Metadata<'a>(pub(super) &'a str);

impl<'a> Metadata<'a> {
    pub fn as_str(&'a self) -> &'a str {
        self.0
    }

    pub fn is_empty(&'a self) -> bool {
        self.as_str().is_empty()
    }

    pub fn iter(&'a self) -> impl Iterator<Item = (&'a str, &'a str)> + DoubleEndedIterator {
        read_properties(self.0)
    }

    pub fn get(&'a self, k: &str) -> Option<&'a str> {
        self.iter().find(|x| x.0 == k).map(|x| x.1)
    }
}

impl AsRef<str> for Metadata<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Metadata<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct MetadataMut<'a>(&'a mut EndPoint);

impl<'a> MetadataMut<'a> {
    pub fn as_str(&'a self) -> &'a str {
        metadata(self.0.as_str())
    }

    pub fn is_empty(&'a self) -> bool {
        self.as_str().is_empty()
    }
}

impl MetadataMut<'_> {
    pub fn extend<I, K, V>(&mut self, iter: I) -> ZResult<()>
    where
        I: Iterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (k, v) in iter {
            let k: &str = k.as_ref();
            let v: &str = v.as_ref();
            self.insert(k, v)?
        }
        Ok(())
    }

    pub fn insert(&mut self, k: &str, v: &str) -> ZResult<()> {
        let ep = EndPoint::new(
            self.0.protocol(),
            self.0.address(),
            extend_properties(self.0.metadata().iter(), k, v),
            self.0.config(),
        )?;

        self.0.inner = ep.inner;
        Ok(())
    }

    pub fn remove(&mut self, k: &str) -> ZResult<()> {
        let ep = EndPoint::new(
            self.0.protocol(),
            self.0.address(),
            remove_properties(self.0.metadata().iter(), k),
            self.0.config(),
        )?;

        self.0.inner = ep.inner;
        Ok(())
    }
}

impl AsRef<str> for MetadataMut<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for MetadataMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Config
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Config<'a>(pub(super) &'a str);

impl<'a> Config<'a> {
    pub fn as_str(&'a self) -> &'a str {
        self.0
    }

    pub fn is_empty(&'a self) -> bool {
        self.as_str().is_empty()
    }

    pub fn iter(&'a self) -> impl Iterator<Item = (&'a str, &'a str)> + DoubleEndedIterator {
        read_properties(self.0)
    }

    pub fn get(&'a self, k: &str) -> Option<&'a str> {
        self.iter().find(|x| x.0 == k).map(|x| x.1)
    }
}

impl AsRef<str> for Config<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Config<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ConfigMut<'a>(&'a mut EndPoint);

impl<'a> ConfigMut<'a> {
    pub fn as_str(&'a self) -> &'a str {
        config(self.0.as_str())
    }

    pub fn is_empty(&'a self) -> bool {
        self.as_str().is_empty()
    }
}

impl ConfigMut<'_> {
    pub fn extend<I, K, V>(&mut self, iter: I) -> ZResult<()>
    where
        I: Iterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        for (k, v) in iter {
            let k: &str = k.as_ref();
            let v: &str = v.as_ref();
            self.insert(k, v)?
        }
        Ok(())
    }

    pub fn insert(&mut self, k: &str, v: &str) -> ZResult<()> {
        let ep = EndPoint::new(
            self.0.protocol(),
            self.0.address(),
            self.0.metadata(),
            extend_properties(self.0.config().iter(), k, v),
        )?;

        self.0.inner = ep.inner;
        Ok(())
    }

    pub fn remove(&mut self, k: &str) -> ZResult<()> {
        let ep = EndPoint::new(
            self.0.protocol(),
            self.0.address(),
            self.0.metadata(),
            remove_properties(self.0.config().iter(), k),
        )?;

        self.0.inner = ep.inner;
        Ok(())
    }
}

impl AsRef<str> for ConfigMut<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ConfigMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
/// A `String` that respects the [`EndPoint`] canon form: `<locator>#<config>`, such that `<locator>` is a valid [`Locator`] `<config>` is of the form `<key1>=<value1>;...;<keyN>=<valueN>` where keys are alphabetically sorted.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(into = "String")]
#[serde(try_from = "String")]
pub struct EndPoint {
    pub(super) inner: String,
}

impl EndPoint {
    pub fn new<A, B, C, D>(protocol: A, address: B, metadata: C, config: D) -> ZResult<Self>
    where
        A: AsRef<str>,
        B: AsRef<str>,
        C: AsRef<str>,
        D: AsRef<str>,
    {
        let p: &str = protocol.as_ref();
        let a: &str = address.as_ref();
        let m: &str = metadata.as_ref();
        let c: &str = config.as_ref();

        let s = match (m.is_empty(), c.is_empty()) {
            (true, true) => format!("{p}{PROTO_SEPARATOR}{a}"),
            (false, true) => format!("{p}{PROTO_SEPARATOR}{a}{METADATA_SEPARATOR}{m}"),
            (true, false) => format!("{p}{PROTO_SEPARATOR}{a}{CONFIG_SEPARATOR}{c}"),
            (false, false) => {
                format!("{p}{PROTO_SEPARATOR}{a}{METADATA_SEPARATOR}{m}{CONFIG_SEPARATOR}{c}")
            }
        };

        Self::try_from(s)
    }

    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    pub fn split(&self) -> (Protocol, Address, Metadata, Config) {
        (
            self.protocol(),
            self.address(),
            self.metadata(),
            self.config(),
        )
    }

    pub fn protocol(&self) -> Protocol {
        Protocol(protocol(self.inner.as_str()))
    }

    pub fn protocol_mut(&mut self) -> ProtocolMut {
        ProtocolMut(self)
    }

    pub fn address(&self) -> Address {
        Address(address(self.inner.as_str()))
    }

    pub fn address_mut(&mut self) -> AddressMut {
        AddressMut(self)
    }

    pub fn metadata(&self) -> Metadata {
        Metadata(metadata(self.inner.as_str()))
    }

    pub fn metadata_mut(&mut self) -> MetadataMut {
        MetadataMut(self)
    }

    pub fn config(&self) -> Config {
        Config(config(self.inner.as_str()))
    }

    pub fn config_mut(&mut self) -> ConfigMut {
        ConfigMut(self)
    }

    pub fn to_locator(&self) -> Locator {
        self.clone().into()
    }
}

impl From<Locator> for EndPoint {
    fn from(val: Locator) -> Self {
        val.0
    }
}

impl fmt::Display for EndPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl From<EndPoint> for String {
    fn from(v: EndPoint) -> String {
        v.inner
    }
}

impl TryFrom<String> for EndPoint {
    type Error = ZError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        const ERR: &str =
            "Endpoints must be of the form <protocol>/<address>[?<metadata>][#<config>]";

        fn sort_hashmap(from: &str, into: &mut String) {
            let mut from = from
                .split(LIST_SEPARATOR)
                .map(|p| split_once(p, FIELD_SEPARATOR))
                .collect::<Vec<(&str, &str)>>();
            from.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

            let mut first = true;
            for (k, v) in from.iter() {
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

        let pidx = s
            .find(PROTO_SEPARATOR)
            .and_then(|i| (!s[..i].is_empty() && !s[i + 1..].is_empty()).then_some(i))
            .ok_or_else(|| zerror!("{}: {}", ERR, s))?;

        match (s.find(METADATA_SEPARATOR), s.find(CONFIG_SEPARATOR)) {
            // No metadata or config at all
            (None, None) => Ok(EndPoint { inner: s }),
            // There is some metadata
            (Some(midx), None) if midx > pidx && !s[midx + 1..].is_empty() => {
                let mut inner = String::with_capacity(s.len());
                inner.push_str(&s[..midx + 1]); // Includes metadata separator
                sort_hashmap(&s[midx + 1..], &mut inner);
                Ok(EndPoint { inner })
            }
            // There is some config
            (None, Some(cidx)) if cidx > pidx && !s[cidx + 1..].is_empty() => {
                let mut inner = String::with_capacity(s.len());
                inner.push_str(&s[..cidx + 1]); // Includes config separator
                sort_hashmap(&s[cidx + 1..], &mut inner);
                Ok(EndPoint { inner })
            }
            // There is some metadata and some config
            (Some(midx), Some(cidx))
                if midx > pidx
                    && cidx > midx
                    && !s[midx + 1..cidx].is_empty()
                    && !s[cidx + 1..].is_empty() =>
            {
                let mut inner = String::with_capacity(s.len());
                inner.push_str(&s[..midx + 1]); // Includes metadata separator

                sort_hashmap(&s[midx + 1..cidx], &mut inner);

                inner.push(CONFIG_SEPARATOR);
                sort_hashmap(&s[cidx + 1..], &mut inner);

                Ok(EndPoint { inner })
            }
            _ => Err(zerror!("{}: {}", ERR, s).into()),
        }
    }
}

impl FromStr for EndPoint {
    type Err = ZError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

impl EndPoint {
    #[cfg(feature = "test")]
    pub fn rand() -> Self {
        use rand::{
            distributions::{Alphanumeric, DistString},
            rngs::ThreadRng,
            Rng,
        };

        const MIN: usize = 2;
        const MAX: usize = 16;

        fn gen_hashmap(rng: &mut ThreadRng, endpoint: &mut String) {
            let num = rng.gen_range(MIN..MAX);
            for i in 0..num {
                if i != 0 {
                    endpoint.push(LIST_SEPARATOR);
                }
                let len = rng.gen_range(MIN..MAX);
                let key = Alphanumeric.sample_string(rng, len);
                endpoint.push_str(key.as_str());

                endpoint.push(FIELD_SEPARATOR);

                let len = rng.gen_range(MIN..MAX);
                let value = Alphanumeric.sample_string(rng, len);
                endpoint.push_str(value.as_str());
            }
        }

        let mut rng = rand::thread_rng();
        let mut endpoint = String::new();

        let len = rng.gen_range(MIN..MAX);
        let proto = Alphanumeric.sample_string(&mut rng, len);
        endpoint.push_str(proto.as_str());

        endpoint.push(PROTO_SEPARATOR);

        let len = rng.gen_range(MIN..MAX);
        let address = Alphanumeric.sample_string(&mut rng, len);
        endpoint.push_str(address.as_str());

        if rng.gen_bool(0.5) {
            endpoint.push(METADATA_SEPARATOR);
            gen_hashmap(&mut rng, &mut endpoint);
        }
        if rng.gen_bool(0.5) {
            endpoint.push(CONFIG_SEPARATOR);
            gen_hashmap(&mut rng, &mut endpoint);
        }

        endpoint.parse().unwrap()
    }
}

#[test]
fn endpoints() {
    assert!(EndPoint::from_str("/").is_err());
    assert!(EndPoint::from_str("?").is_err());
    assert!(EndPoint::from_str("#").is_err());

    assert!(EndPoint::from_str("udp").is_err());
    assert!(EndPoint::from_str("/udp").is_err());
    assert!(EndPoint::from_str("udp/").is_err());

    assert!(EndPoint::from_str("udp/127.0.0.1:7447?").is_err());
    assert!(EndPoint::from_str("udp?127.0.0.1:7447").is_err());
    assert!(EndPoint::from_str("udp?127.0.0.1:7447/meta").is_err());

    assert!(EndPoint::from_str("udp/127.0.0.1:7447#").is_err());
    assert!(EndPoint::from_str("udp/127.0.0.1:7447?#").is_err());
    assert!(EndPoint::from_str("udp/127.0.0.1:7447#?").is_err());
    assert!(EndPoint::from_str("udp#127.0.0.1:7447/").is_err());
    assert!(EndPoint::from_str("udp#127.0.0.1:7447/?").is_err());
    assert!(EndPoint::from_str("udp/127.0.0.1:7447?a=1#").is_err());

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert!(endpoint.metadata().as_str().is_empty());
    assert_eq!(endpoint.metadata().iter().count(), 0);

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447?a=1;b=2").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert_eq!(endpoint.metadata().as_str(), "a=1;b=2");
    assert_eq!(endpoint.metadata().iter().count(), 2);
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("a", "1"))
        .unwrap();
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("b", "2"))
        .unwrap();
    assert!(endpoint.config().as_str().is_empty());
    assert_eq!(endpoint.config().iter().count(), 0);

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447?b=2;a=1").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert_eq!(endpoint.metadata().as_str(), "a=1;b=2");
    assert_eq!(endpoint.metadata().iter().count(), 2);
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("a", "1"))
        .unwrap();
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("b", "2"))
        .unwrap();
    assert!(endpoint.config().as_str().is_empty());
    assert_eq!(endpoint.config().iter().count(), 0);

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447#A=1;B=2").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447#A=1;B=2");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert!(endpoint.metadata().as_str().is_empty());
    assert_eq!(endpoint.metadata().iter().count(), 0);
    assert_eq!(endpoint.config().as_str(), "A=1;B=2");
    assert_eq!(endpoint.config().iter().count(), 2);
    endpoint.config().iter().find(|x| x == &("A", "1")).unwrap();
    endpoint.config().iter().find(|x| x == &("B", "2")).unwrap();

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447#B=2;A=1").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447#A=1;B=2");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert!(endpoint.metadata().as_str().is_empty());
    assert_eq!(endpoint.metadata().iter().count(), 0);
    assert_eq!(endpoint.config().as_str(), "A=1;B=2");
    assert_eq!(endpoint.config().iter().count(), 2);
    endpoint.config().iter().find(|x| x == &("A", "1")).unwrap();
    endpoint.config().iter().find(|x| x == &("B", "2")).unwrap();

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447?a=1;b=2#A=1;B=2").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2#A=1;B=2");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert_eq!(endpoint.metadata().as_str(), "a=1;b=2");
    assert_eq!(endpoint.metadata().iter().count(), 2);
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("a", "1"))
        .unwrap();
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("b", "2"))
        .unwrap();
    assert_eq!(endpoint.config().as_str(), "A=1;B=2");
    assert_eq!(endpoint.config().iter().count(), 2);
    endpoint.config().iter().find(|x| x == &("A", "1")).unwrap();
    endpoint.config().iter().find(|x| x == &("B", "2")).unwrap();

    let endpoint = EndPoint::from_str("udp/127.0.0.1:7447?b=2;a=1#B=2;A=1").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2#A=1;B=2");
    assert_eq!(endpoint.protocol().as_str(), "udp");
    assert_eq!(endpoint.address().as_str(), "127.0.0.1:7447");
    assert_eq!(endpoint.metadata().as_str(), "a=1;b=2");
    assert_eq!(endpoint.metadata().iter().count(), 2);
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("a", "1"))
        .unwrap();
    endpoint
        .metadata()
        .iter()
        .find(|x| x == &("b", "2"))
        .unwrap();
    assert_eq!(endpoint.config().as_str(), "A=1;B=2");
    assert_eq!(endpoint.config().iter().count(), 2);
    endpoint.config().iter().find(|x| x == &("A", "1")).unwrap();
    endpoint.config().iter().find(|x| x == &("B", "2")).unwrap();

    let mut endpoint = EndPoint::from_str("udp/127.0.0.1:7447?a=1;b=2").unwrap();
    endpoint.metadata_mut().insert("c", "3").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2;c=3");

    let mut endpoint = EndPoint::from_str("udp/127.0.0.1:7447?b=2;c=3").unwrap();
    endpoint.metadata_mut().insert("a", "1").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2;c=3");

    let mut endpoint = EndPoint::from_str("udp/127.0.0.1:7447?a=1;b=2").unwrap();
    endpoint.config_mut().insert("A", "1").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2#A=1");

    let mut endpoint = EndPoint::from_str("udp/127.0.0.1:7447?b=2;c=3#B=2").unwrap();
    endpoint.config_mut().insert("A", "1").unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?b=2;c=3#A=1;B=2");

    let mut endpoint = EndPoint::from_str("udp/127.0.0.1:7447").unwrap();
    endpoint
        .metadata_mut()
        .extend([("a", "1"), ("c", "3"), ("b", "2")].iter().copied())
        .unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447?a=1;b=2;c=3");

    let mut endpoint = EndPoint::from_str("udp/127.0.0.1:7447").unwrap();
    endpoint
        .config_mut()
        .extend([("A", "1"), ("C", "3"), ("B", "2")].iter().copied())
        .unwrap();
    assert_eq!(endpoint.as_str(), "udp/127.0.0.1:7447#A=1;B=2;C=3");
}
