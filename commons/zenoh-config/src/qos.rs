use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTreeMut, KeBoxTree};
use zenoh_protocol::core::{key_expr::OwnedKeyExpr, CongestionControl, Reliability};

#[derive(Debug, Deserialize, Default, Serialize, Clone)]
#[serde(remote = "Self")]
pub struct PublisherQoSConfList(pub(crate) Vec<PublisherQoSConf>);

impl<'de> Deserialize<'de> for PublisherQoSConfList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let put_qos_list = PublisherQoSConfList::deserialize(deserializer)?;
        // check for invariant: each key_expr should be unique
        let mut key_set = HashSet::new();
        for put_qos in &put_qos_list.0 {
            for key_expr in &put_qos.key_exprs {
                if !key_set.insert(key_expr) {
                    return Err(format!(
                        "duplicated key_expr '{key_expr}' found in publisher builders config"
                    ))
                    .map_err(serde::de::Error::custom);
                }
            }
        }
        Ok(put_qos_list)
    }
}

impl Serialize for PublisherQoSConfList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        PublisherQoSConfList::serialize(self, serializer)
    }
}

impl From<PublisherQoSConfList> for KeBoxTree<PublisherQoSConfig> {
    fn from(value: PublisherQoSConfList) -> KeBoxTree<PublisherQoSConfig> {
        let mut tree = KeBoxTree::new();
        for conf in value.0 {
            for key_expr in conf.key_exprs {
                // key_expr unicity is checked at deserialization
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
            PublisherCongestionControlConf::Drop => CongestionControl::Drop,
            PublisherCongestionControlConf::Block => CongestionControl::Block,
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
    fn from(value: PublisherReliabilityConf) -> Reliability {
        match value {
            PublisherReliabilityConf::BestEffort => Reliability::BestEffort,
            PublisherReliabilityConf::Reliable => Reliability::Reliable,
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
