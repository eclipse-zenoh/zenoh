//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use std::{env, error::Error, fmt::Display, path::PathBuf, str::FromStr};

use serde::{
    de::{value::MapAccessDeserializer, Visitor},
    Deserialize, Serialize,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(default)]
pub struct LibSearchDirs(Vec<LibSearchDir>);

impl LibSearchDirs {
    pub fn from_paths<T: AsRef<str>>(paths: &[T]) -> Self {
        Self(
            paths
                .iter()
                .map(|s| LibSearchDir::Path(s.as_ref().to_string()))
                .collect(),
        )
    }

    pub fn from_specs<T: AsRef<str>>(paths: &[T]) -> Result<Self, serde_json::Error> {
        let dirs = paths
            .iter()
            .map(|s| {
                let de = &mut serde_json::Deserializer::from_str(s.as_ref());
                LibSearchDir::deserialize(de)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self(dirs))
    }
}

#[derive(Debug)]
pub struct InvalidLibSearchDir {
    found: LibSearchDir,
    source: String,
}

impl Display for InvalidLibSearchDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid library search directory `{:?}`: {}",
            self.found, self.source
        )
    }
}

impl Error for InvalidLibSearchDir {}

pub struct IntoIter {
    iter: std::vec::IntoIter<LibSearchDir>,
}

impl Iterator for IntoIter {
    type Item = Result<PathBuf, InvalidLibSearchDir>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(LibSearchDir::into_path)
    }
}

impl IntoIterator for LibSearchDirs {
    type Item = Result<PathBuf, InvalidLibSearchDir>;

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            iter: self.0.into_iter(),
        }
    }
}

impl Default for LibSearchDirs {
    fn default() -> Self {
        LibSearchDirs(vec![
            LibSearchDir::Spec(LibSearchSpec {
                kind: LibSearchSpecKind::CurrentExeParent,
                value: None,
            }),
            LibSearchDir::Path(".".to_string()),
            LibSearchDir::Path("~/.zenoh/lib".to_string()),
            LibSearchDir::Path("/opt/homebrew/lib".to_string()),
            LibSearchDir::Path("/usr/local/lib".to_string()),
            LibSearchDir::Path("/usr/lib".to_string()),
        ])
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LibSearchDir {
    Path(String),
    Spec(LibSearchSpec),
}

impl LibSearchDir {
    fn into_path(self) -> Result<PathBuf, InvalidLibSearchDir> {
        match self {
            LibSearchDir::Path(path) => LibSearchSpec {
                kind: LibSearchSpecKind::Path,
                value: Some(path),
            }
            .into_path(),
            LibSearchDir::Spec(spec) => spec.into_path(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LibSearchSpec {
    kind: LibSearchSpecKind,
    value: Option<String>,
}

impl LibSearchSpec {
    fn into_path(self) -> Result<PathBuf, InvalidLibSearchDir> {
        fn error_from_source<T: Error>(spec: &LibSearchSpec, err: T) -> InvalidLibSearchDir {
            InvalidLibSearchDir {
                found: LibSearchDir::Spec(spec.clone()),
                source: err.to_string(),
            }
        }

        fn error_from_str(spec: &LibSearchSpec, err: &str) -> InvalidLibSearchDir {
            InvalidLibSearchDir {
                found: LibSearchDir::Spec(spec.clone()),
                source: err.to_string(),
            }
        }

        match self.kind {
            LibSearchSpecKind::Path => {
                let Some(value) = &self.value else {
                    return Err(error_from_str(
                        &self,
                        "`path` specs should have a `value` field",
                    ));
                };

                let expanded =
                    shellexpand::full(value).map_err(|err| error_from_source(&self, err))?;

                let path =
                    PathBuf::from_str(&expanded).map_err(|err| error_from_source(&self, err))?;

                Ok(path)
            }
            LibSearchSpecKind::CurrentExeParent => {
                let current_exe =
                    env::current_exe().map_err(|err| error_from_source(&self, err))?;

                let Some(current_exe_parent) = current_exe.parent() else {
                    return Err(error_from_str(
                        &self,
                        "current executable's path has no parent directory",
                    ));
                };

                let canonicalized = current_exe_parent
                    .canonicalize()
                    .map_err(|err| error_from_source(&self, err))?;

                Ok(canonicalized)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LibSearchSpecKind {
    Path,
    CurrentExeParent,
}

impl<'de> Deserialize<'de> for LibSearchDir {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(LibSearchSpecOrPathVisitor)
    }
}

impl Serialize for LibSearchDir {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            LibSearchDir::Path(path) => serializer.serialize_str(path),
            LibSearchDir::Spec(spec) => spec.serialize(serializer),
        }
    }
}

struct LibSearchSpecOrPathVisitor;

impl<'de> Visitor<'de> for LibSearchSpecOrPathVisitor {
    type Value = LibSearchDir;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("str or map with field `kind` and optionally field `value`")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(LibSearchDir::Path(v.to_string()))
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        LibSearchSpec::deserialize(MapAccessDeserializer::new(map)).map(LibSearchDir::Spec)
    }
}
