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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
//!
//! Configuration to pass to `zenoh::open()` and `zenoh::scout()` functions and associated constants.
#![allow(deprecated)]

pub mod defaults;
mod include;
pub mod qos;
pub mod wrappers;

#[allow(unused_imports)]
use std::convert::TryFrom;
// This is a false positive from the rust analyser
use std::{
    any::Any,
    collections::HashSet,
    fmt,
    io::Read,
    net::SocketAddr,
    num::{NonZeroU16, NonZeroUsize},
    ops::{self, Bound, Deref, RangeBounds},
    path::Path,
    sync::{Arc, Weak},
};

use include::recursive_include;
use nonempty_collections::NEVec;
use qos::{PublisherQoSConfList, QosFilter, QosOverwriteMessage, QosOverwrites};
use secrecy::{CloneableSecret, DebugSecret, Secret, SerializableSecret, Zeroize};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use validated_struct::ValidatedMapAssociatedTypes;
pub use validated_struct::{GetError, ValidatedMap};
pub use wrappers::ZenohId;
pub use zenoh_protocol::core::{
    whatami, EndPoint, Locator, WhatAmI, WhatAmIMatcher, WhatAmIMatcherVisitor,
};
use zenoh_protocol::{
    core::{
        key_expr::{OwnedKeyExpr, OwnedNonWildKeyExpr},
        Bits,
    },
    transport::{BatchSize, TransportSn},
};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_util::{LibLoader, LibSearchDirs};

pub mod mode_dependent;
pub use mode_dependent::*;

pub mod connection_retry;
pub use connection_retry::*;

// Wrappers for secrecy of values
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SecretString(String);

impl ops::Deref for SecretString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SerializableSecret for SecretString {}
impl DebugSecret for SecretString {}
impl CloneableSecret for SecretString {}
impl Zeroize for SecretString {
    fn zeroize(&mut self) {
        self.0 = "".to_string();
    }
}

pub type SecretValue = Secret<SecretString>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransportWeight {
    /// A zid of destination node.
    pub dst_zid: ZenohId,
    /// A weight of link from this node to the destination.
    pub weight: NonZeroU16,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InterceptorFlow {
    Egress,
    Ingress,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DownsamplingMessage {
    Delete,
    #[deprecated = "Use `Put` or `Delete` instead."]
    Push,
    Put,
    Query,
    Reply,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct DownsamplingRuleConf {
    /// A list of key-expressions to which the downsampling will be applied.
    /// Downsampling will be applied for all key extensions if the parameter is None
    pub key_expr: OwnedKeyExpr,
    /// The maximum frequency in Hertz;
    pub freq: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct DownsamplingItemConf {
    /// Optional identifier for the downsampling configuration item
    pub id: Option<String>,
    /// A list of interfaces to which the downsampling will be applied
    /// Downsampling will be applied for all interfaces if the parameter is None
    pub interfaces: Option<NEVec<String>>,
    /// A list of link types, transports having one of those link types will have the downsampling applied
    /// Downsampling will be applied for all link types if the parameter is None
    pub link_protocols: Option<NEVec<InterceptorLink>>,
    // list of message types on which the downsampling will be applied
    pub messages: NEVec<DownsamplingMessage>,
    /// A list of downsampling rules: key_expression and the maximum frequency in Hertz
    pub rules: NEVec<DownsamplingRuleConf>,
    /// Downsampling flow directions: egress and/or ingress
    pub flows: Option<NEVec<InterceptorFlow>>,
}

fn downsampling_validator(d: &Vec<DownsamplingItemConf>) -> bool {
    for item in d {
        if item
            .messages
            .iter()
            .any(|m| *m == DownsamplingMessage::Push)
        {
            tracing::warn!("In 'downsampling/messages' configuration: 'push' is deprecated and may not be supported in future versions, use 'put' and/or 'delete' instead");
        }
    }
    true
}

#[derive(Serialize, Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LowPassFilterConf {
    pub id: Option<String>,
    pub interfaces: Option<NEVec<String>>,
    pub link_protocols: Option<NEVec<InterceptorLink>>,
    pub flows: Option<NEVec<InterceptorFlow>>,
    pub messages: NEVec<LowPassFilterMessage>,
    pub key_exprs: NEVec<OwnedKeyExpr>,
    pub size_limit: usize,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LowPassFilterMessage {
    Put,
    Delete,
    Query,
    Reply,
}

#[derive(Serialize, Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AclConfigRule {
    pub id: String,
    pub key_exprs: NEVec<OwnedKeyExpr>,
    pub messages: NEVec<AclMessage>,
    pub flows: Option<NEVec<InterceptorFlow>>,
    pub permission: Permission,
}

#[derive(Serialize, Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AclConfigSubjects {
    pub id: String,
    pub interfaces: Option<NEVec<Interface>>,
    pub cert_common_names: Option<NEVec<CertCommonName>>,
    pub usernames: Option<NEVec<Username>>,
    pub link_protocols: Option<NEVec<InterceptorLink>>,
    pub zids: Option<NEVec<ZenohId>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfRange {
    start: Option<u64>,
    end: Option<u64>,
}

impl ConfRange {
    pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
        Self { start, end }
    }
}

impl RangeBounds<u64> for ConfRange {
    fn start_bound(&self) -> Bound<&u64> {
        match self.start {
            Some(ref start) => Bound::Included(start),
            None => Bound::Unbounded,
        }
    }
    fn end_bound(&self) -> Bound<&u64> {
        match self.end {
            Some(ref end) => Bound::Included(end),
            None => Bound::Unbounded,
        }
    }
}

impl serde::Serialize for ConfRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!(
            "{}..{}",
            self.start.unwrap_or_default(),
            self.end.unwrap_or_default()
        ))
    }
}

impl<'a> serde::Deserialize<'a> for ConfRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct V;

        impl serde::de::Visitor<'_> for V {
            type Value = ConfRange;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("range string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let (start, end) = v
                    .split_once("..")
                    .ok_or_else(|| serde::de::Error::custom("invalid range"))?;
                let parse_bound = |bound: &str| {
                    (!bound.is_empty())
                        .then(|| bound.parse::<u64>())
                        .transpose()
                        .map_err(|_| serde::de::Error::custom("invalid range bound"))
                };
                Ok(ConfRange::new(parse_bound(start)?, parse_bound(end)?))
            }
        }
        deserializer.deserialize_str(V)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct QosOverwriteItemConf {
    /// Optional identifier for the qos modification configuration item.
    pub id: Option<String>,
    /// A list of ZIDs on which qos will be overwritten when communicating with.
    pub zids: Option<NEVec<ZenohId>>,
    /// A list of interfaces to which the qos will be applied.
    /// QosOverwrite will be applied for all interfaces if the parameter is None.
    pub interfaces: Option<NEVec<String>>,
    /// A list of link types, transports having one of those link types will have the qos overwrite applied
    /// Qos overwrite will be applied for all link types if the parameter is None.
    pub link_protocols: Option<NEVec<InterceptorLink>>,
    /// List of message types on which the qos overwrite will be applied.
    pub messages: NEVec<QosOverwriteMessage>,
    /// List of key expressions to apply qos overwrite.
    pub key_exprs: Option<NEVec<OwnedKeyExpr>>,
    // The qos value to overwrite with.
    pub overwrite: QosOverwrites,
    /// QosOverwrite flow directions: egress and/or ingress.
    pub flows: Option<NEVec<InterceptorFlow>>,
    /// QoS filter to apply to the messages matching this item.
    pub qos: Option<QosFilter>,
    /// payload_size range for the messages matching this item.
    pub payload_size: Option<ConfRange>,
}

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct Interface(pub String);

