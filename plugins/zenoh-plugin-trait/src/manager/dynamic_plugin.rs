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
use std::{path::{PathBuf, Path}, borrow::Cow};

use libloading::Library;
use zenoh_keyexpr::keyexpr;
use zenoh_result::{ZResult, bail};
use zenoh_util::LibLoader;


/// This enum contains information where to load the plugin from.
pub enum DynamicPluginSource {
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
    plugin_version: Cow<'static, keyexpr>,
    vtable: PluginVTable<StartArgs, Instance>,
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance>
    DynamicPluginStarter<StartArgs, Instance>
{
    fn new(lib: Library, path: &Path) -> ZResult<Self> {
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
        let host_compatibility_record = Compatibility::with_plugin_version_keyexpr::<StartArgs, Instance>("**".try_into()? );
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
        Ok(Self {
            _lib: lib,
            path: path.to_path_buf(),
            plugin_version: plugin_compatibility_record.plugin_version(),
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
    report: PluginReport,
    source: DynamicPluginSource,
    starter: Option<DynamicPluginStarter<StartArgs, Instance>>,
    instance: Option<Instance>,
}

impl<StartArgs, Instance> DynamicPlugin<StartArgs, Instance> {
    pub fn new(name: String, source: DynamicPluginSource) -> Self {
        Self {
            name,
            report: PluginReport::new(),
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
    fn plugin_version(&self) -> Option<&str> {
        self.starter.as_ref().map(|v| v.plugin_version)
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
            report: self.report.clone(), // TODO: request condition from started plugin
        }
    }
}

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> DeclaredPlugin<StartArgs, Instance>
    for DynamicPlugin<StartArgs, Instance>
{
    fn load(&mut self) -> ZResult<&mut dyn LoadedPlugin<StartArgs, Instance>> {
        if self.starter.is_none() {
            let (lib, path) = self.source.load().add_error(&mut self.report)?;
            let starter = DynamicPluginStarter::new(lib, path).add_error(&mut self.report)?;
            log::debug!("Plugin {} loaded from {}", self.name, starter.path());
            self.starter = Some(starter);
        } else {
            log::warn!("Plugin `{}` already loaded", self.name);
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

impl<StartArgs: PluginStartArgs, Instance: PluginInstance> LoadedPlugin<StartArgs, Instance>
    for DynamicPlugin<StartArgs, Instance>
{
    fn start(&mut self, args: &StartArgs) -> ZResult<&mut dyn StartedPlugin<StartArgs, Instance>> {
        let starter = self
            .starter
            .as_ref()
            .ok_or_else(|| format!("Plugin `{}` not loaded", self.name))
            .add_error(&mut self.report)?;
        let already_started = self.instance.is_some();
        if !already_started {
            let instance = starter
                .start(self.name(), args)
                .add_error(&mut self.report)?;
            log::debug!("Plugin `{}` started", self.name);
            self.instance = Some(instance);
        } else {
            log::warn!("Plugin `{}` already started", self.name);
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
    fn stop(&mut self) {
        log::debug!("Plugin `{}` stopped", self.name);
        self.instance = None;
    }
    fn instance(&self) -> &Instance {
        self.instance.as_ref().unwrap()
    }
    fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().unwrap()
    }
}

