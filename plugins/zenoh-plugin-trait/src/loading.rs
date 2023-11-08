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
use libloading::Library;
use std::{
    borrow::Cow,
    marker::PhantomData,
    path::{Path, PathBuf},
};
use vtable::{Compatibility, PluginLoaderVersion, PluginVTable, PLUGIN_LOADER_VERSION};
use zenoh_result::{bail, ZResult};
use zenoh_util::LibLoader;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PluginState {
    Declared,
    Loaded,
    Started,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PluginCondition {
    warnings: Vec<Cow<'static, str>>,
    errors: Vec<Cow<'static, str>>,
}

impl PluginCondition {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.warnings.clear();
        self.errors.clear();
    }
    pub fn add_error<S: Into<Cow<'static, str>>>(&mut self, error: S) {
        self.errors.push(error.into());
    }
    pub fn add_warning<S: Into<Cow<'static, str>>>(&mut self, warning: S) {
        self.warnings.push(warning.into());
    }
    pub fn errors(&self) -> &[Cow<'static, str>] {
        &self.errors
    }
    pub fn warnings(&self) -> &[Cow<'static, str>] {
        &self.warnings
    }
}

pub trait PluginConditionAddError {
    fn add_error(self, condition: &mut PluginCondition) -> Self;
}

impl<T, E: ToString> PluginConditionAddError for core::result::Result<T, E> {
    fn add_error(self, condition: &mut PluginCondition) -> Self {
        if let Err(e) = &self {
            condition.add_error(e.to_string());
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginStatus {
    pub state: PluginState,
    pub condition: PluginCondition,
}

pub trait PluginInfo {
    fn name(&self) -> &str;
    fn path(&self) -> &str;
    fn status(&self) -> PluginStatus;
}

pub trait DeclaredPlugin<StartArgs, Instance>: PluginInfo {
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>>;
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>>;
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>>;
}
pub trait LoadedPlugin<StartArgs, Instance>: PluginInfo {
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>>;
    fn started(&self) -> Option<&dyn StartedPlugin<StartArgs, Instance>>;
    fn started_mut(&mut self) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>>;
}

pub trait StartedPlugin<StartArgs, Instance>: PluginInfo {
    fn stop(&mut self);
    fn instance(&self) -> &Instance;
    fn instance_mut(&mut self) -> &mut Instance;
}

struct StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    instance: Option<Instance>,
    phantom: PhantomData<P>,
}

impl<StartArgs, Instance, P> StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn new() -> Self {
        Self {
            instance: None,
            phantom: PhantomData,
        }
    }
}

impl<StartArgs, Instance, P> PluginInfo
    for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn name(&self) -> &str {
        P::STATIC_NAME
    }
    fn path(&self) -> &str {
        "<static>"
    }
    fn status(&self) -> PluginStatus {
        PluginStatus {
            state: self
                .instance
                .as_ref()
                .map_or(PluginState::Loaded, |_| PluginState::Started),
            condition: PluginCondition::new(), // TODO: request runnnig plugin status
        }
    }
}

impl<StartArgs, Instance, P>
    DeclaredPlugin<StartArgs, Instance> for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        Ok(self)
    }
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        Some(self)
    }
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        Some(self)
    }
}

impl<StartArgs, Instance, P>
    LoadedPlugin<StartArgs, Instance> for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_none() {
            self.instance = Some(P::start(self.name(), args)?);
        }
        Ok(self)
    }
    fn started(&self) -> Option<&dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_some() {
            Some(self)
        } else {
            None
        }
    }
    fn started_mut(&mut self) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl<StartArgs, Instance, P>
    StartedPlugin<StartArgs, Instance> for StaticPlugin<StartArgs, Instance, P>
where
    P: Plugin<StartArgs = StartArgs, Instance = Instance>,
{
    fn stop(&mut self) {}
    fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }
    fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().unwrap()
    }
}

/// This enum contains information where to load the plugin from.
enum DynamicPluginSource {
    /// Load plugin with the name in String + `.so | .dll | .dylib`
    /// in LibLoader's search paths.
    ByName((LibLoader, String)),
    /// Load first avalilable plugin from the list of path to plugin files (absolute or relative to the current working directory)
    ByPaths(Vec<String>),
}

impl DynamicPluginSource {
    fn load(&self) -> ZResult<(Library, PathBuf)> {
        match self {
            DynamicPluginSource::ByName((libloader, name)) => unsafe {
                libloader.search_and_load(name)
            },
            DynamicPluginSource::ByPaths(paths) => {
                for path in paths {
                    match unsafe { LibLoader::load_file(path) } {
                        Ok((l, p)) => return Ok((l, p)),
                        Err(e) => log::warn!("Plugin {} load fail: {}", path, e),
                    }
                }
                bail!("Plugin not found in {:?}", &paths)
            }
        }
    }
}

