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

//! # The plugin infrastructure for Zenoh.
//!
//! To build a plugin, implement [`Plugin`].
//!
//! If building a plugin for [`zenohd`](https://crates.io/crates/zenoh), you should use the types exported in [`zenoh::plugins`](https://docs.rs/zenoh/latest/zenoh/plugins) to fill [`Plugin`]'s associated types.  
//! To check your plugin typing for `zenohd`, have your plugin implement [`zenoh::plugins::ZenohPlugin`](https://docs.rs/zenoh/latest/zenoh/plugins/struct.ZenohPlugin)
pub mod loading;
pub mod vtable;

use zenoh_result::ZResult;

pub mod prelude {
    pub use crate::{concat_enabled_features, loading::*, vtable::*, CompatibilityVersion, Plugin};
}

#[macro_export]
macro_rules! concat_enabled_features {
    (prefix = $prefix:literal, features = [$($feature:literal),*]) => {
        {
            use const_format::concatcp;
            concatcp!("" $(,
                if cfg!(feature = $feature) { concatcp!(" ", concatcp!($prefix, "/", $feature)) } else { "" }
            )*)
        }
    };
}

pub trait CompatibilityVersion {
    /// The version of the structure implementing this trait. After any channge in the structure or it's dependencies
    /// whcich may affect the ABI, this version should be incremented.
    fn version() -> u64;
    /// The features enabled during comiplation of the structure implementing this trait.
    /// Different features between the plugin and the host may cuase ABI incompatibility even if the structure version is the same.
    /// Use `concat_enabled_features!` to generate this string
    fn features() -> &'static str;
}

pub enum PluginState {
    Declared,
    Loaded,
    Started,
}

pub enum PluginCondition {
    Ok,
    Warning(String),
    Error(String),
}

pub struct PluginStatus {
    pub state: PluginState,
    pub condition: PluginCondition,
}

pub trait PluginControl {
    fn plugins(&self) -> Vec<&str>;
    fn status(&self, name: &str) -> PluginStatus;
}

pub trait Plugin: Sized + 'static {
    type StartArgs: CompatibilityVersion;
    type RunningPlugin: CompatibilityVersion;
    /// Your plugins' default name when statically linked.
    const STATIC_NAME: &'static str;
    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::RunningPlugin>;
}
