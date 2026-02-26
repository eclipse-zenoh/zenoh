//
// Copyright (c) 2026 ZettaScale Technology
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
use nonempty_collections::NEVec;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use zenoh_core::polyfill::*;
use zenoh_protocol::core::{RegionName, WhatAmIMatcher};

use crate::{Interface, ZenohId};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConf {
    pub south: Option<GatewaySouthConf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GatewaySouthConf {
    Preset(GatewayPresetConf),
    Custom(Vec<GatewayExplicitSouthConf>),
}

impl Default for GatewaySouthConf {
    fn default() -> Self {
        GatewaySouthConf::Preset(GatewayPresetConf::default())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayPresetConf {
    #[default]
    Auto,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayExplicitSouthConf {
    /// Bound filters.
    ///
    /// If [`Some`], a subject matches this filter list iff it matches _any_ of the individual
    /// filters. Thus if the list is empty no subject ever matches.
    ///
    /// If [`None`], a subject _always_ matches.
    pub filters: Option<Vec<GatewayFiltersConf>>,
}

/// Bound filter.
///
/// A subject matches this filter iff it matches _all_ of its individual filter fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayFiltersConf {
    pub modes: Option<WhatAmIMatcher>,
    pub interfaces: Option<NEVec<Interface>>,
    pub zids: Option<NEVec<ZenohId>>,
    pub region_names: Option<NEVec<RegionName>>,
    #[serde(default)]
    pub negated: bool,
}
