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

use std::any::Any;
use std::error::Error;
use std::sync::Arc;
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

/// Zenoh plugins must implement [`Plugin<StartArgs=zenoh::net::runtime::Runtime>`](Plugin)
///
/// Plugin lifecycle:
/// * Each plugin's identity is recovered using `compatibility`
/// * Compatibility between plugins is asserted using `is_compatible_with`
/// * Each plugin's requirements are collected through `get_requirements`
///     * for zenoh plugins, these requirements are dynamic validators to be used upon configuration change requests
/// *
pub trait Plugin: Sized + 'static {
    type StartArgs;
    type RunningPlugin;
    const STATIC_NAME: &'static str;

    /// Returns this plugin's `Compatibility`.
    fn compatibility() -> PluginId;

    /// As Zenoh instanciates plugins, it will append their `Compatibility` to an array.
    /// This array's current state will be shown to the next plugin.
    ///
    /// To signal that your plugin is incompatible with a previously instanciated plugin, return `Err`,
    /// Otherwise, return `Ok(Self::compatibility())`.
    ///
    /// By default, a plugin is non-reentrant to avoid reinstanciation if its dlib is accessible despite it already being statically linked.
    fn is_compatible_with(others: &[PluginId]) -> Result<PluginId, Incompatibility> {
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

    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::RunningPlugin>;
}

pub type ValidationFunction = Arc<
    dyn Fn(
            &str,
            &serde_json::Map<String, serde_json::Value>,
            &serde_json::Map<String, serde_json::Value>,
        ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>>
        + Send
        + Sync,
>;

// pub type GetCallback = Arc<dyn Fn(&Selector<'_>) -> ZResult<Vec<Reply>>>;
// pub type PutCallback = Arc<dyn Fn(&KeyExpr<'_>, &Value, Kind) -> ZResult<()>>;

pub trait RunningPluginTrait<'a>: Send + Sync + Any {
    type GetterIn;
    type GetterOut;
    type SetterIn;
    type SetterOut;
    fn config_checker(&self) -> ValidationFunction;
    fn adminspace_getter(&'a self, input: Self::GetterIn) -> Self::GetterOut;
    fn adminspace_setter(&'a mut self, input: Self::SetterIn) -> Self::SetterOut;
}
