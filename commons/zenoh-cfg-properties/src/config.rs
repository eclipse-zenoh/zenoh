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

/// `"true"`
pub const ZN_TRUE: &str = "true";
/// `"false"`
pub const ZN_FALSE: &str = "false";
/// `"auto"`
pub const ZN_AUTO: &str = "auto";

/// The library mode.
/// String key: `"mode"`.
/// Accepted values: `"peer"`, `"client"`.
/// Default value: `"peer"`.
pub const ZN_MODE_KEY: u64 = 0x40;
pub const ZN_MODE_STR: &str = "mode";
pub const ZN_MODE_DEFAULT: &str = "peer";

/// The locator of a peer to connect to.
/// String key: `"connect"`.
/// Accepted values: `<endpoint>` (ex: `"tcp/10.10.10.10:7447"`).
/// Default value: None.
/// Multiple values accepted.
pub const ZN_CONNECT_KEY: u64 = 0x41;
pub const ZN_CONNECT_STR: &str = "connect";

/// A locator to listen on.
/// String key: `"listen"`.
/// Accepted values: `<endpoint>` (ex: `"tcp/10.10.10.10:7447"`).
/// Default value: None.
/// Multiple values accepted.
pub const ZN_LISTEN_KEY: u64 = 0x42;
pub const ZN_LISTEN_STR: &str = "listen";

/// The user name to use for authentication.
/// String key: `"user"`.
/// Accepted values: `<string>`.
pub const ZN_USER_KEY: u64 = 0x43;
pub const ZN_USER_STR: &str = "user";

/// The password to use for authentication.
/// String key: `"password"`.
/// Accepted values: `<string>`.
pub const ZN_PASSWORD_KEY: u64 = 0x44;
pub const ZN_PASSWORD_STR: &str = "password";

/// Activates/Desactivates multicast scouting.
/// String key: `"multicast_scouting"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"true"`.
pub const ZN_MULTICAST_SCOUTING_KEY: u64 = 0x45;
pub const ZN_MULTICAST_SCOUTING_STR: &str = "multicast_scouting";
pub const ZN_MULTICAST_SCOUTING_DEFAULT: &str = ZN_TRUE;

/// The network interface to use for multicast scouting.
/// String key: `"multicast_interface"`.
/// Accepted values: `"auto"`, `<ip address>`, `<interface name>`.
/// Default value: `"auto"`.
pub const ZN_MULTICAST_INTERFACE_KEY: u64 = 0x46;
pub const ZN_MULTICAST_INTERFACE_STR: &str = "multicast_interface";
pub const ZN_MULTICAST_INTERFACE_DEFAULT: &str = ZN_AUTO;

/// The multicast IPv4 address and ports to use for multicast scouting.
/// String key: `"multicast_ipv4_address"`.
/// Accepted values: `<ipv4 address>:<port>`.
/// Default value: `"224.0.0.224:7446"`.
pub const ZN_MULTICAST_IPV4_ADDRESS_KEY: u64 = 0x47;
pub const ZN_MULTICAST_IPV4_ADDRESS_STR: &str = "multicast_ipv4_address";
pub const ZN_MULTICAST_IPV4_ADDRESS_DEFAULT: &str = "224.0.0.224:7446";

/// In client mode, the period dedicated to scouting a router before failing.
/// String key: `"scouting_timeout"`.
/// Accepted values: `<float in seconds>`.
/// Default value: `"3.0"`.
pub const ZN_SCOUTING_TIMEOUT_KEY: u64 = 0x48;
pub const ZN_SCOUTING_TIMEOUT_STR: &str = "scouting_timeout";
pub const ZN_SCOUTING_TIMEOUT_DEFAULT: &str = "3.0";

/// In peer mode, the period dedicated to scouting first remote peers before doing anything else.
/// String key: `"scouting_delay"`.
/// Accepted values: `<float in seconds>`.
/// Default value: `"0.2"`.
pub const ZN_SCOUTING_DELAY_KEY: u64 = 0x49;
pub const ZN_SCOUTING_DELAY_STR: &str = "scouting_delay";
pub const ZN_SCOUTING_DELAY_DEFAULT: &str = "0.2";

/// Indicates if data messages should be timestamped.
/// String key: `"add_timestamp"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"false"`.
pub const ZN_ADD_TIMESTAMP_KEY: u64 = 0x4A;
pub const ZN_ADD_TIMESTAMP_STR: &str = "add_timestamp";
pub const ZN_ADD_TIMESTAMP_DEFAULT: &str = ZN_FALSE;

