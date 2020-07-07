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
use std::fmt;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::zerror;
use crate::{Path, PathExpr};
use regex::Regex;


#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    pub(crate) path_expr: PathExpr,
    pub(crate) predicate: String,
    pub(crate) projection: Option<String>,
    pub(crate) properties: Option<String>,
    pub(crate) fragment: Option<String>
}

impl Selector {

    fn new(selector: &str) -> ZResult<Selector> {
        let (path, predicate) = 
            if let Some(i) = selector.find(|c| c == '?' || c == '#') {
                selector.split_at(i)
            } else {
                (selector, "")
            };

        let path_expr = PathExpr::new(path.to_string())?;

        const REGEX_PROJECTION: &'static str = r"[^\[\]\(\)#]+";
        const REGEX_PROPERTIES: &'static str = ".*";
        const REGEX_FRAGMENT: &'static str = ".*";

        lazy_static! {
            static ref RE: Regex = Regex::new(&format!(
                "(?:\\?(?P<proj>{})?(?:\\((?P<prop>{})\\))?)?(?:#(?P<frag>{}))?",
                REGEX_PROJECTION, REGEX_PROPERTIES, REGEX_FRAGMENT
            )).unwrap();
        }

        if let Some(caps) = RE.captures(predicate) {
            Ok(Selector{
                path_expr,
                predicate: predicate.to_string(),
                projection: caps.name("proj").map(|s| s.as_str().to_string()),
                properties: caps.name("prop").map(|s| s.as_str().to_string()),
                fragment: caps.name("frag").map(|s| s.as_str().to_string())
            })
        } else {
            zerror!(ZErrorKind::InvalidSelector{ selector: selector.to_string() })
        }
    }

    pub fn is_relative(&self) -> bool {
        self.path_expr.is_relative()
    }

    pub fn with_prefix(&self, prefix: &Path) -> Selector {
        Selector{
            path_expr: self.path_expr.with_prefix(prefix),
            predicate: self.predicate.clone(),
            projection: self.projection.clone(),
            properties: self.properties.clone(),
            fragment: self.fragment.clone()
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.path_expr, self.predicate)
    }
}

impl From<String> for Selector {
    fn from(p: String) -> Selector {
        Self::from(p.as_str())
    }
}

impl From<&str> for Selector {
    fn from(p: &str) -> Selector {
        Selector::new(p).unwrap()
    }
}

// Doesn't compile because of https://github.com/rust-lang/rust/issues/50133
//
// impl TryFrom<String> for Selector {
//     type Error = ZError;
//     fn try_from(p: String) -> Result<Self, Self::Error> {
//         Selector::new(p)
//     }
// }
//
// impl TryFrom<&str> for Selector {
//     type Error = ZError;
//     fn try_from(p: &str) -> ZResult<Selector> {
//         Self::try_from(p.to_string())
//     }
// }
//



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector() {
        assert_eq!(Selector::new("/path/**").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "".into(),
                projection: None,
                properties: None,
                fragment: None
            });

        assert_eq!(Selector::new("/path/**?proj").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "?proj".into(),
                projection: Some("proj".into()),
                properties: None,
                fragment: None
            });

        assert_eq!(Selector::new("/path/**?(prop)").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "?(prop)".into(),
                projection: None,
                properties: Some("prop".into()),
                fragment: None
            });

        assert_eq!(Selector::new("/path/**#frag").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "#frag".into(),
                projection: None,
                properties: None,
                fragment: Some("frag".into()),
            });

        assert_eq!(Selector::new("/path/**?proj(prop)").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "?proj(prop)".into(),
                projection: Some("proj".into()),
                properties: Some("prop".into()),
                fragment: None
            });

        assert_eq!(Selector::new("/path/**?proj#frag").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "?proj#frag".into(),
                projection: Some("proj".into()),
                properties: None,
                fragment: Some("frag".into()),
            });

        assert_eq!(Selector::new("/path/**?(prop)#frag").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "?(prop)#frag".into(),
                projection: None,
                properties: Some("prop".into()),
                fragment: Some("frag".into()),
            });

        assert_eq!(Selector::new("/path/**?proj(prop)#frag").unwrap(),
            Selector { 
                path_expr: "/path/**".into(),
                predicate: "?proj(prop)#frag".into(),
                projection: Some("proj".into()),
                properties: Some("prop".into()),
                fragment: Some("frag".into()),
            });

    }
}