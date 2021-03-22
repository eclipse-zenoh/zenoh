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
use super::runtime::Runtime;
use clap::{Arg, ArgMatches};
use libloading::{Library, Symbol};
use log::{debug, trace, warn};
use std::path::PathBuf;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::{zconfigurable, zerror, LibLoader};

zconfigurable! {
    static ref PLUGIN_PREFIX: String = "zplugin_".to_string();
}

pub struct PluginsMgr {
    pub lib_loader: LibLoader,
    pub plugins: Vec<Plugin>,
}

impl PluginsMgr {
    pub fn new(lib_loader: LibLoader) -> PluginsMgr {
        PluginsMgr {
            lib_loader,
            plugins: vec![],
        }
    }

    pub async fn search_and_load_plugins(&mut self) {
        let libs = unsafe { self.lib_loader.load_all_with_prefix(Some(&*PLUGIN_PREFIX)) };
        for lib in libs {
            match Plugin::new(lib.0, lib.1, lib.2) {
                Ok(plugin) => {
                    debug!(
                        "Plugin {} loaded from {}",
                        plugin.name,
                        plugin.path.display()
                    );
                    self.plugins.push(plugin);
                }
                Err(err) => warn!("{}", err),
            }
        }
    }

    pub fn load_plugins(&mut self, paths: Vec<String>) -> ZResult<()> {
        log::debug!("Plugins to load: {:?}", paths);
        for path in paths {
            let (lib, p) = unsafe { LibLoader::load_file(&path)? };
            let filename = p.file_name().unwrap().to_str().unwrap();
            let prefix = format!("{}{}", *zenoh_util::LIB_PREFIX, *PLUGIN_PREFIX);
            let suffix = &*zenoh_util::LIB_SUFFIX;
            let name = if filename.starts_with(&prefix) && path.ends_with(suffix) {
                filename[(prefix.len())..(filename.len() - suffix.len())].to_string()
            } else {
                filename.to_string()
            };
            let plugin = Plugin::new(lib, p, name)?;
            debug!(
                "Plugin {} loaded from {}",
                plugin.name,
                plugin.path.display()
            );
            self.plugins.push(plugin);
        }
        Ok(())
    }

    pub fn get_plugins_args<'a, 'b>(&self) -> Vec<Arg<'a, 'b>> {
        let mut result: Vec<Arg<'a, 'b>> = vec![];
        for plugin in &self.plugins {
            result.append(&mut plugin.get_expected_args());
        }
        result
    }

    pub async fn start_plugins(&self, runtime: &Runtime, args: &ArgMatches<'_>) {
        for plugin in &self.plugins {
            plugin.start(runtime.clone(), args);
        }
    }
}

impl Default for PluginsMgr {
    #[inline]
    fn default() -> PluginsMgr {
        PluginsMgr::new(LibLoader::default())
    }
}

pub struct Plugin {
    pub name: String,
    pub path: PathBuf,
    lib: Library,
}

const START_FN_NAME: &[u8; 6] = b"start\0";
const GET_ARGS_FN_NAME: &[u8; 18] = b"get_expected_args\0";

type StartFn<'lib> = Symbol<'lib, unsafe extern "C" fn(Runtime, &ArgMatches)>;
type GetArgsFn<'lib, 'a, 'b> = Symbol<'lib, unsafe extern "C" fn() -> Vec<Arg<'a, 'b>>>;

impl Plugin {
    fn new(lib: Library, path: PathBuf, name: String) -> ZResult<Plugin> {
        unsafe {
            // check if it has the expected operations
            // NOTE: we don't save the symbols here
            if lib.get::<GetArgsFn>(GET_ARGS_FN_NAME).is_err() {
                return zerror!(ZErrorKind::Other {
                    descr: format!(
                        "Failed to load plugin from {}: it lacks a get_expected_args() operation",
                        path.to_string_lossy()
                    )
                });
            };
            if lib.get::<StartFn>(START_FN_NAME).is_err() {
                return zerror!(ZErrorKind::Other {
                    descr: format!(
                        "Failed to load plugin from {}: it lacks a start() operation",
                        path.to_string_lossy()
                    )
                });
            };
        }
        Ok(Plugin { name, path, lib })
    }

    pub fn get_expected_args<'a, 'b>(&self) -> Vec<Arg<'a, 'b>> {
        unsafe {
            trace!("Call get_expected_args() of plugin {}", self.name);
            let get_expected_args: GetArgsFn = self.lib.get(GET_ARGS_FN_NAME).unwrap();
            get_expected_args()
        }
    }

    pub fn start(&self, runtime: Runtime, args: &ArgMatches<'_>) {
        unsafe {
            debug!("Start plugin {}", self.name);
            let start: StartFn = self.lib.get(START_FN_NAME).unwrap();
            start(runtime, args)
        }
    }
}
