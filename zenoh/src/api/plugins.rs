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

use serde_json::Value;
use zenoh_core::zconfigurable;
use zenoh_plugin_trait::{Plugin, PluginControl, PluginInstance, PluginReport, PluginStatusRec};
use zenoh_protocol::core::key_expr::keyexpr;
use zenoh_result::ZResult;
use zenoh_util::ffi::{JsonKeyValueMap, JsonValue};

use crate::{api::key_expr::KeyExpr, net::runtime::DynamicRuntime};

zconfigurable! {
    pub static ref PLUGIN_PREFIX: String = "zenoh_plugin_".to_string();
}
/// A zenoh plugin, when started, must return this type.
pub type RunningPlugin = Box<dyn RunningPluginTrait + Send + Sync + 'static>;

/// Zenoh plugins should implement this trait to ensure type-safety, even if the starting arguments and expected plugin types change in a future release.
pub trait ZenohPlugin: Plugin<StartArgs = DynamicRuntime, Instance = RunningPlugin> {}

impl PluginControl for RunningPlugin {
    fn report(&self) -> PluginReport {
        self.as_ref().report()
    }

    fn plugins_status(&self, names: &keyexpr) -> Vec<PluginStatusRec<'_>> {
        self.as_ref().plugins_status(names)
    }
}

impl PluginInstance for RunningPlugin {}

#[non_exhaustive]
#[derive(Debug, Clone)]
/// A Response for the administration space.
pub struct Response {
    pub key: String,
    pub value: JsonValue,
}

impl Response {
    pub fn new(key: String, value: Value) -> Self {
        Self {
            key,
            value: value.into(),
        }
    }
}

pub trait RunningPluginTrait: Send + Sync + PluginControl {
    /// Function that will be called when the configuration relevant to the plugin is about to change.
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
    fn config_checker(
        &self,
        _path: &str,
        _current: &JsonKeyValueMap,
        _new: &JsonKeyValueMap,
    ) -> ZResult<Option<JsonKeyValueMap>> {
        bail!("Runtime configuration change not supported");
    }
    /// Used to request the plugin's status for the administration space.
    /// Function called on any query on the admin space that matches this plugin's sub-part of the admin space.
    /// Thus the plugin can reply with its contribution to the global admin space of this zenohd.
    /// Parameters:
    /// * `key_expr`: the key_expr selector of the query. This key_expr is
    ///   exactly the same as it was requested by the user, for example "@/ROUTER_ID/router/plugins/PLUGIN_NAME/some/plugin/info" or "@/*/router/plugins/*/foo/bar".
    ///   But the plugin's [RunningPluginTrait::adminspace_getter] is called only if the key_expr matches the `plugin_status_key`
    /// * `plugin_status_key`: the actual path to the plugin's status in the admin space. For example "@/ROUTER_ID/router/plugins/PLUGIN_NAME"
    ///   Return value:
    /// * `Ok(Vec<Response>)`: the list of responses to the query. For example if plugins can return information on subkeys "foo", "bar", "foo/buzz" and "bar/buzz"
    ///   and it's requested with the query "@/ROUTER_ID/router/plugins/PLUGIN_NAME/*", it should return only information on "foo" and "bar" subkeys, but not on "foo/buzz" and "bar/buzz",
    ///   as they don't match the query.
    /// * `Err(ZError)`: A problem occurred when processing the query.
    ///
    /// If the plugin implements subplugins (as the storage plugin), then it should also reply with information about its subplugins with the same rules.
    ///
    /// TODO:
    /// * add example
    /// * rework the admin space: rework the "with_extended_string" function, provide it as a utility for plugins
    /// * reorder parameters: plugin_status_key should be first, as it describes the root of the plugin's admin space
    /// * Instead of ZResult, return just Vec. Check, do we really need ZResult? If yes, make it separate for each status record.
    ///
    fn adminspace_getter<'a>(
        &'a self,
        _key_expr: &'a KeyExpr<'a>,
        _plugin_status_key: &str,
    ) -> ZResult<Vec<Response>> {
        Ok(Vec::new())
    }
}

/// The zenoh plugins manager. It handles the full lifetime of plugins, from loading to destruction.
pub type PluginsManager = zenoh_plugin_trait::PluginsManager<DynamicRuntime, RunningPlugin>;
