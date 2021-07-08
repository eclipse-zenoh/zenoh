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
//! * [`PluginLaunch::start`] should be non-blocking, and return a boxed instance of your stoppage type, which should implement [`PluginStopper`].

use clap::{Arg, ArgMatches};
use std::error::Error;
pub mod loading;
pub mod vtable;

pub mod prelude {
    pub use crate::{loading::*, vtable::*, Plugin, PluginStopper};
}

/// Your plugin's compatibility.
/// Currently, this should simply be the plugin crate's name.
/// This structure may evolve to include more detailed information.
#[derive(Clone, Debug, PartialEq)]
pub struct Compatibility {
    pub uid: &'static str,
}

#[derive(Clone, Debug)]
pub struct Incompatibility {
    pub own_compatibility: Compatibility,
    pub conflicting_with: Compatibility,
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

/// Zenoh plugins must implement [`Plugin<Requirements=Vec<clap::Arg<'static, 'static>>, StartArgs=(zenoh::net::runtime::Runtime, &clap::ArgParse)>`](Plugin)
pub trait Plugin: Sized + 'static {
    /// Returns this plugin's [`Compatibility`].
    fn compatibility() -> Compatibility;
    type Requirements;
    type StartArgs;

    /// As Zenoh instanciates plugins, it will append their [`Compatibility`] to an array.
    /// This array's current state will be shown to the next plugin.
    ///
    /// To signal that your plugin is incompatible with a previously instanciated plugin, return `Err`,
    /// Otherwise, return `Ok(Self::compatibility())`.
    ///
    /// By default, a plugin is non-reentrant to avoid reinstanciation if its dlib is accessible despite it already being statically linked.
    fn is_compatible_with(others: &[Compatibility]) -> Result<Compatibility, Incompatibility> {
        let own_compatibility = Self::compatibility();
        if others.iter().any(|c| c == &own_compatibility) {
            let conflicting_with = own_compatibility.clone();
            Err(Incompatibility {
                own_compatibility,
                conflicting_with,
                details: None,
            })
        } else {
            Ok(own_compatibility)
        }
    }

    /// Returns the arguments that are required for the plugin's construction
    fn get_expected_args() -> Vec<Arg<'static, 'static>>;
    fn get_requirements() -> Self::Requirements;
    fn start(args: &Self::StartArgs) -> Result<Box<dyn PluginStopper>, Box<dyn Error>>;
}

/// Allows a [`Plugin`] instance to be stopped.
/// Typically, you can achieve this using a one-shot channel or an [`AtomicBool`](std::sync::atomic::AtomicBool).
/// If you don't want a stopping mechanism, you can use `()` as your [`PluginStopper`].
pub trait PluginStopper: Send + Sync {
    fn stop(&self);
}

impl PluginStopper for () {
    fn stop(&self) {}
}
