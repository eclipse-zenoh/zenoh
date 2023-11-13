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
use std::borrow::Cow;
use zenoh_keyexpr::keyexpr;
use zenoh_result::ZResult;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PluginState {
    Declared,
    Loaded,
    Started,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PluginCondition {
    warnings: Vec<Cow<'static, str>>,
    errors: Vec<Cow<'static, str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginStatus {
    pub state: PluginState,
    pub condition: PluginCondition,
}

pub trait PluginInfo {
    fn name(&self) -> &str;
    fn path(&self) -> &str;
    fn status(&self) -> PluginStatus;
}

pub trait PluginControl {
    fn condition(&self) -> PluginCondition {
        PluginCondition::default()
    }
    /// Collect information of sub-plugins matching `_names` keyexpr
    fn plugins(&self, _names: &keyexpr) -> Vec<(String, PluginStatus)> {
        Vec::new()
    }
}

pub trait PluginStructVersion {
    /// The version of the structure implementing this trait. After any channge in the structure or it's dependencies
    /// whcich may affect the ABI, this version should be incremented.
    fn version() -> u64;
    /// The features enabled during comiplation of the structure implementing this trait.
    /// Different features between the plugin and the host may cuase ABI incompatibility even if the structure version is the same.
    /// Use `concat_enabled_features!` to generate this string
    fn features() -> &'static str;
}

pub trait PluginStartArgs: PluginStructVersion {}

pub trait PluginInstance: PluginStructVersion + PluginControl + Send {}

pub trait Plugin: Sized + 'static {
    type StartArgs: PluginStartArgs;
    type Instance: PluginInstance;
    /// Your plugins' default name when statically linked.
    const STATIC_NAME: &'static str;
    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::Instance>;
}

impl PluginCondition {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.warnings.clear();
        self.errors.clear();
    }
    pub fn add_error<S: Into<Cow<'static, str>>>(&mut self, error: S) {
        self.errors.push(error.into());
    }
    pub fn add_warning<S: Into<Cow<'static, str>>>(&mut self, warning: S) {
        self.warnings.push(warning.into());
    }
    pub fn errors(&self) -> &[Cow<'static, str>] {
        &self.errors
    }
    pub fn warnings(&self) -> &[Cow<'static, str>] {
        &self.warnings
    }
}

pub trait PluginConditionSetter {
    fn add_error(self, condition: &mut PluginCondition) -> Self;
    fn add_warning(self, condition: &mut PluginCondition) -> Self;
}

impl<T, E: ToString> PluginConditionSetter for core::result::Result<T, E> {
    fn add_error(self, condition: &mut PluginCondition) -> Self {
        if let Err(e) = &self {
            condition.add_error(e.to_string());
        }
        self
    }
    fn add_warning(self, condition: &mut PluginCondition) -> Self {
        if let Err(e) = &self {
            condition.add_warning(e.to_string());
        }
        self
    }
}
