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
use std::convert::TryFrom;
use regex::Regex;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::zerror;
use crate::net::ResKey;

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub(crate) p: String
}

impl Path {

    fn is_valid(path: &str) -> bool {
        !path.is_empty() &&
        !path.contains(|c| c == '?' || c == '#' || c == '[' || c == ']' || c == '*')
    }

    pub(crate) fn remove_useless_slashes(path: &str) -> String {
        lazy_static! {
            static ref RE: Regex = Regex::new("/+").unwrap();
        }
        let p = RE.replace_all(path, "/");
        if p.ends_with('/') {
            p[..p.len()-1].to_string()
        } else {
            p.to_string()
        }
    }

    pub fn new(p: String) -> ZResult<Path> {
        if !Self::is_valid(&p) {
            zerror!(ZErrorKind::InvalidPath{ path: p })
        } else {
            Ok(Path{p: Self::remove_useless_slashes(&p)})
        }
    }

    pub fn as_str(&self) -> &str {
        self.p.as_str()
    }

    pub fn is_relative(&self) -> bool {
        !self.p.starts_with('/')
    }

    pub fn with_prefix(&self, prefix: &Path) -> Self {
        if self.is_relative() {
            Self { p: format!("{}/{}", prefix.p, self.p) }
        } else {
            Self { p: format!("{}{}", prefix.p, self.p) }
        }
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.p)
    }
}

// impl From<String> for Path {
//     fn from(p: String) -> Path {
//         Path::new(p).unwrap()
//     }
// }

// impl From<&str> for Path {
//     fn from(p: &str) -> Path {
//         Self::from(p.to_string())
//     }
// }

// Doesn't compile because of https://github.com/rust-lang/rust/issues/50133

impl TryFrom<String> for Path {
    type Error = ZError;
    fn try_from(p: String) -> Result<Self, Self::Error> {
        Path::new(p)
    }
}

impl TryFrom<&str> for Path {
    type Error = ZError;
    fn try_from(p: &str) -> ZResult<Path> {
        Self::try_from(p.to_string())
    }
}


impl From<Path> for ResKey {
    fn from(path: Path) -> Self {
        ResKey::from(path.p.as_str())
    }
}

impl From<&Path> for ResKey {
    fn from(path: &Path) -> Self {
        ResKey::from(path.p.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn test_path() {

        assert_eq!(Path::try_from("a/b").unwrap(), 
            Path { p: "a/b".into() });

        assert_eq!(Path::try_from("/a/b").unwrap(), 
            Path { p: "/a/b".into() });

        assert_eq!(Path::try_from("////a///b///").unwrap(), 
            Path { p: "/a/b".into() });

        assert!(Path::try_from("a/b").unwrap().is_relative());
        assert!(!Path::try_from("/a/b").unwrap().is_relative());

        assert_eq!(Path::try_from("c/d").unwrap()
            .with_prefix(&"/a/b".try_into().unwrap()),
            Path { p: "/a/b/c/d".into() });
        assert_eq!(Path::try_from("/c/d").unwrap()
            .with_prefix(&"/a/b".try_into().unwrap()),
            Path { p: "/a/b/c/d".into() });


    }

}