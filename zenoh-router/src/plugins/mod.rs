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
use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::path::{Path, PathBuf};
use zenoh_util::zconfigurable;

zconfigurable! {
    static ref PLUGIN_PREFIX: String = "zplugin_".to_string();
    static ref PLUGIN_FILE_PREFIX: String = format!("{}{}", DLL_PREFIX, *PLUGIN_PREFIX);
    static ref PLUGIN_FILE_EXTENSION: String = DLL_SUFFIX.to_string();
}

pub struct PluginsMgr {
    pub search_paths: Vec<PathBuf>,
    pub plugins: Vec<Plugin>,
}

impl PluginsMgr {
    pub fn new() -> PluginsMgr {
        let mut search_paths: Vec<PathBuf> = vec![];
        if let Some(dir) = Self::home_dir() {
            let mut dir = dir;
            dir.push(".zenoh/lib");
            search_paths.push(dir)
        };
        if let Some(dir) = Self::exe_parent_dir() {
            search_paths.push(dir)
        };
        if let Some(dir) = Self::current_dir() {
            search_paths.push(dir)
        };
        let usr_local_lib = PathBuf::from("/usr/local/lib");
        if usr_local_lib.is_dir() {
            search_paths.push(usr_local_lib);
        }
        let usr_lib = PathBuf::from("/usr/lib");
        if usr_lib.is_dir() {
            search_paths.push(usr_lib);
        }

        // let plugins: Vec<Plugin> = vec![];

        PluginsMgr {
            search_paths,
            plugins: vec![],
        }
    }

    fn exe_parent_dir() -> Option<PathBuf> {
        match std::env::args().next() {
            Some(path) => Path::new(&path).parent().map(|p| p.to_path_buf()),
            None => {
                warn!("This executable name was not found in args. Can't find it's parent to search plugins.");
                None
            }
        }
    }

    fn home_dir() -> Option<PathBuf> {
        match std::env::var_os("HOME") {
            Some(path) => Some(PathBuf::from(path)),
            None => {
                let tilde = PathBuf::from("~/");
                if tilde.is_dir() {
                    Some(tilde)
                } else {
                    warn!("$HOME directory not defined. Can't use it to search plugins.");
                    None
                }
            }
        }
    }

    fn current_dir() -> Option<PathBuf> {
        match std::env::current_dir() {
            Ok(path) => Some(path),
            Err(err) => {
                warn!(
                    "Invalid current dir: '{}'. Can't use it to search plugins.",
                    err
                );
                None
            }
        }
    }

    pub async fn search_and_load_plugins(&mut self) {
        log::debug!("Search for plugins to load in {:?}", self.search_paths);
        for dir in &self.search_paths {
            trace!("Search plugins in dir {:?} ", dir);
            match dir.read_dir() {
                Ok(read_dir) => {
                    for entry in read_dir {
                        if let Ok(entry) = entry {
                            if let Ok(filename) = entry.file_name().into_string() {
                                if filename.starts_with(&*PLUGIN_FILE_PREFIX)
                                    && filename.ends_with(&*PLUGIN_FILE_EXTENSION)
                                {
                                    let name = &filename[(PLUGIN_FILE_PREFIX.len())
                                        ..(filename.len() - PLUGIN_FILE_EXTENSION.len())];
                                    let path = entry.path();
                                    if !self.plugins.iter().any(|plugin| plugin.name == name) {
                                        match Plugin::load(name, path) {
                                            Ok(plugin) => self.plugins.push(plugin),
                                            Err(err) => warn!("{}", err),
                                        }
                                    } else {
                                        log::debug!(
                                            "Do not load plugin {} from {:?} : already loaded.",
                                            name,
                                            path
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => debug!(
                    "Failed to read in directory {:?} ({}). Can't use it to search plugins.",
                    dir, err
                ),
            }
        }
    }

    pub fn load_plugins(&mut self, paths: Vec<String>) {
        log::debug!("Plugins to load: {:?}", paths);
        paths.iter().for_each(|path| {
            let file = PathBuf::from(path);
            if !file.exists() {
                panic!(format!("Plugin file '{}' doesn't exist", path));
            }
            if !file.is_file() {
                panic!(format!("Path to plugin '{}' doesn't point to a file", path));
            }
            let filename = file.file_name().unwrap().to_str().unwrap();
            let name = if filename.starts_with(&*PLUGIN_FILE_PREFIX)
                && filename.ends_with(&*PLUGIN_FILE_EXTENSION)
            {
                &filename
                    [(PLUGIN_FILE_PREFIX.len())..(filename.len() - PLUGIN_FILE_EXTENSION.len())]
            } else {
                filename
            };
            match Plugin::load(name, file.clone()) {
                Ok(plugin) => self.plugins.push(plugin),
                Err(err) => panic!(err),
            }
        });
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
        PluginsMgr::new()
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
    pub fn load(name: &str, path: PathBuf) -> Result<Plugin, String> {
        debug!("Load plugin {} from {:?}", name, path);
        match Library::new(path.as_os_str()) {
            Ok(lib) => {
                unsafe {
                    // check if it has the expected operations
                    // NOTE: we don't save the symbols here
                    if lib.get::<GetArgsFn>(GET_ARGS_FN_NAME).is_err() {
                        return Err(format!("Failed to load plugin from {}: it lacks a get_expected_args() operation", path.to_string_lossy()));
                    };
                    if lib.get::<StartFn>(START_FN_NAME).is_err() {
                        return Err(format!(
                            "Failed to load plugin from {}: it lacks a start() operation",
                            path.to_string_lossy()
                        ));
                    };
                }
                Ok(Plugin {
                    name: name.to_string(),
                    path,
                    lib,
                })
            }
            Err(err) => Err(format!(
                "Failed to load plugin from {}: {}",
                path.to_string_lossy(),
                err
            )),
        }
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
