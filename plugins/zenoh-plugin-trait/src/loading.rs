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
use std::path::PathBuf;
use vtable::{Compatibility, PluginLoaderVersion, PluginVTable, PLUGIN_LOADER_VERSION};
use zenoh_result::{bail, ZResult};
use zenoh_util::LibLoader;

pub struct PluginRecord<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> {
    starter: Box<dyn PluginStarter<StartArgs, RunningPlugin> + Send + Sync>,
    running_plugin: Option<RunningPlugin>,
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion>
    PluginRecord<StartArgs, RunningPlugin>
{
    fn new<T: PluginStarter<StartArgs, RunningPlugin> + Send + Sync + 'static>(starter: T) -> Self {
        Self {
            starter: Box::new(starter),
            running_plugin: None,
        }
    }
    pub fn running(&self) -> Option<&dyn RunningPluginRecord<StartArgs, RunningPlugin>> {
        if self.running_plugin.is_some() {
            Some(self)
        } else {
            None
        }
    }
    pub fn running_mut(
        &mut self,
    ) -> Option<&mut dyn RunningPluginRecord<StartArgs, RunningPlugin>> {
        if self.running_plugin.is_some() {
            Some(self)
        } else {
            None
        }
    }
    pub fn start(
        &mut self,
        args: &StartArgs,
    ) -> ZResult<(bool, &mut dyn RunningPluginRecord<StartArgs, RunningPlugin>)> {
        let already_running = self.running_plugin.is_some();
        if !already_running {
            self.running_plugin = Some(self.starter.start(args)?);
        }
        Ok((already_running, self))
    }
    pub fn name(&self) -> &str {
        self.starter.name()
    }
    pub fn path(&self) -> &str {
        self.starter.path()
    }
    pub fn deletable(&self) -> bool {
        self.starter.deletable()
    }
}

pub trait RunningPluginRecord<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion>
{
    fn name(&self) -> &str;
    fn path(&self) -> &str;
    fn deletable(&self) -> bool;
    fn stop(&mut self);
    fn running(&self) -> &RunningPlugin;
    fn running_mut(&mut self) -> &mut RunningPlugin;
}

impl<StartArgs: CompatibilityVersion, RunningPligin: CompatibilityVersion>
    RunningPluginRecord<StartArgs, RunningPligin> for PluginRecord<StartArgs, RunningPligin>
{
    fn name(&self) -> &str {
        self.name()
    }
    fn path(&self) -> &str {
        self.path()
    }
    fn deletable(&self) -> bool {
        self.deletable()
    }
    fn stop(&mut self) {
        self.running_plugin = None;
    }
    fn running(&self) -> &RunningPligin {
        self.running_plugin.as_ref().unwrap()
    }
    fn running_mut(&mut self) -> &mut RunningPligin {
        self.running_plugin.as_mut().unwrap()
    }
}

/// A plugins manager that handles starting and stopping plugins.
/// Plugins can be loaded from shared libraries using [`Self::load_plugin_by_name`] or [`Self::load_plugin_by_paths`], or added directly from the binary if available using [`Self::add_static`].
pub struct PluginsManager<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> {
    default_lib_prefix: String,
    loader: Option<LibLoader>,
    plugins: Vec<PluginRecord<StartArgs, RunningPlugin>>,
}

