use std::fmt;

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
use zenoh_protocol::core::{key_expr::OwnedKeyExpr, CongestionControl, Priority, Reliability};

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
    pub congestion_control: Option<CongestionControlConf>,
    pub priority: Option<PriorityConf>,
    pub express: Option<bool>,
    #[cfg(feature = "unstable")]
    pub reliability: Option<ReliabilityConf>,
    #[cfg(feature = "unstable")]
    pub allowed_destination: Option<PublisherLocalityConf>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CongestionControlConf {
    Drop,
    Block,
    #[cfg(feature = "unstable")]
    BlockFirst,
}

impl From<CongestionControlConf> for CongestionControl {
    fn from(value: CongestionControlConf) -> Self {
        match value {
            CongestionControlConf::Drop => Self::Drop,
            CongestionControlConf::Block => Self::Block,
            #[cfg(feature = "unstable")]
            CongestionControlConf::BlockFirst => Self::BlockFirst,
        }
    }
}

impl From<CongestionControl> for CongestionControlConf {
    fn from(value: CongestionControl) -> Self {
        match value {
            CongestionControl::Drop => Self::Drop,
            CongestionControl::Block => Self::Block,
            #[cfg(feature = "unstable")]
            CongestionControl::BlockFirst => Self::BlockFirst,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriorityConf {
    RealTime = 1,
    InteractiveHigh = 2,
    InteractiveLow = 3,
    DataHigh = 4,
    Data = 5,
    DataLow = 6,
    Background = 7,
}

impl From<PriorityConf> for Priority {
    fn from(value: PriorityConf) -> Self {
        match value {
            PriorityConf::RealTime => Self::RealTime,
            PriorityConf::InteractiveHigh => Self::InteractiveHigh,
            PriorityConf::InteractiveLow => Self::InteractiveLow,
            PriorityConf::DataHigh => Self::DataHigh,
            PriorityConf::Data => Self::Data,
            PriorityConf::DataLow => Self::DataLow,
            PriorityConf::Background => Self::Background,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityUpdateConf {
    Priority(PriorityConf),
    Increment(i8),
}

impl serde::Serialize for PriorityUpdateConf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PriorityUpdateConf::Priority(value) => value.serialize(serializer),
            PriorityUpdateConf::Increment(value) => value.serialize(serializer),
        }
    }
}

impl<'a> serde::Deserialize<'a> for PriorityUpdateConf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct PriorityOrIncrement<U>(std::marker::PhantomData<fn() -> U>);

        impl serde::de::Visitor<'_> for PriorityOrIncrement<PriorityUpdateConf> {
            type Value = PriorityUpdateConf;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("priority string or increment integer")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                PriorityConf::deserialize(serde::de::value::StrDeserializer::new(v))
                    .map(PriorityUpdateConf::Priority)
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if v > 7 {
                    Err(serde::de::Error::custom(
                        "invalid priority increment (> +7)",
                    ))
                } else if v < -7 {
                    Err(serde::de::Error::custom(
                        "invalid priority increment (< -7)",
                    ))
                } else {
                    Ok(PriorityUpdateConf::Increment(v as i8))
                }
            }
        }
        deserializer.deserialize_any(PriorityOrIncrement(std::marker::PhantomData))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityConf {
    BestEffort,
    Reliable,
}

impl From<ReliabilityConf> for Reliability {
    fn from(value: ReliabilityConf) -> Self {
        match value {
            ReliabilityConf::BestEffort => Self::BestEffort,
            ReliabilityConf::Reliable => Self::Reliable,
        }
    }
}

impl From<Reliability> for ReliabilityConf {
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QosOverwriteMessage {
    Put,
    Delete,
    Query,
    Reply,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct QosFilter {
    pub congestion_control: Option<CongestionControlConf>,
    pub priority: Option<PriorityConf>,
    pub express: Option<bool>,
    pub reliability: Option<ReliabilityConf>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct QosOverwrites {
    pub congestion_control: Option<CongestionControlConf>,
    pub priority: Option<PriorityUpdateConf>,
    pub express: Option<bool>,
    // TODO: Add support for reliability overwrite (it is not possible right now, since reliability is not a part of RoutingContext, nor NetworkMessage)
    // #[cfg(feature = "unstable")]
    // pub reliability: Option<ReliabilityConf>,
}
