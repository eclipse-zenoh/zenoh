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
use std::{borrow::Cow, convert::TryFrom, str::FromStr};

use zenoh_protocol::core::{
    key_expr::{keyexpr, OwnedKeyExpr},
    Parameters,
};
#[cfg(feature = "unstable")]
use ::{zenoh_result::ZResult, zenoh_util::time_range::TimeRange};

use super::{key_expr::KeyExpr, queryable::Query};

/// A selector is the combination of a [Key Expression](crate::key_expr::KeyExpr), which defines the
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
/// - **`[unstable]`** `_time`: used to express interest in only values dated within a certain time range, values for
///   this parameter must be readable by the [Zenoh Time DSL](zenoh_util::time_range::TimeRange) for the value to be considered valid.
/// - **`[unstable]`** `_anyke`: used in queries to express interest in replies coming from any key expression. By default, only replies
///   whose key expression match query's key expression are accepted. `_anyke` disables the query-reply key expression matching check.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    pub key_expr: Cow<'a, KeyExpr<'a>>,
    /// the part of this selector identifying which values should be part of the selection.
    pub parameters: Cow<'a, Parameters<'a>>,
}

impl<'a> Selector<'a> {
    /// Builds a new selector which owns keyexpr and parameters
    pub fn owned<K, P>(key_expr: K, parameters: P) -> Self
    where
        K: Into<KeyExpr<'a>>,
        P: Into<Parameters<'a>>,
    {
        Self {
            key_expr: Cow::Owned(key_expr.into()),
            parameters: Cow::Owned(parameters.into()),
        }
    }
    /// Build a new selector holding references to keyexpr and parameters
    /// Useful for printing pairs of keyexpr and parameters in url-like format
    pub fn borrowed(key_expr: &'a KeyExpr<'a>, parameters: &'a Parameters<'a>) -> Self {
        Self {
            key_expr: Cow::Borrowed(key_expr),
            parameters: Cow::Borrowed(parameters),
        }
    }

    /// Convert this selector into an owned one.
    pub fn into_owned(self) -> Selector<'static> {
        Selector::owned(
            self.key_expr.into_owned().into_owned(),
            self.parameters.into_owned().into_owned(),
        )
    }
}

impl<'a, K, P> From<(K, P)> for Selector<'a>
where
    K: Into<KeyExpr<'a>>,
    P: Into<Parameters<'a>>,
{
    fn from((key_expr, parameters): (K, P)) -> Self {
        Self::owned(key_expr, parameters)
    }
}

impl<'a> From<Selector<'a>> for (KeyExpr<'a>, Parameters<'a>) {
    fn from(selector: Selector<'a>) -> Self {
        (
            selector.key_expr.into_owned(),
            selector.parameters.into_owned(),
        )
    }
}

impl<'a> From<&'a Selector<'a>> for (&'a KeyExpr<'a>, &'a Parameters<'a>) {
    fn from(selector: &'a Selector<'a>) -> Self {
        (selector.key_expr.as_ref(), selector.parameters.as_ref())
    }
}

#[zenoh_macros::unstable]
/// The trait allows to set/read parameters processed by the zenoh library itself
pub trait ZenohParameters {
    /// Text parameter names are not part of the public API. They exposed just to provide information about current parameters
    /// namings, allowing user to avoid conflicts with custom parameters. It's also possible that some of these zenoh-specific parameters
    /// which now are stored in the key-value pairs will be later passed in some other way, keeping the same get/set interface functions.
    const REPLY_KEY_EXPR_ANY_SEL_PARAM: &'static str = "_anyke";
    const TIME_RANGE_KEY: &'static str = "_time";
    /// Sets the time range targeted by the selector parameters.
    fn set_time_range<T: Into<Option<TimeRange>>>(&mut self, time_range: T);
    /// Sets the parameter allowing to receive replies from queryables not matching
    /// the requested key expression. This may happen in this scenario:
    /// - we are requesting keyexpr `a/b`.
    /// - queryable is declared to handle `a/*` queries and contains data for `a/b` and `a/c`.
    /// - queryable receives our request and sends two replies with data for `a/b` and `a/c`
    ///
    /// Normally only `a/b` reply would be accepted, but with `_anyke` parameter set, both replies are accepted.
    /// NOTE: `_anyke` indicates that ANY key expression is allowed. I.e., if `_anyke` parameter is set, a reply
    ///       on `x/y/z` is valid even if the queryable is declared on `a/*`.
    fn set_reply_key_expr_any(&mut self);
    /// Extracts the standardized `_time` argument from the selector parameters.
    /// Returns `None` if the `_time` argument is not present or `Some` with the result of parsing the `_time` argument
    /// if it is present.
    fn time_range(&self) -> Option<ZResult<TimeRange>>;
    /// Returns true if `_anyke` parameter is present in the selector parameters
    fn reply_key_expr_any(&self) -> bool;
}

