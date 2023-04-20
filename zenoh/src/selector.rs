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

//! [Selector](https://github.com/eclipse-zenoh/roadmap/tree/main/rfcs/ALL/Selectors) to issue queries

use zenoh_protocol::core::key_expr::{keyexpr, OwnedKeyExpr};
use zenoh_result::ZResult;
pub use zenoh_util::time_range::{TimeBound, TimeExpr, TimeRange};

use crate::{prelude::KeyExpr, queryable::Query};

use std::{
    borrow::{Borrow, Cow},
    collections::HashMap,
    convert::TryFrom,
    hash::Hash,
};

/// A selector is the combination of a [Key Expression](crate::prelude::KeyExpr), which defines the
/// set of keys that are relevant to an operation, and a set of parameters
/// with a few intendend uses:
/// - specifying arguments to a queryable, allowing the passing of Remote Procedure Call parameters
/// - filtering by value,
/// - filtering by metadata, such as the timestamp of a value,
/// - specifying arguments to zenoh when using the REST API.
///
/// When in string form, selectors look a lot like a URI, with similar semantics:
/// - the `key_expr` before the first `?` must be a valid key expression.
/// - the `parameters` after the first `?` should be encoded like the query section of a URL:
///     - parameters are separated by `&`,
///     - the parameter name and value are separated by the first `=`,
///     - in the absence of `=`, the parameter value is considered to be the empty string,
///     - both name and value should use percent-encoding to escape characters,
///     - defining a value for the same parameter name twice is considered undefined behavior,
///       with the encouraged behaviour being to reject operations when a duplicate parameter is detected.
///
/// Zenoh intends to standardize the usage of a set of parameter names. To avoid conflicting with RPC parameters,
/// the Zenoh team has settled on reserving the set of parameter names that start with non-alphanumeric characters.
///
/// The full specification for selectors is available [here](https://github.com/eclipse-zenoh/roadmap/tree/main/rfcs/ALL/Selectors),
/// it includes standardized parameters.
///
/// Queryable implementers are encouraged to prefer these standardized parameter names when implementing their
/// associated features, and to prefix their own parameter names to avoid having conflicting parameter names with other
/// queryables.
///
/// Here are the currently standardized parameters for Zenoh (check the specification page for the exhaustive list):
/// - `_time`: used to express interest in only values dated within a certain time range, values for
///   this parameter must be readable by the [Zenoh Time DSL](zenoh_util::time_range::TimeRange) for the value to be considered valid.
/// - **`[unstable]`** `_anyke`: used in queries to express interest in replies coming from any key expression. By default, only replies
///   whose key expression match query's key expression are accepted. `_anyke` disables the query-reply key expression matching check.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    pub key_expr: KeyExpr<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    pub(crate) parameters: Cow<'a, str>,
}

pub const TIME_RANGE_KEY: &str = "_time";
impl<'a> Selector<'a> {
    /// Gets the parameters as a raw string.
    pub fn parameters(&self) -> &str {
        &self.parameters
    }
    /// Extracts the selector parameters into a hashmap, returning an error in case of duplicated parameter names.
    pub fn parameters_map<K, V>(&'a self) -> ZResult<HashMap<K, V>>
    where
        K: AsRef<str> + std::hash::Hash + std::cmp::Eq,
        ExtractedName<'a, Self>: Into<K>,
        ExtractedValue<'a, Self>: Into<V>,
    {
        self.decode_into_map()
    }
    /// Extracts the selector parameters' name-value pairs into a hashmap, returning an error in case of duplicated parameters.
    pub fn parameters_cowmap(&'a self) -> ZResult<HashMap<Cow<'a, str>, Cow<'a, str>>> {
        self.decode_into_map()
    }
    /// Extracts the selector parameters' name-value pairs into a hashmap, returning an error in case of duplicated parameters.
    pub fn parameters_stringmap(&'a self) -> ZResult<HashMap<String, String>> {
        self.decode_into_map()
    }
    /// Gets a mutable reference to the parameters as a String.
    ///
    /// Note that calling this function may cause an allocation and copy if the selector's parameters wasn't
    /// already owned by `self`. `self` owns its parameters as soon as this function returns.
    pub fn parameters_mut(&mut self) -> &mut String {
        if let Cow::Borrowed(s) = self.parameters {
            self.parameters = Cow::Owned(s.to_owned())
        }
        if let Cow::Owned(s) = &mut self.parameters {
            s
        } else {
            unsafe { std::hint::unreachable_unchecked() } // this is safe because we just replaced the borrowed variant
        }
    }
    pub fn set_parameters(&mut self, selector: impl Into<Cow<'a, str>>) {
        self.parameters = selector.into();
    }
    pub fn borrowing_clone(&'a self) -> Self {
        Selector {
            key_expr: self.key_expr.clone(),
            parameters: self.parameters.as_ref().into(),
        }
    }
    pub fn into_owned(self) -> Selector<'static> {
        Selector {
            key_expr: self.key_expr.into_owned(),
            parameters: self.parameters.into_owned().into(),
        }
    }

