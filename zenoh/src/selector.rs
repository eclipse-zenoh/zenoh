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
use crate::{Properties, Query, ResKey, ZErrorKind};
use regex::Regex;
use std::convert::TryFrom;
use std::fmt;
use zenoh_util::core::ZError;

/// The "starttime" property key for time-range selection
pub const PROP_STARTTIME: &str = "starttime";
/// The "stoptime" property key for time-range selection
pub const PROP_STOPTIME: &str = "stoptime";

#[derive(Clone, Debug, PartialEq)]
/// A zenoh Selector is the conjunction of an expression identifying a set
/// of resources and some optional parts allowing to refine the set of resources.
///
/// Structure of a selector:
/// ```text
/// /s1/s2/.../sn?x>1&y<2&...&z=4(p1=v1;p2=v2;...;pn=vn)[a;b;x;y;...;z]
/// |           | |             | |                   |  |           |
/// |--- expr --| |--- filter --| |---- properties ---|  |--fragment-|
/// ```
/// where:
///  * __expr__: an expression identifying a set of Resources.
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
    /// the expression part of this `Selector` (before `?` character).
    pub res_name: &'a str,
    /// the predicate part of this `Selector`, as used in zenoh.
    /// I.e. all characters starting from `?`.
    pub predicate: &'a str,
}

impl<'a> Selector<'a> {
    /// Sets the predicate part of this `Selector`.
    #[inline(always)]
    pub fn with_predicate(mut self, predicate: &'a str) -> Self {
        self.predicate = predicate;
        self
    }

    /// Parses the predicate part of this `Selector`.
    pub fn parse_predicate(&self) -> Result<Predicate<'a>, ZError> {
        Predicate::try_from(self.predicate)
    }

    /// Returns true if the `Selector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        match Predicate::try_from(self.predicate) {
            Ok(predicate) => predicate.has_time_range(),
            _ => false,
        }
    }
}

impl fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.res_name, self.predicate)
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a str> for Selector<'a> {
    fn from(s: &'a str) -> Self {
        let (res_name, predicate) = if let Some(i) = s.find(|c| c == '?') {
            s.split_at(i)
        } else {
            (s, "")
        };
        Selector {
            res_name,
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
            res_name: &q.res_name,
            predicate: &q.predicate,
        }
    }
}

/// A zenoh `KeySelector` is the conjunction of a [`ResKey`](crate::ResKey) identifying a set
/// of resources and some optional parts allowing to refine the set of resources.
///
/// Structure of a selector:
/// ```text
/// /s1/s2/.../sn?x>1&y<2&...&z=4(p1=v1;p2=v2;...;pn=vn)[a;b;x;y;...;z]
/// |           | |             | |                   |  |           |
/// |--res_key--| |--- filter --| |---- properties ---|  |--fragment-|
/// ```
/// where:
///  * __res_key__: a identifying a set of Resources.
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
#[derive(Clone, Debug, PartialEq)]
pub struct KeySelector<'a> {
    /// the [ResKey](crate::ResKey) of this `KeySelector`.
    pub key: ResKey<'a>,
    /// the predicate part of this `KeySelector`, as used in zenoh.
    /// I.e. all characters starting from `?`.
    pub predicate: &'a str,
}

impl<'a> KeySelector<'a> {
    /// Creates a new `KeySelector` from a String.
    #[inline(always)]
    pub(crate) fn new(key: ResKey<'a>, predicate: &'a str) -> Self {
        KeySelector { key, predicate }
    }

    /// Sets the predicate part of this `KeySelector`.
    #[inline(always)]
    pub fn with_predicate(mut self, predicate: &'a str) -> Self {
        self.predicate = predicate;
        self
    }

    /// Parses the predicate part of this `KeySelector`.
    pub fn parse_predicate(&self) -> Result<Predicate<'a>, ZError> {
        Predicate::try_from(self.predicate)
    }

    /// Returns true if the `KeySelector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        match Predicate::try_from(self.predicate) {
            Ok(predicate) => predicate.has_time_range(),
            _ => false,
        }
    }
}

impl fmt::Display for KeySelector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.key, self.predicate)
    }
}

impl<'a> From<&KeySelector<'a>> for KeySelector<'a> {
    fn from(s: &KeySelector<'a>) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a str> for KeySelector<'a> {
    fn from(s: &'a str) -> Self {
        let (key, predicate) = if let Some(i) = s.find(|c| c == '?') {
            s.split_at(i)
        } else {
            (s, "")
        };
        KeySelector {
            key: key.into(),
            predicate,
        }
    }
}

impl<'a> From<&'a String> for KeySelector<'a> {
    fn from(s: &'a String) -> Self {
        Self::from(s.as_str())
    }
}

impl<'a> From<&'a Query> for KeySelector<'a> {
    fn from(q: &'a Query) -> Self {
        KeySelector {
            key: q.res_name.as_str().into(),
            predicate: &q.predicate,
        }
    }
}

