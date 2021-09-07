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
/// An expression identifying a selection of resources.
///
/// A selector is the conjunction of an key selector identifying a set
/// of resource keys and a value selector filtering out the resource values.
///
/// Structure of a selector:
/// ```text
/// /s1/s2/..../sn?x>1&y<2&...&z=4(p1=v1;p2=v2;...;pn=vn)#a;b;x;y;...;z
/// |key_selector||---------------- value_selector -------------------|
///                |--- filter --| |---- properties ---|  |  fragment |
/// ```
/// where:
///  * __key_selector__: an expression identifying a set of Resources.
///  * __filter__: a list of value_selectors separated by `'&'` allowing to perform filtering on the values
///    associated with the matching keys. Each value_selector has the form "`field`-`operator`-`value`" value where:
///      * _field_ is the name of a field in the value (is applicable and is existing. otherwise the value_selector is false)
///      * _operator_ is one of a comparison operators: `<` , `>` , `<=` , `>=` , `=` , `!=`
///      * _value_ is the the value to compare the field’s value with
///  * __fragment__: a list of fields names allowing to return a sub-part of each value.
///    This feature only applies to structured values using a “self-describing” encoding, such as JSON or XML.
///    It allows to select only some fields within the structure. A new structure with only the selected fields
///    will be used in place of the original value.
///
/// _**NOTE**_: _the filters and fragments are not yet supported in current zenoh version._
pub struct Selector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    /// I.e. all characters before `?`.
    pub key_selector: &'a str,
    /// the part of this selector identifying which values should be part of the selection.
    /// I.e. all characters starting from `?`.
    pub value_selector: &'a str,
}

impl<'a> Selector<'a> {
    /// Sets the value_selector part of this `Selector`.
    #[inline(always)]
    pub fn with_value_selector(mut self, value_selector: &'a str) -> Self {
        self.value_selector = value_selector;
        self
    }

    /// Parses the value_selector part of this `Selector`.
    pub fn parse_value_selector(&self) -> Result<ValueSelector<'a>, ZError> {
        ValueSelector::try_from(self.value_selector)
    }

    /// Returns true if the `Selector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        match ValueSelector::try_from(self.value_selector) {
            Ok(value_selector) => value_selector.has_time_range(),
            _ => false,
        }
    }
}

impl fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.key_selector, self.value_selector)
    }
}

impl<'a> From<&Selector<'a>> for Selector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a str> for Selector<'a> {
    fn from(s: &'a str) -> Self {
        let (key_selector, value_selector) = if let Some(i) = s.find(|c| c == '?') {
            s.split_at(i)
        } else {
            (s, "")
        };
        Selector {
            key_selector,
            value_selector,
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
            key_selector: &q.key_selector,
            value_selector: &q.value_selector,
        }
    }
}

/// An expression identifying a selection of resources.
///
/// A KeyedSelector is similar to a [`Selector`] except that it's key_selector
/// is represented as a [`ResKey`] rather than a [`&str`].
#[derive(Clone, Debug, PartialEq)]
pub struct KeyedSelector<'a> {
    /// The part of this selector identifying which keys should be part of the selection.
    /// I.e. all characters before `?`.
    pub key_selector: ResKey<'a>,
    /// the part of this selector identifying which values should be part of the selection.
    /// I.e. all characters starting from `?`.
    pub value_selector: &'a str,
}

impl<'a> KeyedSelector<'a> {
    /// Creates a new `KeyedSelector` from a String.
    #[inline(always)]
    pub(crate) fn new(key_selector: ResKey<'a>, value_selector: &'a str) -> Self {
        KeyedSelector {
            key_selector,
            value_selector,
        }
    }

    /// Sets the value_selector part of this `KeyedSelector`.
    #[inline(always)]
    pub fn with_value_selector(mut self, value_selector: &'a str) -> Self {
        self.value_selector = value_selector;
        self
    }

    /// Parses the value_selector part of this `KeyedSelector`.
    pub fn parse_value_selector(&self) -> Result<ValueSelector<'a>, ZError> {
        ValueSelector::try_from(self.value_selector)
    }

    /// Returns true if the `KeyedSelector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        match ValueSelector::try_from(self.value_selector) {
            Ok(value_selector) => value_selector.has_time_range(),
            _ => false,
        }
    }
}

impl fmt::Display for KeyedSelector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.key_selector, self.value_selector)
    }
}

