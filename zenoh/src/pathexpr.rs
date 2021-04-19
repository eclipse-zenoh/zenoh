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
use crate::net::utils::resource_name;
use crate::net::ResKey;
use crate::Path;
use std::convert::{From, TryFrom};
use std::{fmt, ops::Div};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

/// A zenoh Path Expression used to express a selection of [`Path`]s.
///
/// Similarly to a [`Path`], a Path is a set of strings separated by '/' , as in a filesystem path.
/// But unlike a [`Path`] it can contain `'*'` characters:
///  - a single `'*'` character matches any set of characters in a path, except `'/'`.
///  - while `"**"` matches any set of characters in a path, including `'/'`.
///
/// A Path Expression can be absolute (i.e. starting with a `'/'`) or relative to a [`Workspace`](super::Workspace).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathExpr {
    pub(crate) p: String,
}

impl PathExpr {
    fn is_valid(path: &str) -> bool {
        !path.is_empty() && !path.contains(|c| c == '?' || c == '#' || c == '[' || c == ']')
    }

    /// Creates a new PathExpr from a String, checking its validity.  
    /// Returns `Err(`[`ZError`]`)` if not valid.
    pub fn new(p: String) -> ZResult<PathExpr> {
        if !Self::is_valid(&p) {
            zerror!(ZErrorKind::InvalidPathExpr { path: p })
        } else {
            Ok(PathExpr {
                p: Path::remove_useless_slashes(&p),
            })
        }
    }

    /// Returns the Path as a &str.
    pub fn as_str(&self) -> &str {
        self.p.as_str()
    }

    /// Returns true is this PathExpr is relative (i.e. not starting with `'/'`).
    pub fn is_relative(&self) -> bool {
        !self.p.starts_with('/')
    }

    /// Returns true is this PathExpr is a [`Path`] (i.e. not containing any `'*'`).
    pub fn is_a_path(&self) -> bool {
        !self.p.contains('*')
    }

    /// Returns the concatenation of `prefix` with this PathExpr.
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

    /// If this PathExpr starts with `prefix` returns a copy of this PathExpr with the prefix removed.  
    /// Otherwise, returns `None`.
    pub fn strip_prefix(&self, prefix: &Path) -> Option<Self> {
        self.p
            .strip_prefix(&prefix.p)
            .map(|p| PathExpr { p: p.to_string() })
    }

    /// Returns true if `path` matches this PathExpr.
    pub fn matches(&self, path: &Path) -> bool {
        resource_name::intersect(&self.p, &path.p)
    }
}

impl Div<String> for PathExpr {
    type Output = PathExpr;

    fn div(self, rhs: String) -> Self::Output {
        &self / rhs
    }
}

impl Div<&str> for PathExpr {
    type Output = PathExpr;

    fn div(self, rhs: &str) -> Self::Output {
        &self / rhs
    }
}

impl Div<String> for &PathExpr {
    type Output = PathExpr;

    fn div(self, rhs: String) -> Self::Output {
        PathExpr::try_from(format!("{}/{}", self, rhs)).unwrap()
    }
}

impl Div<&str> for &PathExpr {
    type Output = PathExpr;

    fn div(self, rhs: &str) -> Self::Output {
        PathExpr::try_from(format!("{}/{}", self, rhs)).unwrap()
    }
}

impl fmt::Display for PathExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.p)
    }
}

impl TryFrom<String> for PathExpr {
    type Error = ZError;
    fn try_from(p: String) -> Result<Self, Self::Error> {
        PathExpr::new(p)
    }
}

impl TryFrom<&str> for PathExpr {
    type Error = ZError;
    fn try_from(p: &str) -> ZResult<PathExpr> {
        Self::try_from(p.to_string())
    }
}

impl From<&Path> for PathExpr {
    fn from(path: &Path) -> Self {
        // No need to check validity as PathExpr is valid
        PathExpr { p: path.p.clone() }
    }
}

impl From<Path> for PathExpr {
    fn from(path: Path) -> Self {
        // No need to check validity as PathExpr is valid
        PathExpr { p: path.p }
    }
}

impl From<PathExpr> for ResKey {
    fn from(path: PathExpr) -> Self {
        ResKey::from(path.p.as_str())
    }
}

impl From<&PathExpr> for ResKey {
    fn from(path: &PathExpr) -> Self {
        ResKey::from(path.p.as_str())
    }
}

/// Creates a [`PathExpr`] from a string.
///
/// # Panics
/// Panics if the string contains forbidden characters `'?'`, `'#'`, `'['`, `']'`.
pub fn pathexpr(path: impl AsRef<str>) -> PathExpr {
    PathExpr::try_from(path.as_ref()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathexpr_div() {
        assert_eq!(
            PathExpr::try_from("a").unwrap() / "b",
            PathExpr::try_from("a/b").unwrap()
        );
        assert_eq!(
            PathExpr::try_from("a").unwrap() / String::from("b"),
            PathExpr::try_from("a/b").unwrap()
        );
    }
}