impl std::fmt::Display for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Interface({})", self.0)
    }
}

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct CertCommonName(pub String);

impl std::fmt::Display for CertCommonName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CertCommonName({})", self.0)
    }
}

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct Username(pub String);

impl std::fmt::Display for Username {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Username({})", self.0)
    }
}

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum InterceptorLink {
    Tcp,
    Udp,
    Tls,
    Quic,
    Serial,
    Unixpipe,
    UnixsockStream,
    Vsock,
    Ws,
}

impl std::fmt::Display for InterceptorLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transport({self:?})")
    }
}

#[derive(Serialize, Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct AclConfigPolicyEntry {
    pub id: Option<String>,
    pub rules: Vec<String>,
    pub subjects: Vec<String>,
}

#[derive(Clone, Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRule {
    pub subject_id: usize,
    pub key_expr: OwnedKeyExpr,
    pub message: AclMessage,
    pub permission: Permission,
    pub flow: InterceptorFlow,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AclMessage {
    Put,
    Delete,
    DeclareSubscriber,
    Query,
    DeclareQueryable,
    Reply,
    LivelinessToken,
    DeclareLivelinessSubscriber,
    LivelinessQuery,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Allow,
    Deny,
}

/// Strategy for autoconnection, mainly to avoid nodes connecting to each other redundantly.
#[derive(Default, Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AutoConnectStrategy {
    /// Always attempt to connect to another node, may result in redundant connection which
    /// will be then be closed.
    #[default]
    Always,
    /// A node will attempt to connect to another one only if its own zid is greater than the
    /// other one. If both nodes use this strategy, only one will attempt the connection.
    /// This strategy may not be suited if one of the node is not reachable by the other one,
    /// for example because of a private IP.
    GreaterZid,
}

pub trait ConfigValidator: Send + Sync {
    fn check_config(
        &self,
        _plugin_name: &str,
        _path: &str,
        _current: &serde_json::Map<String, serde_json::Value>,
        _new: &serde_json::Map<String, serde_json::Value>,
    ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>> {
        Ok(None)
    }
}

// Necessary to allow to set default emplty weak reference value to plugin.validator field
// because empty weak value is not allowed for Arc<dyn Trait>
impl ConfigValidator for () {}

/// Creates an empty zenoh net Session configuration.
pub fn empty() -> Config {
    Config::default()
}

/// Creates a default zenoh net Session configuration (equivalent to `peer`).
pub fn default() -> Config {
    peer()
}

/// Creates a default `'peer'` mode zenoh net Session configuration.
pub fn peer() -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config
}

/// Creates a default `'client'` mode zenoh net Session configuration.
pub fn client<I: IntoIterator<Item = T>, T: Into<EndPoint>>(peers: I) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config.connect.endpoints =
        ModeDependentValue::Unique(peers.into_iter().map(|t| t.into()).collect());
    config
}

#[test]
fn config_keys() {
    let c = Config::default();
    dbg!(Vec::from_iter(c.keys()));
}

