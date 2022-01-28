//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use crate::prelude::Selector;
use crate::Result as ZResult;
use zenoh_core::zconfigurable;

use crate::net::runtime::Runtime;
zconfigurable! {
    pub static ref PLUGIN_PREFIX: String = "zplugin_".to_string();
}

/// Zenoh plugins should implement this trait to ensure type-safety event if the starting arguments and expected plugin types change.
pub trait ZenohPlugin: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin> {}

/// A zenoh plugin receives a reference to a value of this type when started.
pub type StartArgs = Runtime;
/// A zenoh plugin, when started, must return this type.
pub type RunningPlugin = Box<dyn RunningPluginTrait + 'static>;
#[derive(serde::Serialize, Debug, Clone)]
pub struct Response {
    pub key: String,
    pub value: serde_json::Value,
}
pub trait RunningPluginTrait: Send + Sync + std::any::Any {
    fn config_checker(&self) -> ValidationFunction;
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
