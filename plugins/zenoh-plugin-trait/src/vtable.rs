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

use crate::{Plugin, StructVersion, FEATURES};

pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 1;

type StartFn<StartArgs, Instance> = fn(&str, &StartArgs) -> ZResult<Instance>;

#[repr(C)]
pub struct PluginVTable<StartArgs, Instance> {
    pub plugin_version: &'static str,
    pub plugin_long_version: &'static str,
    pub start: StartFn<StartArgs, Instance>,
}
impl<StartArgs, Instance> StructVersion for PluginVTable<StartArgs, Instance> {
    fn struct_version() -> u64 {
        1
    }
    fn struct_features() -> &'static str {
        FEATURES
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

/// This macro will add a non-mangled functions which provides plugin version and loads it into the host.
/// If plugin library should work also as static, consider calling this macro under feature condition
#[macro_export]
macro_rules! declare_plugin {
    ($ty: path) => {
        #[no_mangle]
        fn get_plugin_loader_version() -> $crate::PluginLoaderVersion {
            $crate::PLUGIN_LOADER_VERSION
        }

        #[no_mangle]
        fn get_compatibility() -> $crate::Compatibility {
            $crate::Compatibility::with_plugin_version::<
                <$ty as $crate::Plugin>::StartArgs,
                <$ty as $crate::Plugin>::Instance,
                $ty,
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