validated_struct::validator! {
    #[derive(Default)]
    #[recursive_attrs]
    #[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
    #[serde(default)]
    #[serde(deny_unknown_fields)]
    #[doc(hidden)]
    Config {
        /// The Zenoh ID of the instance. This ID MUST be unique throughout your Zenoh infrastructure and cannot exceed 16 bytes of length. If left unset, a random u128 will be generated.
        /// If not specified a random Zenoh ID will be generated upon session creation.
        id: Option<ZenohId>,
        /// The metadata of the instance. Arbitrary json data available from the admin space
        metadata: Value,
        /// The node's mode ("router" (default value in `zenohd`), "peer" or "client").
        mode: Option<whatami::WhatAmI>,
        /// Which zenoh nodes to connect to.
        pub connect:
        ConnectConfig {
            /// global timeout for full connect cycle
            pub timeout_ms: Option<ModeDependentValue<i64>>,
            /// The list of endpoints to connect to
            pub endpoints: ModeDependentValue<Vec<EndPoint>>,
            /// if connection timeout exceed, exit from application
            pub exit_on_failure: Option<ModeDependentValue<bool>>,
            pub retry: Option<connection_retry::ConnectionRetryModeDependentConf>,
        },
        /// Which endpoints to listen on.
        pub listen:
        ListenConfig {
            /// global timeout for full listen cycle
            pub timeout_ms: Option<ModeDependentValue<i64>>,
            /// The list of endpoints to listen on
            pub endpoints: ModeDependentValue<Vec<EndPoint>>,
            /// if connection timeout exceed, exit from application
            pub exit_on_failure: Option<ModeDependentValue<bool>>,
            pub retry: Option<connection_retry::ConnectionRetryModeDependentConf>,
        },
        /// Configure the session open behavior.
        pub open: #[derive(Default)]
        OpenConf {
            /// Configure the conditions to be met before session open returns.
            pub return_conditions: #[derive(Default)]
            ReturnConditionsConf {
                /// Session open waits to connect to scouted peers and routers before returning.
                /// When set to false, first publications and queries after session open from peers may be lost.
                connect_scouted: Option<bool>,
                /// Session open waits to receive initial declares from connected peers before returning.
                /// Setting to false may cause extra traffic at startup from peers.
                declares: Option<bool>,
            },
        },
        pub scouting: #[derive(Default)]
        ScoutingConf {
            /// In client mode, the period dedicated to scouting for a router before failing. In milliseconds.
            timeout: Option<u64>,
            /// In peer mode, the period dedicated to scouting remote peers before attempting other operations. In milliseconds.
            delay: Option<u64>,
            /// The multicast scouting configuration.
            pub multicast: #[derive(Default)]
            ScoutingMulticastConf {
                /// Whether multicast scouting is enabled or not. If left empty, `zenohd` will set it according to the presence of the `--no-multicast-scouting` argument.
                enabled: Option<bool>,
                /// The socket which should be used for multicast scouting. `zenohd` will use `224.0.0.224:7446` by default if none is provided.
                address: Option<SocketAddr>,
                /// The network interface which should be used for multicast scouting. `zenohd` will automatically select an interface if none is provided.
                interface: Option<String>,
                /// The time-to-live on multicast scouting packets. (default: 1)
                pub ttl: Option<u32>,
                /// Which type of Zenoh instances to automatically establish sessions with upon discovery through UDP multicast.
                autoconnect: Option<ModeDependentValue<WhatAmIMatcher>>,
                /// Strategy for autoconnection, mainly to avoid nodes connecting to each other redundantly.
                autoconnect_strategy: Option<ModeDependentValue<TargetDependentValue<AutoConnectStrategy>>>,
                /// Whether or not to listen for scout messages on UDP multicast and reply to them.
                listen: Option<ModeDependentValue<bool>>,
            },
            /// The gossip scouting configuration.
            pub gossip: #[derive(Default)]
            GossipConf {
                /// Whether gossip scouting is enabled or not.
                enabled: Option<bool>,
                /// When true, gossip scouting information are propagated multiple hops to all nodes in the local network.
                /// When false, gossip scouting information are only propagated to the next hop.
                /// Activating multihop gossip implies more scouting traffic and a lower scalability.
                /// It mostly makes sense when using "linkstate" routing mode where all nodes in the subsystem don't have
                /// direct connectivity with each other.
                multihop: Option<bool>,
                /// Which type of Zenoh instances to send gossip messages to.
                target: Option<ModeDependentValue<WhatAmIMatcher>>,
                /// Which type of Zenoh instances to automatically establish sessions with upon discovery through gossip.
                autoconnect: Option<ModeDependentValue<WhatAmIMatcher>>,
                /// Strategy for autoconnection, mainly to avoid nodes connecting to each other redundantly.
                autoconnect_strategy: Option<ModeDependentValue<TargetDependentValue<AutoConnectStrategy>>>,
            },
        },

        /// Configuration of data messages timestamps management.
        pub timestamping: #[derive(Default)]
        TimestampingConf {
            /// Whether data messages should be timestamped if not already.
            enabled: Option<ModeDependentValue<bool>>,
            /// Whether data messages with timestamps in the future should be dropped or not.
            /// If set to false (default), messages with timestamps in the future are retimestamped.
            /// Timestamps are ignored if timestamping is disabled.
            drop_future_timestamp: Option<bool>,
        },

        /// The default timeout to apply to queries in milliseconds.
        queries_default_timeout: Option<u64>,

        /// The routing strategy to use and it's configuration.
        pub routing: #[derive(Default)]
        RoutingConf {
            /// The routing strategy to use in routers and it's configuration.
            pub router: #[derive(Default)]
            RouterRoutingConf {
                /// When set to true a router will forward data between two peers
                /// directly connected to it if it detects that those peers are not
                /// connected to each other.
                /// The failover brokering only works if gossip discovery is enabled.
                peers_failover_brokering: Option<bool>,
                /// Linkstate mode configuration.
                pub linkstate: #[derive(Default)]
                LinkstateConf {
                    /// Weights of the outgoing links in linkstate mode.
                    /// If none of the two endpoint nodes of a transport specifies its weight, a weight of 100 is applied.
                    /// If only one of the two endpoint nodes of a transport specifies its weight, the specified weight is applied.
                    /// If both endpoint nodes of a transport specify its weight, the greater weight is applied.
                    pub transport_weights: Vec<TransportWeight>,
                },
            },
            /// The routing strategy to use in peers and it's configuration.
            pub peer: #[derive(Default)]
            PeerRoutingConf {
                /// The routing strategy to use in peers. ("peer_to_peer" or "linkstate").
                /// This option needs to be set to the same value in all peers and routers of the subsystem.
                mode: Option<String>,
                /// Linkstate mode configuration (only taken into account if mode == "linkstate").
                pub linkstate: LinkstateConf,
            },
            /// The interests-based routing configuration.
            /// This configuration applies regardless of the mode (router, peer or client).
            pub interests: #[derive(Default)]
            InterestsConf {
                /// The timeout to wait for incoming interests declarations.
                timeout: Option<u64>,
            },
        },

        /// The declarations aggregation strategy.
        pub aggregation: #[derive(Default)]
        AggregationConf {
            /// A list of key-expressions for which all included subscribers will be aggregated into.
            subscribers: Vec<OwnedKeyExpr>,
            /// A list of key-expressions for which all included publishers will be aggregated into.
            publishers: Vec<OwnedKeyExpr>,
        },

        /// Overwrite QoS options for Zenoh messages by key expression (ignores Zenoh API QoS config)
        pub qos: #[derive(Default)]
        QoSConfig {
            /// A list of QoS configurations for PUT and DELETE messages by key expressions
            publication: PublisherQoSConfList,
            /// Configuration of the qos overwrite interceptor rules
            network: Vec<QosOverwriteItemConf>,
        },

        pub transport: #[derive(Default)]
        TransportConf {
            pub unicast: TransportUnicastConf {
                /// Timeout in milliseconds when opening a link (default: 10000).
                open_timeout: u64,
                /// Timeout in milliseconds when accepting a link (default: 10000).
                accept_timeout: u64,
                /// Number of links that may stay pending during accept phase (default: 100).
                accept_pending: usize,
                /// Maximum number of unicast sessions (default: 1000)
                max_sessions: usize,
                /// Maximum number of unicast incoming links per transport session (default: 1)
                /// If set to a value greater than 1, multiple outgoing links are also allowed;
                /// otherwise, only one outgoing link is allowed.
                /// Issue https://github.com/eclipse-zenoh/zenoh/issues/1533
                max_links: usize,
                /// Enables the LowLatency transport (default `false`).
                /// This option does not make LowLatency transport mandatory, the actual implementation of transport
                /// used will depend on Establish procedure and other party's settings
                lowlatency: bool,
                pub qos: QoSUnicastConf {
                    /// Whether QoS is enabled or not.
                    /// If set to `false`, the QoS will be disabled. (default `true`).
                    enabled: bool
                },
                pub compression: CompressionUnicastConf {
                    /// You must compile zenoh with "transport_compression" feature to be able to enable compression.
                    /// When enabled is true, batches will be sent compressed. (default `false`).
                    enabled: bool,
                },
            },
            pub multicast: TransportMulticastConf {
                /// Link join interval duration in milliseconds (default: 2500)
                join_interval: Option<u64>,
                /// Maximum number of multicast sessions (default: 1000)
                max_sessions: Option<usize>,
                pub qos: QoSMulticastConf {
                    /// Whether QoS is enabled or not.
                    /// If set to `false`, the QoS will be disabled. (default `false`).
                    enabled: bool
                },
                pub compression: CompressionMulticastConf {
                    /// You must compile zenoh with "transport_compression" feature to be able to enable compression.
                    /// When enabled is true, batches will be sent compressed. (default `false`).
                    enabled: bool,
                },
            },
            pub link: #[derive(Default)]
            TransportLinkConf {
                // An optional whitelist of protocols to be used for accepting and opening sessions.
                // If not configured, all the supported protocols are automatically whitelisted.
                pub protocols: Option<Vec<String>>,
                pub tx: LinkTxConf {
                    /// The resolution in bits to be used for the message sequence numbers.
                    /// When establishing a session with another Zenoh instance, the lowest value of the two instances will be used.
                    /// Accepted values: 8bit, 16bit, 32bit, 64bit.
                    sequence_number_resolution: Bits where (sequence_number_resolution_validator),
                    /// Link lease duration in milliseconds (default: 10000)
                    lease: u64,
                    /// Number of keep-alive messages in a link lease duration (default: 4)
                    keep_alive: usize,
                    /// Zenoh's MTU equivalent (default: 2^16-1) (max: 2^16-1)
                    batch_size: BatchSize,
                    pub queue: #[derive(Default)]
                    QueueConf {
                        /// The size of each priority queue indicates the number of batches a given queue can contain.
                        /// The amount of memory being allocated for each queue is then SIZE_XXX * BATCH_SIZE.
                        /// In the case of the transport link MTU being smaller than the ZN_BATCH_SIZE,
                        /// then amount of memory being allocated for each queue is SIZE_XXX * LINK_MTU.
                        /// If qos is false, then only the DATA priority will be allocated.
                        pub size: QueueSizeConf {
                            control: usize,
                            real_time: usize,
                            interactive_high: usize,
                            interactive_low: usize,
                            data_high: usize,
                            data: usize,
                            data_low: usize,
                            background: usize,
                        } where (queue_size_validator),
                        /// Congestion occurs when the queue is empty (no available batch).
                        /// Using CongestionControl::Block the caller is blocked until a batch is available and re-inserted into the queue.
                        /// Using CongestionControl::Drop the message might be dropped, depending on conditions configured here.
                        pub congestion_control: #[derive(Default)]
                        CongestionControlConf {
                            /// Behavior pushing CongestionControl::Drop messages to the queue.
                            pub drop: CongestionControlDropConf {
                                /// The maximum time in microseconds to wait for an available batch before dropping a droppable message
                                /// if still no batch is available.
                                wait_before_drop: i64,
                                /// The maximum deadline limit for multi-fragment messages.
                                max_wait_before_drop_fragments: i64,
                            },
                            /// Behavior pushing CongestionControl::Block messages to the queue.
                            pub block: CongestionControlBlockConf {
                                /// The maximum time in microseconds to wait for an available batch before closing the transport session
                                /// when sending a blocking message if still no batch is available.
                                wait_before_close: i64,
                            },
                        },
                        pub batching: BatchingConf {
                            /// Perform adaptive batching of messages if they are smaller of the batch_size.
                            /// When the network is detected to not be fast enough to transmit every message individually, many small messages may be
                            /// batched together and sent all at once on the wire reducing the overall network overhead. This is typically of a high-throughput
                            /// scenario mainly composed of small messages. In other words, batching is activated by the network back-pressure.
                            enabled: bool,
                            /// The maximum time limit (in ms) a message should be retained for batching when back-pressure happens.
                            time_limit: u64,
                        },
                        /// Perform lazy memory allocation of batches in the prioritiey queues. If set to false all batches are initialized at
                        /// initialization time. If set to true the batches will be allocated when needed up to the maximum number of batches
                        /// configured in the size configuration parameter.
                        pub allocation: #[derive(Default, Copy, PartialEq, Eq)]
                        QueueAllocConf {
                            pub mode: QueueAllocMode,
                        },
                    },
                    // Number of threads used for TX
                    threads: usize,
                },
                pub rx: LinkRxConf {
                    /// Receiving buffer size in bytes for each link
                    /// The default the rx_buffer_size value is the same as the default batch size: 65535.
                    /// For very high throughput scenarios, the rx_buffer_size can be increased to accommodate
                    /// more in-flight data. This is particularly relevant when dealing with large messages.
                    /// E.g. for 16MiB rx_buffer_size set the value to: 16777216.
                    buffer_size: usize,
                    /// Maximum size of the defragmentation buffer at receiver end (default: 1GiB).
                    /// Fragmented messages that are larger than the configured size will be dropped.
                    max_message_size: usize,
                },
                pub tls: #[derive(Default)]
                TLSConf {
                    root_ca_certificate: Option<String>,
                    listen_private_key: Option<String>,
                    listen_certificate: Option<String>,
                    enable_mtls: Option<bool>,
                    connect_private_key: Option<String>,
                    connect_certificate: Option<String>,
                    verify_name_on_connect: Option<bool>,
                    close_link_on_expiration: Option<bool>,
                    /// Configure TCP write buffer size
                    pub so_sndbuf: Option<u32>,
                    /// Configure TCP read buffer size
                    pub so_rcvbuf: Option<u32>,
                    // Skip serializing field because they contain secrets
                    #[serde(skip_serializing)]
                    root_ca_certificate_base64: Option<SecretValue>,
                    #[serde(skip_serializing)]
                    listen_private_key_base64:  Option<SecretValue>,
                    #[serde(skip_serializing)]
                    listen_certificate_base64: Option<SecretValue>,
                    #[serde(skip_serializing)]
                    connect_private_key_base64 :  Option<SecretValue>,
                    #[serde(skip_serializing)]
                    connect_certificate_base64 :  Option<SecretValue>,
                },
                pub tcp: #[derive(Default)]
                TcpConf {
                    /// Configure TCP write buffer size
                    pub so_sndbuf: Option<u32>,
                    /// Configure TCP read buffer size
                    pub so_rcvbuf: Option<u32>,
                },
                pub unixpipe: #[derive(Default)]
                UnixPipeConf {
                    file_access_mask: Option<u32>
                },
            },
            pub shared_memory:
            ShmConf {
                /// Whether shared memory is enabled or not.
                /// If set to `true`, the SHM buffer optimization support will be announced to other parties. (default `true`).
                /// This option doesn't make SHM buffer optimization mandatory, the real support depends on other party setting
                /// A probing procedure for shared memory is performed upon session opening. To enable zenoh to operate
                /// over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
                /// subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
                enabled: bool,
                /// SHM resources initialization mode (default "lazy").
                /// - "lazy": SHM subsystem internals will be initialized lazily upon the first SHM buffer
                /// allocation or reception. This setting provides better startup time and optimizes resource usage,
                /// but produces extra latency at the first SHM buffer interaction.
                /// - "init": SHM subsystem internals will be initialized upon Session opening. This setting sacrifices
                /// startup time, but guarantees no latency impact when first SHM buffer is processed.
                mode: ShmInitMode,
                pub transport_optimization:
                LargeMessageTransportOpt {
                    /// Enables transport optimization for large messages (default `true`).
                    /// Implicitly puts large messages into shared memory for transports with SHM-compatible connection.
                    enabled: bool,
                    /// SHM arena size in bytes used for transport optimization (default `16 * 1024 * 1024`).
                    pool_size: NonZeroUsize,
                    /// Allow optimization for messages equal or larger than this threshold in bytes (default `3072`).
                    message_size_threshold: usize,
                },
            },
            pub auth: #[derive(Default)]
            AuthConf {
                /// The configuration of authentication.
                /// A password implies a username is required.
                pub usrpwd: #[derive(Default)]
                UsrPwdConf {
                    user: Option<String>,
                    password: Option<String>,
                    /// The path to a file containing the user password dictionary, a file containing `<user>:<password>`
                    dictionary_file: Option<String>,
                } where (user_conf_validator),
                pub pubkey: #[derive(Default)]
                PubKeyConf {
                    public_key_pem: Option<String>,
                    private_key_pem: Option<String>,
                    public_key_file: Option<String>,
                    private_key_file: Option<String>,
                    key_size: Option<usize>,
                    known_keys_file: Option<String>,
                },
            },

        },
        /// Configuration of the admin space.
        pub adminspace: #[derive(Default)]
        /// <div class="stab unstable">
        ///   <span class="emoji">🔬</span>
        ///   This API has been marked as unstable: it works as advertised, but we may change it in a future release.
        ///   To use it, you must enable zenoh's <code>unstable</code> feature flag.
        /// </div>
        AdminSpaceConf {
            /// Enable the admin space
            #[serde(default = "set_false")]
            pub enabled: bool,
            /// Permissions on the admin space
            pub permissions:
            PermissionsConf {
                /// Whether the admin space replies to queries (true by default).
                #[serde(default = "set_true")]
                pub read: bool,
                /// Whether the admin space accepts config changes at runtime (false by default).
                #[serde(default = "set_false")]
                pub write: bool,
            },

        },

        /// Namespace prefix.
        /// If not None, all outgoing key expressions will be
        /// automatically prefixed with specified string,
        /// and all incoming key expressions will be stripped
        /// of specified prefix.
        /// Namespace is applied to the session.
        /// E. g. if session has a namespace of "1" then session.put("my/keyexpr", message),
        /// will put a message into "1/my/keyexpr". Same applies to all other operations within this session.
        pub namespace: Option<OwnedNonWildKeyExpr>,

        /// Configuration of the downsampling.
        downsampling: Vec<DownsamplingItemConf> where (downsampling_validator),

        /// Configuration of the access control (ACL)
        pub access_control: AclConfig {
            pub enabled: bool,
            pub default_permission: Permission,
            pub rules: Option<Vec<AclConfigRule>>,
            pub subjects: Option<Vec<AclConfigSubjects>>,
            pub policies: Option<Vec<AclConfigPolicyEntry>>,
        },

        /// Configuration of the low-pass filter
        pub low_pass_filter: Vec<LowPassFilterConf>,

        /// A list of directories where plugins may be searched for if no `__path__` was specified for them.
        /// The executable's current directory will be added to the search paths.
        pub plugins_loading: #[derive(Default)]
        PluginsLoading {
            pub enabled: bool,
            pub search_dirs: LibSearchDirs,
        },
        #[validated(recursive_accessors)]
        /// The configuration for plugins.
        ///
        /// Please refer to [`PluginsConfig`]'s documentation for further details.
        plugins: PluginsConfig,
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueAllocMode {
    Init,
    #[default]
    Lazy,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShmInitMode {
    Init,
    #[default]
    Lazy,
}

impl Default for PermissionsConf {
    fn default() -> Self {
        PermissionsConf {
            read: true,
            write: false,
        }
    }
}

fn set_true() -> bool {
    true
}
fn set_false() -> bool {
    false
}

#[test]
fn config_deser() {
    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
        scouting: {
          multicast: {
            enabled: false,
            autoconnect: ["peer", "router"]
          }
        }
      }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(*config.scouting().multicast().enabled(), Some(false));
    assert_eq!(
        config.scouting().multicast().autoconnect().router(),
        Some(&WhatAmIMatcher::empty().router().peer())
    );
    assert_eq!(
        config.scouting().multicast().autoconnect().peer(),
        Some(&WhatAmIMatcher::empty().router().peer())
    );
    assert_eq!(
        config.scouting().multicast().autoconnect().client(),
        Some(&WhatAmIMatcher::empty().router().peer())
    );
    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
        scouting: {
          multicast: {
            enabled: false,
            autoconnect: {router: [], peer: ["peer", "router"]}
          }
        }
      }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(*config.scouting().multicast().enabled(), Some(false));
    assert_eq!(
        config.scouting().multicast().autoconnect().router(),
        Some(&WhatAmIMatcher::empty())
    );
    assert_eq!(
        config.scouting().multicast().autoconnect().peer(),
        Some(&WhatAmIMatcher::empty().router().peer())
    );
    assert_eq!(config.scouting().multicast().autoconnect().client(), None);
    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{transport: { auth: { usrpwd: { user: null, password: null, dictionary_file: "file" }}}}"#,
        )
            .unwrap(),
    )
        .unwrap();
    assert_eq!(
        config
            .transport()
            .auth()
            .usrpwd()
            .dictionary_file()
            .as_ref()
            .map(|s| s.as_ref()),
        Some("file")
    );
    std::mem::drop(Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{transport: { auth: { usrpwd: { user: null, password: null, user_password_dictionary: "file" }}}}"#,
        )
            .unwrap(),
    )
        .unwrap_err());

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    overwrite: {
                      priority: "foo",
                    },
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    );
    assert!(config.is_err());

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    overwrite: {
                      priority: +8,
                    },
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    );
    assert!(config.is_err());

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    overwrite: {
                      priority: "data_high",
                    },
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config.qos().network().first().unwrap().overwrite.priority,
        Some(qos::PriorityUpdateConf::Priority(
            qos::PriorityConf::DataHigh
        ))
    );

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    overwrite: {
                      priority: +1,
                    },
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config.qos().network().first().unwrap().overwrite.priority,
        Some(qos::PriorityUpdateConf::Increment(1))
    );

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    payload_size: "0..99",
                    overwrite: {},
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config
            .qos()
            .network()
            .first()
            .unwrap()
            .payload_size
            .as_ref()
            .map(|r| (r.start_bound(), r.end_bound())),
        Some((Bound::Included(&0), Bound::Included(&99)))
    );

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    payload_size: "100..",
                    overwrite: {},
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config
            .qos()
            .network()
            .first()
            .unwrap()
            .payload_size
            .as_ref()
            .map(|r| (r.start_bound(), r.end_bound())),
        Some((Bound::Included(&100), Bound::Unbounded))
    );

    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
              qos: {
                network: [
                  {
                    messages: ["put"],
                    qos: {
                      congestion_control: "drop",
                      priority: "data",
                      express: true,
                      reliability: "reliable",
                    },
                    overwrite: {},
                  },
                ],
              }
            }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config.qos().network().first().unwrap().qos,
        Some(QosFilter {
            congestion_control: Some(qos::CongestionControlConf::Drop),
            priority: Some(qos::PriorityConf::Data),
            express: Some(true),
            reliability: Some(qos::ReliabilityConf::Reliable),
        })
    );

    dbg!(Config::from_file("../../DEFAULT_CONFIG.json5").unwrap());
}

