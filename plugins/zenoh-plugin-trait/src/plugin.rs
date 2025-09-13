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
use std::{borrow::Cow, ops::BitOrAssign};

use serde::{Deserialize, Serialize};
use zenoh_keyexpr::keyexpr;
use zenoh_result::ZResult;

use crate::StructVersion;

/// The diff in the configuration when plugins are updated:
/// - Delete: the plugin has been removed from the configuration at runtime
/// - Start: the plugin has been added to the configuration at runtime
#[derive(Debug, Clone)]
pub enum PluginDiff {
    Delete(String),
    Start(zenoh_config::PluginLoad),
}

/// The plugin can be in one of these states:
/// - Declared: the plugin is declared in the configuration file, but not loaded yet or failed to load
/// - Loaded: the plugin is loaded, but not started yet or failed to start
/// - Started: the plugin is started and running
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PluginState {
    Declared,
    Loaded,
    Started,
}

/// The severity level of a plugin report messages
/// - Normal: the message(s) are just notifications
/// - Warning: at least one warning is reported
/// - Error: at least one error is reported
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PluginReportLevel {
    #[default]
    Info,
    Warning,
    Error,
}

/// Allow using the `|=` operator to update the severity level of a report
impl BitOrAssign for PluginReportLevel {
    fn bitor_assign(&mut self, rhs: Self) {
        if *self < rhs {
            *self = rhs;
        }
    }
}

/// A plugin report contains a severity level and a list of messages
/// describing the plugin's situation (for the Declared state - dynamic library loading errors, for the Loaded state - plugin start errors, etc)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Default, Deserialize)]
pub struct PluginReport {
    level: PluginReportLevel,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    messages: Vec<Cow<'static, str>>,
}

/// Trait allowing getting all information about the plugin
pub trait PluginStatus {
    /// Returns the name of the plugin
    fn name(&self) -> &str;
    /// Returns the ID of the plugin
    fn id(&self) -> &str;
    /// Returns the version of the loaded plugin (usually the version of the plugin's crate)
    fn version(&self) -> Option<&str>;
    /// Returns the long version of the loaded plugin (usually the version of the plugin's crate + git commit hash)
    fn long_version(&self) -> Option<&str>;
    /// Returns the path of the loaded plugin
    fn path(&self) -> &str;
    /// Returns the plugin's state (Declared, Loaded, Started)
    fn state(&self) -> PluginState;
    /// Returns the plugin's current report: a list of messages and the severity level
    /// When the status is changed, the report is cleared
    fn report(&self) -> PluginReport;
}

/// The structure which contains all information about the plugin status in a single cloneable structure
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginStatusRec<'a> {
    pub name: Cow<'a, str>,
    pub id: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Cow<'a, str>>,
    pub long_version: Option<Cow<'a, str>>,
    pub path: Cow<'a, str>,
    pub state: PluginState,
    pub report: PluginReport,
}

impl PluginStatus for PluginStatusRec<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    fn long_version(&self) -> Option<&str> {
        self.long_version.as_deref()
    }
    fn path(&self) -> &str {
        &self.path
    }
    fn state(&self) -> PluginState {
        self.state
    }
    fn report(&self) -> PluginReport {
        self.report.clone()
    }
}

impl<'a> PluginStatusRec<'a> {
    /// Construct the status structure from the getter interface
    pub fn new<T: PluginStatus + ?Sized>(plugin: &'a T) -> Self {
        Self {
            name: Cow::Borrowed(plugin.name()),
            id: Cow::Borrowed(plugin.id()),
            version: plugin.version().map(Cow::Borrowed),
            long_version: plugin.long_version().map(Cow::Borrowed),
            path: Cow::Borrowed(plugin.path()),
            state: plugin.state(),
            report: plugin.report(),
        }
    }
    /// Convert the status structure to the owned version
    pub fn into_owned(self) -> PluginStatusRec<'static> {
        PluginStatusRec {
            name: Cow::Owned(self.name.into_owned()),
            id: Cow::Owned(self.id.into_owned()),
            version: self.version.map(|v| Cow::Owned(v.into_owned())),
            long_version: self.long_version.map(|v| Cow::Owned(v.into_owned())),
            path: Cow::Owned(self.path.into_owned()),
            state: self.state,
            report: self.report,
        }
    }
    pub(crate) fn prepend_name(self, prefix: &str) -> Self {
        Self {
            name: Cow::Owned(format!("{}/{}", prefix, self.name)),
            ..self
        }
    }
}