struct DynamicPluginStarter<StartArgs, Instance> {
    _lib: Library,
    path: PathBuf,
    vtable: PluginVTable<StartArgs, Instance>,
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    DynamicPluginStarter<StartArgs, Instance>
{
    fn get_vtable(lib: &Library, path: &Path) -> ZResult<PluginVTable<StartArgs, Instance>> {
        log::debug!("Loading plugin {}", &path.to_str().unwrap(),);
        let get_plugin_loader_version =
            unsafe { lib.get::<fn() -> PluginLoaderVersion>(b"get_plugin_loader_version")? };
        let plugin_loader_version = get_plugin_loader_version();
        log::debug!("Plugin loader version: {}", &plugin_loader_version);
        if plugin_loader_version != PLUGIN_LOADER_VERSION {
            bail!(
                "Plugin loader version mismatch: host = {}, plugin = {}",
                PLUGIN_LOADER_VERSION,
                plugin_loader_version
            );
        }
        let get_compatibility = unsafe { lib.get::<fn() -> Compatibility>(b"get_compatibility")? };
        let plugin_compatibility_record = get_compatibility();
        let host_compatibility_record = Compatibility::new::<StartArgs, Instance>();
        log::debug!(
            "Plugin compativilty record: {:?}",
            &plugin_compatibility_record
        );
        if !plugin_compatibility_record.are_compatible(&host_compatibility_record) {
            bail!(
                "Plugin compatibility mismatch:\n\nHost:\n{}\nPlugin:\n{}\n",
                host_compatibility_record,
                plugin_compatibility_record
            );
        }
        let load_plugin =
            unsafe { lib.get::<fn() -> PluginVTable<StartArgs, Instance>>(b"load_plugin")? };
        let vtable = load_plugin();
        Ok(vtable)
    }
    fn new(lib: Library, path: PathBuf) -> ZResult<Self> {
        let vtable = Self::get_vtable(&lib, &path)?;
        Ok(Self {
            _lib: lib,
            path,
            vtable,
        })
    }
    fn start(&self, name: &str, args: &StartArgs) -> ZResult<Instance> {
        (self.vtable.start)(name, args)
    }
    fn path(&self) -> &str {
        self.path.to_str().unwrap()
    }
}

struct DynamicPlugin<StartArgs, Instance> {
    name: String,
    condition: PluginCondition,
    source: DynamicPluginSource,
    starter: Option<DynamicPluginStarter<StartArgs, Instance>>,
    instance: Option<Instance>,
}

impl<StartArgs, Instance>
    DynamicPlugin<StartArgs, Instance>
{
    fn new(name: String, source: DynamicPluginSource) -> Self {
        Self {
            name,
            condition: PluginCondition::new(),
            source,
            starter: None,
            instance: None,
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginInfo
    for DynamicPlugin<StartArgs, Instance>
{
    fn name(&self) -> &str {
        self.name.as_str()
    }
    fn path(&self) -> &str {
        self.starter.as_ref().map_or("<not loaded>", |v| v.path())
    }
    fn status(&self) -> PluginStatus {
        PluginStatus {
            state: if self.starter.is_some() {
                if self.instance.is_some() {
                    PluginState::Started
                } else {
                    PluginState::Loaded
                }
            } else {
                PluginState::Declared
            },
            condition: self.condition.clone(), // TODO: request condition from started plugin
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    DeclaredPlugin<StartArgs, Instance> for DynamicPlugin<StartArgs, Instance>
{
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        if self.starter.is_none() {
            let (lib, path) = self.source.load().add_error(&mut self.condition)?;
            let starter = DynamicPluginStarter::new(lib, path).add_error(&mut self.condition)?;
            self.starter = Some(starter);
        }
        Ok(self)
    }
    fn loaded(&self) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        if self.starter.is_some() {
            Some(self)
        } else {
            None
        }
    }
    fn loaded_mut(&mut self) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        if self.starter.is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    LoadedPlugin<StartArgs, Instance> for DynamicPlugin<StartArgs, Instance>
{
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>> {
        let starter = self
            .starter
            .as_ref()
            .ok_or_else(|| format!("Plugin `{}` not loaded", self.name))
            .add_error(&mut self.condition)?;
        let already_started = self.instance.is_some();
        if !already_started {
            let instance = starter
                .start(self.name(), args)
                .add_error(&mut self.condition)?;
            self.instance = Some(instance);
        }
        Ok(self)
    }
    fn started(&self) -> Option<&dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_some() {
            Some(self)
        } else {
            None
        }
    }
    fn started_mut(&mut self) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>> {
        if self.instance.is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    StartedPlugin<StartArgs, Instance> for DynamicPlugin<StartArgs, Instance>
{
    fn stop(&mut self) {
        self.instance = None;
    }
    fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }
    fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().unwrap()
    }
}

struct PluginRecord<StartArgs: PluginStartArgs, Instance: PluginInstance>(
    Box<dyn DeclaredPlugin<StartArgs, Instance> + Send>,
);

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    PluginRecord<StartArgs, Instance>
{
    fn new<P: DeclaredPlugin<StartArgs, Instance> + Send + 'static>(plugin: P) -> Self {
        Self(Box::new(plugin))
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginInfo
    for PluginRecord<StartArgs, Instance>
{
    fn name(&self) -> &str {
        self.0.name()
    }
    fn path(&self) -> &str {
        self.0.path()
    }
    fn status(&self) -> PluginStatus {
        self.0.status()
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    DeclaredPlugin<StartArgs, Instance> for PluginRecord<StartArgs, Instance>
{
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

impl<
        StartArgs: PluginStartArgs + 'static,
        Instance: PluginInstance + 'static,
    > PluginsManager<StartArgs, Instance>
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
    pub fn add_static_plugin<
        P: Plugin<StartArgs = StartArgs, Instance = Instance> + Send + Sync,
    >(
        mut self,
    ) -> Self {
        let plugin_loader: StaticPlugin<StartArgs, Instance, P> = StaticPlugin::new();
        self.plugins.push(PluginRecord::new(plugin_loader));
        self
    }

    /// Add dynamic plugin to the manager by name, automatically prepending the default library prefix
    pub fn add_dynamic_plugin_by_name<S: Into<String>>(
        &mut self,
        name: S,
        plugin_name: &str,
    ) -> ZResult<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let plugin_name = format!("{}{}", self.default_lib_prefix, plugin_name);
        let libloader = self
            .loader
            .as_ref()
            .ok_or("Dynamic plugin loading is disabled")?
            .clone();
        let loader = DynamicPlugin::new(
            name.into(),
            DynamicPluginSource::ByName((libloader, plugin_name)),
        );
        self.plugins.push(PluginRecord::new(loader));
        Ok(self.plugins.last_mut().unwrap())
    }

    /// Add first available dynamic plugin from the list of paths to the plugin files
    pub fn add_dynamic_plugin_by_paths<S: Into<String>, P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: S,
        paths: &[P],
    ) -> ZResult<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let name = name.into();
        let paths = paths.iter().map(|p| p.as_ref().into()).collect();
        let loader = DynamicPlugin::new(name, DynamicPluginSource::ByPaths(paths));
        self.plugins.push(PluginRecord::new(loader));
        Ok(self.plugins.last_mut().unwrap())
    }

    fn get_plugin_index(&self, name: &str) -> Option<usize> {
        self.plugins.iter().position(|p| p.name() == name)
    }

    /// Lists all plugins
    pub fn plugins(&self) -> impl Iterator<Item = &dyn DeclaredPlugin<StartArgs, Instance>> + '_ {
        self.plugins
            .iter()
            .map(|p| p as &dyn DeclaredPlugin<StartArgs, Instance>)
    }

    /// Lists all plugins mutable
    pub fn plugins_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn DeclaredPlugin<StartArgs, Instance>> + '_ {
        self.plugins
            .iter_mut()
            .map(|p| p as &mut dyn DeclaredPlugin<StartArgs, Instance>)
    }

    /// Lists the loaded plugins
    pub fn loaded_plugins(
        &self,
    ) -> impl Iterator<Item = &dyn LoadedPlugin<StartArgs, Instance>> + '_ {
        self.plugins().filter_map(|p| p.loaded())
    }

    /// Lists the loaded plugins mutable
    pub fn loaded_plugins_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn LoadedPlugin<StartArgs, Instance>> + '_ {
        // self.plugins_mut().filter_map(|p| p.loaded_mut())
        self.plugins_mut().filter_map(|p| p.loaded_mut())
    }

    /// Lists the started plugins
    pub fn started_plugins(
        &self,
    ) -> impl Iterator<Item = &dyn StartedPlugin<StartArgs, Instance>> + '_ {
        self.loaded_plugins().filter_map(|p| p.started())
    }

