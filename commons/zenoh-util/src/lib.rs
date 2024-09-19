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
mod lib_loader;
pub mod lib_search_dirs;
pub mod net;
pub mod time_range;

pub use lib_loader::*;
pub mod timer;
pub use timer::*;
pub mod log;
pub use lib_search_dirs::*;
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
                match home::home_dir() {
                    Some(mut dir) => {
                        dir.push(DEFAULT_ZENOH_HOME_DIRNAME);
                        dir
                    }
                    None => PathBuf::from(DEFAULT_ZENOH_HOME_DIRNAME),
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