/// This trait allows getting information about the plugin status and the status of its subplugins, if any.
pub trait PluginControl {
    /// Returns the current state of the running plugin. By default, the state is `PluginReportLevel::Normal` and the list of messages is empty.
    /// This can be overridden by the plugin implementation if the plugin is able to report its status: no connection to the database, etc.
    fn report(&self) -> PluginReport {
        PluginReport::default()
    }
    /// Collects information of sub-plugins matching the `_names` key expression. The information is richer than the one returned by `report()`: it contains external information about the running plugin, such as its name, path on disk, load status, etc.
    /// Returns an empty list by default.
    fn plugins_status(&self, _names: &keyexpr) -> Vec<PluginStatusRec<'_>> {
        Vec::new()
    }
}

pub trait PluginStartArgs: StructVersion {}

pub trait PluginInstance: PluginControl + Send + Sync {}

/// Base plugin trait. The loaded plugin
pub trait Plugin: Sized + 'static {
    type StartArgs: PluginStartArgs;
    type Instance: PluginInstance;
    /// Plugins' default name when statically linked.
    const DEFAULT_NAME: &'static str;
    /// Plugin's version. Used only for information purposes. It's recommended to use [plugin_version!](crate::plugin_version!) macro to generate this string.
    const PLUGIN_VERSION: &'static str;
    /// Plugin's long version (with git commit hash). Used only for information purposes. It's recommended to use [plugin_version!](crate::plugin_version!) macro to generate this string.
    const PLUGIN_LONG_VERSION: &'static str;
    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::Instance>;
}

#[macro_export]
macro_rules! plugin_version {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}

#[macro_export]
macro_rules! plugin_long_version {
    () => {
        $crate::export::git_version::git_version!(prefix = "v", cargo_prefix = "v")
    };
}

impl PluginReport {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn get_level(&self) -> PluginReportLevel {
        self.level
    }
    pub fn add_error<S: Into<Cow<'static, str>>>(&mut self, error: S) {
        self.level |= PluginReportLevel::Error;
        self.messages.push(error.into());
    }
    pub fn add_warning<S: Into<Cow<'static, str>>>(&mut self, warning: S) {
        self.level |= PluginReportLevel::Warning;
        self.messages.push(warning.into());
    }
    pub fn add_info<S: Into<Cow<'static, str>>>(&mut self, message: S) {
        self.level |= PluginReportLevel::Info;
        self.messages.push(message.into());
    }
    pub fn messages(&self) -> &[Cow<'static, str>] {
        &self.messages
    }
}

pub trait PluginConditionSetter {
    fn add_error(self, report: &mut PluginReport) -> Self;
    fn add_warning(self, report: &mut PluginReport) -> Self;
    fn add_info(self, report: &mut PluginReport) -> Self;
}

impl<T, E: ToString> PluginConditionSetter for core::result::Result<T, E> {
    fn add_error(self, report: &mut PluginReport) -> Self {
        if let Err(e) = &self {
            report.add_error(e.to_string());
        }
        self
    }
    fn add_warning(self, report: &mut PluginReport) -> Self {
        if let Err(e) = &self {
            report.add_warning(e.to_string());
        }
        self
    }
    fn add_info(self, report: &mut PluginReport) -> Self {
        if let Err(e) = &self {
            report.add_info(e.to_string());
        }
        self
    }
}
