//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh_config::{Config, PluginLoad};
use zenoh_result::ZResult;

use crate::{
    api::plugins::{PluginsManager, PLUGIN_PREFIX},
    net::runtime::Runtime,
};

pub(crate) fn load_plugin(
    plugin_mgr: &mut PluginsManager,
    name: &str,
    id: &str,
    paths: &Option<Vec<String>>,
    required: bool,
) -> ZResult<()> {
    let declared = if let Some(declared) = plugin_mgr.plugin_mut(name) {
        tracing::warn!("Plugin `{}` was already declared", declared.id());
        declared
    } else if let Some(paths) = paths {
        plugin_mgr.declare_dynamic_plugin_by_paths(name, id, paths, required)?
    } else {
        plugin_mgr.declare_dynamic_plugin_by_name(id, name, required)?
    };

    if let Some(loaded) = declared.loaded_mut() {
        tracing::warn!(
            "Plugin `{}` was already loaded from {}",
            loaded.id(),
            loaded.path()
        );
    } else {
        let _ = declared.load()?;
    };
    Ok(())
}

pub(crate) fn load_plugins(config: &Config) -> PluginsManager {
    let mut manager = PluginsManager::dynamic(config.libloader(), PLUGIN_PREFIX.to_string());
    // Static plugins are to be added here, with `.add_static::<PluginType>()`
    for plugin_load in config.plugins().load_requests() {
        let PluginLoad {
            id,
            name,
            paths,
            required,
        } = plugin_load;
        tracing::info!(
            "Loading {req} plugin \"{id}\"",
            req = if required { "required" } else { "" }
        );
        if let Err(e) = load_plugin(&mut manager, &name, &id, &paths, required) {
            if required {
                panic!("Plugin load failure: {e}")
            } else {
                tracing::error!("Plugin load failure: {e}")
            }
        }
    }
    manager
}

pub(crate) fn start_plugins(runtime: &Runtime) {
    let mut manager = runtime.plugins_manager();
    let dynamic_runtime = runtime.clone().into();
    for plugin in manager.loaded_plugins_iter_mut() {
        let required = plugin.required();
        tracing::info!(
            "Starting {req} plugin \"{name}\"",
            req = if required { "required" } else { "" },
            name = plugin.id()
        );
        match plugin.start(&dynamic_runtime) {
            Ok(_) => {
                tracing::info!(
                    "Successfully started plugin {} from {:?}",
                    plugin.id(),
                    plugin.path()
                );
            }
            Err(e) => {
                let report = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.to_string())) {
                        Ok(s) => s,
                        Err(_) => panic!("Formatting the error from plugin {} ({:?}) failed, this is likely due to ABI instability.\r\nMake sure your plugin was built with the same version of cargo as zenohd", plugin.name(), plugin.path()),
                    };
                if required {
                    panic!(
                        "Plugin \"{}\" failed to start: {}",
                        plugin.id(),
                        if report.is_empty() {
                            "no details provided"
                        } else {
                            report.as_str()
                        }
                    );
                } else {
                    tracing::error!(
                        "Required plugin \"{}\" failed to start: {}",
                        plugin.id(),
                        if report.is_empty() {
                            "no details provided"
                        } else {
                            report.as_str()
                        }
                    );
                }
            }
        }
        tracing::info!("Finished loading plugins");
    }
}
