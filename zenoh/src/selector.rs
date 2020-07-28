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
use crate::{Path, PathExpr, Properties};
use regex::Regex;
use std::convert::TryFrom;
use std::fmt;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub path_expr: PathExpr,
    pub predicate: String,
    pub projection: Option<String>,
    pub properties: Properties,
    pub fragment: Option<String>,
}

impl Selector {
    pub(crate) fn new(res_name: &str, predicate: &str) -> ZResult<Selector> {
        let path_expr: PathExpr = PathExpr::try_from(res_name)?;

        const REGEX_PROJECTION: &str = r"[^\[\]\(\)#]+";
        const REGEX_PROPERTIES: &str = ".*";
        const REGEX_FRAGMENT: &str = ".*";

        lazy_static! {
            static ref RE: Regex = Regex::new(&format!(
                "(?:\\?(?P<proj>{})?(?:\\((?P<prop>{})\\))?)?(?:#(?P<frag>{}))?",
                REGEX_PROJECTION, REGEX_PROPERTIES, REGEX_FRAGMENT
            ))
            .unwrap();
        }

        if let Some(caps) = RE.captures(predicate) {
            Ok(Selector {
                path_expr,
                predicate: predicate.to_string(),
                projection: caps.name("proj").map(|s| s.as_str().to_string()),
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

    pub fn with_prefix(&self, prefix: &Path) -> Selector {
        Selector {
            path_expr: self.path_expr.with_prefix(prefix),
            predicate: self.predicate.clone(),
            projection: self.projection.clone(),
            properties: self.properties.clone(),
            fragment: self.fragment.clone(),
        }
    }

    pub fn strip_prefix(&self, prefix: &Path) -> Option<Self> {
        self.path_expr
            .strip_prefix(prefix)
            .map(|path_expr| Selector {
                path_expr,
                predicate: self.predicate.clone(),
                projection: self.projection.clone(),
                properties: self.properties.clone(),
                fragment: self.fragment.clone(),
            })
    }

    pub fn is_relative(&self) -> bool {
        self.path_expr.is_relative()
    }

    pub fn matches(&self, path: &Path) -> bool {
        self.path_expr.matches(path)
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
        let (path_expr, predicate) = if let Some(i) = s.find(|c| c == '?' || c == '#') {
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
                projection: None,
                properties: Properties::default(),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj".into(),
                projection: Some("proj".into()),
                properties: Properties::default(),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?(prop)").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?(prop)".into(),
                projection: None,
                properties: Properties::from(&[("prop", "")][..]),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**#frag").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "#frag".into(),
                projection: None,
                properties: Properties::default(),
                fragment: Some("frag".into()),
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj(prop)").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj(prop)".into(),
                projection: Some("proj".into()),
                properties: Properties::from(&[("prop", "")][..]),
                fragment: None
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj#frag").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj#frag".into(),
                projection: Some("proj".into()),
                properties: Properties::default(),
                fragment: Some("frag".into()),
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?(prop)#frag").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?(prop)#frag".into(),
                projection: None,
                properties: Properties::from(&[("prop", "")][..]),
                fragment: Some("frag".into()),
            }
        );

        assert_eq!(
            Selector::try_from("/path/**?proj(prop)#frag").unwrap(),
            Selector {
                path_expr: "/path/**".try_into().unwrap(),
                predicate: "?proj(prop)#frag".into(),
                projection: Some("proj".into()),
                properties: Properties::from(&[("prop", "")][..]),
                fragment: Some("frag".into()),
            }
        );
    }
}
