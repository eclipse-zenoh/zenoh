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

//! `zenohd`'s plugin system. For more details, consult the [detailed documentation](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Plugins/Zenoh%20Plugins.md).

use crate::prelude::Selector;
pub use crate::runtime::Runtime;
pub use crate::Result as ZResult;
use zenoh_core::zconfigurable;

zconfigurable! {
    pub static ref PLUGIN_PREFIX: String = "zenoh_plugin_".to_string();
}

/// Zenoh plugins should implement this trait to ensure type-safety, even if the starting arguments and expected plugin types change in a future release.
pub trait ZenohPlugin: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin> {}

/// A zenoh plugin receives a reference to a value of this type when started.
pub type StartArgs = Runtime;
/// A zenoh plugin, when started, must return this type.
pub type RunningPlugin = Box<dyn RunningPluginTrait + 'static>;

#[non_exhaustive]
#[derive(serde::Serialize, Debug, Clone)]
/// A Response for the administration space.
pub struct Response {
    pub key: String,
    pub value: serde_json::Value,
}

impl Response {
    pub fn new(key: String, value: serde_json::Value) -> Self {
        Self { key, value }
    }
}

pub trait RunningPluginTrait: Send + Sync + std::any::Any {
    /// Returns a function that will be called when configuration relevant to the plugin is about to change.
    ///
    /// This function is called with 3 arguments:
    /// * `path`, the relative path from the plugin's configuration root to the changed value.
    /// * `current`, the current configuration of the plugin (from its root).
    /// * `new`, the proposed new configuration of the plugin.
    ///
    /// It may return one of 3 cases:
    /// * `Err` indicates that the plugin refuses this configuration change, cancelling it altogether.
    ///   Useful when the changes affect settings that aren't hot-configurable for your plugin.
    /// * `Ok(None)` indicates that the plugin has accepted the configuration change.
    /// * `Ok(Some(value))` indicates that the plugin would rather the new configuration be `value`.
    fn config_checker(&self) -> ValidationFunction;
    /// Used to request your plugin's status for the administration space.
    fn adminspace_getter<'a>(
        &'a self,
        selector: &'a Selector<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<Response>>;
}

/// The zenoh plugins manager. It handles the full lifetime of plugins, from loading to destruction.
pub type PluginsManager = zenoh_plugin_trait::loading::PluginsManager<StartArgs, RunningPlugin>;

pub use zenoh_plugin_trait::Plugin;
pub type ValidationFunction = std::sync::Arc<
    dyn Fn(
            &str,
            &serde_json::Map<String, serde_json::Value>,
            &serde_json::Map<String, serde_json::Value>,
        ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>>
        + Send
        + Sync,
>;