/// Indicates if the link state protocol should run.
/// String key: `"link_state"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"true"`.
pub const ZN_LINK_STATE_KEY: u64 = 0x4B;
pub const ZN_LINK_STATE_STR: &str = "link_state";
pub const ZN_LINK_STATE_DEFAULT: &str = ZN_TRUE;

/// The file path containing the user password dictionary.
/// String key: `"user_password_dictionary"`.
/// Accepted values: `<file path>`.
pub const ZN_USER_PASSWORD_DICTIONARY_KEY: u64 = 0x4C;
pub const ZN_USER_PASSWORD_DICTIONARY_STR: &str = "user_password_dictionary";

/// Indicates if peers should connect to each other
/// when they discover each other (through multicast
/// or gossip discovery).
/// String key: `"peers_autoconnect"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"true"`.
pub const ZN_PEERS_AUTOCONNECT_KEY: u64 = 0x4D;
pub const ZN_PEERS_AUTOCONNECT_STR: &str = "peers_autoconnect";
pub const ZN_PEERS_AUTOCONNECT_DEFAULT: &str = ZN_TRUE;

/// The file path containing the TLS server private key.
/// String key: `"tls_server_private_key"`.
/// Accepted values: `<file path>`.
/// Default value: None.
pub const ZN_TLS_SERVER_PRIVATE_KEY_KEY: u64 = 0x4E;
pub const ZN_TLS_SERVER_PRIVATE_KEY_STR: &str = "tls_server_private_key";

/// The file path containing the TLS server certificate.
/// String key: `"tls_server_certificate"`.
/// Accepted values: `<file path>`.
/// Default value: None.
pub const ZN_TLS_SERVER_CERTIFICATE_KEY: u64 = 0x4F;
pub const ZN_TLS_SERVER_CERTIFICATE_STR: &str = "tls_server_certificate";

/// The file path containing the TLS root CA certificate.
/// String key: `"tls_root_ca_certificate"`.
/// Accepted values: `<file path>`.
/// Default value: None.
pub const ZN_TLS_ROOT_CA_CERTIFICATE_KEY: u64 = 0x50;
pub const ZN_TLS_ROOT_CA_CERTIFICATE_STR: &str = "tls_root_ca_certificate";

/// Indicates if the shared-memory features should be used.
/// String key: `"shm"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"true"`.
pub const ZN_SHM_KEY: u64 = 0x51;
pub const ZN_SHM_STR: &str = "shm";
pub const ZN_SHM_DEFAULT: &str = ZN_TRUE;

/// Indicates if routers should connect to each other
/// when they discover each other through multicast.
/// String key: `"routers_autoconnect_multicast"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"false"`.
pub const ZN_ROUTERS_AUTOCONNECT_MULTICAST_KEY: u64 = 0x52;
pub const ZN_ROUTERS_AUTOCONNECT_MULTICAST_STR: &str = "routers_autoconnect_multicast";
pub const ZN_ROUTERS_AUTOCONNECT_MULTICAST_DEFAULT: &str = ZN_FALSE;

/// Indicates if routers should connect to each other
/// when they discover each other through gossip discovery.
/// String key: `"routers_autoconnect_gossip"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"false"`.
pub const ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY: u64 = 0x53;
pub const ZN_ROUTERS_AUTOCONNECT_GOSSIP_STR: &str = "routers_autoconnect_gossip";
pub const ZN_ROUTERS_AUTOCONNECT_GOSSIP_DEFAULT: &str = ZN_FALSE;

pub const ZN_JOIN_SUBSCRIPTIONS_KEY: u64 = 0x61;
pub const ZN_JOIN_SUBSCRIPTIONS_STR: &str = "join_subscriptions";

pub const ZN_JOIN_PUBLICATIONS_KEY: u64 = 0x62;
pub const ZN_JOIN_PUBLICATIONS_STR: &str = "join_publications";

/// Configures the link lease expressed in milliseconds.
/// String key: `"link_lease"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `10000 (10 seconds)`.
pub const ZN_LINK_LEASE_KEY: u64 = 0x63;
pub const ZN_LINK_LEASE_STR: &str = "link_lease";
pub const ZN_LINK_LEASE_DEFAULT: &str = "10000";

/// Configures the number of keep-alive messages in a link lease duration.
/// String key: `"link_keep_alive"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `4 (2.5 seconds)`.
pub const ZN_LINK_KEEP_ALIVE_KEY: u64 = 0x64;
pub const ZN_LINK_KEEP_ALIVE_STR: &str = "link_keep_alive";
pub const ZN_LINK_KEEP_ALIVE_DEFAULT: &str = "4";

