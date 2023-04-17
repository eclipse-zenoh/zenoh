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

pub type PluginVTableVersion = u16;
type LoadPluginResultInner = Result<PluginVTableInner<(), ()>, PluginVTableVersion>;
pub type LoadPluginResult<A, B> = Result<PluginVTable<A, B>, PluginVTableVersion>;

/// This number should change any time the internal structure of [`PluginVTable`] changes
pub const PLUGIN_VTABLE_VERSION: PluginVTableVersion = 1;

type StartFn<StartArgs, RunningPlugin> = fn(&str, &StartArgs) -> ZResult<RunningPlugin>;

#[repr(C)]
struct PluginVTableInner<StartArgs, RunningPlugin> {
    start: StartFn<StartArgs, RunningPlugin>,
    compatibility: fn() -> ZResult<crate::Compatibility>,
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

/// For use with dynamically loaded plugins. Its size will not change accross versions, but its internal structure might.
///
/// To ensure compatibility, its size and alignment must allow `size_of::<Result<PluginVTable, PluginVTableVersion>>() == 64` (one cache line).
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
                compatibility: ConcretePlugin::compatibility,
            },
            padding: PluginVTablePadding::new(),
        }
    }

    /// Ensures [PluginVTable]'s size stays the same between versions
    fn __size_check() {
        unsafe {
            std::mem::transmute::<_, [u8; 64]>(std::mem::MaybeUninit::<
                Result<Self, PluginVTableVersion>,
            >::uninit())
        };
    }

    pub fn start(&self, name: &str, start_args: &StartArgs) -> ZResult<RunningPlugin> {
        (self.inner.start)(name, start_args)
    }
    pub fn compatibility(&self) -> ZResult<Compatibility> {
        (self.inner.compatibility)()
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
            fn load_plugin(
                version: $crate::prelude::PluginVTableVersion,
            ) -> $crate::prelude::LoadPluginResult<
                <$ty as $crate::prelude::Plugin>::StartArgs,
                <$ty as $crate::prelude::Plugin>::RunningPlugin,
            > {
                if version == $crate::prelude::PLUGIN_VTABLE_VERSION {
                    Ok($crate::prelude::PluginVTable::new::<$ty>())
                } else {
                    Err($crate::prelude::PLUGIN_VTABLE_VERSION)
                }
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
