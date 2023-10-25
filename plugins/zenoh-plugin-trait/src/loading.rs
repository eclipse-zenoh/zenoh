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

struct PluginInstance<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> {
    starter: Box<dyn PluginStarter<StartArgs, RunningPlugin> + Send + Sync>,
    running_plugin: Option<RunningPlugin>,
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> PluginInfo
    for PluginInstance<StartArgs, RunningPlugin>
{
    fn name(&self) -> &str {
        self.starter.name()
    }
    fn path(&self) -> &str {
        self.starter.path()
    }
    fn deletable(&self) -> bool {
        self.starter.deletable()
    }
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion>
    PluginInstance<StartArgs, RunningPlugin>
{
    fn new<T: PluginStarter<StartArgs, RunningPlugin> + Send + Sync + 'static>(starter: T) -> Self {
        Self {
            starter: Box::new(starter),
            running_plugin: None,
        }
    }
    fn get_running_plugin(&self) -> Option<&RunningPlugin> {
        self.running_plugin.as_ref()
    }
    fn stop(&mut self) -> bool {
        if self.running_plugin.is_some() {
            self.running_plugin = None;
            true
        } else {
            false
        }
    }
    fn start(&mut self, args: &StartArgs) -> ZResult<bool> {
        if self.running_plugin.is_some() {
            return Ok(false);
        }
        let plugin = self.starter.start(args)?;
        self.running_plugin = Some(plugin);
        Ok(true)
    }
    fn as_plugin_info(&self) -> &dyn PluginInfo {
        self
    }
}

/// A plugins manager that handles starting and stopping plugins.
/// Plugins can be loaded from shared libraries using [`Self::load_plugin_by_name`] or [`Self::load_plugin_by_paths`], or added directly from the binary if available using [`Self::add_static`].
pub struct PluginsManager<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> {
    default_lib_prefix: String,
    loader: Option<LibLoader>,
    plugins: Vec<PluginInstance<StartArgs, RunningPlugin>>,
}

/// Unique identifier of plugin in plugin manager. Actually is just an index in the plugins list.
/// It's guaranteed that if intex is obtained from plugin manager, it's valid for this plugin manager.
/// (at least at this moment, when there is no pluing unload support).
/// Using it instead of plugin name allows to avoid checking for ``Option`` every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginIndex(usize);

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
        self.plugins.push(PluginInstance::new(plugin_starter));
        self
    }

    /// Returns `true` if the plugin with the given index exists in the manager.
    pub fn check_plugin_index(&self, index: PluginIndex) -> bool {
        index.0 < self.plugins.len()
    }

    /// Returns plugin index by name
    pub fn get_plugin_index(&self, name: &str) -> Option<PluginIndex> {
        self.plugins
            .iter()
            .position(|p| p.name() == name)
            .map(PluginIndex)
    }

    /// Returns plugin index by name or error if plugin not found
    pub fn get_plugin_index_err(&self, name: &str) -> ZResult<PluginIndex> {
        self.get_plugin_index(name)
            .ok_or_else(|| format!("Plugin `{}` not found", name).into())
    }

    /// Starts plugin.
    /// Returns
    /// Ok(true) => plugin was successfully started
    /// Ok(false) => plugin was already running
    /// Err(e) => starting the plugin failed due to `e`
    pub fn start(&mut self, index: PluginIndex, args: &StartArgs) -> ZResult<bool> {
        let instance = &mut self.plugins[index.0];
        let already_started = instance.start(args)?;
        Ok(already_started)
    }

    /// Stops `plugin`, returning `true` if it was indeed running.
    pub fn stop(&mut self, index: PluginIndex) -> ZResult<bool> {
        let instance = &mut self.plugins[index.0];
        let was_running = instance.stop();
        Ok(was_running)
    }
    /// Lists the loaded plugins
    pub fn plugins(&self) -> impl Iterator<Item = PluginIndex> + '_ {
        self.plugins.iter().enumerate().map(|(i, _)| PluginIndex(i))
    }
    /// Lists the loaded plugins
    pub fn running_plugins(&self) -> impl Iterator<Item = PluginIndex> + '_ {
        self.plugins.iter().enumerate().filter_map(|(i, p)| {
            if p.get_running_plugin().is_some() {
                Some(PluginIndex(i))
            } else {
                None
            }
        })
    }
    /// Returns running plugin interface of plugin or none if plugin is stopped
    pub fn running_plugin(&self, index: PluginIndex) -> Option<&RunningPlugin> {
        self.plugins[index.0].get_running_plugin()
    }
    /// Returns plugin information
    pub fn plugin(&self, index: PluginIndex) -> &dyn PluginInfo {
        self.plugins[index.0].as_plugin_info()
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
    pub fn load_plugin_by_backend_name<T: AsRef<str>, T1: AsRef<str>>(
        &mut self,
        name: T,
        backend_name: T1,
    ) -> ZResult<PluginIndex> {
        let name = name.as_ref();
        if self.get_plugin_index(name).is_some() {
            bail!("Plugin `{}` already loaded", name);
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
        self.plugins.push(PluginInstance::new(plugin));
        Ok(PluginIndex(self.plugins.len() - 1))
    }
    /// Tries to load a plugin from the list of path to plugin (absolute or relative to the current working directory)
    pub fn load_plugin_by_paths<T: AsRef<str>, P: AsRef<str> + std::fmt::Debug>(
        &mut self,
        name: T,
        paths: &[P],
    ) -> ZResult<PluginIndex> {
        let name = name.as_ref();
        if self.get_plugin_index(name).is_some() {
            bail!("Plugin `{}` already loaded", name);
        }
        for path in paths {
            let path = path.as_ref();
            match unsafe { LibLoader::load_file(path) } {
                Ok((lib, p)) => {
                    let plugin = Self::load_plugin(name, lib, p)?;
                    self.plugins.push(PluginInstance::new(plugin));
                    return Ok(PluginIndex(self.plugins.len() - 1));
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

pub struct DynamicPlugin<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion> {
    _lib: Library,
    vtable: PluginVTable<StartArgs, RunningPlugin>,
    pub name: String,
    pub path: PathBuf,
}

impl<StartArgs: CompatibilityVersion, RunningPlugin: CompatibilityVersion>
    DynamicPlugin<StartArgs, RunningPlugin>
{
    fn new(name: String, lib: Library, path: PathBuf) -> ZResult<Self> {
        let get_plugin_loader_version =
            unsafe { lib.get::<fn() -> PluginLoaderVersion>(b"get_plugin_loader_version")? };
        let plugin_loader_version = get_plugin_loader_version();
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
        if !plugin_compatibility_record.are_compatible(&host_compatibility_record) {
            bail!(
                "Plugin compatibility mismatch:\nhost = {:?}\nplugin = {:?}\n",
                host_compatibility_record,
                plugin_compatibility_record
            );
        }

        // TODO: check loader version and compatibility
        let load_plugin =
            unsafe { lib.get::<fn() -> PluginVTable<StartArgs, RunningPlugin>>(b"load_plugin")? };
        let vtable = load_plugin();
        Ok(Self {
            _lib: lib,
            vtable,
            name,
            path,
        })
    }
}
