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
use std::path::{Path, PathBuf};

use libloading::Library;
use zenoh_result::{bail, zerror, ZResult};
use zenoh_util::LibLoader;

use crate::*;

/// This enum contains information where to load the plugin from.
pub enum DynamicPluginSource {
    /// Load plugin with the name in String + `.so | .dll | .dylib`
    /// in LibLoader's search paths.
    ByName((LibLoader, String)),
    /// Load first available plugin from the list of path to plugin files (absolute or relative to the current working directory)
    ByPaths(Vec<String>),
}

impl DynamicPluginSource {
    fn load(&self) -> ZResult<Option<(Library, PathBuf)>> {
        match self {
            DynamicPluginSource::ByName((libloader, name)) => unsafe {
                libloader.search_and_load(name)
            },
            DynamicPluginSource::ByPaths(paths) => {
                for path in paths {
                    match unsafe { LibLoader::load_file(path) } {
                        Ok((l, p)) => return Ok(Some((l, p))),
                        Err(e) => tracing::debug!("Attempt to load {} failed: {}", path, e),
                    }
                }
                Err(zerror!("Plugin not found in {:?}", &paths).into())
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
        tracing::debug!("Loading plugin {}", path.to_str().unwrap(),);
        let get_plugin_loader_version =
            unsafe { lib.get::<fn() -> PluginLoaderVersion>(b"get_plugin_loader_version")? };
        let plugin_loader_version = get_plugin_loader_version();
        tracing::debug!("Plugin loader version: {}", &plugin_loader_version);
        if plugin_loader_version != PLUGIN_LOADER_VERSION {
            bail!(
                "Plugin loader version mismatch: host = {}, plugin = {}",
                PLUGIN_LOADER_VERSION,
                plugin_loader_version
            );
        }
        let get_compatibility = unsafe { lib.get::<fn() -> Compatibility>(b"get_compatibility")? };
        let plugin_compatibility_record = get_compatibility();
        let host_compatibility_record =
            Compatibility::new(StartArgs::struct_version(), StartArgs::struct_features());
        tracing::debug!(
            "Plugin compatibility record: {:?}",
            &plugin_compatibility_record
        );
        if let Err(e) = host_compatibility_record.check(&plugin_compatibility_record) {
            bail!("Plugin compatibility mismatch:\n {}", e);
        }
        let load_plugin =
            unsafe { lib.get::<fn() -> PluginVTable<StartArgs, Instance>>(b"load_plugin")? };

        Ok(load_plugin())
    }
    fn new(lib: Library, path: PathBuf) -> ZResult<Self> {
        let vtable = Self::get_vtable(&lib, &path)
            .map_err(|e| format!("Error loading {}: {}", path.to_str().unwrap(), e))?;
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

pub struct DynamicPlugin<StartArgs, Instance> {
    name: String,
    id: String,
    required: bool,
    report: PluginReport,
    source: DynamicPluginSource,
    starter: Option<DynamicPluginStarter<StartArgs, Instance>>,
    instance: Option<Instance>,
}

impl<StartArgs, Instance> DynamicPlugin<StartArgs, Instance> {
    pub fn new(name: String, id: String, source: DynamicPluginSource, required: bool) -> Self {
        Self {
            name,
            id,
            required,
            report: PluginReport::new(),
            source,
            starter: None,
            instance: None,
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> PluginStatus
    for DynamicPlugin<StartArgs, Instance>
{
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn version(&self) -> Option<&str> {
        self.starter.as_ref().map(|v| v.vtable.plugin_version)
    }
    fn long_version(&self) -> Option<&str> {
        self.starter.as_ref().map(|v| v.vtable.plugin_long_version)
    }
    fn path(&self) -> &str {
        if let Some(starter) = &self.starter {
            starter.path()
        } else {
            "__not_loaded__"
        }
    }
    fn state(&self) -> PluginState {
        if self.starter.is_some() {
            if self.instance.is_some() {
                PluginState::Started
            } else {
                PluginState::Loaded
            }
        } else {
            PluginState::Declared
        }
    }
    fn report(&self) -> PluginReport {
        if let Some(instance) = &self.instance {
            instance.report()
        } else {
            self.report.clone()
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> DeclaredPlugin<StartArgs, Instance>
    for DynamicPlugin<StartArgs, Instance>
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn load(&mut self) -> ZResult<Option<&mut dyn LoadedPlugin<StartArgs, Instance>>> {
        if self.starter.is_none() {
            let Some((lib, path)) = self.source.load().add_error(&mut self.report)? else {
                tracing::warn!(
                    "Plugin `{}` will not be loaded as plugin loading is disabled",
                    self.name
                );
                return Ok(None);
            };
            let starter = DynamicPluginStarter::new(lib, path).add_error(&mut self.report)?;
            tracing::debug!("Plugin {} loaded from {}", self.name, starter.path());
            self.starter = Some(starter);
        } else {
            tracing::warn!("Plugin `{}` already loaded", self.name);
        }
        Ok(Some(self))
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

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> LoadedPlugin<StartArgs, Instance>
    for DynamicPlugin<StartArgs, Instance>
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn required(&self) -> bool {
        self.required
    }
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>> {
        let starter = self
            .starter
            .as_ref()
            .ok_or_else(|| format!("Plugin `{}` not loaded", self.name))
            .add_error(&mut self.report)?;
        let already_started = self.instance.is_some();
        if !already_started {
            let instance = starter.start(self.id(), args).add_error(&mut self.report)?;
            tracing::debug!("Plugin `{}` started", self.name);
            self.instance = Some(instance);
        } else {
            tracing::warn!("Plugin `{}` already started", self.name);
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

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> StartedPlugin<StartArgs, Instance>
    for DynamicPlugin<StartArgs, Instance>
{
    fn as_status(&self) -> &dyn PluginStatus {
        self
    }
    fn stop(&mut self) {
        tracing::debug!("Plugin `{}` stopped", self.name);
        self.report.clear();
        self.instance = None;
    }
    fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }
    fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().unwrap()
    }
}
