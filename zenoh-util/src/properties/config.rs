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
use super::{IntKeyProperties, KeyTranscoder};

mod consts {
    /// `"true"`
    pub const ZN_TRUE: &str = "true";
    /// `"false"`
    pub const ZN_FALSE: &str = "false";
    /// `"auto"`
    pub const ZN_AUTO: &str = "auto";

    /// The library mode.
    /// String key : `"mode"`.
    /// Accepted values : `"peer"`, `"client"`.
    /// Default value : `"peer"`.
    pub const ZN_MODE_KEY: u64 = 0x40;
    pub const ZN_MODE_STR: &str = "mode";
    pub const ZN_MODE_DEFAULT: &str = "peer";

    /// The locator of a peer to connect to.
    /// String key : `"peer"`.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_PEER_KEY: u64 = 0x41;
    pub const ZN_PEER_STR: &str = "peer";

    /// A locator to listen on.
    /// String key : `"listener"`.
    /// Accepted values : `<locator>` (ex: `"tcp/10.10.10.10:7447"`).
    /// Default value : None.
    /// Multiple values accepted.
    pub const ZN_LISTENER_KEY: u64 = 0x42;
    pub const ZN_LISTENER_STR: &str = "listener";

    /// The user name to use for authentication.
    /// String key : `"user"`.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_USER_KEY: u64 = 0x43;
    pub const ZN_USER_STR: &str = "user";

    /// The password to use for authentication.
    /// String key : `"password"`.
    /// Accepted values : `<string>`.
    /// Default value : None.
    pub const ZN_PASSWORD_KEY: u64 = 0x44;
    pub const ZN_PASSWORD_STR: &str = "password";

    /// Activates/Desactivates multicast scouting.
    /// String key : `"multicast_scouting"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_MULTICAST_SCOUTING_KEY: u64 = 0x45;
    pub const ZN_MULTICAST_SCOUTING_STR: &str = "multicast_scouting";
    pub const ZN_MULTICAST_SCOUTING_DEFAULT: &str = ZN_TRUE;

    /// The network interface to use for multicast scouting.
    /// String key : `"multicast_interface"`.
    /// Accepted values : `"auto"`, `<ip address>`, `<interface name>`.
    /// Default value : `"auto"`.
    pub const ZN_MULTICAST_INTERFACE_KEY: u64 = 0x46;
    pub const ZN_MULTICAST_INTERFACE_STR: &str = "multicast_interface";
    pub const ZN_MULTICAST_INTERFACE_DEFAULT: &str = ZN_AUTO;

    /// The multicast address and ports to use for multicast scouting.
    /// String key : `"multicast_address"`.
    /// Accepted values : `<ip address>:<port>`.
    /// Default value : `"224.0.0.224:7447"`.
    pub const ZN_MULTICAST_ADDRESS_KEY: u64 = 0x47;
    pub const ZN_MULTICAST_ADDRESS_STR: &str = "multicast_address";
    pub const ZN_MULTICAST_ADDRESS_DEFAULT: &str = "224.0.0.224:7447";

    /// In client mode, the period dedicated to scouting a router before failing.
    /// String key : `"scouting_timeout"`.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"3.0"`.
    pub const ZN_SCOUTING_TIMEOUT_KEY: u64 = 0x48;
    pub const ZN_SCOUTING_TIMEOUT_STR: &str = "scouting_timeout";
    pub const ZN_SCOUTING_TIMEOUT_DEFAULT: &str = "3.0";

    /// In peer mode, the period dedicated to scouting first remote peers before doing anything else.
    /// String key : `"scouting_delay"`.
    /// Accepted values : `<float in seconds>`.
    /// Default value : `"0.2"`.
    pub const ZN_SCOUTING_DELAY_KEY: u64 = 0x49;
    pub const ZN_SCOUTING_DELAY_STR: &str = "scouting_delay";
    pub const ZN_SCOUTING_DELAY_DEFAULT: &str = "0.2";

    /// Indicates if data messages should be timestamped.
    /// String key : `"add_timestamp"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"false"`.
    pub const ZN_ADD_TIMESTAMP_KEY: u64 = 0x4A;
    pub const ZN_ADD_TIMESTAMP_STR: &str = "add_timestamp";
    pub const ZN_ADD_TIMESTAMP_DEFAULT: &str = ZN_FALSE;

    /// Indicates if the link state protocol should run.
    /// String key : `"link_state"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_LINK_STATE_KEY: u64 = 0x4B;
    pub const ZN_LINK_STATE_STR: &str = "link_state";
    pub const ZN_LINK_STATE_DEFAULT: &str = ZN_TRUE;

    /// The file path containing the user password dictionary.
    /// String key : `"user_password_dictionary"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_USER_PASSWORD_DICTIONARY_KEY: u64 = 0x4C;
    pub const ZN_USER_PASSWORD_DICTIONARY_STR: &str = "user_password_dictionary";

