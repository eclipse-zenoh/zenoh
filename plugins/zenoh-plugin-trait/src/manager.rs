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
mod dynamic_plugin;
mod static_plugin;

use zenoh_keyexpr::keyexpr;
use zenoh_result::ZResult;
use zenoh_util::LibLoader;

use self::{
    dynamic_plugin::{DynamicPlugin, DynamicPluginSource},
    static_plugin::StaticPlugin,
};
use crate::*;

pub trait DeclaredPlugin<StartArgs, Instance>: PluginStatus {
    fn as_status(&self) -> &dyn PluginStatus;
    fn load(&mut self) -> ZResult<Option<&mut dyn LoadedPlugin<StartArgs, Instance>>>;
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>>;
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>>;
}
pub trait LoadedPlugin<StartArgs, Instance>: PluginStatus {
    fn as_status(&self) -> &dyn PluginStatus;
    fn required(&self) -> bool;
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
    Box<dyn DeclaredPlugin<StartArgs, Instance> + Send + Sync>,
);

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginRecord<StartArgs, Instance> {
    fn new<P: DeclaredPlugin<StartArgs, Instance> + Send + Sync + 'static>(plugin: P) -> Self {
        Self(Box::new(plugin))
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginStatus
    for PluginRecord<StartArgs, Instance>
{
    fn name(&self) -> &str {
        self.0.name()
    }

    fn id(&self) -> &str {
        self.0.id()
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
    fn load(&mut self) -> ZResult<Option<&mut dyn LoadedPlugin<StartArgs, Instance>>> {
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
/// Plugins can be loaded from shared libraries using [`Self::declare_dynamic_plugin_by_name`] or [`Self::declare_dynamic_plugin_by_paths`], or added directly from the binary if available using [`Self::declare_static_plugin`].
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
            plugins: Default::default(),
        }
    }
    /// Constructs a new plugin manager with dynamic library loading disabled.
    pub fn static_plugins_only() -> Self {
        PluginsManager {
            default_lib_prefix: String::new(),
            loader: None,
            plugins: Default::default(),
        }
    }

    /// Adds a statically linked plugin to the manager.
    pub fn declare_static_plugin<
        P: Plugin<StartArgs = StartArgs, Instance = Instance> + Send + Sync,
        S: Into<String>,
    >(
        &mut self,
        id: S,
        required: bool,
    ) {
        let id = id.into();
        let plugin_loader: StaticPlugin<StartArgs, Instance, P> =
            StaticPlugin::new(id.clone(), required);

        if self.get_plugin_index(&id).is_some() {
            tracing::warn!(
                "Duplicate plugin with ID: {id}, only the last declared one will be loaded"
            )
        }

        self.plugins.push(PluginRecord::new(plugin_loader));
        tracing::debug!(
            "Declared static plugin Id:{} - Name:{}",
            self.plugins.last().unwrap().id(),
            self.plugins.last().unwrap().name()
        );
    }

    /// Add dynamic plugin to the manager by name, automatically prepending the default library prefix
    pub fn declare_dynamic_plugin_by_name<S: Into<String>>(
        &mut self,
        id: S,
        plugin_name: S,
        required: bool,
    ) -> ZResult<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let plugin_name = plugin_name.into();
        let id = id.into();
        let libplugin_name = format!("{}{}", self.default_lib_prefix, plugin_name);
        let libloader = self
            .loader
            .as_ref()
            .ok_or("Dynamic plugin loading is disabled")?
            .clone();
        tracing::debug!(
            "Declared dynamic plugin {} by name {}",
            &id,
            &libplugin_name
        );
        let loader = DynamicPlugin::new(
            plugin_name,
            id.clone(),
            DynamicPluginSource::ByName((libloader, libplugin_name)),
            required,
        );

        if self.get_plugin_index(&id).is_some() {
            tracing::warn!(
                "Duplicate plugin with ID: {id}, only the last declared one will be loaded"
            )
        }
        self.plugins.push(PluginRecord::new(loader));
        Ok(self.plugins.last_mut().unwrap())
    }

    /// Add first available dynamic plugin from the list of paths to the plugin files
    pub fn declare_dynamic_plugin_by_paths<S: Into<String>, P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: S,
        id: S,
        paths: &[P],
        required: bool,
    ) -> ZResult<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let name = name.into();
        let id = id.into();
        let paths = paths.iter().map(|p| p.as_ref().into()).collect();
        tracing::debug!("Declared dynamic plugin {} by paths {:?}", &id, &paths);
        let loader = DynamicPlugin::new(
            name,
            id.clone(),
            DynamicPluginSource::ByPaths(paths),
            required,
        );

        if self.get_plugin_index(&id).is_some() {
            tracing::warn!(
                "Duplicate plugin with ID: {id}, only the last declared one will be loaded"
            )
        }

        self.plugins.push(PluginRecord::new(loader));
        Ok(self.plugins.last_mut().unwrap())
    }

    fn get_plugin_index(&self, id: &str) -> Option<usize> {
        self.plugins.iter().position(|p| p.id() == id)
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

    /// Returns single plugin record by id
    pub fn plugin(&self, id: &str) -> Option<&dyn DeclaredPlugin<StartArgs, Instance>> {
        let index = self.get_plugin_index(id)?;
        Some(&self.plugins[index])
    }

    /// Returns mutable plugin record by id
    pub fn plugin_mut(&mut self, id: &str) -> Option<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let index = self.get_plugin_index(id)?;
        Some(&mut self.plugins[index])
    }

    /// Returns loaded plugin record by id
    pub fn loaded_plugin(&self, id: &str) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        self.plugin(id)?.loaded()
    }

    /// Returns mutable loaded plugin record by id
    pub fn loaded_plugin_mut(
        &mut self,
        id: &str,
    ) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        self.plugin_mut(id)?.loaded_mut()
    }

    /// Returns started plugin record by id
    pub fn started_plugin(&self, id: &str) -> Option<&dyn StartedPlugin<StartArgs, Instance>> {
        self.loaded_plugin(id)?.started()
    }

    /// Returns mutable started plugin record by id
    pub fn started_plugin_mut(
        &mut self,
        id: &str,
    ) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>> {
        self.loaded_plugin_mut(id)?.started_mut()
    }
}

impl<StartArgs: PluginStartArgs + 'static, Instance: PluginInstance + 'static> PluginControl
    for PluginsManager<StartArgs, Instance>
{
    fn plugins_status(&self, names: &keyexpr) -> Vec<PluginStatusRec<'_>> {
        tracing::debug!(
            "Plugin manager with prefix `{}` : requested plugins_status {:?}",
            self.default_lib_prefix,
            names
        );
        let mut plugins = Vec::new();
        for plugin in self.declared_plugins_iter() {
            let id = unsafe { keyexpr::from_str_unchecked(plugin.id()) };
            if names.includes(id) {
                let status = PluginStatusRec::new(plugin.as_status());
                plugins.push(status);
            }
            // for running plugins append their subplugins prepended with the running plugin name
            if let Some(plugin) = plugin.loaded() {
                if let Some(plugin) = plugin.started() {
                    if let [names, ..] = names.strip_prefix(id)[..] {
                        plugins.append(
                            &mut plugin
                                .instance()
                                .plugins_status(names)
                                .into_iter()
                                .map(|s| s.prepend_name(id))
                                .collect(),
                        );
                    }
                }
            }
        }
        plugins
    }
}