impl Config {
    pub fn insert<'d, D: serde::Deserializer<'d>>(
        &mut self,
        key: &str,
        value: D,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<D::Error>,
    {
        <Self as ValidatedMap>::insert(self, key, value)
    }

    pub fn get(
        &self,
        key: &str,
    ) -> Result<<Self as ValidatedMapAssociatedTypes<'_>>::Accessor, GetError> {
        <Self as ValidatedMap>::get(self, key)
    }

    pub fn get_json(&self, key: &str) -> Result<String, GetError> {
        <Self as ValidatedMap>::get_json(self, key)
    }

    pub fn insert_json5(
        &mut self,
        key: &str,
        value: &str,
    ) -> Result<(), validated_struct::InsertionError> {
        <Self as ValidatedMap>::insert_json5(self, key, value)
    }

    pub fn keys(&self) -> impl Iterator<Item = String> {
        <Self as ValidatedMap>::keys(self).into_iter()
    }

    pub fn set_plugin_validator<T: ConfigValidator + 'static>(&mut self, validator: Weak<T>) {
        self.plugins.validator = validator;
    }

    pub fn plugin(&self, name: &str) -> Option<&Value> {
        self.plugins.values.get(name)
    }

    pub fn sift_privates(&self) -> Self {
        let mut copy = self.clone();
        copy.plugins.sift_privates();
        copy
    }

    pub fn remove<K: AsRef<str>>(&mut self, key: K) -> ZResult<()> {
        let key = key.as_ref();

        let key = key.strip_prefix('/').unwrap_or(key);
        if !key.starts_with("plugins/") {
            bail!(
                "Removal of values from Config is only supported for keys starting with `plugins/`"
            )
        }
        self.plugins.remove(&key["plugins/".len()..])
    }

    pub fn get_retry_config(
        &self,
        endpoint: Option<&EndPoint>,
        listen: bool,
    ) -> ConnectionRetryConf {
        get_retry_config(self, endpoint, listen)
    }
}

