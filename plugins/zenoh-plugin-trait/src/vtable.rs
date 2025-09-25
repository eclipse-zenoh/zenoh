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
use zenoh_result::ZResult;

use crate::Plugin;

pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 2;

type StartFn<StartArgs, Instance> = fn(&str, &StartArgs) -> ZResult<Instance>;

#[repr(C)]
pub struct PluginVTable<StartArgs, Instance> {
    pub plugin_version: &'static str,
    pub plugin_long_version: &'static str,
    pub start: StartFn<StartArgs, Instance>,
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

/// This macro adds non-mangled functions which provides plugin version and loads it into the host.
/// If plugin library should work also as static, consider calling this macro under feature condition
///
/// The functions declared by this macro are:
///
/// - `get_plugin_loader_version` - returns `PLUGIN_LOADER_VERSION` const of the crate. The [`PluginsManager`](crate::manager::PluginsManager)
///   will check if this version is compatible with the host.
/// - `get_compatibility` - returns [`Compatibility`](crate::Compatibility) struct which contains all version information (Rust compiler version, features used, version of plugin's structures).
///   The layout  of this structure is guaranteed to be stable until the [`PLUGIN_LOADER_VERSION`](crate::PLUGIN_LOADER_VERSION) is changed,
///   so it's safe to use it in the host after call to `get_plugin_loader_version` returns compatible version.
///   Then the [`PluginsManager`](crate::manager::PluginsManager) compares the returned [`Compatibility`](crate::Compatibility) with it's own and decides if it can continue loading the plugin.
/// - `load_plugin` - returns [`PluginVTable`](crate::PluginVTable) which is able to create plugin's instance.
///
#[macro_export]
macro_rules! declare_plugin {
    ($ty: path) => {
        #[no_mangle]
        fn get_plugin_loader_version() -> $crate::PluginLoaderVersion {
            $crate::PLUGIN_LOADER_VERSION
        }

        #[no_mangle]
        fn get_compatibility() -> $crate::Compatibility {
            use zenoh_plugin_trait::StructVersion;
            $crate::Compatibility::new(
                <$ty as $crate::Plugin>::StartArgs::struct_version(),
                <$ty as $crate::Plugin>::StartArgs::struct_features(),
            )
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