impl<'a> From<&KeyedSelector<'a>> for KeyedSelector<'a> {
    fn from(s: &KeyedSelector<'a>) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a str> for KeyedSelector<'a> {
    fn from(s: &'a str) -> Self {
        let (key_selector, value_selector) = if let Some(i) = s.find(|c| c == '?') {
            s.split_at(i)
        } else {
            (s, "")
        };
        KeyedSelector {
            key_selector: key_selector.into(),
            value_selector,
        }
    }
}

impl<'a> From<&'a String> for KeyedSelector<'a> {
    fn from(s: &'a String) -> Self {
        Self::from(s.as_str())
    }
}

impl<'a> From<&'a Query> for KeyedSelector<'a> {
    fn from(q: &'a Query) -> Self {
        KeyedSelector {
            key_selector: q.key_selector.as_str().into(),
            value_selector: &q.value_selector,
        }
    }
}

impl<'a> From<&ResKey<'a>> for KeyedSelector<'a> {
    fn from(from: &ResKey<'a>) -> Self {
        Self::new(from.clone(), "")
    }
}

impl<'a> From<ResKey<'a>> for KeyedSelector<'a> {
    fn from(key_selector: ResKey<'a>) -> Self {
        Self {
            key_selector,
            value_selector: "",
        }
    }
}

impl<'a> From<&Selector<'a>> for KeyedSelector<'a> {
    fn from(s: &Selector<'a>) -> Self {
        Self {
            key_selector: s.key_selector.into(),
            value_selector: s.value_selector,
        }
    }
}

impl<'a> From<Selector<'a>> for KeyedSelector<'a> {
    fn from(s: Selector<'a>) -> Self {
        Self::from(&s)
    }
}

/// A struct that can be used to help decoding or encoding the value_selector part of a [`Query`](crate::Query).
///
/// # Examples
/// ```
/// use std::convert::TryInto;
/// use zenoh::*;
///
/// let value_selector: ValueSelector = "?x>1&y<2&z=4(p1=v1;p2=v2;pn=vn)[a;b;x;y;z]".try_into().unwrap();
/// assert_eq!(value_selector.filter, "x>1&y<2&z=4");
/// assert_eq!(value_selector.properties.get("p2").unwrap().as_str(), "v2");
/// assert_eq!(value_selector.fragment, "a;b;x;y;z");
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
///     let value_selector = query.selector().parse_value_selector().unwrap();
///     println!("filter: {}", value_selector.filter);
///     println!("properties: {}", value_selector.properties);
///     println!("fragment: {}", value_selector.fragment);
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
/// let value_selector = ValueSelector::empty()
///     .with_filter("x>1&y<2")
///     .with_properties(properties)
///     .with_fragment("x;y");
///
/// let mut replies = session.get(
///     &Selector::from("/resource/name").with_value_selector(&value_selector.to_string())
/// ).await.unwrap();
/// # })
/// ```
#[derive(Debug, Clone)]
pub struct ValueSelector<'a> {
    /// the filter part of this `ValueSelector`, if any (all characters after `?` and before `(` or `[`)
    pub filter: &'a str,
    /// the properties part of this `ValueSelector`) (all characters between ``( )`` and after `?`)
    pub properties: Properties,
    /// the fragment part of this `ValueSelector`, if any (all characters between ``[ ]`` and after `?`)
    pub fragment: &'a str,
}

impl<'a> ValueSelector<'a> {
    /// Creates a new `ValueSelector`.
    pub fn new(filter: &'a str, properties: Properties, fragment: &'a str) -> Self {
        ValueSelector {
            filter,
            properties,
            fragment,
        }
    }

    /// Creates an empty `ValueSelector`.
    pub fn empty() -> Self {
        ValueSelector::new("", Properties::default(), "")
    }

    /// Sets the filter part of this `ValueSelector`.
    pub fn with_filter(mut self, filter: &'a str) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the properties part of this `ValueSelector`.
    pub fn with_properties(mut self, properties: Properties) -> Self {
        self.properties = properties;
        self
    }

    /// Sets the fragment part of this `ValueSelector`.
    pub fn with_fragment(mut self, fragment: &'a str) -> Self {
        self.fragment = fragment;
        self
    }

    /// Returns true if the `ValueSelector` specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        self.properties.contains_key(PROP_STARTTIME) || self.properties.contains_key(PROP_STOPTIME)
    }
}

impl fmt::Display for ValueSelector<'_> {
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

impl<'a> TryFrom<&'a str> for ValueSelector<'a> {
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
            Ok(ValueSelector {
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

impl<'a> TryFrom<&'a String> for ValueSelector<'a> {
    type Error = ZError;

    fn try_from(s: &'a String) -> Result<Self, Self::Error> {
        ValueSelector::try_from(s.as_str())
    }
}
