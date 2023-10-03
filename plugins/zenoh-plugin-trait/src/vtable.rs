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


/// This number should change any time the [`Compatibility`] structure changes or plugin loading sequence changes
pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 1;

type StartFn<StartArgs, RunningPlugin> = fn(&str, &StartArgs) -> ZResult<RunningPlugin>;

#[repr(C)]
struct PluginVTableInner<StartArgs, RunningPlugin> {
    start: StartFn<StartArgs, RunningPlugin>,
}

/// Automagical padding such that [PluginVTable::init]'s result is the size of a cache line
#[repr(C)]
struct PluginVTablePadding {
    __padding: [u8; PluginVTablePadding::padding_length()],
}
impl PluginVTablePadding {
    const fn padding_length() -> usize {
        64 - std::mem::size_of::<LoadPluginResultInner>()
    }
    fn new() -> Self {
        PluginVTablePadding {
            __padding: [0; Self::padding_length()],
        }
    }
}

#[repr(C)]
pub struct PluginVTable<StartArgs, RunningPlugin> {
    inner: PluginVTableInner<StartArgs, RunningPlugin>,
    padding: PluginVTablePadding,
}

impl<StartArgs, RunningPlugin> PluginVTable<StartArgs, RunningPlugin> {
    pub fn new<ConcretePlugin: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin>>(
    ) -> Self {
        PluginVTable {
            inner: PluginVTableInner {
                start: ConcretePlugin::start,
            },
            padding: PluginVTablePadding::new(),
        }
    }
    pub fn start(&self, name: &str, start_args: &StartArgs) -> ZResult<RunningPlugin> {
        (self.inner.start)(name, start_args)
    }
}

pub use no_mangle::*;
#[cfg(feature = "no_mangle")]
pub mod no_mangle {
    /// This macro will add a set of non-mangled functions for plugin loading if feature `no_mangle` is enabled (which it is by default).
    #[macro_export]
    macro_rules! declare_plugin {
        ($ty: path) => {
            #[no_mangle]
            fn get_plugin_loader_version() -> $crate::prelude::PluginVTableVersion {
                $crate::prelude::PLUGIN_LOADER_VERSION
            }
            #[no_mangle]
            fn get_plugin_vtable_compatibility() -> $crate::Compatibility {
                <<$ty as $crate::prelude::Plugin>::StartArgs as $crate::prelude::Validator>::compatibility()
            }
            #[no_mangle]
            fn get_plugin_vtable() -> $crate::prelude::PluginVTable<
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
