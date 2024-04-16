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

use crate::{prelude::KeyExpr, queryable::Query};
use std::{
    collections::HashMap,
    convert::TryFrom,
    ops::{Deref, DerefMut},
    str::FromStr,
};
use zenoh_protocol::core::{
    key_expr::{keyexpr, OwnedKeyExpr},
    Properties,
};
#[cfg(feature = "unstable")]
use ::{zenoh_result::ZResult, zenoh_util::time_range::TimeRange};

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
    pub(crate) key_expr: KeyExpr<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    pub(crate) parameters: Parameters<'a>,
}

pub const TIME_RANGE_KEY: &str = "_time";
impl<'a> Selector<'a> {
    /// Builds a new selector
    pub fn new<K, P>(key_expr: K, parameters: P) -> Self
    where
        K: Into<KeyExpr<'a>>,
        P: Into<Parameters<'a>>,
    {
        Self {
            key_expr: key_expr.into(),
            parameters: parameters.into(),
        }
    }

    /// Gets the key-expression.
    pub fn key_expr(&'a self) -> &KeyExpr<'a> {
        &self.key_expr
    }

    /// Gets a reference to selector's [`Parameters`].
    pub fn parameters(&self) -> &Parameters<'a> {
        &self.parameters
    }

    /// Gets a mutable reference to selector's [`Parameters`].
    pub fn parameters_mut(&mut self) -> &mut Parameters<'a> {
        &mut self.parameters
    }

    /// Sets the `parameters` part of this `Selector`.
    #[inline(always)]
    pub fn set_parameters<P>(&mut self, parameters: P)
    where
        P: Into<Parameters<'static>>,
    {
        self.parameters = parameters.into();
    }

    /// Create an owned version of this selector with `'static` lifetime.
    pub fn into_owned(self) -> Selector<'static> {
        Selector {
            key_expr: self.key_expr.into_owned(),
            parameters: self.parameters.into_owned(),
        }
    }

    /// Returns this selectors components as a tuple.
    pub fn split(self) -> (KeyExpr<'a>, Parameters<'a>) {
        (self.key_expr, self.parameters)
    }

    #[zenoh_macros::unstable]
    /// Sets the time range targeted by the selector.
    pub fn set_time_range<T: Into<Option<TimeRange>>>(&mut self, time_range: T) {
        self.parameters_mut().set_time_range(time_range);
    }

    #[zenoh_macros::unstable]
    /// Extracts the standardized `_time` argument from the selector parameters.
    ///
    /// The default implementation still causes a complete pass through the selector parameters to ensure that there are no duplicates of the `_time` key.
    pub fn time_range(&self) -> ZResult<Option<TimeRange>> {
        self.parameters().time_range()
    }

    #[cfg(any(feature = "unstable", test))]
    pub(crate) fn accept_any_keyexpr(&self) -> ZResult<Option<bool>> {
        self.parameters().accept_any_keyexpr()
    }
}

/// A wrapper type to help decode zenoh selector parameters.
///
/// Most methods will return an Error if duplicates of a same parameter are found, to avoid HTTP Parameter Pollution like vulnerabilities.
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
pub struct Parameters<'a>(Properties<'a>);

impl<'a> Deref for Parameters<'a> {
    type Target = Properties<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for Parameters<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Display for Parameters<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for Parameters<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl<'a, T> From<T> for Parameters<'a>
where
    T: Into<Properties<'a>>,
{
    fn from(value: T) -> Self {
        Parameters(value.into())
    }
}

impl<'s> From<&'s Parameters<'s>> for HashMap<&'s str, &'s str> {
    fn from(props: &'s Parameters<'s>) -> Self {
        HashMap::from(&props.0)
    }
}

impl From<&Parameters<'_>> for HashMap<String, String> {
    fn from(props: &Parameters) -> Self {
        HashMap::from(&props.0)
    }
}

impl From<Parameters<'_>> for HashMap<String, String> {
    fn from(props: Parameters) -> Self {
        HashMap::from(props.0)
    }
}

