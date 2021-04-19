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
use crate::net::ResKey;
use regex::Regex;
use std::convert::TryFrom;
use std::fmt;
use std::ops::Div;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

/// A zenoh Path is a set of strings separated by `'/'` , as in a filesystem path.
///
/// A Path cannot contain any `'*'` character. Examples of paths:
/// `"/demo/example/hello"` , `"/org/eclipse/building/be/floor/1/office/2"` ...
///
/// A path can be absolute (i.e. starting with a `'/'`) or relative to a [`Workspace`](super::Workspace).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path {
    pub(crate) p: String,
}

impl Path {
    fn is_valid(path: &str) -> bool {
        !path.is_empty()
            && !path.contains(|c| c == '?' || c == '#' || c == '[' || c == ']' || c == '*')
    }

    pub(crate) fn remove_useless_slashes(path: &str) -> String {
        lazy_static! {
            static ref RE: Regex = Regex::new("/+").unwrap();
        }
        let p = RE.replace_all(path, "/");
        if p.len() > 1 {
            // remove last '/' if any and return as String
            p.strip_suffix("/").unwrap_or(&p).to_string()
        } else {
            p.to_string()
        }
    }

    /// Creates a new Path from a String, checking its validity.  
    /// Returns `Err(`[`ZError`]`)` if not valid.
    pub fn new(p: &str) -> ZResult<Path> {
        if !Self::is_valid(&p) {
            zerror!(ZErrorKind::InvalidPath {
                path: p.to_string()
            })
        } else {
            Ok(Path {
                p: Self::remove_useless_slashes(&p),
            })
        }
    }

    /// Returns the Path as a &str.
    pub fn as_str(&self) -> &str {
        self.p.as_str()
    }

    /// Returns true is this Path is relative (i.e. not starting with `'/'`).
    pub fn is_relative(&self) -> bool {
        !self.p.starts_with('/')
    }

    /// Returns the last segment of this Path.  
    /// I.e.: the part after the last '/', or the complete Path if there is no '/'.
    pub fn last_segment(&self) -> &str {
        match self.p.rfind('/') {
            Some(i) => &self.p[i + 1..],
            None => self.p.as_str(),
        }
    }

    /// Returns the concatenation of `prefix` with this Path.
    pub fn with_prefix(&self, prefix: &Path) -> Self {
        if self.is_relative() {
            Self {
                p: format!("{}/{}", prefix.p, self.p),
            }
        } else {
            Self {
                p: format!("{}{}", prefix.p, self.p),
            }
        }
    }

    /// If this Path starts with `prefix` returns a copy of this Path with the prefix removed.  
    /// Otherwise, returns `None`.
    pub fn strip_prefix(&self, prefix: &Path) -> Option<Self> {
        self.p
            .strip_prefix(&prefix.p)
            .map(|p| Path { p: p.to_string() })
    }
}

impl Div<String> for Path {
    type Output = Path;

    fn div(self, rhs: String) -> Self::Output {
        &self / rhs
    }
}

impl Div<&str> for Path {
    type Output = Path;

    fn div(self, rhs: &str) -> Self::Output {
        &self / rhs
    }
}

impl Div<String> for &Path {
    type Output = Path;

    fn div(self, rhs: String) -> Self::Output {
        Path::try_from(format!("{}/{}", self, rhs)).unwrap()
    }
}

impl Div<&str> for &Path {
    type Output = Path;

    fn div(self, rhs: &str) -> Self::Output {
        Path::try_from(format!("{}/{}", self, rhs)).unwrap()
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.p)
    }
}

impl TryFrom<String> for Path {
    type Error = ZError;
    fn try_from(p: String) -> Result<Self, Self::Error> {
        Path::new(&p)
    }
}

impl TryFrom<&str> for Path {
    type Error = ZError;
    fn try_from(p: &str) -> ZResult<Path> {
        Path::new(p)
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

/// Creates a [`Path`] from a string.
///
/// # Panics
/// Panics if the string contains forbidden characters `'?'`, `'#'`, `'['`, `']'`, `'*'`.
pub fn path(path: impl AsRef<str>) -> Path {
    Path::try_from(path.as_ref()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn path_div() {
        assert_eq!(Path::try_from("a").unwrap() / "b", Path { p: "a/b".into() });
        assert_eq!(
            Path::try_from("a").unwrap() / String::from("b"),
            Path { p: "a/b".into() }
        );
    }

    #[test]
    fn test_path() {
        assert_eq!(Path::try_from("a/b").unwrap(), Path { p: "a/b".into() });

        assert_eq!(Path::try_from("/a/b").unwrap(), Path { p: "/a/b".into() });

        assert_eq!(
            Path::try_from("////a///b///").unwrap(),
            Path { p: "/a/b".into() }
        );

        assert!(Path::try_from("a/b").unwrap().is_relative());
        assert!(!Path::try_from("/a/b").unwrap().is_relative());

        assert_eq!(
            Path::try_from("c/d")
                .unwrap()
                .with_prefix(&"/a/b".try_into().unwrap()),
            Path {
                p: "/a/b/c/d".into()
            }
        );
        assert_eq!(
            Path::try_from("/c/d")
                .unwrap()
                .with_prefix(&"/a/b".try_into().unwrap()),
            Path {
                p: "/a/b/c/d".into()
            }
        );
    }
}