impl<'a> From<&ResKey<'a>> for KeySelector<'a> {
    fn from(from: &ResKey<'a>) -> Self {
        Self::new(from.clone(), "")
    }
}

impl<'a> From<ResKey<'a>> for KeySelector<'a> {
    fn from(key: ResKey<'a>) -> Self {
        Self { key, predicate: "" }
    }
}

impl<'a> From<&Selector<'a>> for KeySelector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        Self {
            key: s.res_name.into(),
            predicate: s.predicate,
        }
    }
}

impl<'a> From<Selector<'a>> for KeySelector<'a> {
    fn from(s: Selector<'a>) -> Self {
        Self::from(&s)
    }
}

/// A struct that can be used to help decoding or encoding the predicate part of a [`Query`](crate::Query).
///
/// # Examples
/// ```
/// use std::convert::TryInto;
/// use zenoh::*;
///
/// let predicate: Predicate = "?x>1&y<2&z=4(p1=v1;p2=v2;pn=vn)[a;b;x;y;z]".try_into().unwrap();
/// assert_eq!(predicate.filter, "x>1&y<2&z=4");
/// assert_eq!(predicate.properties.get("p2").unwrap().as_str(), "v2");
/// assert_eq!(predicate.fragment, "a;b;x;y;z");
/// ```
///
/// ```no_run
/// # async_std::task::block_on(async {
/// # use zenoh::*;
/// # use futures::prelude::*;
/// # let session = open(config::peer()).await.unwrap();
///
/// use std::convert::TryInto;
///
/// let mut queryable = session.register_queryable("/resource/name").await.unwrap();
/// while let Some(query) = queryable.receiver().next().await {
///     let predicate = query.selector().parse_predicate().unwrap();
///     println!("filter: {}", predicate.filter);
///     println!("properties: {}", predicate.properties);
///     println!("fragment: {}", predicate.fragment);
/// }
/// # })
/// ```
///
/// ```
/// # async_std::task::block_on(async {
/// # use zenoh::*;
/// # use futures::prelude::*;
/// # let session = open(config::peer()).await.unwrap();
/// # let mut properties = Properties::default();
///
/// let predicate = Predicate::empty()
///     .with_filter("x>1&y<2")
///     .with_properties(properties)
///     .with_fragment("x;y");
///
/// let mut replies = session.get(
///     &Selector::from("/resource/name").with_predicate(&predicate.to_string())
/// ).await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct Predicate<'a> {
    /// the filter part of this `Predicate`, if any (all characters after `?` and before `(` or `[`)
    pub filter: &'a str,
    /// the properties part of this `Predicate`) (all characters between ``( )`` and after `?`)
    pub properties: Properties,
    /// the fragment part of this `Predicate`, if any (all characters between ``[ ]`` and after `?`)
    pub fragment: &'a str,
}

impl<'a> Predicate<'a> {
    /// Creates a new `Predicate`.
    pub fn new(filter: &'a str, properties: Properties, fragment: &'a str) -> Self {
        Predicate {
            filter,
            properties,
            fragment,
        }
    }

    /// Creates an empty `Predicate`.
    pub fn empty() -> Self {
        Predicate::new("", Properties::default(), "")
    }

    /// Sets the filter part of this `Predicate`.
    pub fn with_filter(mut self, filter: &'a str) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the properties part of this `Predicate`.
    pub fn with_properties(mut self, properties: Properties) -> Self {
        self.properties = properties;
        self
    }

    /// Sets the fragment part of this `Predicate`.
    pub fn with_fragment(mut self, fragment: &'a str) -> Self {
        self.fragment = fragment;
        self
    }

    /// Returns true if the `Predicate` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        self.properties.contains_key(PROP_STARTTIME) || self.properties.contains_key(PROP_STOPTIME)
    }
}

impl fmt::Display for Predicate<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = "?".to_string();
        s.push_str(self.filter);
        if !self.properties.is_empty() {
            s.push('(');
            s.push_str(self.properties.to_string().as_str());
            s.push(')');
        }
        if !self.fragment.is_empty() {
            s.push('[');
            s.push_str(self.fragment);
            s.push(']');
        }
        write!(f, "{}", s)
    }
}

impl<'a> TryFrom<&'a str> for Predicate<'a> {
    type Error = ZError;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
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

        if let Some(caps) = RE.captures(s) {
            Ok(Predicate {
                filter: caps.name("proj").map(|s| s.as_str()).unwrap_or(""),
                properties: caps
                    .name("prop")
                    .map(|s| s.as_str().into())
                    .unwrap_or_default(),
                fragment: caps.name("frag").map(|s| s.as_str()).unwrap_or(""),
            })
        } else {
            zerror!(ZErrorKind::InvalidSelector {
                selector: s.to_string(),
            })
        }
    }
}

impl<'a> TryFrom<&'a String> for Predicate<'a> {
    type Error = ZError;

    fn try_from(s: &'a String) -> Result<Self, Self::Error> {
        Predicate::try_from(s.as_str())
    }
}
