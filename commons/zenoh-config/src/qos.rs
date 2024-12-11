//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTreeMut, KeBoxTree};
use zenoh_protocol::core::{key_expr::OwnedKeyExpr, CongestionControl, Reliability};

#[derive(Debug, Deserialize, Default, Serialize, Clone)]
pub struct PublisherQoSConfList(pub(crate) Vec<PublisherQoSConf>);

impl From<PublisherQoSConfList> for KeBoxTree<PublisherQoSConfig> {
    fn from(value: PublisherQoSConfList) -> KeBoxTree<PublisherQoSConfig> {
        let mut tree = KeBoxTree::new();
        for conf in value.0 {
            for key_expr in conf.key_exprs {
                // NOTE: we don't check key_expr unicity
                tree.insert(&key_expr, conf.config.clone());
            }
        }
        tree
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct PublisherQoSConf {
    pub key_exprs: Vec<OwnedKeyExpr>,
    pub config: PublisherQoSConfig,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct PublisherQoSConfig {
    pub congestion_control: Option<PublisherCongestionControlConf>,
    pub priority: Option<PublisherPriorityConf>,
    pub express: Option<bool>,
    #[cfg(feature = "unstable")]
    pub reliability: Option<PublisherReliabilityConf>,
    #[cfg(feature = "unstable")]
    pub allowed_destination: Option<PublisherLocalityConf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PublisherCongestionControlConf {
    Drop,
    Block,
}

impl From<PublisherCongestionControlConf> for CongestionControl {
    fn from(value: PublisherCongestionControlConf) -> Self {
        match value {
            PublisherCongestionControlConf::Drop => Self::Drop,
            PublisherCongestionControlConf::Block => Self::Block,
        }
    }
}

impl From<CongestionControl> for PublisherCongestionControlConf {
    fn from(value: CongestionControl) -> Self {
        match value {
            CongestionControl::Drop => Self::Drop,
            CongestionControl::Block => Self::Block,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PublisherPriorityConf {
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PublisherReliabilityConf {
    BestEffort,
    Reliable,
}

impl From<PublisherReliabilityConf> for Reliability {
    fn from(value: PublisherReliabilityConf) -> Self {
        match value {
            PublisherReliabilityConf::BestEffort => Self::BestEffort,
            PublisherReliabilityConf::Reliable => Self::Reliable,
        }
    }
}

impl From<Reliability> for PublisherReliabilityConf {
    fn from(value: Reliability) -> Self {
        match value {
            Reliability::BestEffort => Self::BestEffort,
            Reliability::Reliable => Self::Reliable,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PublisherLocalityConf {
    SessionLocal,
    Remote,
    Any,
}