    /// Indicates if peers should connect to each other
    /// when they discover each other (through multicast
    /// or gossip discovery).
    /// String key : `"peers_autoconnect"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_PEERS_AUTOCONNECT_KEY: u64 = 0x4D;
    pub const ZN_PEERS_AUTOCONNECT_STR: &str = "peers_autoconnect";
    pub const ZN_PEERS_AUTOCONNECT_DEFAULT: &str = ZN_TRUE;

    /// The file path containing the TLS server private key.
    /// String key : `"tls_private_key"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_TLS_SERVER_PRIVATE_KEY_KEY: u64 = 0x4E;
    pub const ZN_TLS_SERVER_PRIVATE_KEY_STR: &str = "tls_server_private_key";

    /// The file path containing the TLS server certificate.
    /// String key : `"tls_private_key"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_TLS_SERVER_CERTIFICATE_KEY: u64 = 0x4F;
    pub const ZN_TLS_SERVER_CERTIFICATE_STR: &str = "tls_server_certificate";

    /// The file path containing the TLS root CA certificate.
    /// String key : `"tls_private_key"`.
    /// Accepted values : `<file path>`.
    /// Default value : None.
    pub const ZN_TLS_ROOT_CA_CERTIFICATE_KEY: u64 = 0x50;
    pub const ZN_TLS_ROOT_CA_CERTIFICATE_STR: &str = "tls_root_ca_certificate";

    /// Indicates if the zero-copy features should be used.
    /// String key : `"ser_copy"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_ZERO_COPY_KEY: u64 = 0x51;
    pub const ZN_ZERO_COPY_STR: &str = "zero_copy";
    pub const ZN_ZERO_COPY_DEFAULT: &str = ZN_TRUE;

    /// Indicates if routers should connect to each other
    /// when they discover each other through multicast.
    /// String key : `"routers_autoconnect_multicast"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"false"`.
    pub const ZN_ROUTERS_AUTOCONNECT_MULTICAST_KEY: u64 = 0x52;
    pub const ZN_ROUTERS_AUTOCONNECT_MULTICAST_STR: &str = "routers_autoconnect_multicast";
    pub const ZN_ROUTERS_AUTOCONNECT_MULTICAST_DEFAULT: &str = ZN_FALSE;

    /// Indicates if routers should connect to each other
    /// when they discover each other through gossip discovery.
    /// String key : `"routers_autoconnect_gossip"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"false"`.
    pub const ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY: u64 = 0x53;
    pub const ZN_ROUTERS_AUTOCONNECT_GOSSIP_STR: &str = "routers_autoconnect_gossip";
    pub const ZN_ROUTERS_AUTOCONNECT_GOSSIP_DEFAULT: &str = ZN_FALSE;

    /// Indicates if local writes/queries should reach local subscribers/queryables.
    /// String key : `"local_routing"`.
    /// Accepted values : `"true"`, `"false"`.
    /// Default value : `"true"`.
    pub const ZN_LOCAL_ROUTING_KEY: u64 = 0x60;
    pub const ZN_LOCAL_ROUTING_STR: &str = "local_routing";
    pub const ZN_LOCAL_ROUTING_DEFAULT: &str = ZN_TRUE;

    pub const ZN_JOIN_SUBSCRIPTIONS_KEY: u64 = 0x61;
    pub const ZN_JOIN_SUBSCRIPTIONS_STR: &str = "join_subscriptions";

    pub const ZN_JOIN_PUBLICATIONS_KEY: u64 = 0x62;
    pub const ZN_JOIN_PUBLICATIONS_STR: &str = "join_publications";
}

pub use consts::*;

pub type ConfigProperties = IntKeyProperties<ConfigTranscoder>;

pub struct ConfigTranscoder;

