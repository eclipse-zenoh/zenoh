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
// use super::{Path, PathExpr};
use crate::{Properties, Query, ResKey};
use regex::Regex;
use std::borrow::Cow;
use std::fmt;

/// The "starttime" property key for time-range selection
pub const PROP_STARTTIME: &str = "starttime";
/// The "stoptime" property key for time-range selection
pub const PROP_STOPTIME: &str = "stoptime";

#[derive(Clone, Debug, PartialEq)]
/// A zenoh Selector is the conjunction of a [path expression](super::PathExpr) identifying a set
/// of paths and some optional parts allowing to refine the set of paths and associated values.
///
/// Structure of a selector:
/// ```text
/// /s1/s2/.../sn?x>1&y<2&...&z=4(p1=v1;p2=v2;...;pn=vn)[a;b;x;y;...;z]
/// |           | |             | |                   |  |           |
/// |-- expr ---| |--- filter --| |---- properties ---|  |--fragment-|
/// ```
/// where:
///  * __expr__: is a [`PathExpr`].
///  * __filter__: a list of predicates separated by `'&'` allowing to perform filtering on the values
///    associated with the matching keys. Each predicate has the form "`field`-`operator`-`value`" value where:
///      * _field_ is the name of a field in the value (is applicable and is existing. otherwise the predicate is false)
///      * _operator_ is one of a comparison operators: `<` , `>` , `<=` , `>=` , `=` , `!=`
///      * _value_ is the the value to compare the field’s value with
///  * __fragment__: a list of fields names allowing to return a sub-part of each value.
///    This feature only applies to structured values using a “self-describing” encoding, such as JSON or XML.
///    It allows to select only some fields within the structure. A new structure with only the selected fields
///    will be used in place of the original value.
///
/// _**NOTE**_: _the filters and fragments are not yet supported in current zenoh version._
pub struct Selector<'a> {
    /// the path expression part of this Selector (before `?` character).
    key: Cow<'a, ResKey>,
    /// the predicate part of this Selector, as used in zenoh-net.
    /// I.e. all characters starting from `?`.
    predicate: &'a str,
}

impl<'a> Selector<'a> {
    /// Creates a new Selector from a String, checking its validity.
    /// Returns `Err(`[`ZError`]`)` if not valid.
    #[inline(always)]
    pub(crate) fn new(key: &'a ResKey, predicate: &'a str) -> Self {
        Selector {
            key: Cow::Borrowed(key),
            predicate,
        }
    }

    #[inline(always)]
    pub fn key(&'a self) -> &'a ResKey {
        &self.key
    }

    #[inline(always)]
    pub fn predicate(&'a self) -> &'a str {
        self.predicate
    }

    #[inline(always)]
    pub fn with_predicate(mut self, predicate: &'a str) -> Self {
        self.predicate = predicate;
        self
    }

    /// Returns true if the Selector specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        const REGEX_PROJECTION: &str = r"[^\[\]\(\)\[\]]+";
        const REGEX_PROPERTIES: &str = ".*";
        const REGEX_FRAGMENT: &str = ".*";

        lazy_static! {
            static ref RE: Regex = Regex::new(&format!(
                "(?:\\?(?P<proj>{})?(?:\\((?P<prop>{})\\))?)?(?:\\[(?P<frag>{})\\])?",
                REGEX_PROJECTION, REGEX_PROPERTIES, REGEX_FRAGMENT
            ))
            .unwrap();
        }

        if let Some(caps) = RE.captures(&self.predicate) {
            let props: Properties = caps
                .name("prop")
                .map(|s| s.as_str().into())
                .unwrap_or_default();
            props.contains_key(PROP_STARTTIME) || props.contains_key(PROP_STOPTIME)
        } else {
            false
        }
    }
}

impl fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.key, self.predicate)
    }
}

impl<'a> From<&'a str> for Selector<'a> {
    fn from(s: &'a str) -> Self {
        let (key, predicate) = if let Some(i) = s.find(|c| c == '?') {
            s.split_at(i)
        } else {
            (s, "")
        };
        Selector {
            key: Cow::Owned(key.into()),
            predicate,
        }
    }
}

impl<'a> From<&'a String> for Selector<'a> {
    fn from(s: &'a String) -> Self {
        Self::from(s.as_str())
    }
}

impl<'a> From<&'a Query> for Selector<'a> {
    fn from(q: &'a Query) -> Self {
        Selector {
            key: Cow::Owned(q.res_name.as_str().into()),
            predicate: &q.predicate,
        }
    }
}

impl<'a> From<&'a ResKey> for Selector<'a> {
    fn from(from: &'a ResKey) -> Self {
        Self::new(from, "")
    }
}

impl<'a> From<ResKey> for Selector<'a> {
    fn from(from: ResKey) -> Self {
        Self {
            key: Cow::Owned(from),
            predicate: "",
        }
    }
}
