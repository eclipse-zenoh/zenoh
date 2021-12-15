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

//! # The plugin infrastructure for Zenoh.
//!
//! To build a plugin, up to 2 types may be constructed :
//! * A [`Plugin`] type.
//! * [`Plugin::start`] should be non-blocking, and return a boxed instance of your stoppage type, which should implement [`PluginStopper`].
use std::error::Error;
pub mod loading;
pub mod vtable;
use zenoh_util::core::Result as ZResult;

pub mod prelude {
    pub use crate::{loading::*, vtable::*, Plugin};
}

/// Your plugin's identifier.
/// Currently, this should simply be the plugin crate's name.
/// This structure may evolve to include more detailed information.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginId {
    pub uid: &'static str,
}

#[derive(Clone, Debug)]
pub struct Incompatibility {
    pub own_compatibility: PluginId,
    pub conflicting_with: PluginId,
    pub details: Option<String>,
}

impl std::fmt::Display for Incompatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} prevented {} from initing",
            self.conflicting_with.uid, self.own_compatibility.uid
        )?;
        if let Some(details) = &self.details {
            write!(f, ": {}", details)?;
        }
        write!(f, ".")
    }
}

impl Error for Incompatibility {}

pub trait Plugin: Sized + 'static {
    type StartArgs;
    type RunningPlugin;
    const STATIC_NAME: &'static str;

    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::RunningPlugin>;
}