#[derive(Debug)]
pub enum ConfigOpenErr {
    IoError(std::io::Error),
    JsonParseErr(json5::Error),
    InvalidConfiguration(Box<Config>),
}
impl std::fmt::Display for ConfigOpenErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigOpenErr::IoError(e) => write!(f, "Couldn't open file: {e}"),
            ConfigOpenErr::JsonParseErr(e) => write!(f, "JSON5 parsing error {e}"),
            ConfigOpenErr::InvalidConfiguration(c) => write!(
                f,
                "Invalid configuration {}",
                serde_json::to_string(c).unwrap()
            ),
        }
    }
}
impl std::error::Error for ConfigOpenErr {}
impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> ZResult<Self> {
        let path = path.as_ref();
        let mut config = Self::_from_file(path)?;
        config.plugins.load_external_configs()?;
        Ok(config)
    }

    fn _from_file(path: &Path) -> ZResult<Config> {
        match std::fs::File::open(path) {
            Ok(mut f) => {
                let mut content = String::new();
                if let Err(e) = f.read_to_string(&mut content) {
                    bail!(e)
                }
                if content.is_empty() {
                    bail!("Empty config file");
                }
                match path
                    .extension()
                    .map(|s| s.to_str().unwrap())
                {
                    Some("json") | Some("json5") => match json5::Deserializer::from_str(&content) {
                        Ok(mut d) => Config::from_deserializer(&mut d).map_err(|e| match e {
                            Ok(c) => zerror!("Invalid configuration: {}", c).into(),
                            Err(e) => zerror!("JSON error: {:?}", e).into(),
                        }),
                        Err(e) => bail!(e),
                    },
                    Some("yaml") | Some("yml") => Config::from_deserializer(serde_yaml::Deserializer::from_str(&content)).map_err(|e| match e {
                        Ok(c) => zerror!("Invalid configuration: {}", c).into(),
                        Err(e) => zerror!("YAML error: {:?}", e).into(),
                    }),
                    Some(other) => bail!("Unsupported file type '.{}' (.json, .json5 and .yaml are supported)", other),
                    None => bail!("Unsupported file type. Configuration files must have an extension (.json, .json5 and .yaml supported)")
                }
            }
            Err(e) => bail!(e),
        }
    }

    pub fn libloader(&self) -> LibLoader {
        if self.plugins_loading.enabled {
            LibLoader::new(self.plugins_loading.search_dirs().clone())
        } else {
            LibLoader::empty()
        }
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        serde_json::to_value(self)
            .map(|mut json| {
                sift_privates(&mut json);
                write!(f, "{json}")
            })
            .map_err(|e| {
                _ = write!(f, "{e:?}");
                fmt::Error
            })?
    }
}