    /// Lists the started plugins mutable
    pub fn started_plugins_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn StartedPlugin<StartArgs, Instance>> + '_ {
        self.loaded_plugins_mut().filter_map(|p| p.started_mut())
    }

    /// Returns single plugin record
    pub fn plugin(&self, name: &str) -> Option<&dyn DeclaredPlugin<StartArgs, Instance>> {
        let index = self.get_plugin_index(name)?;
        Some(&self.plugins[index])
    }

    /// Returns mutable plugin record
    pub fn plugin_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut dyn DeclaredPlugin<StartArgs, Instance>> {
        let index = self.get_plugin_index(name)?;
        Some(&mut self.plugins[index])
    }

    /// Returns loaded plugin record
    pub fn loaded_plugin(&self, name: &str) -> Option<&dyn LoadedPlugin<StartArgs, Instance>> {
        self.plugin(name)?.loaded()
    }

    /// Returns mutable loaded plugin record
    pub fn loaded_plugin_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        self.plugin_mut(name)?.loaded_mut()
    }

    /// Returns started plugin record
    pub fn started_plugin(&self, name: &str) -> Option<&dyn StartedPlugin<StartArgs, Instance>> {
        self.loaded_plugin(name)?.started()
    }

    /// Returns mutable started plugin record
    pub fn started_plugin_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut dyn StartedPlugin<StartArgs, Instance>> {
        self.loaded_plugin_mut(name)?.started_mut()
    }
}
