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

//! Selector to issue queries

use zenoh_core::Result as ZResult;
use zenoh_protocol_core::key_expr::{keyexpr, OwnedKeyExpr};
pub use zenoh_util::time_range::{TimeBound, TimeExpr, TimeRange};

use crate::{prelude::KeyExpr, queryable::Query};

use std::{
    borrow::{Borrow, Cow},
    collections::HashMap,
    convert::TryFrom,
    hash::Hash,
};

/// A selector is the combination of a [Key Expression](crate::prelude::KeyExpr), which defines the
/// set of keys that are relevant to an operation, and a `value_selector`, a set of parameters
/// with a few uses:
/// - specifying arguments to a queryable, allowing the passing of Remote Procedure Call parameters
/// - filtering by value,
/// - filtering by metadata, such as the timestamp of a value,
///
/// When in string form, selectors look a lot like a URI, with similar semantics:
/// - the `key_expr` before the first `?` must be a valid key expression.
/// - the `value_selector` after the first `?` should be encoded like the query section of a URL:
///     - parameters are separated by `&`,
///     - the parameter name and value are separated by the first `=`,
///     - in the absence of `=`, the parameter value is considered to be the empty string,
///     - both name and value should use percent-encoding to escape characters,
///     - defining a value for the same parameter name twice is considered undefined behavior.
///
/// Zenoh intends to standardize the usage of a set of parameter names. To avoid conflicting with RPC parameters,
/// the Zenoh team has settled on reserving the set of parameter names that start with non-alphanumeric characters.
///
/// This document will summarize the standardized parameter names for which Zenoh provides helpers to facilitate
/// coherent behavior for some operations.
///
/// Queryable implementers are encouraged to prefer these standardized parameter names when implementing their
/// associated features, and to prefix their own parameter names to avoid having conflicting parameter names with other
/// queryables.
///
/// Here are the currently standardized parameters for Zenoh:
/// - `_time`: used to express interest in only values dated within a certain time range, values for
///   this parameter must be readable by the [Zenoh Time DSL](zenoh_util::time_range::TimeRange) for the value to be considered valid.
/// - `_filter`: *TBD* Zenoh intends to provide helper tools to allow the value associated with
///   this parameter to be treated as a predicate that the value should fulfill before being returned.
///   A DSL will be designed by the Zenoh team to express these predicates.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    pub key_expr: KeyExpr<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    pub(crate) value_selector: Cow<'a, str>,
}

pub const TIME_RANGE_KEY: &str = "_time";
impl<'a> Selector<'a> {
    /// Gets the value selector as a raw string.
    pub fn value_selector(&self) -> &str {
        &self.value_selector
    }
    /// Extracts the value selector's parameters into a hashmap, returning an error in case of duplicated parameter names.
    pub fn value_selector_map<K, V>(&'a self) -> ZResult<HashMap<K, V>>
    where
        K: AsRef<str> + std::hash::Hash + std::cmp::Eq,
        ExtractedName<'a, Self>: Into<K>,
        ExtractedValue<'a, Self>: Into<V>,
    {
        self.decode_into_map()
    }
    /// Extracts the value selector's name-value pairs into a hashmap, returning an error in case of duplicated parameters.
    pub fn value_selector_cowmap(&'a self) -> ZResult<HashMap<Cow<'a, str>, Cow<'a, str>>> {
        self.decode_into_map()
    }
    /// Extracts the value selector's name-value pairs into a hashmap, returning an error in case of duplicated parameters.
    pub fn value_selector_stringmap(&'a self) -> ZResult<HashMap<String, String>> {
        self.decode_into_map()
    }
    /// Gets a mutable reference to the value_selector as a String.
    ///
    /// Note that calling this function may cause an allocation and copy if the value selector wasn't
    /// already owned by `self`. `self` owns its value selector as soon as this function returns.
    pub fn value_selector_mut(&mut self) -> &mut String {
        if let Cow::Borrowed(s) = self.value_selector {
            self.value_selector = Cow::Owned(s.to_owned())
        }
        if let Cow::Owned(s) = &mut self.value_selector {
            s
        } else {
            unsafe { std::hint::unreachable_unchecked() } // this is safe because we just replaced the borrowed variant
        }
    }
    pub fn set_value_selector(&mut self, selector: impl Into<Cow<'a, str>>) {
        self.value_selector = selector.into();
    }
    pub fn borrowing_clone(&'a self) -> Self {
        Selector {
            key_expr: self.key_expr.clone(),
            value_selector: self.value_selector.as_ref().into(),
        }
    }
    pub fn into_owned(self) -> Selector<'static> {
        Selector {
            key_expr: self.key_expr.into_owned(),
            value_selector: self.value_selector.into_owned().into(),
        }
    }