#[test]
fn config_from_json() {
    let from_str = serde_json::Deserializer::from_str;
    let mut config = Config::from_deserializer(&mut from_str(r#"{}"#)).unwrap();
    config
        .insert("transport/link/tx/lease", &mut from_str("168"))
        .unwrap();
    dbg!(std::mem::size_of_val(&config));
    println!("{}", serde_json::to_string_pretty(&config).unwrap());
}

fn sequence_number_resolution_validator(b: &Bits) -> bool {
    b <= &Bits::from(TransportSn::MAX)
}

fn queue_size_validator(q: &QueueSizeConf) -> bool {
    fn check(size: &usize) -> bool {
        (QueueSizeConf::MIN..=QueueSizeConf::MAX).contains(size)
    }

    let QueueSizeConf {
        control,
        real_time,
        interactive_low,
        interactive_high,
        data_high,
        data,
        data_low,
        background,
    } = q;
    check(control)
        && check(real_time)
        && check(interactive_low)
        && check(interactive_high)
        && check(data_high)
        && check(data)
        && check(data_low)
        && check(background)
}

fn user_conf_validator(u: &UsrPwdConf) -> bool {
    (u.password().is_none() && u.user().is_none()) || (u.password().is_some() && u.user().is_some())
}

/// This part of the configuration is highly dynamic (any [`serde_json::Value`] may be put in there), but should follow this scheme:
/// ```javascript
/// plugins: {
///     // `plugin_name` must be unique per configuration, and will be used to find the appropriate
///     // dynamic library to load if no `__path__` is specified
///     [plugin_name]: {
///         // Defaults to `false`. Setting this to `true` does 2 things:
///         // * If `zenohd` fails to locate the requested plugin, it will crash instead of logging an error.
///         // * Plugins are expected to check this value to set their panic-behaviour: plugins are encouraged
///         //   to panic upon non-recoverable errors if their `__required__` flag is set to `true`, and to
///         //   simply log them otherwise
///         __required__: bool,
///         // The path(s) where the plugin is expected to be located.
///         // If none is specified, `zenohd` will search for a `<dylib_prefix>zenoh_plugin_<plugin_name>.<dylib_suffix>` file in the search directories.
///         // If any path is specified, file-search will be disabled, and the first path leading to
///         // an existing file will be used
///         __path__: string | [string],
///         // [plugin_name] may require additional configuration
///         ...
///     }
/// }
/// ```
#[derive(Clone)]
pub struct PluginsConfig {
    values: Value,
    validator: std::sync::Weak<dyn ConfigValidator>,
}
fn sift_privates(value: &mut serde_json::Value) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        Value::Array(a) => a.iter_mut().for_each(sift_privates),
        Value::Object(o) => {
            o.remove("private");
            o.values_mut().for_each(sift_privates);
        }
    }
}

