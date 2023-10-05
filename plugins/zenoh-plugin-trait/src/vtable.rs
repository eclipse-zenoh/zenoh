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
pub use no_mangle::*;

/// This number should change any time the [`Compatibility`] structure changes or plugin loading sequence changes
pub type PluginLoaderVersion = u64;
pub const PLUGIN_LOADER_VERSION: PluginLoaderVersion = 1;

pub use no_mangle::*;
#[cfg(feature = "no_mangle")]
pub mod no_mangle {
    /// This macro will add a set of non-mangled functions for plugin loading if feature `no_mangle` is enabled (which it is by default).
    #[macro_export]
    macro_rules! declare_plugin {
        ($ident: ident, $trait: item) => {
            #[no_mangle]
            fn get_plugin_loader_version() -> $crate::prelude::PluginLoaderVersion {
                $crate::prelude::PLUGIN_LOADER_VERSION
            }
            #[no_mangle]
            fn get_plugin_compatibility() -> $crate::Compatibility {
                <$item as $trait>::compatibility()
            }
            #[no_mangle]
            fn get_plugin_trait() -> &'static dyn $trait {
                &$ident as &dyn $trait
            }
        };
    }
}
#[cfg(not(feature = "no_mangle"))]
pub mod no_mangle {
    #[macro_export]
    macro_rules! declare_plugin {
        ($ident: ident, $trait: item) => {};
    }
}
