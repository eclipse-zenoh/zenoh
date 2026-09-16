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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use lazy_static::lazy_static;

pub mod ffi;
#[cfg(not(target_arch = "wasm32"))]
mod lib_loader;
pub mod lib_search_dirs;
#[cfg(not(target_arch = "wasm32"))]
pub mod net;
pub mod time_range;

#[cfg(not(target_arch = "wasm32"))]
pub use lib_loader::*;
pub mod timer;
pub use timer::*;
pub mod log;
pub use lib_search_dirs::*;

// `LibLoader` dlopen()s native plugin files, which doesn't exist in a browser sandbox and whose
// `libloading` dependency doesn't build for wasm32-unknown-unknown. `zenoh-config`'s `Config`
// embeds one unconditionally though, so keep the same name/shape here as an inert no-op.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Default)]
pub struct LibLoader;

#[cfg(target_arch = "wasm32")]
impl LibLoader {
    pub fn empty() -> LibLoader {
        LibLoader
    }

    pub fn new(_dirs: LibSearchDirs) -> LibLoader {
        LibLoader
    }

    pub fn search_paths(&self) -> Option<&[std::path::PathBuf]> {
        None
    }
}

// The real `net` module enumerates network interfaces to expand a listener bound to an
// unspecified address -- a listener-only concern a browser build never needs, kept as an empty
// stub so callers that reach for it unconditionally still type-check.
#[cfg(target_arch = "wasm32")]
pub mod net {
    pub fn get_local_addresses(
        _interface: Option<&str>,
    ) -> zenoh_result::ZResult<Vec<std::net::IpAddr>> {
        Ok(Vec::new())
    }
}
pub use log::*;

/// The "ZENOH_HOME" environment variable name
pub const ZENOH_HOME_ENV_VAR: &str = "ZENOH_HOME";

const DEFAULT_ZENOH_HOME_DIRNAME: &str = ".zenoh";

/// Return the path to the ${ZENOH_HOME} directory (~/.zenoh by default).
pub fn zenoh_home() -> &'static std::path::Path {
    use std::path::PathBuf;
    lazy_static! {
        static ref ROOT: PathBuf = {
            if let Some(dir) = std::env::var_os(ZENOH_HOME_ENV_VAR) {
                PathBuf::from(dir)
            } else {
                // No home directory in a browser sandbox, and `home` doesn't build for wasm32.
                #[cfg(target_arch = "wasm32")]
                {
                    PathBuf::from(DEFAULT_ZENOH_HOME_DIRNAME)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match home::home_dir() {
                        Some(mut dir) => {
                            dir.push(DEFAULT_ZENOH_HOME_DIRNAME);
                            dir
                        }
                        None => PathBuf::from(DEFAULT_ZENOH_HOME_DIRNAME),
                    }
                }
            }
        };
    }
    ROOT.as_path()
}

#[doc(hidden)]
pub use const_format::concatcp as __concatcp;
#[macro_export]
macro_rules! concat_enabled_features {
    (prefix = $prefix:literal, features = [$($feature:literal),* $(,)?]) => {
        {
            $crate::__concatcp!($(
                if cfg!(feature = $feature) { $crate::__concatcp!(" ", $prefix, "/", $feature) } else { "" }
            ),*)
        }
    };
}
