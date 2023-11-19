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
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, ops::BitOrAssign};
use zenoh_keyexpr::keyexpr;
use zenoh_result::ZResult;

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
/// - Normal: the message(s) is just a notification
/// - Warning: at least one warning is reported
/// - Error: at least one error is reported
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PluginReportLevel {
    #[default]
    Normal,
    Warning,
    Error,
}

/// Allow to use `|=` operator to update the severity level of a report
impl BitOrAssign for PluginReportLevel {
    fn bitor_assign(&mut self, rhs: Self) {
        if *self < rhs {
            *self = rhs;
        }
    }
}

/// A plugin report contains a severity level and a list of messages
/// describing the plugin's situation (for Declared state - dynamic library loading errors, for Loaded state - plugin start errors, etc)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Default, Deserialize)]
pub struct PluginReport {
    level: PluginReportLevel,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    messages: Vec<Cow<'static, str>>,
}

/// The status of a plugin contains all information about the plugin in single cloneable structure
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginStatus<'a> {
    pub name: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Cow<'a, str>>,
    pub path: Cow<'a, str>,
    pub state: PluginState,
    pub report: PluginReport,
}

impl PluginStatus<'_> {
    pub fn into_owned(self) -> PluginStatus<'static> {
        PluginStatus {
            name: Cow::Owned(self.name.into_owned()),
            version: self.version.map(|v| Cow::Owned(v.into_owned())),
            path: Cow::Owned(self.path.into_owned()),
            state: self.state,
            report: self.report,
        }
    }
}

pub trait PluginStatusGetter {
    /// Returns name of the plugin
    fn name(&self) -> &str;
    /// Returns version of the loaded plugin (usually the version of the plugin's crate)
    fn version(&self) -> Option<&str>;
    /// Returns path of the loaded plugin
    fn path(&self) -> &str;
    /// Returns the plugin's state (Declared, Loaded, Started)
    fn state(&self) -> PluginState;
    /// Returns the plugin's current report: a list of messages and the severity level
    /// When status is changed, report is cleared
    fn report(&self) -> PluginReport;
    /// Returns all the information about the plugin in signed structure
    fn status(&self) -> PluginStatus {
        PluginStatus {
            name: Cow::Borrowed(self.name()),
            version: self.version().map(Cow::Borrowed),
            path: Cow::Borrowed(self.path()),
            state: self.state(),
            report: self.report(),
        }
    }
}

pub trait PluginControl {
    fn report(&self) -> PluginReport {
        PluginReport::default()
    }
    /// Collect information of sub-plugins matching `_names` keyexpr
    fn plugins_status(&self, _names: &keyexpr) -> Vec<(String, PluginStatus)> {
        Vec::new()
    }
}

pub trait PluginStructVersion {
    /// The version of the structure implementing this trait. After any channge in the structure or it's dependencies
    /// whcich may affect the ABI, this version should be incremented.
    fn struct_version() -> u64;
    /// The features enabled during compilation of the structure implementing this trait.
    /// Different features between the plugin and the host may cuase ABI incompatibility even if the structure version is the same.
    /// Use `concat_enabled_features!` to generate this string
    fn struct_features() -> &'static str;
}

pub trait PluginStartArgs: PluginStructVersion {}

pub trait PluginInstance: PluginStructVersion + PluginControl + Send {}

pub trait Plugin: Sized + 'static {
    type StartArgs: PluginStartArgs;
    type Instance: PluginInstance;
    /// Plugins' default name when statically linked.
    const DEFAULT_NAME: &'static str;
    /// Plugin's version. Used only for information purposes.
    const PLUGIN_VERSION: &'static str;
    /// Starts your plugin. Use `Ok` to return your plugin's control structure
    fn start(name: &str, args: &Self::StartArgs) -> ZResult<Self::Instance>;
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
    pub fn add_message<S: Into<Cow<'static, str>>>(&mut self, message: S) {
        self.level |= PluginReportLevel::Normal;
        self.messages.push(message.into());
    }
    pub fn messages(&self) -> &[Cow<'static, str>] {
        &self.messages
    }
}

pub trait PluginConditionSetter {
    fn add_error(self, report: &mut PluginReport) -> Self;
    fn add_warning(self, report: &mut PluginReport) -> Self;
    fn add_message(self, report: &mut PluginReport) -> Self;
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
    fn add_message(self, report: &mut PluginReport) -> Self {
        if let Err(e) = &self {
            report.add_message(e.to_string());
        }
        self
    }
}