    #[deprecated = "If you have ownership of this selector, prefer `Selector::into_owned`"]
    pub fn to_owned(&self) -> Selector<'static> {
        self.borrowing_clone().into_owned()
    }

    /// Returns this selectors components as a tuple.
    pub fn split(self) -> (KeyExpr<'a>, Cow<'a, str>) {
        (self.key_expr, self.value_selector)
    }

    /// Sets the `value_selector` part of this `Selector`.
    #[inline(always)]
    pub fn with_value_selector(mut self, value_selector: &'a str) -> Self {
        self.value_selector = value_selector.into();
        self
    }

    pub fn extend<'b, I, K, V>(&'b mut self, parameters: I)
    where
        I: IntoIterator,
        I::Item: std::borrow::Borrow<(K, V)>,
        K: AsRef<str> + 'b,
        V: AsRef<str> + 'b,
    {
        let it = parameters.into_iter();
        let selector = self.value_selector_mut();
        let mut encoder = form_urlencoded::Serializer::new(selector);
        encoder.extend_pairs(it).finish();
    }

    /// Sets the time range targeted by the selector.
    pub fn with_time_range(&mut self, time_range: TimeRange) {
        self.remove_time_range();
        let selector = self.value_selector_mut();
        if !selector.is_empty() {
            selector.push('&')
        }
        use std::fmt::Write;
        write!(selector, "{}={}", TIME_RANGE_KEY, time_range).unwrap(); // This unwrap is safe because `String: Write` should be infallibe.
    }

    pub fn remove_time_range(&mut self) {
        let selector = self.value_selector_mut();

        let mut splice_start = 0;
        let mut splice_end = 0;
        for argument in selector.split('&') {
            if argument.starts_with(TIME_RANGE_KEY)
                && matches!(
                    argument.as_bytes().get(TIME_RANGE_KEY.len()),
                    None | Some(b'=')
                )
            {
                splice_end = splice_start + argument.len();
                break;
            }
            splice_start += argument.len() + 1
        }
        if splice_end > 0 {
            selector.drain(splice_start..(splice_end + (splice_end != selector.len()) as usize));
        }
    }
}

#[test]
fn selector_accessors() {
    let time_range = "[now(-2s)..now(2s)]".parse().unwrap();
    for selector in [
        "hello/there?_timetrick",
        "hello/there?_timetrick&_time",
        "hello/there?_timetrick&_time&_filter",
        "hello/there?_timetrick&_time=[..]",
        "hello/there?_timetrick&_time=[..]&_filter",
    ] {
        let mut selector = Selector::try_from(selector).unwrap();
        selector.with_time_range(time_range);
        assert_eq!(selector.time_range().unwrap().unwrap(), time_range);
        assert!(dbg!(selector.value_selector()).contains("_time=[now(-2s)..now(2s)]"));
        let map_selector = selector.value_selector_cowmap().unwrap();
        assert_eq!(
            selector.time_range().unwrap(),
            map_selector.time_range().unwrap()
        );
    }
}
pub trait Parameter: Sized {
    type Name: AsRef<str> + Sized;
    type Value: AsRef<str> + Sized;
    fn name(&self) -> &Self::Name;
    fn value(&self) -> &Self::Value;
    fn split(self) -> (Self::Name, Self::Value);
    fn extract_name(self) -> Self::Name {
        self.split().0
    }
    fn extract_value(self) -> Self::Value {
        self.split().1
    }
}
impl<N: AsRef<str> + Sized, V: AsRef<str> + Sized> Parameter for (N, V) {
    type Name = N;
    type Value = V;
    fn name(&self) -> &N {
        &self.0
    }
    fn value(&self) -> &V {
        &self.1
    }
    fn split(self) -> (Self::Name, Self::Value) {
        self
    }
    fn extract_name(self) -> Self::Name {
        self.0
    }
    fn extract_value(self) -> Self::Value {
        self.1
    }
}

#[allow(type_alias_bounds)]
type ExtractedName<'a, VS: ValueSelector<'a>> =
    <<VS::Decoder as Iterator>::Item as Parameter>::Name;
#[allow(type_alias_bounds)]
type ExtractedValue<'a, VS: ValueSelector<'a>> =
    <<VS::Decoder as Iterator>::Item as Parameter>::Value;
