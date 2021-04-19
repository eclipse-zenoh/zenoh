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
use crate::net::Query;
use crate::{Path, PathExpr, Properties};
use regex::Regex;
use std::convert::TryFrom;
use std::fmt;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

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
pub struct Selector {
    /// the path expression part of this Selector (before `?` character).
    pub path_expr: PathExpr,
    /// the predicate part of this Selector, as used in zenoh-net.
    /// I.e. all characters starting from `?`.
    pub predicate: String,
    /// the filter part of this Selector, if any (all characters after `?` and before `(` or `[`)
    pub filter: Option<String>,
    /// the properties part of this Selector (all characters between ``( )`` and after `?`)
    pub properties: Properties,
    /// the fragment part of this Selector, if any (all characters between ``[ ]`` and after `?`)
    pub fragment: Option<String>,
}

impl Selector {
    /// Creates a new Selector from a String, checking its validity.
    /// Returns `Err(`[`ZError`]`)` if not valid.
    pub(crate) fn new(res_name: &str, predicate: &str) -> ZResult<Selector> {
        let path_expr: PathExpr = PathExpr::try_from(res_name)?;

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

        if let Some(caps) = RE.captures(predicate) {
            Ok(Selector {
                path_expr,
                predicate: predicate.to_string(),
                filter: caps.name("proj").map(|s| s.as_str().to_string()),
                properties: caps
                    .name("prop")
                    .map(|s| s.as_str().into())
                    .unwrap_or_default(),
                fragment: caps.name("frag").map(|s| s.as_str().to_string()),
            })
        } else {
            zerror!(ZErrorKind::InvalidSelector {
                selector: format!("{}{}", res_name, predicate)
            })
        }
    }

    /// Returns the concatenation of `prefix` with this Selector.
    pub fn with_prefix(&self, prefix: &Path) -> Selector {
        Selector {
            path_expr: self.path_expr.with_prefix(prefix),
            predicate: self.predicate.clone(),
            filter: self.filter.clone(),
            properties: self.properties.clone(),
            fragment: self.fragment.clone(),
        }
    }

    /// If this Selector starts with `prefix` returns a copy of this Selector with the prefix removed.  
    /// Otherwise, returns `None`.
    pub fn strip_prefix(&self, prefix: &Path) -> Option<Self> {
        self.path_expr
            .strip_prefix(prefix)
            .map(|path_expr| Selector {
                path_expr,
                predicate: self.predicate.clone(),
                filter: self.filter.clone(),
                properties: self.properties.clone(),
                fragment: self.fragment.clone(),
            })
    }

    /// Returns true is this Selector is relative (i.e. not starting with `'/'`).
    pub fn is_relative(&self) -> bool {
        self.path_expr.is_relative()
    }

    /// Returns true if `path` matches this Selector's path expression.
    pub fn matches(&self, path: &Path) -> bool {
        self.path_expr.matches(path)
    }

    /// Returns true if the Selector specifies a time-range in its properties
    /// (i.e. using `"starttime"` or `"stoptime"`)
    pub fn has_time_range(&self) -> bool {
        self.properties.contains_key(PROP_STARTTIME) || self.properties.contains_key(PROP_STOPTIME)
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.path_expr, self.predicate)
    }
}

impl TryFrom<&str> for Selector {
    type Error = ZError;
    fn try_from(s: &str) -> ZResult<Selector> {
        let (path_expr, predicate) = if let Some(i) = s.find(|c| c == '?') {
            s.split_at(i)
        } else {
            (s, "")
        };
        Self::new(path_expr, predicate)
    }
}

impl TryFrom<String> for Selector {
    type Error = ZError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from(s.as_str())
    }
}

impl TryFrom<&Query> for Selector {
    type Error = ZError;
    fn try_from(q: &Query) -> Result<Self, Self::Error> {
        Self::new(&q.res_name, &q.predicate)
    }
}

/// Creates a [`Selector`] from a string.
///
/// # Panics
/// Panics if the string is not a valid [`Selector`].
pub fn selector(selector: impl AsRef<str>) -> Selector {
    Selector::try_from(selector.as_ref()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn test_selector() {
        assert_eq!(
            Selector::try_from("/path/**").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "".into(),
                filter: None,
                properties: Properties::default(),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj".into(),
                filter: Some("proj".into()),
                properties: Properties::default(),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?(prop)").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?(prop)".into(),
                filter: None,
                properties: Properties::from(&[("prop", "")][..]),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?[frag]").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?[frag]".into(),
                filter: None,
                properties: Properties::default(),
                fragment: Some("frag".into()),
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj(prop)").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj(prop)".into(),
                filter: Some("proj".into()),
                properties: Properties::from(&[("prop", "")][..]),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj[frag]").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj[frag]".into(),
                filter: Some("proj".into()),
                properties: Properties::default(),
                fragment: Some("frag".into()),
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?(prop)[frag]").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?(prop)[frag]".into(),
                filter: None,
                properties: Properties::from(&[("prop", "")][..]),
                fragment: Some("frag".into()),
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj(prop)[frag]").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj(prop)[frag]".into(),
                filter: Some("proj".into()),
                properties: Properties::from(&[("prop", "")][..]),
                fragment: Some("frag".into()),
            }
        );
    }
}
