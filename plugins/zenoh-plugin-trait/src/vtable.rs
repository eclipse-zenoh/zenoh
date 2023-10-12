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
pub use no_mangle::*;
use zenoh_result::ZResult;

pub type CompatibilityVersion = u64;
pub const COMPATIBILITY_VERSION: CompatibilityVersion = 1;

type StartFn<StartArgs, RunningPlugin> = fn(&str, &StartArgs) -> ZResult<RunningPlugin>;

#[repr(C)]
pub struct PluginVTable<StartArgs, RunningPlugin> {
    pub start: StartFn<StartArgs, RunningPlugin>,
}

impl<StartArgs, RunningPlugin> Version for PluginVTable<StartArgs, RunningPlugin> {
    fn version() -> u32 {
        1
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Compatibility {
    rust_version: RustVersion,
    vtable_version: PluginVTableVersion,
}

impl Compatibility {}

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

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PluginVTableVersion {
    vtable_version: u32,
    start_args_type_version: u32,
    running_plugin_type_version: u32,
    start_args_type_name: &'static str,
    running_plugin_type_name: &'static str,
}

impl PluginVTableVersion {
    pub fn new<StartArgs, RunningPlugin>() -> Self {
        Self {
            vtable_version: 1,
            start_args_type_version: 1,
            running_plugin_type_version: 1,
            start_args_type_name: std::any::type_name::<StartArgs>(),
            running_plugin_type_name: std::any::type_name::<RunningPlugin>(),
        }
    }
}

impl<StartArgs, RunningPlugin> PluginVTable<StartArgs, RunningPlugin> {
    pub fn new<ConcretePlugin: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin>>(
    ) -> Self {
        Self {
            start: ConcretePlugin::start,
        }
    }
}

pub use no_mangle::*;
#[cfg(feature = "no_mangle")]
pub mod no_mangle {
    /// This macro will add a non-mangled `load_plugin` function to the library if feature `no_mangle` is enabled (which it is by default).
    #[macro_export]
    macro_rules! declare_plugin {
        ($ty: path) => {
            #[no_mangle]
            fn get_compatibility_version() -> $crate::prelude::CompatibilityVersion {
                $crate::prelude::PLUGIN_LOADER_VERSION
            }

            #[no_mangle]
            fn get_compatibility() -> $crate::prelude::Compatibility {
                // TODO: add vtable version (including type parameters) to the compatibility information
                $crate::prelude::Compatibility::new()
            }

            #[no_mangle]
            fn load_plugin() -> $crate::prelude::PluginVTable<
                <$ty as $crate::prelude::Plugin>::StartArgs,
                <$ty as $crate::prelude::Plugin>::RunningPlugin,
            > {
                $crate::prelude::PluginVTable::new::<$ty>()
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