impl KeyTranscoder for ConfigTranscoder {
    fn encode(key: &str) -> Option<u64> {
        match &key.to_lowercase()[..] {
            ZN_MODE_STR => Some(ZN_MODE_KEY),
            ZN_PEER_STR => Some(ZN_PEER_KEY),
            ZN_LISTENER_STR => Some(ZN_LISTENER_KEY),
            ZN_USER_STR => Some(ZN_USER_KEY),
            ZN_PASSWORD_STR => Some(ZN_PASSWORD_KEY),
            ZN_MULTICAST_SCOUTING_STR => Some(ZN_MULTICAST_SCOUTING_KEY),
            ZN_MULTICAST_INTERFACE_STR => Some(ZN_MULTICAST_INTERFACE_KEY),
            ZN_MULTICAST_ADDRESS_STR => Some(ZN_MULTICAST_ADDRESS_KEY),
            ZN_SCOUTING_TIMEOUT_STR => Some(ZN_SCOUTING_TIMEOUT_KEY),
            ZN_SCOUTING_DELAY_STR => Some(ZN_SCOUTING_DELAY_KEY),
            ZN_ADD_TIMESTAMP_STR => Some(ZN_ADD_TIMESTAMP_KEY),
            ZN_LINK_STATE_STR => Some(ZN_LINK_STATE_KEY),
            ZN_USER_PASSWORD_DICTIONARY_STR => Some(ZN_USER_PASSWORD_DICTIONARY_KEY),
            ZN_PEERS_AUTOCONNECT_STR => Some(ZN_PEERS_AUTOCONNECT_KEY),
            ZN_TLS_SERVER_PRIVATE_KEY_STR => Some(ZN_TLS_SERVER_PRIVATE_KEY_KEY),
            ZN_TLS_SERVER_CERTIFICATE_STR => Some(ZN_TLS_SERVER_CERTIFICATE_KEY),
            ZN_TLS_ROOT_CA_CERTIFICATE_STR => Some(ZN_TLS_ROOT_CA_CERTIFICATE_KEY),
            ZN_ZERO_COPY_STR => Some(ZN_ZERO_COPY_KEY),
            ZN_ROUTERS_AUTOCONNECT_MULTICAST_STR => Some(ZN_ROUTERS_AUTOCONNECT_MULTICAST_KEY),
            ZN_ROUTERS_AUTOCONNECT_GOSSIP_STR => Some(ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY),
            ZN_LOCAL_ROUTING_STR => Some(ZN_LOCAL_ROUTING_KEY),
            ZN_JOIN_SUBSCRIPTIONS_STR => Some(ZN_JOIN_SUBSCRIPTIONS_KEY),
            ZN_JOIN_PUBLICATIONS_STR => Some(ZN_JOIN_PUBLICATIONS_KEY),
            _ => None,
        }
    }

    fn decode(key: u64) -> Option<String> {
        match key {
            ZN_MODE_KEY => Some(ZN_MODE_STR.to_string()),
            ZN_PEER_KEY => Some(ZN_PEER_STR.to_string()),
            ZN_LISTENER_KEY => Some(ZN_LISTENER_STR.to_string()),
            ZN_USER_KEY => Some(ZN_USER_STR.to_string()),
            ZN_PASSWORD_KEY => Some(ZN_PASSWORD_STR.to_string()),
            ZN_MULTICAST_SCOUTING_KEY => Some(ZN_MULTICAST_SCOUTING_STR.to_string()),
            ZN_MULTICAST_INTERFACE_KEY => Some(ZN_MULTICAST_INTERFACE_STR.to_string()),
            ZN_MULTICAST_ADDRESS_KEY => Some(ZN_MULTICAST_ADDRESS_STR.to_string()),
            ZN_SCOUTING_TIMEOUT_KEY => Some(ZN_SCOUTING_TIMEOUT_STR.to_string()),
            ZN_SCOUTING_DELAY_KEY => Some(ZN_SCOUTING_DELAY_STR.to_string()),
            ZN_ADD_TIMESTAMP_KEY => Some(ZN_ADD_TIMESTAMP_STR.to_string()),
            ZN_LINK_STATE_KEY => Some(ZN_LINK_STATE_STR.to_string()),
            ZN_USER_PASSWORD_DICTIONARY_KEY => Some(ZN_USER_PASSWORD_DICTIONARY_STR.to_string()),
            ZN_PEERS_AUTOCONNECT_KEY => Some(ZN_PEERS_AUTOCONNECT_STR.to_string()),
            ZN_TLS_SERVER_PRIVATE_KEY_KEY => Some(ZN_TLS_SERVER_PRIVATE_KEY_STR.to_string()),
            ZN_TLS_SERVER_CERTIFICATE_KEY => Some(ZN_TLS_SERVER_CERTIFICATE_STR.to_string()),
            ZN_TLS_ROOT_CA_CERTIFICATE_KEY => Some(ZN_TLS_ROOT_CA_CERTIFICATE_STR.to_string()),
            ZN_ZERO_COPY_KEY => Some(ZN_ZERO_COPY_STR.to_string()),
            ZN_ROUTERS_AUTOCONNECT_MULTICAST_KEY => {
                Some(ZN_ROUTERS_AUTOCONNECT_MULTICAST_STR.to_string())
            }
            ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY => {
                Some(ZN_ROUTERS_AUTOCONNECT_GOSSIP_STR.to_string())
            }
            ZN_LOCAL_ROUTING_KEY => Some(ZN_LOCAL_ROUTING_STR.to_string()),
            ZN_JOIN_SUBSCRIPTIONS_KEY => Some(ZN_JOIN_SUBSCRIPTIONS_STR.to_string()),
            ZN_JOIN_PUBLICATIONS_KEY => Some(ZN_JOIN_PUBLICATIONS_STR.to_string()),
            _ => None,
        }
    }
}