impl Parameters<'_> {
    /// Create an owned version of these parameters with `'static` lifetime.
    pub fn into_owned(self) -> Parameters<'static> {
        Parameters(self.0.into_owned())
    }

    #[zenoh_macros::unstable]
    /// Sets the time range targeted by the selector.
    pub fn set_time_range<T: Into<Option<TimeRange>>>(&mut self, time_range: T) {
        let mut time_range: Option<TimeRange> = time_range.into();
        match time_range.take() {
            Some(tr) => self.0.insert(TIME_RANGE_KEY, format!("{}", tr)),
            None => self.0.remove(TIME_RANGE_KEY),
        };
    }

    #[zenoh_macros::unstable]
    /// Extracts the standardized `_time` argument from the selector parameters.
    ///
    /// The default implementation still causes a complete pass through the selector parameters to ensure that there are no duplicates of the `_time` key.
    fn time_range(&self) -> ZResult<Option<TimeRange>> {
        match self.0.get(TIME_RANGE_KEY) {
            Some(tr) => Ok(Some(tr.parse()?)),
            None => Ok(None),
        }
    }

    #[cfg(any(feature = "unstable", test))]
    pub(crate) fn set_accept_any_keyexpr<T: Into<Option<bool>>>(&mut self, anyke: T) {
        use crate::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM as ANYKE;

        let mut anyke: Option<bool> = anyke.into();
        match anyke.take() {
            Some(ak) => {
                if ak {
                    self.0.insert(ANYKE, "")
                } else {
                    self.0.insert(ANYKE, "false")
                }
            }
            None => self.0.remove(ANYKE),
        };
    }

    #[cfg(any(feature = "unstable", test))]
    pub(crate) fn accept_any_keyexpr(&self) -> ZResult<Option<bool>> {
        use crate::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM as ANYKE;

        match self.0.get(ANYKE) {
            Some(ak) => Ok(Some(ak.parse()?)),
            None => Ok(None),
        }
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
            write!(f, "?{}", self.parameters.as_str())?;
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
impl FromStr for Selector<'static> {
    type Err = zenoh_result::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.to_owned().try_into()
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
            parameters: q.inner.parameters.clone(),
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

#[test]
fn selector_accessors() {
    let time_range = "[now(-2s)..now(2s)]".parse().unwrap();
    for selector in [
        "hello/there?_timetrick",
        "hello/there?_timetrick;_time",
        "hello/there?_timetrick;_time;_filter",
        "hello/there?_timetrick;_time=[..]",
        "hello/there?_timetrick;_time=[..];_filter",
    ] {
        let mut selector = Selector::try_from(selector).unwrap();
        println!("Parameters start: {}", selector.parameters());
        for i in selector.parameters().iter() {
            println!("\t{:?}", i);
        }

        assert_eq!(selector.parameters().get("_timetrick").unwrap(), "");

        selector.set_time_range(time_range);
        assert_eq!(selector.time_range().unwrap().unwrap(), time_range);
        assert!(selector.parameters().contains_key(TIME_RANGE_KEY));

        let hm: HashMap<&str, &str> = HashMap::from(selector.parameters());
        assert!(hm.contains_key(TIME_RANGE_KEY));

        selector.parameters_mut().insert("_filter", "");
        assert_eq!(selector.parameters().get("_filter").unwrap(), "");

        let hm: HashMap<String, String> = HashMap::from(selector.parameters());
        assert!(hm.contains_key(TIME_RANGE_KEY));

        selector.parameters_mut().extend_from_iter(hm.iter());
        assert_eq!(selector.parameters().get("_filter").unwrap(), "");

        selector.parameters_mut().set_accept_any_keyexpr(true);

        println!("Parameters end: {}", selector.parameters());
        for i in selector.parameters().iter() {
            println!("\t{:?}", i);
        }

        assert_eq!(
            HashMap::<String, String>::from(selector.parameters()),
            HashMap::<String, String>::from(Parameters::from(
                "_anyke;_filter;_time=[now(-2s)..now(2s)];_timetrick"
            ))
        );
    }
}