impl<StartArgs: 'static + CompatibilityVersion, RunningPlugin: 'static + CompatibilityVersion>
    PluginsManager<StartArgs, RunningPlugin>
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
    pub fn add_static<
        P: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin> + Send + Sync,
    >(
        mut self,
    ) -> Self {
        let plugin_starter: StaticPlugin<P> = StaticPlugin::new();
        self.plugins.push(PluginRecord::new(plugin_starter));
        self
    }

    /// Returns plugin index by name
    fn get_plugin_index(&self, name: &str) -> Option<usize> {
        self.plugins.iter().position(|p| p.name() == name)
    }

    /// Returns plugin index by name or error if plugin not found
    fn get_plugin_index_err(&self, name: &str) -> ZResult<usize> {
        self.get_plugin_index(name)
            .ok_or_else(|| format!("Plugin `{}` not found", name).into())
    }

    /// Lists the loaded plugins
    pub fn plugins(&self) -> impl Iterator<Item = &PluginRecord<StartArgs, RunningPlugin>> + '_ {
        self.plugins.iter()
    }

    /// Lists the loaded plugins mutable
    pub fn plugins_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut PluginRecord<StartArgs, RunningPlugin>> + '_ {
        self.plugins.iter_mut()
    }

    /// Lists the running plugins
    pub fn running_plugins(
        &self,
    ) -> impl Iterator<Item = &dyn RunningPluginRecord<StartArgs, RunningPlugin>> + '_ {
        self.plugins().filter_map(|p| p.running())
    }

    /// Lists the running plugins mutable
    pub fn running_plugins_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut dyn RunningPluginRecord<StartArgs, RunningPlugin>> + '_ {
        self.plugins_mut().filter_map(|p| p.running_mut())
    }

    /// Returns plugin record
    pub fn plugin(&self, name: &str) -> ZResult<&PluginRecord<StartArgs, RunningPlugin>> {
        Ok(&self.plugins[self.get_plugin_index_err(name)?])
    }

    /// Returns mutable plugin record
    pub fn plugin_mut(
        &mut self,
        name: &str,
    ) -> ZResult<&mut PluginRecord<StartArgs, RunningPlugin>> {
        let index = self.get_plugin_index_err(name)?;
        Ok(&mut self.plugins[index])
    }

    /// Returns running plugin record
    pub fn running_plugin(
        &self,
        name: &str,
    ) -> ZResult<&dyn RunningPluginRecord<StartArgs, RunningPlugin>> {
        Ok(self
            .plugin(name)?
            .running()
            .ok_or_else(|| format!("Plugin `{}` is not running", name))?)
    }

    /// Returns mutable running plugin record
    pub fn running_plugin_mut(
        &mut self,
        name: &str,
    ) -> ZResult<&mut dyn RunningPluginRecord<StartArgs, RunningPlugin>> {
        Ok(self
            .plugin_mut(name)?
            .running_mut()
            .ok_or_else(|| format!("Plugin `{}` is not running", name))?)
    }

    fn load_plugin(
        name: &str,
        lib: Library,
        path: PathBuf,
    ) -> ZResult<DynamicPlugin<StartArgs, RunningPlugin>> {
        DynamicPlugin::new(name.into(), lib, path)
    }

    /// Tries to load a plugin with the name `defaukt_lib_prefix` + `backend_name` + `.so | .dll | .dylib`
    /// in lib_loader's search paths.
    /// Returns a tuple of (retval, plugin_record)
    /// where `retval`` is true if the plugin was successfully loaded, false if pluginw with this name it was already loaded
    pub fn load_plugin_by_backend_name<T: AsRef<str>, T1: AsRef<str>>(
        &mut self,
        name: T,
        backend_name: T1,
    ) -> ZResult<(bool, &mut PluginRecord<StartArgs, RunningPlugin>)> {
        let name = name.as_ref();
        if let Some(index) = self.get_plugin_index(name) {
            return Ok((false, &mut self.plugins[index]));
        }
        let backend_name = backend_name.as_ref();
        let (lib, p) = match &mut self.loader {
            Some(l) => unsafe { l.search_and_load(&format!("{}{}", &self.default_lib_prefix, &backend_name))? },
            None => bail!("Can't load dynamic plugin `{}`, as dynamic loading is not enabled for this plugin manager.", &name),
        };
        let plugin = match Self::load_plugin(name, lib, p.clone()) {
            Ok(p) => p,
            Err(e) => bail!("After loading `{:?}`: {}", &p, e),
        };
        self.plugins.push(PluginRecord::new(plugin));
        Ok((true, self.plugins.last_mut().unwrap()))
    }
    /// Tries to load a plugin from the list of path to plugin (absolute or relative to the current working directory)
    /// Returns a tuple of (retval, plugin_record)
    /// where `retval`` is true if the plugin was successfully loaded, false if pluginw with this name it was already loaded
    pub fn load_plugin_by_paths<T: AsRef<str>, P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: T,
        paths: &[P],
    ) -> ZResult<(bool, &mut PluginRecord<StartArgs, RunningPlugin>)> {
        let name = name.as_ref();
        if let Some(index) = self.get_plugin_index(name) {
            return Ok((false, &mut self.plugins[index]));
        }
        for path in paths {
            let path = path.as_ref();
            match unsafe { LibLoader::load_file(path) } {
                Ok((lib, p)) => {
                    let plugin = Self::load_plugin(name, lib, p)?;
                    self.plugins.push(PluginRecord::new(plugin));
                    return Ok((true, self.plugins.last_mut().unwrap()));
                }
                Err(e) => log::warn!("Plugin '{}' load fail at {}: {}", &name, path, e),
            }
        }
        bail!("Plugin '{}' not found in {:?}", name, &paths)
    }
}

pub trait PluginInfo {
    fn name(&self) -> &str;
    fn path(&self) -> &str;
    fn deletable(&self) -> bool;
}