/// A trait to help decode zenoh value selectors as a set of parameters.
///
/// Most methods will return an Error if duplicates of a same parameter are found, to avoid HTTP Parameter Pollution like vulnerabilities.
pub trait ValueSelector<'a> {
    type Decoder: Iterator + 'a;
    /// Returns this value selector as an iterator of parameters.
    fn decode(&'a self) -> Self::Decoder
    where
        <Self::Decoder as Iterator>::Item: Parameter;

    /// Extracts all parameters into a HashMap, returning an error if duplicate parameters arrise.
    fn decode_into_map<N, V>(&'a self) -> ZResult<HashMap<N, V>>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
        N: AsRef<str> + std::hash::Hash + std::cmp::Eq,
        ExtractedName<'a, Self>: Into<N>,
        ExtractedValue<'a, Self>: Into<V>,
    {
        let mut result: HashMap<N, V> = HashMap::new();
        for (name, value) in self.decode().map(Parameter::split) {
            match result.entry(name.into()) {
                std::collections::hash_map::Entry::Occupied(e) => {
                    bail!("Duplicated parameter `{}` detected", e.key().as_ref())
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(value.into());
                }
            }
        }
        Ok(result)
    }

    /// Extracts the requested parameters from the value selector.
    ///
    /// The default implementation is done in a single pass through the value selector, returning an error if some of the requested parameters are present more than once.
    fn get_attrs<const N: usize>(
        &'a self,
        names: [&str; N],
    ) -> ZResult<[Option<ExtractedValue<'a, Self>>; N]>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
    {
        let mut result = unsafe {
            let mut result: std::mem::MaybeUninit<[Option<ExtractedValue<'a, Self>>; N]> =
                std::mem::MaybeUninit::uninit();
            for slot in result.assume_init_mut() {
                std::ptr::write(slot, None);
            }
            result.assume_init()
        };
        for pair in self.decode() {
            if let Some(index) = names.iter().position(|k| *k == pair.name().as_ref()) {
                let slot = &mut result[index];
                if slot.is_some() {
                    bail!("Duplicated parameter `{}` detected.", names[index])
                }
                *slot = Some(pair.extract_value())
            }
        }
        Ok(result)
    }

    /// Extracts the requested arguments from the value selector as booleans, following the Zenoh convention that if a parameter name is present and has a value different from "false", its value is truthy.
    ///
    /// The default implementation is done in a single pass through the value selector, returning an error if some of the requested parameters are present more than once.
    fn get_bools<const N: usize>(&'a self, names: [&str; N]) -> ZResult<[bool; N]>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
    {
        Ok(self.get_attrs(names)?.map(|v| match v {
            None => false,
            Some(s) => s.as_ref() != "false",
        }))
    }

    /// Extracts the standardized `_time` argument from the value selector.
    ///
    /// The default implementation still causes a complete pass through the value selector to ensure that there are no duplicates of the `_time` key.
    fn time_range(&'a self) -> ZResult<Option<TimeRange>>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
    {
        Ok(match &self.get_attrs([TIME_RANGE_KEY])?[0] {
            Some(s) => Some(s.as_ref().parse()?),
            None => None,
        })
    }
}
impl<'a> ValueSelector<'a> for Selector<'a> {
    type Decoder = <str as ValueSelector<'a>>::Decoder;
    fn decode(&'a self) -> Self::Decoder {
        self.value_selector().decode()
    }
}
impl<'a> ValueSelector<'a> for str {
    type Decoder = form_urlencoded::Parse<'a>;
    fn decode(&'a self) -> Self::Decoder {
        form_urlencoded::parse(self.as_bytes())
    }
}

impl<'a, K: Borrow<str> + Hash + Eq + 'a, V: Borrow<str> + 'a> ValueSelector<'a> for HashMap<K, V> {
    type Decoder = std::collections::hash_map::Iter<'a, K, V>;
    fn decode(&'a self) -> Self::Decoder {
        self.iter()
    }
    fn get_attrs<const N: usize>(
        &'a self,
        names: [&str; N],
    ) -> ZResult<[Option<ExtractedValue<'a, Self>>; N]>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
    {
        // `Ok(names.map(|key| self.get(key)))` would be very slightly faster, but doesn't compile for some reason :(
        Ok(names.map(|key| self.get_key_value(key).map(|kv| kv.extract_value())))
    }
}

impl std::fmt::Debug for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "sel\"{}\"", self)
    }
}

impl std::fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}?{}", self.key_expr, self.value_selector)
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl TryFrom<String> for Selector<'_> {
    type Error = zenoh_core::Error;
    fn try_from(mut s: String) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let value_selector = s[qmark_position + 1..].to_owned();
                s.truncate(qmark_position);
                Ok(KeyExpr::try_from(s)?.with_owned_value_selector(value_selector))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}

impl<'a> TryFrom<&'a str> for Selector<'a> {
    type Error = zenoh_core::Error;
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let value_selector = &s[qmark_position + 1..];
                Ok(KeyExpr::try_from(&s[..qmark_position])?.with_value_selector(value_selector))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}

impl<'a> TryFrom<&'a String> for Selector<'a> {
    type Error = zenoh_core::Error;
    fn try_from(s: &'a String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl<'a> From<&'a Query> for Selector<'a> {
    fn from(q: &'a Query) -> Self {
        Selector {
            key_expr: q.key_expr.clone(),
            value_selector: (&q.value_selector).into(),
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &KeyExpr<'a>) -> Self {
        Self {
            key_expr: key_selector.clone(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<&'a keyexpr> for Selector<'a> {
    fn from(key_selector: &'a keyexpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<&'a OwnedKeyExpr> for Selector<'a> {
    fn from(key_selector: &'a OwnedKeyExpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            value_selector: "".into(),
        }
    }
}

impl From<OwnedKeyExpr> for Selector<'static> {
    fn from(key_selector: OwnedKeyExpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            value_selector: "".into(),
        }
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self {
            key_expr: key_selector,
            value_selector: "".into(),
        }
    }
}
