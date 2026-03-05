//
// Copyright (c) 2023 ZettaScale Technology
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

use std::fmt::Display;

use zenoh_result::{bail, ZResult};

pub trait StructVersion {
    /// The version of the structure which implements this trait.
    fn struct_version() -> &'static str;
    /// The features enabled during compilation of the structure implementing this trait.
    /// Different features between the plugin and the host may cause ABI incompatibility even if the structure version is the same.
    /// Use `concat_enabled_features!` to generate this string
    fn struct_features() -> &'static str;
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Compatibility {
    rust_version: RustVersion,
    zenoh_version: stabby::str::Str<'static>,
    zenoh_features: stabby::str::Str<'static>,
}

impl Compatibility {
    pub fn new(zenoh_version: &'static str, zenoh_features: &'static str) -> Self {
        let rust_version = RustVersion::new();
        Self {
            rust_version,
            zenoh_version: zenoh_version.into(),
            zenoh_features: zenoh_features.into(),
        }
    }

    pub fn check(&self, other: &Self) -> ZResult<()> {
        fn get_version_and_commit(version: &str) -> (&str, &str) {
            let parts = version.split('-').collect::<Vec<_>>();
            (
                parts.first().cloned().unwrap_or("undefined"),
                parts.get(1).cloned().unwrap_or("undefined"),
            )
        }

        fn version_equals(left: &str, right: &str) -> bool {
            const RELEASE_COMMIT: &str = "release"; // fallback from `zenoh::GIT_COMMIT`
            let (left_version, left_commit) = get_version_and_commit(left);
            let (right_version, right_commit) = get_version_and_commit(right);
            if left_version != right_version {
                return false;
            }
            // We check equality of git hashes of zenoh crate used by plugin and host.
            // This is mostly done for development purposes, to avoid crashes in case of
            // internal API change during releases.
            // If we receive "release" instead of commit hash, it means that
            // zenoh crate is taken from crates.io (or is built in the environment without git ?).
            // In this case we ignore the commit hash check.
            if left_commit != right_commit
                && left_commit != RELEASE_COMMIT
                && right_commit != RELEASE_COMMIT
            {
                return false;
            }
            true
        }

        if self.rust_version != other.rust_version {
            bail!(
                "Incompatible rustc versions:\n host: {}\n plugin: {}",
                self.rust_version,
                other.rust_version
            )
        } else if !version_equals(&self.zenoh_version, &other.zenoh_version) {
            bail!(
                "Incompatible Zenoh versions:\n host: {}\n plugin: {}",
                self.zenoh_version,
                other.zenoh_version
            )
        } else if self.zenoh_features != other.zenoh_features {
            bail!(
                "Incompatible Zenoh feature sets:\n host: {}\n plugin: {}",
                self.zenoh_features,
                other.zenoh_features
            )
        } else {
            Ok(())
        }
    }
}

impl Display for Compatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Rust version:\n{}", self.rust_version)?;
        writeln!(f, "Zenoh version:\n {}", &self.zenoh_version)?;
        writeln!(f, "Zenoh features:\n {}", &self.zenoh_features)?;
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RustVersion {
    major: u32,
    minor: u32,
    patch: u32,
    stable: bool,
    commit: stabby::str::Str<'static>,
}

impl Display for RustVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Rust {}.{}.{}{} commit {}",
            self.major,
            self.minor,
            self.patch,
            if self.stable { "" } else { "-nightly" },
            self.commit
        )
    }
}

const RELEASE_AND_COMMIT: (&str, &str) = zenoh_macros::rustc_version_release!();
impl RustVersion {
    pub fn new() -> Self {
        let (release, commit) = RELEASE_AND_COMMIT;
        let (release, stable) = if let Some(p) = release.chars().position(|c| c == '-') {
            (&release[..p], false)
        } else {
            (release, true)
        };
        let mut split = release.split('.').map(|s| s.trim());
        RustVersion {
            major: split.next().unwrap().parse().unwrap(),
            minor: split.next().unwrap().parse().unwrap(),
            patch: split.next().unwrap().parse().unwrap(),
            stable,
            commit: commit.into(),
        }
    }
}

impl Default for RustVersion {
    fn default() -> Self {
        Self::new()
    }
}
