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

mod dynamic_plugin;
mod static_plugin;

use crate::*;
use zenoh_keyexpr::keyexpr;
use zenoh_result::ZResult;
use zenoh_util::LibLoader;

use self::{
    dynamic_plugin::{DynamicPlugin, DynamicPluginSource},
    static_plugin::StaticPlugin,
};

pub trait DeclaredPlugin<StartArgs, Instance>: PluginStatus {
    fn as_status(&self) -> &dyn PluginStatus;
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>>;
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>>;
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>>;
}
pub trait LoadedPlugin<StartArgs, Instance>: PluginStatus {
    fn as_status(&self) -> &dyn PluginStatus;
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>>;
    fn started(&self) -> Option<&dyn StartedPlugin<StartArgs, Instance>>;
    fn started_mut(&mut self) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>>;
}

pub trait StartedPlugin<StartArgs, Instance>: PluginStatus {
    fn as_status(&self) -> &dyn PluginStatus;
    fn stop(&mut self);
    fn instance(&self) -> &Instance;
    fn instance_mut(&mut self) -> &mut Instance;
}

struct PluginRecord<StartArgs: PluginStartArgs, Instance: PluginInstance>(
    Box<dyn DeclaredPlugin<StartArgs, Instance> + Send>,
);

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginRecord<StartArgs, Instance> {
    fn new<P: DeclaredPlugin<StartArgs, Instance> + Send + 'static>(plugin: P) -> Self {
        Self(Box::new(plugin))
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginStatus
    for PluginRecord<StartArgs, Instance>
{
    fn name(&self) -> &str {
        self.0.name()
    }
    fn version(&self) -> Option<&str> {
        self.0.version()
    }
    fn long_version(&self) -> Option<&str> {
        self.0.long_version()
    }
    fn path(&self) -> &str {
        self.0.path()
    }
    fn state(&self) -> PluginState {
        self.0.state()
    }
    fn report(&self) -> PluginReport {
        self.0.report()
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> DeclaredPlugin<StartArgs, Instance>
    for PluginRecord<StartArgs, Instance>
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        self.0.load()
    }
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        self.0.loaded()
    }
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        self.0.loaded_mut()
    }
}

/// A plugins manager that handles starting and stopping plugins.
/// Plugins can be loaded from shared libraries using [`Self::load_plugin_by_name`] or [`Self::load_plugin_by_paths`], or added directly from the binary if available using [`Self::add_static`].
pub struct PluginsManager<StartArgs: PluginStartArgs, Instance: PluginInstance> {
    default_lib_prefix: String,
    loader: Option<LibLoader>,
    plugins: Vec<PluginRecord<StartArgs, Instance>>,
}

