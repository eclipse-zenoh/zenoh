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
use std::fmt::Display;
use zenoh_result::ZResult;

pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 1;

type StartFn<StartArgs, Instance> = fn(&str, &StartArgs) -> ZResult<Instance>;

#[repr(C)]
pub struct PluginVTable<StartArgs, Instance> {
    pub plugin_version: &'static str,
    pub plugin_long_version: &'static str,
    pub start: StartFn<StartArgs, Instance>,
}
impl<StartArgs, Instance> PluginStructVersion for PluginVTable<StartArgs, Instance> {
    fn struct_version() -> u64 {
        2
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
    rust_version: Option<RustVersion>,
    vtable_version: Option<StructVersion>,
    start_args_version: Option<StructVersion>,
    instance_version: Option<StructVersion>,
    plugin_version: Option<&'static str>,
    plugin_long_version: Option<&'static str>,
}

impl Compatibility {
    pub fn with_plugin_version<
        StartArgsType: PluginStartArgs,
        InstanceType: PluginInstance,
        PluginType: Plugin<StartArgs = StartArgsType, Instance = InstanceType>,
    >() -> Self {
        let rust_version = Some(RustVersion::new());
        let vtable_version = Some(StructVersion::new::<
            PluginVTable<StartArgsType, InstanceType>,
        >());
        let start_args_version = Some(StructVersion::new::<StartArgsType>());
        let instance_version = Some(StructVersion::new::<InstanceType>());
        let plugin_version = Some(PluginType::PLUGIN_VERSION);
        let plugin_long_version = Some(PluginType::PLUGIN_LONG_VERSION);
        Self {
            rust_version,
            vtable_version,
            start_args_version,
            instance_version,
            plugin_version,
            plugin_long_version,
        }
    }
    pub fn with_empty_plugin_version<
        StartArgsType: PluginStartArgs,
        InstanceType: PluginInstance,
    >() -> Self {
        let rust_version = Some(RustVersion::new());
        let vtable_version = Some(StructVersion::new::<
            PluginVTable<StartArgsType, InstanceType>,
        >());
        let start_args_version = Some(StructVersion::new::<StartArgsType>());
        let instance_version = Some(StructVersion::new::<InstanceType>());
        Self {
            rust_version,
            vtable_version,
            start_args_version,
            instance_version,
            plugin_version: None,
            plugin_long_version: None,
        }
    }
    pub fn plugin_version(&self) -> Option<&'static str> {
        self.plugin_version
    }
    pub fn plugin_long_version(&self) -> Option<&'static str> {
        self.plugin_long_version
    }
    /// Compares fields if both are Some, otherwise skips the comparison.
    /// Returns true if all the comparisons returned true, otherwise false.
    /// If comparison passed or skipped, the corresponding field in both structs is set to None.
    /// If comparison failed, the corresponding field in both structs is kept as is.
    /// This allows not only to check compatibility, but also point to exact reasons of incompatibility.
    pub fn compare(&mut self, other: &mut Self) -> bool {
        let mut result = true;
        Self::compare_field_fn(
            &mut result,
            &mut self.rust_version,
            &mut other.rust_version,
            RustVersion::are_compatible,
        );
        Self::compare_field(
            &mut result,
            &mut self.vtable_version,
            &mut other.vtable_version,
        );
        Self::compare_field(
            &mut result,
            &mut self.start_args_version,
            &mut other.start_args_version,
        );
        Self::compare_field(
            &mut result,
            &mut self.instance_version,
            &mut other.instance_version,
        );
        // TODO: here we can later implement check for plugin version range compatibility
        Self::compare_field(
            &mut result,
            &mut self.plugin_version,
            &mut other.plugin_version,
        );
        Self::compare_field(
            &mut result,
            &mut self.plugin_long_version,
            &mut other.plugin_long_version,
        );
        result
    }

    // Utility function for compare single field
    fn compare_field_fn<T, F: Fn(&T, &T) -> bool>(
        result: &mut bool,
        a: &mut Option<T>,
        b: &mut Option<T>,
        compare: F,
    ) {
        let compatible = if let (Some(a), Some(b)) = (&a, &b) {
            compare(a, b)
        } else {
            true
        };
        if compatible {
            *a = None;
            *b = None;
        } else {
            *result = false;
        }
    }
    fn compare_field<T: PartialEq>(result: &mut bool, a: &mut Option<T>, b: &mut Option<T>) {
        Self::compare_field_fn(result, a, b, |a, b| a == b);
    }
}

impl Display for Compatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(rust_version) = &self.rust_version {
            writeln!(f, "Rust version:\n{}", rust_version)?;
        }
        if let Some(vtable_version) = &self.vtable_version {
            writeln!(f, "VTable version:\n{}", vtable_version)?;
        }
        if let Some(start_args_version) = &self.start_args_version {
            writeln!(f, "StartArgs version:\n{}", start_args_version)?;
        }
        if let Some(instance_version) = &self.instance_version {
            writeln!(f, "Instance version:\n{}", instance_version)?;
        }
        if let Some(plugin_version) = &self.plugin_version {
            writeln!(f, "Plugin version: {}", plugin_version)?;
        }
        Ok(())
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
            plugin_version: ConcretePlugin::PLUGIN_VERSION,
            plugin_long_version: ConcretePlugin::PLUGIN_LONG_VERSION,
            start: ConcretePlugin::start,
        }
    }
}

/// This macro will add a non-mangled functions which provides plugin version and loads it if feature `no_mangle` is enabled in the destination crate.
#[macro_export]
macro_rules! declare_plugin {
    ($ty: path) => {
        #[cfg(feature = "no_mangle")]
        #[no_mangle]
        fn get_plugin_loader_version() -> $crate::PluginLoaderVersion {
            $crate::PLUGIN_LOADER_VERSION
        }

        #[cfg(feature = "no_mangle")]
        #[no_mangle]
        fn get_compatibility() -> $crate::Compatibility {
            $crate::Compatibility::with_plugin_version::<
                <$ty as $crate::Plugin>::StartArgs,
                <$ty as $crate::Plugin>::Instance,
                $ty,
            >()
        }

        #[cfg(feature = "no_mangle")]
        #[no_mangle]
        fn load_plugin() -> $crate::PluginVTable<
            <$ty as $crate::Plugin>::StartArgs,
            <$ty as $crate::Plugin>::Instance,
        > {
            $crate::PluginVTable::new::<$ty>()
        }
    };
}
