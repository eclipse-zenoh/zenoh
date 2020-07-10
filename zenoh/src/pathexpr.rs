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
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::zerror;
use crate::net::ResKey;
use crate::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct PathExpr {
    pub(crate) p: String
}

impl PathExpr {

    fn is_valid(path: &str) -> bool {
        !path.is_empty() &&
        !path.contains(|c| c == '?' || c == '#' || c == '[' || c == ']')
    }

    pub fn new(p: String) -> ZResult<PathExpr> {
        if !Self::is_valid(&p) {
            zerror!(ZErrorKind::InvalidPathExpr{ path: p })
        } else {
            Ok(PathExpr{p: Path::remove_useless_slashes(&p)})
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
