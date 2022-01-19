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
#[allow(unused_imports)] // This should re-export macros to not break API
#[macro_use]
pub extern crate zenoh_core;
#[allow(unused_imports)] // This should re-export macros to not break API
#[macro_use]
pub extern crate zenoh_sync;
#[macro_use]
extern crate lazy_static;
use std::path::{Path, PathBuf};
pub use zenoh_collections as collections;
pub use zenoh_core as core;
pub use zenoh_core::macros::*;
pub use zenoh_crypto as crypto;
pub mod ffi;
mod lib_loader;
pub mod net;
pub use lib_loader::*;
pub use zenoh_cfg_properties as properties;
pub use zenoh_sync as sync;

/// the "ZENOH_HOME" environement variable name
pub const ZENOH_HOME_ENV_VAR: &str = "ZENOH_HOME";

const DEFAULT_ZENOH_HOME_DIRNAME: &str = ".zenoh";

/// Return the path to the ${ZENOH_HOME} directory (~/.zenoh by default).
pub fn zenoh_home() -> &'static Path {
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
