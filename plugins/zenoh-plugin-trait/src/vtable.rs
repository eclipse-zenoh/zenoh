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
use crate::*;
use std::{borrow::Cow, fmt::Display};
use zenoh_keyexpr::keyexpr;
use zenoh_result::ZResult;

pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 1;

type StartFn<StartArgs, Instance> = fn(&str, &StartArgs) -> ZResult<Instance>;

#[repr(C)]
pub struct PluginVTable<StartArgs, Instance> {
    pub start: StartFn<StartArgs, Instance>,
}
impl<StartArgs, Instance> PluginStructVersion for PluginVTable<StartArgs, Instance> {
    fn struct_version() -> u64 {
        1
    }
    fn struct_features() -> &'static str {
        FEATURES
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StructVersion {
    pub version: u64,
    pub name: &'static str,
    pub features: &'static str,
}

impl StructVersion {
    pub fn new<T: PluginStructVersion>() -> Self {
        Self {
            version: T::struct_version(),
            name: std::any::type_name::<T>(),
            features: T::struct_features(),
        }
    }
}

impl Display for StructVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "  version: {}\n  type: {}\n  features: {}\n",
            self.version, self.name, self.features
        )
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Compatibility {
    rust_version: RustVersion,
    vtable_version: StructVersion,
    start_args_version: StructVersion,
    instance_version: StructVersion,
    plugin_version: Cow<'static, keyexpr>,
}

impl Compatibility {
    pub fn new<
        StartArgsType: PluginStartArgs,
        InstanceType: PluginInstance,
        PluginType: Plugin<StartArgs = StartArgsType, Instance = InstanceType>,
    >() -> Self {
        let rust_version = RustVersion::new();
        let vtable_version = StructVersion::new::<PluginVTable<StartArgsType, InstanceType>>();
        let start_args_version = StructVersion::new::<StartArgsType>();
        let instance_version = StructVersion::new::<InstanceType>();
        let plugin_version = Cow::Borrowed(PluginType::PLUGIN_VERSION);
        Self {
            rust_version,
            vtable_version,
            start_args_version,
            instance_version,
            plugin_version,
        }
    }
    pub fn with_plugin_version_keyexpr<
        StartArgsType: PluginStartArgs,
        InstanceType: PluginInstance,
    >(
        plugin_version: Cow<'static, keyexpr>,
    ) -> Self {
        let rust_version = RustVersion::new();
        let vtable_version = StructVersion::new::<PluginVTable<StartArgsType, InstanceType>>();
        let start_args_version = StructVersion::new::<StartArgsType>();
        let instance_version = StructVersion::new::<InstanceType>();
        Self {
            rust_version,
            vtable_version,
            start_args_version,
            instance_version,
            plugin_version,
        }
    }
    pub fn plugin_version(&self) -> Cow<'static, keyexpr> {
        self.plugin_version
    }
    pub fn are_compatible(&self, other: &Self) -> bool {
        RustVersion::are_compatible(&self.rust_version, &other.rust_version)
            && self.vtable_version == other.vtable_version
            && self.start_args_version == other.start_args_version
            && self.instance_version == other.instance_version
            && self
                .plugin_version
                .intersects(other.plugin_version.as_ref())
    }
}

impl Display for Compatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\nVTable:{}StartArgs:{}Instance:{}Plugin:{}",
            self.rust_version,
            self.vtable_version,
            self.start_args_version,
            self.instance_version,
            self.plugin_version
        )
    }
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

impl<StartArgs, Instance> PluginVTable<StartArgs, Instance> {
    pub fn new<ConcretePlugin: Plugin<StartArgs = StartArgs, Instance = Instance>>() -> Self {
        Self {
            start: ConcretePlugin::start,
        }
    }
}

pub use no_mangle::*;
#[cfg(feature = "no_mangle")]
pub mod no_mangle {
    /// This macro will add a non-mangled functions which provides plugin version and loads it if feature `no_mangle` is enabled (which it is by default).
    #[macro_export]
    macro_rules! declare_plugin {
        ($ty: path) => {
            #[no_mangle]
            fn get_plugin_loader_version() -> $crate::PluginLoaderVersion {
                $crate::PLUGIN_LOADER_VERSION
            }

            #[no_mangle]
            fn get_compatibility() -> $crate::Compatibility {
                $crate::Compatibility::new::<
                    <$ty as $crate::Plugin>::StartArgs,
                    <$ty as $crate::Plugin>::Instance,
                >()
            }

            #[no_mangle]
            fn load_plugin() -> $crate::PluginVTable<
                <$ty as $crate::Plugin>::StartArgs,
                <$ty as $crate::Plugin>::Instance,
            > {
                $crate::PluginVTable::new::<$ty>()
            }
        };
    }
}
#[cfg(not(feature = "no_mangle"))]
pub mod no_mangle {
    #[macro_export]
    macro_rules! declare_plugin {
        ($ty: path) => {};
    }
}
