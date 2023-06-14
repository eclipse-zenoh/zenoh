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
use libloading::Library;
use log::{debug, warn};
use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::ffi::OsString;
use std::ops::Deref;
use std::path::PathBuf;
use zenoh_core::zconfigurable;
use zenoh_result::{bail, ZResult};

zconfigurable! {
    /// The libraries prefix for the current platform (usually: `"lib"`)
    pub static ref LIB_PREFIX: String = DLL_PREFIX.to_string();
    /// The libraries suffix for the current platform (`".dll"` or `".so"` or `".dylib"`...)
    pub static ref LIB_SUFFIX: String = DLL_SUFFIX.to_string();
    /// The default list of paths where to search for libraries to load
    pub static ref LIB_DEFAULT_SEARCH_PATHS: String = "/usr/local/lib:/usr/lib:~/.zenoh/lib:.".to_string();
}

/// LibLoader allows search for librairies and to load them.
#[derive(Clone, Debug)]
pub struct LibLoader {
    search_paths: Vec<PathBuf>,
}

impl LibLoader {
    /// Returns the list of search paths used by `LibLoader::default()`
    pub fn default_search_paths() -> &'static str {
        &LIB_DEFAULT_SEARCH_PATHS
    }

    /// Creates a new [LibLoader] with a set of paths where the libraries will be searched for.
    /// If `exe_parent_dir`is true, the parent directory of the current executable is also added
    /// to the set of paths for search.
    pub fn new<S>(search_dirs: &[S], exe_parent_dir: bool) -> LibLoader
    where
        S: AsRef<str>,
    {
        let mut search_paths: Vec<PathBuf> = vec![];
        for s in search_dirs {
            match shellexpand::full(s) {
                Ok(cow_str) => match PathBuf::from(&*cow_str).canonicalize() {
                    Ok(path) => search_paths.push(path),
                    Err(err) => debug!("Cannot search for libraries in {}: {}", cow_str, err),
                },
                Err(err) => warn!("Cannot search for libraries in '{}': {} ", s.as_ref(), err),
            }
        }
        Self::_new(search_paths, exe_parent_dir)
    }
    fn _new(mut search_paths: Vec<PathBuf>, exe_parent_dir: bool) -> Self {
        if exe_parent_dir {
            match std::env::current_exe() {
                Ok(path) => match path.parent() {
                    Some(p) => if p.is_dir() {
                        search_paths.push(p.canonicalize().unwrap())
                    },
                    None => warn!("Can't search for plugins in executable parent directory: no parent directory for {}.", path.to_string_lossy()),
                },
                Err(e) => warn!("Can't search for plugins in executable parent directory: {}.", e),
            }
        }

        LibLoader { search_paths }
    }

    /// Return the list of search paths used by this [LibLoader]
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Load a library from the specified path.
    ///
    /// # Safety
    ///
    /// This function calls [libloading::Library::new()](https://docs.rs/libloading/0.7.0/libloading/struct.Library.html#method.new)
    /// which is unsafe.
    pub unsafe fn load_file(path: &str) -> ZResult<(Library, PathBuf)> {
        let path = Self::str_to_canonical_path(path)?;

        if !path.exists() {
            bail!("Library file '{}' doesn't exist", path.display())
        } else if !path.is_file() {
            bail!("Library file '{}' is not a file", path.display())
        } else {
            Ok((Library::new(path.clone())?, path))
        }
    }

    /// Search for library with filename: [struct@LIB_PREFIX]+`name`+[struct@LIB_SUFFIX] and load it.
    /// The result is a tuple with:
    ///    * the [Library]
    ///    * its full path
    ///
    /// # Safety
    ///
    /// This function calls [libloading::Library::new()](https://docs.rs/libloading/0.7.0/libloading/struct.Library.html#method.new)
    /// which is unsafe.
    pub unsafe fn search_and_load(&self, name: &str) -> ZResult<(Library, PathBuf)> {
        let filename = format!("{}{}{}", *LIB_PREFIX, name, *LIB_SUFFIX);
        let filename_ostr = OsString::from(&filename);
        log::debug!(
            "Search for library {} to load in {:?}",
            filename,
            self.search_paths
        );
        for dir in &self.search_paths {
            match dir.read_dir() {
                Ok(read_dir) => {
                    for entry in read_dir.flatten() {
                        if entry.file_name() == filename_ostr {
                            let path = entry.path();
                            return Ok((Library::new(path.clone())?, path));
                        }
                    }
                }
                Err(err) => debug!(
                    "Failed to read in directory {:?} ({}). Can't use it to search for libraries.",
                    dir, err
                ),
            }
        }
        bail!("Library file '{}' not found", filename)
    }

    /// Search and load all librairies with filename starting with [struct@LIB_PREFIX]+`prefix` and ending with [struct@LIB_SUFFIX].
    /// The result is a list of tuple with:
    ///    * the [Library]
    ///    * its full path
    ///    * its short name (i.e. filename stripped of prefix and suffix)
    ///
    /// # Safety
    ///
    /// This function calls [libloading::Library::new()](https://docs.rs/libloading/0.7.0/libloading/struct.Library.html#method.new)
    /// which is unsafe.
    pub unsafe fn load_all_with_prefix(
        &self,
        prefix: Option<&str>,
    ) -> Vec<(Library, PathBuf, String)> {
        let lib_prefix = format!("{}{}", *LIB_PREFIX, prefix.unwrap_or(""));
        log::debug!(
            "Search for libraries {}*{} to load in {:?}",
            lib_prefix,
            *LIB_SUFFIX,
            self.search_paths
        );

        let mut result = vec![];
        for dir in &self.search_paths {
            match dir.read_dir() {
                Ok(read_dir) => {
                    for entry in read_dir.flatten() {
                        if let Ok(filename) = entry.file_name().into_string() {
                            if filename.starts_with(&lib_prefix) && filename.ends_with(&*LIB_SUFFIX)
                            {
                                let name = &filename
                                    [(lib_prefix.len())..(filename.len() - LIB_SUFFIX.len())];
                                let path = entry.path();
                                if !result.iter().any(|(_, _, n)| n == name) {
                                    match Library::new(path.as_os_str()) {
                                        Ok(lib) => result.push((lib, path, name.to_string())),
                                        Err(err) => warn!("{}", err),
                                    }
                                } else {
                                    debug!(
                                        "Do not load plugin {} from {:?}: already loaded.",
                                        name, path
                                    );
                                }
                            }
                        }
                    }
                }
                Err(err) => debug!(
                    "Failed to read in directory {:?} ({}). Can't use it to search for libraries.",
                    dir, err
                ),
            }
        }
        result
    }

    pub fn _plugin_name(path: &std::path::Path) -> Option<&str> {
        path.file_name().and_then(|f| {
            f.to_str().map(|s| {
                let start = if s.starts_with(LIB_PREFIX.as_str()) {
                    LIB_PREFIX.len()
                } else {
                    0
                };
                let end = s.len()
                    - if s.ends_with(LIB_SUFFIX.as_str()) {
                        LIB_SUFFIX.len()
                    } else {
                        0
                    };
                &s[start..end]
            })
        })
    }
    pub fn plugin_name<P>(path: &P) -> Option<&str>
    where
        P: AsRef<std::path::Path>,
    {
        Self::_plugin_name(path.as_ref())
    }

    fn str_to_canonical_path(s: &str) -> ZResult<PathBuf> {
        let cow_str = shellexpand::full(s)?;
        Ok(PathBuf::from(cow_str.deref()).canonicalize()?)
    }
}

impl Default for LibLoader {
    fn default() -> Self {
        let paths: Vec<&str> = (*LIB_DEFAULT_SEARCH_PATHS).split(':').collect();
        LibLoader::new(&paths, true)
    }
}
