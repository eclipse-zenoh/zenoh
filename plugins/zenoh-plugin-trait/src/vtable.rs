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

pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 1;

type StartFn<StartArgs, RunningPlugin> = fn(&str, &StartArgs) -> ZResult<RunningPlugin>;

#[repr(C)]
pub struct PluginVTable<StartArgs, RunningPlugin> {
    pub start: StartFn<StartArgs, RunningPlugin>,
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
            fn get_plugin_loader_version() -> $crate::prelude::PluginLoaderVersion {
                $crate::prelude::PLUGIN_LOADER_VERSION
            }

            #[no_mangle]
            fn get_plugin_compatibility() -> $crate::prelude::Compatibility {
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