#[cfg(feature = "unstable")]
impl ZenohParameters for Parameters<'_> {
    /// Sets the time range targeted by the selector parameters.
    fn set_time_range<T: Into<Option<TimeRange>>>(&mut self, time_range: T) {
        let mut time_range: Option<TimeRange> = time_range.into();
        match time_range.take() {
            Some(tr) => self.insert(Self::TIME_RANGE_KEY, format!("{}", tr)),
            None => self.remove(Self::TIME_RANGE_KEY),
        };
    }

    /// Sets parameter allowing to querier to reply to this request even
    /// it the requested key expression does not match the reply key expression.
    fn set_reply_key_expr_any(&mut self) {
        self.insert(Self::REPLY_KEY_EXPR_ANY_SEL_PARAM, "");
    }

    /// Extracts the standardized `_time` argument from the selector parameters.
    ///
    /// The default implementation still causes a complete pass through the selector parameters to ensure that there are no duplicates of the `_time` key.
    fn time_range(&self) -> Option<ZResult<TimeRange>> {
        self.get(Self::TIME_RANGE_KEY)
            .map(|tr| tr.parse().map_err(Into::into))
    }

    /// Returns true if `_anyke` parameter is present in the selector parameters
    fn reply_key_expr_any(&self) -> bool {
        self.contains_key(Self::REPLY_KEY_EXPR_ANY_SEL_PARAM)
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
                Ok(Selector::owned(KeyExpr::try_from(s)?, parameters))
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
                Ok(Selector::owned(
                    KeyExpr::try_from(&s[..qmark_position])?,
                    params,
                ))
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
        Self {
            key_expr: Cow::Borrowed(&q.inner.key_expr),
            parameters: Cow::Borrowed(&q.inner.parameters),
        }
    }
}

impl<'a> From<&'a KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: &'a KeyExpr<'a>) -> Self {
        Self {
            key_expr: Cow::Borrowed(key_selector),
            parameters: Cow::Owned("".into()),
        }
    }
}

impl<'a> From<&'a keyexpr> for Selector<'a> {
    fn from(key_selector: &'a keyexpr) -> Self {
        Self {
            key_expr: Cow::Owned(key_selector.into()),
            parameters: Cow::Owned("".into()),
        }
    }
}

impl<'a> From<&'a OwnedKeyExpr> for Selector<'a> {
    fn from(key_selector: &'a OwnedKeyExpr) -> Self {
        Self {
            key_expr: Cow::Owned(key_selector.into()),
            parameters: Cow::Owned("".into()),
        }
    }
}

impl From<OwnedKeyExpr> for Selector<'static> {
    fn from(key_selector: OwnedKeyExpr) -> Self {
        Self {
            key_expr: Cow::Owned(key_selector.into()),
            parameters: Cow::Owned("".into()),
        }
    }
}

impl<'a> From<KeyExpr<'a>> for Selector<'a> {
    fn from(key_selector: KeyExpr<'a>) -> Self {
        Self {
            key_expr: Cow::Owned(key_selector),
            parameters: Cow::Owned("".into()),
        }
    }
}

#[cfg(feature = "unstable")]
#[test]
fn selector_accessors() {
    use std::collections::HashMap;

    for s in [
        "hello/there?_timetrick",
        "hello/there?_timetrick;_time",
        "hello/there?_timetrick;_time;_filter",
        "hello/there?_timetrick;_time=[..]",
        "hello/there?_timetrick;_time=[..];_filter",
    ] {
        let Selector {
            key_expr,
            parameters,
        } = s.try_into().unwrap();
        assert_eq!(key_expr.as_str(), "hello/there");
        let mut parameters = parameters.into_owned();

        println!("Parameters start: {}", parameters);
        for i in parameters.iter() {
            println!("\t{:?}", i);
        }

        assert_eq!(parameters.get("_timetrick").unwrap(), "");

        const TIME_RANGE_KEY: &str = Parameters::TIME_RANGE_KEY;
        const ANYKE: &str = Parameters::REPLY_KEY_EXPR_ANY_SEL_PARAM;

        let time_range = "[now(-2s)..now(2s)]";
        zcondfeat!(
            "unstable",
            {
                let time_range = time_range.parse().unwrap();
                parameters.set_time_range(time_range);
                assert_eq!(parameters.time_range().unwrap().unwrap(), time_range);
            },
            {
                parameters.insert(TIME_RANGE_KEY, time_range);
            }
        );
        assert_eq!(parameters.get(TIME_RANGE_KEY).unwrap(), time_range);

        let hm: HashMap<&str, &str> = HashMap::from(&parameters);
        assert!(hm.contains_key(TIME_RANGE_KEY));

        parameters.insert("_filter", "");
        assert_eq!(parameters.get("_filter").unwrap(), "");

        let hm: HashMap<String, String> = HashMap::from(&parameters);
        assert!(hm.contains_key(TIME_RANGE_KEY));

        parameters.extend_from_iter(hm.iter());
        assert_eq!(parameters.get("_filter").unwrap(), "");

        parameters.insert(ANYKE, "");

        println!("Parameters end: {}", parameters);
        for i in parameters.iter() {
            println!("\t{:?}", i);
        }

        assert_eq!(
            HashMap::<String, String>::from(&parameters),
            HashMap::<String, String>::from(Parameters::from(
                "_anyke;_filter;_time=[now(-2s)..now(2s)];_timetrick"
            ))
        );
    }
}