/// Configures the sequence number resolution.
/// String key: `"seq_num_resolution"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `268435456` (2^28).
pub const ZN_SEQ_NUM_RESOLUTION_KEY: u64 = 0x65;
pub const ZN_SEQ_NUM_RESOLUTION_STR: &str = "seq_num_resolution";
pub const ZN_SEQ_NUM_RESOLUTION_DEFAULT: &str = "268435456";

/// Configures the timeout in milliseconds when opening a link.
/// String key: `"open_timeout"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `10000`.
pub const ZN_OPEN_TIMEOUT_KEY: u64 = 0x66;
pub const ZN_OPEN_TIMEOUT_STR: &str = "open_timeout";
pub const ZN_OPEN_TIMEOUT_DEFAULT: &str = "10000";

/// Configures the number of open session that can be in pending state.
/// String key: `"open_pending"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `1024`.
pub const ZN_OPEN_INCOMING_PENDING_KEY: u64 = 0x67;
pub const ZN_OPEN_INCOMING_PENDING_STR: &str = "open_pending";
pub const ZN_OPEN_INCOMING_PENDING_DEFAULT: &str = "100";

/// Configures the peer ID.
/// String key: `"peer_id"`.
/// Accepted values: `<hex u128>`.
pub const ZN_PEER_ID_KEY: u64 = 0x68;
pub const ZN_PEER_ID_STR: &str = "peer_id";

/// Configures the batch size.
/// String key: `"batch_size"`.
/// Accepted values: `<unsigned 16-bit integer>`.
/// Default value: `65535`.
pub const ZN_BATCH_SIZE_KEY: u64 = 0x69;
pub const ZN_BATCH_SIZE_STR: &str = "batch_size";
pub const ZN_BATCH_SIZE_DEFAULT: &str = "65535";

/// Configures the maximum number of simultaneous open unicast sessions.
/// String key: `"max_sessions_unicast"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `1024`.
pub const ZN_MAX_SESSIONS_UNICAST_KEY: u64 = 0x70;
pub const ZN_MAX_SESSIONS_UNICAST_STR: &str = "max_sessions_unicast";
pub const ZN_MAX_SESSIONS_UNICAST_DEFAULT: &str = "1024";

/// Configures the maximum number of inbound links per open session.
/// String key: `"max_links"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `1`.
pub const ZN_MAX_LINKS_KEY: u64 = 0x71;
pub const ZN_MAX_LINKS_STR: &str = "max_links";
pub const ZN_MAX_LINKS_DEFAULT: &str = "1";

/// Configures the zenoh version.
/// String key: `"version"`.
/// Accepted values: `<unsigned integer>`.
pub const ZN_VERSION_KEY: u64 = 0x72;
pub const ZN_VERSION_STR: &str = "version";

/// Configures the QoS support.
/// String key: `"qos"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"true"`.
pub const ZN_QOS_KEY: u64 = 0x73;
pub const ZN_QOS_STR: &str = "qos";
pub const ZN_QOS_DEFAULT: &str = ZN_TRUE;

/// Configures the link keep alive expressed in milliseconds.
/// String key: `"join_interval"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `2500`.
pub const ZN_JOIN_INTERVAL_KEY: u64 = 0x74;
pub const ZN_JOIN_INTERVAL_STR: &str = "join_interval";
pub const ZN_JOIN_INTERVAL_DEFAULT: &str = "2500";

/// Configures the maximum size in bytes of the defragmentation
/// buffer at receiving side. Messages that have been fragmented
/// and that are larger than the configured size will be dropped.
/// String key: `"defrag_buff_size"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `1073741824` (1GiB).
pub const ZN_DEFRAG_BUFF_SIZE_KEY: u64 = 0x75;
pub const ZN_DEFRAG_BUFF_SIZE_STR: &str = "defrag_buff_size";
pub const ZN_DEFRAG_BUFF_SIZE_DEFAULT: &str = "1073741824";

/// Configures the buffer size in bytes at receiving side for each link.
/// String key: `"link_rx_buff_size"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `65535` (64KiB).
pub const ZN_LINK_RX_BUFF_SIZE_KEY: u64 = 0x76;
pub const ZN_LINK_RX_BUFF_SIZE_STR: &str = "link_rx_buff_size";
pub const ZN_LINK_RX_BUFF_SIZE_DEFAULT: &str = "65535";

/// The multicast IPv6 address and ports to use for multicast scouting.
/// String key: `"multicast_ipv6_address"`.
/// Accepted values: `<ipv6 address>:<port>`.
/// Default value: `"[ff24::224]:7446"`.
pub const ZN_MULTICAST_IPV6_ADDRESS_KEY: u64 = 0x77;
pub const ZN_MULTICAST_IPV6_ADDRESS_STR: &str = "multicast_ipv6_address";
pub const ZN_MULTICAST_IPV6_ADDRESS_DEFAULT: &str = "[ff24::224]:7446";