    #[deprecated = "If you have ownership of this selector, prefer `Selector::into_owned`"]
    pub fn to_owned(&self) -> Selector<'static> {
        self.borrowing_clone().into_owned()
    }

    /// Returns this selectors components as a tuple.
    pub fn split(self) -> (KeyExpr<'a>, Cow<'a, str>) {
        (self.key_expr, self.parameters)
    }

    /// Sets the `parameters` part of this `Selector`.
    #[inline(always)]
    pub fn with_parameters(mut self, parameters: &'a str) -> Self {
        self.parameters = parameters.into();
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
        let selector = self.parameters_mut();
        let mut encoder = form_urlencoded::Serializer::new(selector);
        encoder.extend_pairs(it).finish();
    }

    /// Sets the time range targeted by the selector.
    pub fn with_time_range(&mut self, time_range: TimeRange) {
        self.remove_time_range();
        let selector = self.parameters_mut();
        if !selector.is_empty() {
            selector.push('&')
        }
        use std::fmt::Write;
        write!(selector, "{TIME_RANGE_KEY}={time_range}").unwrap(); // This unwrap is safe because `String: Write` should be infallibe.
    }

    pub fn remove_time_range(&mut self) {
        let selector = self.parameters_mut();

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
    #[cfg(any(feature = "unstable", test))]
    pub(crate) fn parameter_index(&self, param_name: &str) -> ZResult<Option<u32>> {
        let starts_with_param = |s: &str| {
            if let Some(rest) = s.strip_prefix(param_name) {
                matches!(rest.as_bytes().first(), None | Some(b'='))
            } else {
                false
            }
        };
        let mut acc = 0;
        let mut res = None;
        for chunk in self.parameters().split('&') {
            if starts_with_param(chunk) {
                if res.is_none() {
                    res = Some(acc)
                } else {
                    bail!(
                        "parameter `{}` appeared multiple times in selector `{}`.",
                        param_name,
                        self
                    )
                }
            }
            acc += chunk.len() as u32 + 1;
        }
        Ok(res)
    }
    #[cfg(any(feature = "unstable", test))]
    pub(crate) fn accept_any_keyexpr(self, any: bool) -> ZResult<Selector<'static>> {
        use crate::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM;
        let mut s = self.into_owned();
        let any_selparam = s.parameter_index(_REPLY_KEY_EXPR_ANY_SEL_PARAM)?;
        match (any, any_selparam) {
            (true, None) => {
                let s = s.parameters_mut();
                if !s.is_empty() {
                    s.push('&')
                }
                s.push_str(_REPLY_KEY_EXPR_ANY_SEL_PARAM);
            }
            (false, Some(index)) => {
                let s = dbg!(s.parameters_mut());
                let mut start = index as usize;
                let pend = start + _REPLY_KEY_EXPR_ANY_SEL_PARAM.len();
                if dbg!(start) != 0 {
                    start -= 1
                }
                match dbg!(&s[pend..]).find('&') {
                    Some(end) => std::mem::drop(s.drain(start..end + pend)),
                    None => s.truncate(start),
                }
                dbg!(s);
            }
            _ => {}
        }
        Ok(s)
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
        assert!(dbg!(selector.parameters()).contains("_time=[now(-2s)..now(2s)]"));
        let map_selector = selector.parameters_cowmap().unwrap();
        assert_eq!(
            selector.time_range().unwrap(),
            map_selector.time_range().unwrap()
        );
        let without_any = selector.to_string();
        let with_any = selector.to_string() + "&" + crate::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM;
        selector = selector.accept_any_keyexpr(false).unwrap();
        assert_eq!(selector.to_string(), without_any);
        selector = selector.accept_any_keyexpr(true).unwrap();
        assert_eq!(selector.to_string(), with_any);
        selector = selector.accept_any_keyexpr(true).unwrap();
        assert_eq!(selector.to_string(), with_any);
        selector = selector.accept_any_keyexpr(false).unwrap();
        assert_eq!(selector.to_string(), without_any);
        selector = selector.accept_any_keyexpr(true).unwrap();
        assert_eq!(selector.to_string(), with_any);
        selector.parameters_mut().push_str("&other");
        assert_eq!(selector.to_string(), with_any + "&other");
        selector = selector.accept_any_keyexpr(false).unwrap();
        assert_eq!(selector.to_string(), without_any + "&other");
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
type ExtractedName<'a, VS: Parameters<'a>> = <<VS::Decoder as Iterator>::Item as Parameter>::Name;
#[allow(type_alias_bounds)]
type ExtractedValue<'a, VS: Parameters<'a>> = <<VS::Decoder as Iterator>::Item as Parameter>::Value;
/// A trait to help decode zenoh selector parameters.
///
/// Most methods will return an Error if duplicates of a same parameter are found, to avoid HTTP Parameter Pollution like vulnerabilities.
pub trait Parameters<'a> {
    type Decoder: Iterator + 'a;
    /// Returns this selector's parameters as an iterator.
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

    /// Extracts the requested parameters from the selector parameters.
    ///
    /// The default implementation is done in a single pass through the selector parameters, returning an error if any of the requested parameters are present more than once.
    fn get_parameters<const N: usize>(
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

    /// Extracts the requested arguments from the selector parameters as booleans, following the Zenoh convention that if a parameter name is present and has a value different from "false", its value is truthy.
    ///
    /// The default implementation is done in a single pass through the selector parameters, returning an error if some of the requested parameters are present more than once.
    fn get_bools<const N: usize>(&'a self, names: [&str; N]) -> ZResult<[bool; N]>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
    {
        Ok(self.get_parameters(names)?.map(|v| match v {
            None => false,
            Some(s) => s.as_ref() != "false",
        }))
    }

    /// Extracts the standardized `_time` argument from the selector parameters.
    ///
    /// The default implementation still causes a complete pass through the selector parameters to ensure that there are no duplicates of the `_time` key.
    fn time_range(&'a self) -> ZResult<Option<TimeRange>>
    where
        <Self::Decoder as Iterator>::Item: Parameter,
    {
        Ok(match &self.get_parameters([TIME_RANGE_KEY])?[0] {
            Some(s) => Some(s.as_ref().parse()?),
            None => None,
        })
    }
}
impl<'a> Parameters<'a> for Selector<'a> {
    type Decoder = <str as Parameters<'a>>::Decoder;
    fn decode(&'a self) -> Self::Decoder {
        self.parameters().decode()
    }
}
impl<'a> Parameters<'a> for str {
    type Decoder = form_urlencoded::Parse<'a>;
    fn decode(&'a self) -> Self::Decoder {
        form_urlencoded::parse(self.as_bytes())
    }
}

impl<'a, K: Borrow<str> + Hash + Eq + 'a, V: Borrow<str> + 'a> Parameters<'a> for HashMap<K, V> {
    type Decoder = std::collections::hash_map::Iter<'a, K, V>;
    fn decode(&'a self) -> Self::Decoder {
        self.iter()
    }
    fn get_parameters<const N: usize>(
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
        write!(f, "sel\"{self}\"")
    }
}

impl std::fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.key_expr)?;
        if !self.parameters.is_empty() {
            write!(f, "?{}", self.parameters)?;
        }
        Ok(())
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl TryFrom<String> for Selector<'_> {
    type Error = zenoh_result::Error;
    fn try_from(mut s: String) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let parameters = s[qmark_position + 1..].to_owned();
                s.truncate(qmark_position);
                Ok(KeyExpr::try_from(s)?.with_owned_parameters(parameters))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}