fn load_external_plugin_config(title: &str, value: &mut Value) -> ZResult<()> {
    let Some(values) = value.as_object_mut() else {
        bail!("{} must be object", title);
    };
    recursive_include(title, values, HashSet::new(), "__config__", ".")
}

#[derive(Debug, Clone)]
pub struct PluginLoad {
    pub id: String,
    pub name: String,
    pub paths: Option<Vec<String>>,
    pub required: bool,
}
impl PluginsConfig {
    pub fn sift_privates(&mut self) {
        sift_privates(&mut self.values);
    }
    fn load_external_configs(&mut self) -> ZResult<()> {
        let Some(values) = self.values.as_object_mut() else {
            bail!("plugins configuration must be an object")
        };
        for (name, value) in values.iter_mut() {
            load_external_plugin_config(format!("plugins.{}", name.as_str()).as_str(), value)?;
        }
        Ok(())
    }
    pub fn load_requests(&'_ self) -> impl Iterator<Item = PluginLoad> + '_ {
        self.values.as_object().unwrap().iter().map(|(id, value)| {
            let value = value.as_object().expect("Plugin configurations must be objects");
            let required = match value.get("__required__") {
                None => false,
                Some(Value::Bool(b)) => *b,
                _ => panic!("Plugin '{id}' has an invalid '__required__' configuration property (must be a boolean)")
            };
            let name = match value.get("__plugin__") {
                Some(Value::String(p)) => p,
                _ => id,
            };

            if let Some(paths) = value.get("__path__") {
                let paths = match paths {
                    Value::String(s) => vec![s.clone()],
                    Value::Array(a) => a.iter().map(|s| if let Value::String(s) = s { s.clone() } else { panic!("Plugin '{id}' has an invalid '__path__' configuration property (must be either string or array of strings)") }).collect(),
                    _ => panic!("Plugin '{id}' has an invalid '__path__' configuration property (must be either string or array of strings)")
                };
                PluginLoad { id: id.clone(), name: name.clone(), paths: Some(paths), required }
            } else {
                PluginLoad { id: id.clone(), name: name.clone(), paths: None, required }
            }
        })
    }
    pub fn remove(&mut self, key: &str) -> ZResult<()> {
        let mut split = key.split('/');
        let plugin = split.next().unwrap();
        let mut current = match split.next() {
            Some(first_in_plugin) => first_in_plugin,
            None => {
                self.values.as_object_mut().unwrap().remove(plugin);
                return Ok(());
            }
        };
        let (old_conf, mut new_conf) = match self.values.get_mut(plugin) {
            Some(plugin) => {
                let clone = plugin.clone();
                (plugin, clone)
            }
            None => bail!("No plugin {} to edit", plugin),
        };
        let mut remove_from = &mut new_conf;
        for next in split {
            match remove_from {
                Value::Object(o) => match o.get_mut(current) {
                    Some(v) => {
                        remove_from = unsafe {
                            std::mem::transmute::<&mut serde_json::Value, &mut serde_json::Value>(v)
                        }
                    }
                    None => bail!("{:?} has no {} property", o, current),
                },
                Value::Array(a) => {
                    let index: usize = current.parse()?;
                    if a.len() <= index {
                        bail!("{:?} cannot be indexed at {}", a, index)
                    }
                    remove_from = &mut a[index];
                }
                other => bail!("{} cannot be indexed", other),
            }
            current = next
        }
        match remove_from {
            Value::Object(o) => {
                if o.remove(current).is_none() {
                    bail!("{:?} has no {} property", o, current)
                }
            }
            Value::Array(a) => {
                let index: usize = current.parse()?;
                if a.len() <= index {
                    bail!("{:?} cannot be indexed at {}", a, index)
                }
                a.remove(index);
            }
            other => bail!("{} cannot be indexed", other),
        }
        let new_conf = if let Some(validator) = self.validator.upgrade() {
            match validator.check_config(
                plugin,
                &key[("plugins/".len() + plugin.len())..],
                old_conf.as_object().unwrap(),
                new_conf.as_object().unwrap(),
            )? {
                None => new_conf,
                Some(new_conf) => Value::Object(new_conf),
            }
        } else {
            new_conf
        };
        *old_conf = new_conf;
        Ok(())
    }
}
impl serde::Serialize for PluginsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut value = self.values.clone();
        sift_privates(&mut value);
        value.serialize(serializer)
    }
}
impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            values: Value::Object(Default::default()),
            validator: std::sync::Weak::<()>::new(),
        }
    }
}
impl<'a> serde::Deserialize<'a> for PluginsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        Ok(PluginsConfig {
            values: serde::Deserialize::deserialize(deserializer)?,
            validator: std::sync::Weak::<()>::new(),
        })
    }
}

impl std::fmt::Debug for PluginsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut values: Value = self.values.clone();
        sift_privates(&mut values);
        write!(f, "{values:?}")
    }
}