impl<StartArgs: PluginStartArgs + 'static, Instance: PluginInstance + 'static>
    PluginsManager<StartArgs, Instance>
{
    /// Constructs a new plugin manager with dynamic library loading enabled.
    pub fn dynamic<S: Into<String>>(loader: LibLoader, default_lib_prefix: S) -> Self {
        PluginsManager {
            default_lib_prefix: default_lib_prefix.into(),
            loader: Some(loader),
            plugins: Vec::new(),
        }
    }
    /// Constructs a new plugin manager with dynamic library loading disabled.
    pub fn static_plugins_only() -> Self {
        PluginsManager {
            default_lib_prefix: String::new(),
            loader: None,
            plugins: Vec::new(),
        }
    }

    /// Adds a statically linked plugin to the manager.
    pub fn declare_static_plugin<
        P: Plugin<StartArgs = StartArgs, Instance = Instance> + Send + Sync,
    >(
        mut self,
    ) -> Self {
        let plugin_loader: StaticPlugin<StartArgs, Instance, P> = StaticPlugin::new();
        self.plugins.push(PluginRecord::new(plugin_loader));
        tracing::debug!(
            "Declared static plugin {}",
            self.plugins.last().unwrap().name()
        );
        self
    }

    /// Add dynamic plugin to the manager by name, automatically prepending the default library prefix
    pub fn declare_dynamic_plugin_by_name<S: Into<String>>(
        &mut self,
        name: S,
        plugin_name: &str,
    ) -> ZResult<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let name = name.into();
        let plugin_name = format!("{}{}", self.default_lib_prefix, plugin_name);
        let libloader = self
            .loader
            .as_ref()
            .ok_or("Dynamic plugin loading is disabled")?
            .clone();
        tracing::debug!("Declared dynamic plugin {} by name {}", &name, &plugin_name);
        let loader =
            DynamicPlugin::new(name, DynamicPluginSource::ByName((libloader, plugin_name)));
        self.plugins.push(PluginRecord::new(loader));
        Ok(self.plugins.last_mut().unwrap())
    }

    /// Add first available dynamic plugin from the list of paths to the plugin files
    pub fn declare_dynamic_plugin_by_paths<S: Into<String>, P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: S,
        paths: &[P],
    ) -> ZResult<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let name = name.into();
        let paths = paths.iter().map(|p| p.as_ref().into()).collect();
        tracing::debug!("Declared dynamic plugin {} by paths {:?}", &name, &paths);
        let loader = DynamicPlugin::new(name, DynamicPluginSource::ByPaths(paths));
        self.plugins.push(PluginRecord::new(loader));
        Ok(self.plugins.last_mut().unwrap())
    }

    fn get_plugin_index(&self, name: &str) -> Option<usize> {
        self.plugins.iter().position(|p| p.name() == name)
    }

    /// Lists all plugins
    pub fn declared_plugins_iter(
        &self,
    ) -> impl Iterator<Item = &dyn DeclaredPlugin<StartArgs, Instance>> + '_ {
        self.plugins
            .iter()
            .map(|p| p as &dyn DeclaredPlugin<StartArgs, Instance>)
    }

    /// Lists all plugins mutable
    pub fn declared_plugins_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn DeclaredPlugin<StartArgs, Instance>> + '_ {
        self.plugins
            .iter_mut()
            .map(|p| p as &mut dyn DeclaredPlugin<StartArgs, Instance>)
    }

    /// Lists the loaded plugins
    pub fn loaded_plugins_iter(
        &self,
    ) -> impl Iterator<Item = &dyn LoadedPlugin<StartArgs, Instance>> + '_ {
        self.declared_plugins_iter().filter_map(|p| p.loaded())
    }

    /// Lists the loaded plugins mutable
    pub fn loaded_plugins_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn LoadedPlugin<StartArgs, Instance>> + '_ {
        // self.plugins_mut().filter_map(|p| p.loaded_mut())
        self.declared_plugins_iter_mut()
            .filter_map(|p| p.loaded_mut())
    }

    /// Lists the started plugins
    pub fn started_plugins_iter(
        &self,
    ) -> impl Iterator<Item = &dyn StartedPlugin<StartArgs, Instance>> + '_ {
        self.loaded_plugins_iter().filter_map(|p| p.started())
    }

    /// Lists the started plugins mutable
    pub fn started_plugins_iter_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn StartedPlugin<StartArgs, Instance>> + '_ {
        self.loaded_plugins_iter_mut()
            .filter_map(|p| p.started_mut())
    }

    /// Returns single plugin record by name
    pub fn plugin(&self, name: &str) -> Option<&dyn DeclaredPlugin<StartArgs, Instance>> {
        let index = self.get_plugin_index(name)?;
        Some(&self.plugins[index])
    }

    /// Returns mutable plugin record by name
    pub fn plugin_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let index = self.get_plugin_index(name)?;
        Some(&mut self.plugins[index])
    }

    /// Returns loaded plugin record by name
    pub fn loaded_plugin(&self, name: &str) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        self.plugin(name)?.loaded()
    }

    /// Returns mutable loaded plugin record by name
    pub fn loaded_plugin_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        self.plugin_mut(name)?.loaded_mut()
    }

    /// Returns started plugin record by name
    pub fn started_plugin(&self, name: &str) -> Option<&dyn StartedPlugin<StartArgs, Instance>> {
        self.loaded_plugin(name)?.started()
    }

    /// Returns mutable started plugin record by name
    pub fn started_plugin_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>> {
        self.loaded_plugin_mut(name)?.started_mut()
    }
}

impl<StartArgs: PluginStartArgs + 'static, Instance: PluginInstance + 'static> PluginControl
    for PluginsManager<StartArgs, Instance>
{
    fn plugins_status(&self, names: &keyexpr) -> Vec<PluginStatusRec> {
        tracing::debug!(
            "Plugin manager with prefix `{}` : requested plugins_status {:?}",
            self.default_lib_prefix,
            names
        );
        let mut plugins = Vec::new();
        for plugin in self.declared_plugins_iter() {
            let name = unsafe { keyexpr::from_str_unchecked(plugin.name()) };
            if names.includes(name) {
                let status = PluginStatusRec::new(plugin.as_status());
                plugins.push(status);
            }
            // for running plugins append their subplugins prepended with the running plugin name
            if let Some(plugin) = plugin.loaded() {
                if let Some(plugin) = plugin.started() {
                    if let [names, ..] = names.strip_prefix(name)[..] {
                        plugins.append(
                            &mut plugin
                                .instance()
                                .plugins_status(names)
                                .into_iter()
                                .map(|s| s.prepend_name(name))
                                .collect(),
                        );
                    }
                }
            }
        }
        plugins
    }
}