trait PluginStarter<StartArgs, RunningPlugin>: PluginInfo {
    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin>;
    fn as_plugin_info(&self) -> &dyn PluginInfo;
}

struct StaticPlugin<P> {
    inner: std::marker::PhantomData<P>,
}

impl<P> StaticPlugin<P> {
    fn new() -> Self {
        StaticPlugin {
            inner: std::marker::PhantomData,
        }
    }
}

impl<StartArgs, RunningPlugin, P> PluginInfo for StaticPlugin<P>
where
    P: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin>,
{
    fn name(&self) -> &str {
        P::STATIC_NAME
    }
    fn path(&self) -> &str {
        "<statically_linked>"
    }
    fn deletable(&self) -> bool {
        false
    }
}

impl<StartArgs, RunningPlugin, P> PluginStarter<StartArgs, RunningPlugin> for StaticPlugin<P>
where
    P: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin>,
{
    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin> {
        P::start(P::STATIC_NAME, args)
    }
    fn as_plugin_info(&self) -> &dyn PluginInfo {
        self
    }
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> PluginInfo
    for DynamicPlugin<StartArgs, RunningPlugin>
{
    fn name(&self) -> &str {
        &self.name
    }
    fn path(&self) -> &str {
        self.path.to_str().unwrap()
    }
    fn deletable(&self) -> bool {
        true
    }
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion>
    PluginStarter<StartArgs, RunningPlugin> for DynamicPlugin<StartArgs, RunningPlugin>
{
    fn start(&self, args: &StartArgs) -> ZResult<RunningPlugin> {
        (self.vtable.start)(self.name(), args)
    }
    fn as_plugin_info(&self) -> &dyn PluginInfo {
        self
    }
}

struct 

pub struct DynamicPlugin<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> {
    name: String,
    lib: Option<Library>,
    vtable: Option<PluginVTable<StartArgs, RunningPlugin>>,
    path: Option<PathBuf>,
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion>
    DynamicPlugin<StartArgs, RunningPlugin>
{
    pub fn new(name: String, lib: Library, path: PathBuf) -> Self {
        Self {
            name,
            lib: None,
            vtable: None,
            path: None,
        }
    }

    fn get_vtable(path: &PathBuf) -> ZResult<PluginVTable<StartArgs, RunningPlugin>> {
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
        let host_compatibility_record = Compatibility::new::<StartArgs, RunningPlugin>();
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
            unsafe { lib.get::<fn() -> PluginVTable<StartArgs, RunningPlugin>>(b"load_plugin")? };
        let vtable = load_plugin();
        Ok(vtable)
    }

    /// Tries to load a plugin with the name `libname` + `.so | .dll | .dylib`
    /// in lib_loader's search paths.
    /// Returns a tuple of (retval, plugin_record)
    /// where `retval`` is true if the plugin was successfully loaded, false if pluginw with this name it was already loaded
    fn load_by_libname(
        &self,
        libloader: &LibLoader,
        libname: &str,
    ) -> ZResult<Library, PathBuf> {
        let (lib, p) = unsafe { libloader.search_and_load(libname)? };

        let plugin = match Self::load_plugin(name, lib, p.clone()) {
            Ok(p) => p,
            Err(e) => bail!("After loading `{:?}`: {}", &p, e),
        };
        self.plugins.push(PluginRecord::new(plugin));
        Ok((true, self.plugins.last_mut().unwrap()))
    }
    /// Tries to load a plugin from the list of path to plugin (absolute or relative to the current working directory)
    /// Returns a tuple of (retval, plugin_record)
    /// where `retval`` is true if the plugin was successfully loaded, false if pluginw with this name it was already loaded
    pub fn load_plugin_by_paths<T: AsRef<str>, P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: T,
        paths: &[P],
    ) -> ZResult<(bool, &mut PluginRecord<StartArgs, RunningPlugin>)> {
        let name = name.as_ref();
        if let Some(index) = self.get_plugin_index(name) {
            return Ok((false, &mut self.plugins[index]));
        }
        for path in paths {
            let path = path.as_ref();
            match unsafe { LibLoader::load_file(path) } {
                Ok((lib, p)) => {
                    let plugin = Self::load_plugin(name, lib, p)?;
                    self.plugins.push(PluginRecord::new(plugin));
                    return Ok((true, self.plugins.last_mut().unwrap()));
                }
                Err(e) => log::warn!("Plugin '{}' load fail at {}: {}", &name, path, e),
            }
        }
        bail!("Plugin '{}' not found in {:?}", name, &paths)
    }
}

}
