//
// Copyright (c) 2022 ZettaScale Technology
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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
#[macro_use]
extern crate lazy_static;
use std::path::{Path, PathBuf};
#[cfg(features = "zenoh-collections")]
#[deprecated = "This module is now a separate crate. Use the `zenoh_collections` crate directly for shorter compile-times. You may disable this re-export by disabling `zenoh-util`'s default features."]
pub use zenoh_collections as collections;
#[deprecated = "This module is now a separate crate. Use the `zenoh_core` crate directly for shorter compile-times. You may disable this re-export by disabling `zenoh-util`'s default features."]
pub use zenoh_core as core;
#[cfg(features = "zenoh-crypto")]
#[deprecated = "This module is now a separate crate. Use the `zenoh_crypto` crate directly for shorter compile-times. You may disable this re-export by disabling `zenoh-util`'s default features."]
pub use zenoh_crypto as crypto;
pub mod ffi;
mod lib_loader;
pub mod net;
pub mod time_range;
pub use lib_loader::*;
pub mod timer;
pub use timer::*;
#[cfg(features = "zenoh-cfg-properties")]
#[deprecated = "This module is now a separate crate. Use the `zenoh_cfg_properties` crate directly for shorter compile-times. You may disable this re-export by disabling `zenoh-util`'s default features."]
pub use zenoh_cfg_properties as properties;
#[cfg(features = "zenoh-sync")]
#[deprecated = "This module is now a separate crate. Use the `zenoh_sync` crate directly for shorter compile-times. You may disable this re-export by disabling `zenoh-util`'s default features."]
pub use zenoh_sync as sync;

/// The "ZENOH_HOME" environement variable name
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