/// The public RSA key.
/// String key: `"auth_rsa_public_key_pem"`.
/// Accepted values: `<RSA key in PEM format>`.
pub const ZN_AUTH_RSA_PUBLIC_KEY_PEM_KEY: u64 = 0x78;
pub const ZN_AUTH_RSA_PUBLIC_KEY_PEM_STR: &str = "auth_rsa_public_key_pem";

/// The private RSA key.
/// String key: `"auth_rsa_private_key_pem"`.
/// Accepted values: `<RSA key in PEM format>`.
pub const ZN_AUTH_RSA_PRIVATE_KEY_PEM_KEY: u64 = 0x79;
pub const ZN_AUTH_RSA_PRIVATE_KEY_PEM_STR: &str = "auth_rsa_private_key_pem";

/// The public RSA key.
/// String key: `"auth_rsa_public_key_pem"`.
/// Accepted values: `<file path>`.
pub const ZN_AUTH_RSA_PUBLIC_KEY_FILE_KEY: u64 = 0x80;
pub const ZN_AUTH_RSA_PUBLIC_KEY_FILE_STR: &str = "auth_rsa_public_key_file";

/// The private RSA key.
/// String key: `"auth_rsa_private_key_pem"`.
/// Accepted values: `<file path>`.
pub const ZN_AUTH_RSA_PRIVATE_KEY_FILE_KEY: u64 = 0x81;
pub const ZN_AUTH_RSA_PRIVATE_KEY_FILE_STR: &str = "auth_rsa_private_key_file";

/// The default RSA key size.
/// String key: `"auth_rsa_key_size"`.
/// Accepted values: `<unsigned integer>`.
pub const ZN_AUTH_RSA_KEY_SIZE_KEY: u64 = 0x82;
pub const ZN_AUTH_RSA_KEY_SIZE_STR: &str = "auth_rsa_key_size";
pub const ZN_AUTH_RSA_KEY_SIZE_DEFAULT: &str = "512";

/// The list of known RSA public keys.
/// String key: `"auth_rsa_known_keys_file"`.
/// Accepted values: `<file path>`.
pub const ZN_AUTH_RSA_KNOWN_KEYS_FILE_KEY: u64 = 0x83;
pub const ZN_AUTH_RSA_KNOWN_KEYS_FILE_STR: &str = "auth_rsa_known_keys_file";

/// Configures the maximum number of simultaneous open unicast sessions.
/// String key: `"max_sessions_unicast"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `1024`.
pub const ZN_MAX_SESSIONS_MULTICAST_KEY: u64 = 0x84;
pub const ZN_MAX_SESSIONS_MULTICAST_STR: &str = "max_sessions_multicast";
pub const ZN_MAX_SESSIONS_MULTICAST_DEFAULT: &str = "1024";

/// The file path containing the TLS server private key.
/// String key: `"tls_client_private_key"`.
/// Accepted values: `<file path>`.
/// Default value: None.
pub const ZN_TLS_CLIENT_PRIVATE_KEY_KEY: u64 = 0x85;
pub const ZN_TLS_CLIENT_PRIVATE_KEY_STR: &str = "tls_client_private_key";

/// The file path containing the TLS server certificate.
/// String key: `"tls_client_private_key"`.
/// Accepted values: `<file path>`.
/// Default value: None.
pub const ZN_TLS_CLIENT_CERTIFICATE_KEY: u64 = 0x86;
pub const ZN_TLS_CLIENT_CERTIFICATE_STR: &str = "tls_client_certificate";

/// The file path containing the TLS server certificate.
/// String key: `"tls_private_key"`.
/// Accepted values: `"true"`, `"false"`.
/// Default value: `"false"`.
pub const ZN_TLS_CLIENT_AUTH_KEY: u64 = 0x87;
pub const ZN_TLS_CLIENT_AUTH_STR: &str = "tls_client_auth";
pub const ZN_TLS_CLIENT_AUTH_DEFAULT: &str = ZN_FALSE;

/// The default timeout to apply to queries in milliseconds.
/// String key: `"queries_default_timeout"`.
/// Accepted values: `<unsigned integer>`.
/// Default value: `10000`.
pub const ZN_QUERIES_DEFAULT_TIMEOUT_KEY: u64 = 0x88;
pub const ZN_QUERIES_DEFAULT_TIMEOUT_STR: &str = "local_routing";
pub const ZN_QUERIES_DEFAULT_TIMEOUT_DEFAULT: &str = "10000";
