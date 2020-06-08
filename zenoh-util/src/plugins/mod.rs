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
use std::path::{Path, PathBuf};
use log::{debug, warn};
use libloading::{Library, Symbol};
use std::future::Future;
use std::pin::Pin;


pub struct PluginsMgr {
    search_paths: Vec<PathBuf>,
    plugins: Vec<Plugin>
}


impl PluginsMgr {

    pub fn new() -> PluginsMgr {
        let mut search_paths: Vec<PathBuf> = vec![];
        if let Some(dir) = Self::home_dir() { 
            let mut dir = dir;
            dir.push(".zenoh/lib"); search_paths.push(dir)
        };
        if let Some(dir) = Self::exe_parent_dir() { search_paths.push(dir) };
        if let Some(dir) = Self::current_dir() { search_paths.push(dir) };
        let usr_local_lib = PathBuf::from("/usr/local/lib");
        if usr_local_lib.is_dir() {
            search_paths.push(usr_local_lib);
        }
        let usr_lib = PathBuf::from("/usr/lib");
        if usr_lib.is_dir() {
            search_paths.push(usr_lib);
        }
        
        // let plugins: Vec<Plugin> = vec![];

        PluginsMgr { search_paths, plugins: vec![] }
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
                warn!("Invalid current dir: '{}'. Can't use it to search plugins.", err);
                None
            }
        }
    }

    pub async fn search_and_load_plugins(&mut self, prefix: &str, extension: &str) {
        for dir in &self.search_paths {
            debug!("Search plugins in dir {:?} ", dir);
            match dir.read_dir() {
                Ok(read_dir) =>
                    for entry in read_dir {
                        if let Ok(entry) = entry {
                            if let Ok(filename) = entry.file_name().into_string() {
                                if filename.starts_with(prefix) && filename.ends_with(extension) {
                                    let name = &filename[(prefix.len())..(filename.len()-extension.len())];
                                    let path = entry.path();
                                    let args: Vec<String> = vec![];  // TODO
                                    debug!("Load plugin {} from {:?} with args: {:?}", name, path, args);
                                    match Library::new(path.as_os_str()) {
                                        Ok(lib) => {
                                            unsafe {
                                                debug!("Call start() of plugin {}", name);
                                                let start: Symbol<unsafe extern fn() -> Pin<Box<dyn Future<Output=()>>>> =
                                                    lib.get(b"start\0").unwrap();
                                                start().as_mut().await;
                                            }
                                            let plugin = Plugin { _name: name.to_string(), _path: path, _args: args, _lib:lib };
                                            self.plugins.push(plugin);
                                        }
                                        Err(err) => warn!("Failed to load plugin from {}: {}", path.to_string_lossy(), err)
                                    }
                                }
                            }
                        }
                    }
                Err(err) => debug!("Failed to read in directory {:?} ({}). Can't use it to search plugins.", dir, err)
            }
        }
    }
}

impl Default for PluginsMgr {
    #[inline]
    fn default() -> PluginsMgr {
        PluginsMgr::new()
    }
}

struct Plugin {
    _name: String,
    _path: PathBuf,
    _args: Vec<String>,
    _lib: Library
}