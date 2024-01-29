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
//! Plugin is a struct which implements the [`Plugin`] trait. This trait has two associated types:
//! - `StartArgs`: the type of the arguments passed to the plugin's [`start`](Plugin::start) function.
//! - `Instance`: the type of the plugin's instance.
//!
//! The actual work of the plugin is performed by the instance, which is created by the [`start`](Plugin::start) function.
//!
//! Plugins are loaded, started and stopped by [`PluginsManager`](crate::manager::PluginsManager). Stopping pluign is just dropping it's instance.
//!
//! Plugins can be static and dynamic.
//!
//! Static plugin is just a type which implements [`Plugin`] trait. It can be added to [`PluginsManager`](crate::manager::PluginsManager) by [`PluginsManager::add_static_plugin`](crate::manager::PluginsManager::add_static_plugin) method.
//!
//! Dynamic pluign is a shared library which exports set of C-repr (unmangled) functions which allows to check plugin compatibility and create plugin instance. These functiuons are defined automatically by [`declare_plugin`](crate::declare_plugin) macro.
//!
mod compatibility;
mod manager;
mod plugin;
mod vtable;

pub use compatibility::{Compatibility, PluginStructVersion, StructVersion};
pub use manager::{DeclaredPlugin, LoadedPlugin, PluginsManager, StartedPlugin};
pub use plugin::{
    Plugin, PluginConditionSetter, PluginControl, PluginInstance, PluginReport, PluginStartArgs,
    PluginState, PluginStatus, PluginStatusRec,
};
pub use vtable::{PluginLoaderVersion, PluginVTable, PLUGIN_LOADER_VERSION};
use zenoh_util::concat_enabled_features;

pub const FEATURES: &str =
    concat_enabled_features!(prefix = "zenoh-plugin-trait", features = ["default"]);