trait PartialMerge: Sized {
    fn merge(self, path: &str, value: Self) -> Result<Self, validated_struct::InsertionError>;
}
impl PartialMerge for serde_json::Value {
    fn merge(
        mut self,
        path: &str,
        new_value: Self,
    ) -> Result<Self, validated_struct::InsertionError> {
        let mut value = &mut self;
        let mut key = path;
        let key_not_found = || {
            Err(validated_struct::InsertionError::String(format!(
                "{path} not found"
            )))
        };
        while !key.is_empty() {
            let (current, new_key) = validated_struct::split_once(key, '/');
            key = new_key;
            if current.is_empty() {
                continue;
            }
            value = match value {
                Value::Bool(_) | Value::Number(_) | Value::String(_) => return key_not_found(),
                Value::Null => match current {
                    "0" | "+" => {
                        *value = Value::Array(vec![Value::Null]);
                        &mut value[0]
                    }
                    _ => {
                        *value = Value::Object(Default::default());
                        value
                            .as_object_mut()
                            .unwrap()
                            .entry(current)
                            .or_insert(Value::Null)
                    }
                },
                Value::Array(a) => match current {
                    "+" => {
                        a.push(Value::Null);
                        a.last_mut().unwrap()
                    }
                    "0" if a.is_empty() => {
                        a.push(Value::Null);
                        a.last_mut().unwrap()
                    }
                    _ => match current.parse::<usize>() {
                        Ok(i) => match a.get_mut(i) {
                            Some(r) => r,
                            None => return key_not_found(),
                        },
                        Err(_) => return key_not_found(),
                    },
                },
                Value::Object(v) => v.entry(current).or_insert(Value::Null),
            }
        }
        *value = new_value;
        Ok(self)
    }
}
impl<'a> validated_struct::ValidatedMapAssociatedTypes<'a> for PluginsConfig {
    type Accessor = &'a dyn Any;
}
impl validated_struct::ValidatedMap for PluginsConfig {
    fn insert<'d, D: serde::Deserializer<'d>>(
        &mut self,
        key: &str,
        deserializer: D,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<D::Error>,
    {
        let (plugin, key) = validated_struct::split_once(key, '/');
        let new_value: Value = serde::Deserialize::deserialize(deserializer)?;
        let value = self
            .values
            .as_object_mut()
            .unwrap()
            .entry(plugin)
            .or_insert(Value::Null);
        let new_value = value.clone().merge(key, new_value)?;
        *value = if let Some(validator) = self.validator.upgrade() {
            // New plugin configuration for compare with original configuration.
            // Return error if it's not an object.
            // Note: it's ok if original "new_value" is not an object: this can be some subkey of the plugin configuration. But the result of the merge should be an object.
            // Error occurs  if the original plugin configuration is not an object itself (e.g. null).
            let Some(new_plugin_config) = new_value.as_object() else {
                return Err(format!(
                    "Attempt to provide non-object value as configuration for plugin `{plugin}`"
                )
                .into());
            };
            // Original plugin configuration for compare with new configuration.
            // If for some reason it's not defined or not an object, we default to an empty object.
            // Usually this happens when no plugin with this name defined. Reject then should be performed by the validator with `plugin not found` error.
            let empty_config = Map::new();
            let current_plugin_config = value.as_object().unwrap_or(&empty_config);
            match validator.check_config(plugin, key, current_plugin_config, new_plugin_config) {
                // Validator made changes to the proposed configuration, take these changes
                Ok(Some(val)) => Value::Object(val),
                // Validator accepted the proposed configuration as is
                Ok(None) => new_value,
                // Validator rejected the proposed configuration
                Err(e) => return Err(format!("{e}").into()),
            }
        } else {
            new_value
        };
        Ok(())
    }
    fn get<'a>(&'a self, mut key: &str) -> Result<&'a dyn Any, GetError> {
        let (current, new_key) = validated_struct::split_once(key, '/');
        key = new_key;
        let mut value = match self.values.get(current) {
            Some(matched) => matched,
            None => return Err(GetError::NoMatchingKey),
        };
        while !key.is_empty() {
            let (current, new_key) = validated_struct::split_once(key, '/');
            key = new_key;
            let matched = match value {
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_) => return Err(GetError::NoMatchingKey),
                serde_json::Value::Array(a) => a.get(match current.parse::<usize>() {
                    Ok(i) => i,
                    Err(_) => return Err(GetError::NoMatchingKey),
                }),
                serde_json::Value::Object(v) => v.get(current),
            };
            value = match matched {
                Some(matched) => matched,
                None => return Err(GetError::NoMatchingKey),
            }
        }
        Ok(value)
    }

    type Keys = Vec<String>;
    fn keys(&self) -> Self::Keys {
        self.values.as_object().unwrap().keys().cloned().collect()
    }

    fn get_json(&self, mut key: &str) -> Result<String, GetError> {
        let (current, new_key) = validated_struct::split_once(key, '/');
        key = new_key;
        let mut value = match self.values.get(current) {
            Some(matched) => matched,
            None => return Err(GetError::NoMatchingKey),
        };
        while !key.is_empty() {
            let (current, new_key) = validated_struct::split_once(key, '/');
            key = new_key;
            let matched = match value {
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_) => return Err(GetError::NoMatchingKey),
                serde_json::Value::Array(a) => a.get(match current.parse::<usize>() {
                    Ok(i) => i,
                    Err(_) => return Err(GetError::NoMatchingKey),
                }),
                serde_json::Value::Object(v) => v.get(current),
            };
            value = match matched {
                Some(matched) => matched,
                None => return Err(GetError::NoMatchingKey),
            }
        }
        Ok(serde_json::to_string(value).unwrap())
    }
}

#[macro_export]
macro_rules! unwrap_or_default {
    ($val:ident$(.$field:ident($($param:ident)?))*) => {
        $val$(.$field($($param)?))*.clone().unwrap_or(zenoh_config::defaults$(::$field$(($param))?)*.into())
    };
}

pub trait IConfig: Send + Sync {
    fn get(&self, key: &str) -> ZResult<String>;
    fn queries_default_timeout_ms(&self) -> u64;
    fn insert_json5(&self, key: &str, value: &str) -> ZResult<()>;
    fn to_json(&self) -> String;
}

pub struct GenericConfig(Arc<dyn IConfig>);

impl Deref for GenericConfig {
    type Target = Arc<dyn IConfig>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl GenericConfig {
    pub fn new(value: Arc<dyn IConfig>) -> Self {
        GenericConfig(value)
    }

    pub fn get_typed<T: for<'a> Deserialize<'a>>(&self, key: &str) -> ZResult<T> {
        self.0
            .get(key)
            .and_then(|v| serde_json::from_str::<T>(&v).map_err(|e| e.into()))
    }

    pub fn get_plugin_config(&self, plugin_name: &str) -> ZResult<Value> {
        self.get(&("plugins/".to_owned() + plugin_name))
            .and_then(|v| serde_json::from_str(&v).map_err(|e| e.into()))
    }
}

impl fmt::Display for GenericConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_json())
    }
}
