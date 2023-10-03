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

//! # The plugin infrastructure for Zenoh.
//!
//! To build a plugin, implement [`Plugin`].
//!
//! If building a plugin for [`zenohd`](https://crates.io/crates/zenoh), you should use the types exported in [`zenoh::plugins`](https://docs.rs/zenoh/latest/zenoh/plugins) to fill [`Plugin`]'s associated types.  
//! To check your plugin typing for `zenohd`, have your plugin implement [`zenoh::plugins::ZenohPlugin`](https://docs.rs/zenoh/latest/zenoh/plugins/struct.ZenohPlugin)
pub mod loading;
pub mod vtable;

use zenoh_result::ZResult;

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StructLayout {
    pub name: &'static str,
    pub size: usize,
    pub alignment: usize,
    pub fields_count: usize,
    pub fields: &'static [StructField],
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StructField {
    pub name: &'static str,
    pub offset: usize,
    pub size: usize,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RustVersion {
    major: u64,
    minor: u64,
    patch: u64,
    stable: bool,
    commit: &'static str,
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
            commit,
        }
    }
    pub fn are_compatible(a: &Self, b: &Self) -> bool {
        if a.stable && b.stable {
            a.major == b.major && a.minor == b.minor && a.patch == b.patch
        } else {
            a == b
        }
    }
}

impl Default for RustVersion {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Compatibility {
    // Rust compiler version. If plugin and host are compiled with the same compiler, whe can hope for ABI compatibility, Though there is still no guarantee.
    pub rust_version: RustVersion,
    // Layouts of the structs used in plugin API. These layout may change depending on features enabled, so even same plugin api version and same compiler version doesn't guarantee
    // same ABI. Also the compiuler is theoretically allowed to produce different layout on different runs, so better to check real layout at runtime.
    pub struct_layouts: &'static [StructLayout],
}

impl Compatibility {
    pub fn new(struct_layouts: &'static [StructLayout]) -> Self {
        Compatibility {
            rust_version: RustVersion::new(),
            struct_layouts,
        }
    }
    pub fn are_compatible(&self, other: &Self) -> bool {
            RustVersion::are_compatible(&self.rust_version, &other.rust_version)
            && self.struct_layouts.len() == other.struct_layouts.len()
            && self
                .struct_layouts
                .iter()
                .zip(other.struct_layouts.iter())
                .all(|(a, b)| a == b)
    }
}

pub mod prelude {
    pub use crate::{loading::*, vtable::*, Plugin, Validator};
}

pub trait PluginVTable {
    fn compatibility() -> Compatibility;
}

pub trait Plugin: Sized + 'static {
    type StartArgs: Validator;
    type RunningPlugin;
    /// Your plugins' default name when statically linked.
    const STATIC_NAME: &'static str;
    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::RunningPlugin>;
}