impl<'a> TryFrom<&'a str> for Selector<'a> {
    type Error = zenoh_result::Error;
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        match s.find('?') {
            Some(qmark_position) => {
                let params = &s[qmark_position + 1..];
                Ok(KeyExpr::try_from(&s[..qmark_position])?.with_parameters(params))
            }
            None => Ok(KeyExpr::try_from(s)?.into()),
        }
    }
}

impl<'a> TryFrom<&'a String> for Selector<'a> {
    type Error = zenoh_result::Error;
    fn try_from(s: &'a String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl<'a> From<&'a Query> for Selector<'a> {
    fn from(q: &'a Query) -> Self {
        Selector {
            key_expr: q.inner.key_expr.clone(),
            parameters: (&q.inner.parameters).into(),
        }
    }
}

impl<'a> From<&KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &KeyExpr<'a>) -> Self {
        Self {
            key_expr: key_selector.clone(),
            parameters: "".into(),
        }
    }
}

impl<'a> From<&'a keyexpr> for Selector<'a> {
    fn from(key_selector: &'a keyexpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            parameters: "".into(),
        }
    }
}

impl<'a> From<&'a OwnedKeyExpr> for Selector<'a> {
    fn from(key_selector: &'a OwnedKeyExpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            parameters: "".into(),
        }
    }
}

impl From<OwnedKeyExpr> for Selector<'static> {
    fn from(key_selector: OwnedKeyExpr) -> Self {
        Self {
            key_expr: key_selector.into(),
            parameters: "".into(),
        }
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self {
            key_expr: key_selector,
            parameters: "".into(),
        }
    }
}
