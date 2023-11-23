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
//!
mod manager;
mod plugin;
mod vtable;

pub use manager::{DeclaredPlugin, LoadedPlugin, PluginsManager, StartedPlugin};
pub use plugin::{
    Plugin, PluginConditionSetter, PluginControl, PluginInstance, PluginReport, PluginStartArgs,
    PluginState, PluginStatus, PluginStatusRec, PluginStructVersion,
};
pub use vtable::{Compatibility, PluginLoaderVersion, PluginVTable, PLUGIN_LOADER_VERSION};

#[macro_export]
macro_rules! concat_enabled_features {
    (prefix = $prefix:literal, features = [$($feature:literal),*]) => {
        {
            use const_format::concatcp;
            concatcp!("" $(,
                if cfg!(feature = $feature) { concatcp!(" ", concatcp!($prefix, "/", $feature)) } else { "" }
            )*)
        }
    };
}

pub const FEATURES: &str = concat_enabled_features!(
    prefix = "zenoh-plugin-trait",
    features = ["default", "no_mangle"]
);
