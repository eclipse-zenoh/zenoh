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

//! Properties to pass to [`open`](super::open) and [`scout`](super::scout) functions as configuration
//! and associated constants.

use crate::net::link::Locator;
pub use crate::net::protocol::core::{whatami, WhatAmI, ZInt};
use serde_json::Value;
use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    io::Read,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};
use validated_struct::{GetError, ValidatedMap};
use zenoh_plugin_trait::ValidationFunction;
pub use zenoh_util::properties::config::*;
use zenoh_util::LibLoader;

/// A set of Key/Value (`u64`/`String`) pairs to pass to [`open`](super::open)  
/// to configure the zenoh [`Session`](crate::Session).
///
/// Multiple values are coma separated.
///
/// The [`IntKeyProperties`](zenoh_util::properties::IntKeyProperties) can be built from (`String`/`String`)
/// [`Properties`](crate::properties::Properties) and reverse.

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
pub fn client<I: IntoIterator<Item = T>, T: Into<Locator>>(peers: I) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config.peers.extend(peers.into_iter().map(|t| t.into()));
    config
}

#[test]
fn config_keys() {
    use validated_struct::ValidatedMap;
    let c = Config::default();
    dbg!(c.ikeys());
    dbg!(c.keys());
}
validated_struct::validator! {
    #[recursive_attrs]
    #[derive(serde::Deserialize, serde::Serialize, Clone, Debug, Default, IntKeyMapLike)]
    Config {
        #[intkey(ZN_PEER_ID_KEY, into = string_to_cowstr, from = string_from_str)]
        peer_id: Option<String>,
        /// The node's mode (router, peer or client)
        #[intkey(ZN_MODE_KEY, into = whatami_to_cowstr, from = whatami_from_str)]
        mode: Option<whatami::WhatAmI>,
        /// Which locators to connect to.
        #[serde(default)]
        #[intkey(ZN_PEER_KEY, into = locvec_to_cowstr, from = locvec_from_str)]
        pub peers: Vec<Locator>,
        /// Which locators to listen on.
        #[serde(default)]
        #[intkey(ZN_LISTENER_KEY, into = locvec_to_cowstr, from = locvec_from_str)]
        pub listeners: Vec<Locator>,
        #[serde(default)]
        pub scouting: ScoutingConf {
            /// In client mode, the period dedicated to scouting for a router before failing
            #[intkey(ZN_SCOUTING_TIMEOUT_KEY, into = f64_to_cowstr, from = f64_from_str)]
            timeout: Option<f64>,
            /// In peer mode, the period dedicated to scouting remote peers before attempting other operations
            #[intkey(ZN_SCOUTING_DELAY_KEY, into = f64_to_cowstr, from = f64_from_str)]
            delay: Option<f64>,
            /// How multicast should behave
            #[serde(default)]
            pub multicast: ScoutingMulticastConf {
                /// Whether multicast scouting is enabled or not
                #[intkey(ZN_MULTICAST_SCOUTING_KEY, into = bool_to_cowstr, from = bool_from_str)]
                enabled: Option<bool>,
                /// The socket which should be used for multicast scouting
                #[intkey(ZN_MULTICAST_IPV4_ADDRESS_KEY, into = addr_to_cowstr, from = addr_from_str)]
                address: Option<SocketAddr>,
                /// The network interface which should be used for multicast scouting
                #[intkey(ZN_MULTICAST_INTERFACE_KEY, into = string_to_cowstr, from = string_from_str)]
                interface: Option<String>,
                #[intkey(ZN_ROUTERS_AUTOCONNECT_MULTICAST_KEY, into = whatamimatcher_to_cowstr, from = whatamimatcher_from_str)]
                autoconnect: Option<whatami::WhatAmIMatcher>,
            },
            #[serde(default)]
            pub gossip: GossipConf {
                #[intkey(skip)]
                enabled: Option<bool>,
                #[intkey(ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY, into = whatamimatcher_to_cowstr, from = whatamimatcher_from_str)]
                autoconnect: Option<whatami::WhatAmIMatcher>,
            }
        },
        /// Whether data messages should be timestamped
        #[intkey(ZN_ADD_TIMESTAMP_KEY, into = bool_to_cowstr, from = bool_from_str)]
        add_timestamp: Option<bool>,
        /// Whether the link state protocol should be enabled
        #[intkey(ZN_LINK_STATE_KEY, into = bool_to_cowstr, from = bool_from_str)]
        link_state: Option<bool>,
        /// Whether peers should connect to each other upon discovery (through multicast or gossip)
        #[intkey(ZN_PEERS_AUTOCONNECT_KEY, into = bool_to_cowstr, from = bool_from_str)]
        peers_autoconnect: Option<bool>,
        /// Whether local writes/queries should reach local subscribers/queryables
        #[intkey(ZN_LOCAL_ROUTING_KEY, into = bool_to_cowstr, from = bool_from_str)]
        local_routing: Option<bool>,
        #[serde(default)]
        pub join_on_startup: JoinConfig {
            #[serde(default)]
            #[intkey(ZN_JOIN_SUBSCRIPTIONS_KEY, into = commastringvec_to_cowstr, from = commastringvec_from_str)]
            subscriptions: Vec<String>,
            #[serde(default)]
            #[intkey(ZN_JOIN_PUBLICATIONS_KEY, into = commastringvec_to_cowstr, from = commastringvec_from_str)]
            publications: Vec<String>,
        },
        #[intkey(ZN_SHM_KEY, into = bool_to_cowstr, from = bool_from_str)]
        shared_memory: Option<bool>,
        #[serde(default)]
        pub transport: TransportConf {
            #[intkey(ZN_SEQ_NUM_RESOLUTION_KEY, into = u64_to_cowstr, from = u64_from_str)]
            sequence_number_resolution: Option<ZInt>,
            #[intkey(ZN_QOS_KEY, into = bool_to_cowstr, from = bool_from_str)]
            qos: Option<bool>,
            #[serde(default)]
            pub unicast: TransportUnicastConf {
                /// Timeout in milliseconds when opening a link
                #[intkey(ZN_OPEN_TIMEOUT_KEY, into = u64_to_cowstr, from = u64_from_str)]
                open_timeout: Option<ZInt>,
                #[intkey(ZN_OPEN_INCOMING_PENDING_KEY, into = usize_to_cowstr, from = usize_from_str)]
                open_pending: Option<usize>,
                #[intkey(ZN_MAX_SESSIONS_UNICAST_KEY, into = usize_to_cowstr, from = usize_from_str)]
                max_sessions: Option<usize>,
                #[intkey(ZN_MAX_LINKS_KEY, into = usize_to_cowstr, from = usize_from_str)]
                max_links: Option<usize>,
            },
            #[serde(default)]
            pub multicast: TransportMulticastConf {
                /// Link keep-alive duration in milliseconds
                #[intkey(ZN_JOIN_INTERVAL_KEY, into = u64_to_cowstr, from = u64_from_str)]
                join_interval: Option<ZInt>,
                #[intkey(ZN_MAX_SESSIONS_MULTICAST_KEY, into = usize_to_cowstr, from = usize_from_str)]
                max_sessions: Option<usize>,
            },
            #[serde(default)]
            pub link: TransportLinkConf {
                #[intkey(ZN_BATCH_SIZE_KEY, into = u16_to_cowstr, from = u16_from_str)]
                batch_size: Option<u16>,
                /// Link lease duration in milliseconds, into = usize_to_cowstr, from = usize_from_str
                #[intkey(ZN_LINK_LEASE_KEY, into = u64_to_cowstr, from = u64_from_str)]
                lease: Option<ZInt>,
                /// Link keep-alive duration in milliseconds
                #[intkey(ZN_LINK_KEEP_ALIVE_KEY, into = u64_to_cowstr, from = u64_from_str)]
                keep_alive: Option<ZInt>,
                /// Receiving buffer size for each link
                #[intkey(ZN_LINK_RX_BUFF_SIZE_KEY, into = usize_to_cowstr, from = usize_from_str)]
                rx_buff_size: Option<usize>,
                /// Maximum size of the defragmentation buffer at receiver end.
                /// Fragmented messages that are larger than the configured size will be dropped.
                #[intkey(ZN_DEFRAG_BUFF_SIZE_KEY, into = usize_to_cowstr, from = usize_from_str)]
                defrag_buffer_size: Option<usize>,
                #[serde(default)]
                pub tls: TLSConf {
                    #[intkey(ZN_TLS_ROOT_CA_CERTIFICATE_KEY, into = string_to_cowstr, from = string_from_str)]
                    root_ca_certificate: Option<String>,
                    #[intkey(ZN_TLS_SERVER_PRIVATE_KEY_KEY, into = string_to_cowstr, from = string_from_str)]
                    server_private_key: Option<String>,
                    #[intkey(ZN_TLS_SERVER_CERTIFICATE_KEY, into = string_to_cowstr, from = string_from_str)]
                    server_certificate: Option<String>,
                    #[intkey(ZN_TLS_CLIENT_AUTH_KEY, into = bool_to_cowstr, from = bool_from_str)]
                    client_auth: Option<bool>,
                    #[intkey(ZN_TLS_CLIENT_PRIVATE_KEY_KEY, into = string_to_cowstr, from = string_from_str)]
                    client_private_key: Option<String>,
                    #[intkey(ZN_TLS_CLIENT_CERTIFICATE_KEY, into = string_to_cowstr, from = string_from_str)]
                    client_certificate: Option<String>,
                },
            },
            #[serde(default)]
            pub auth: AuthConf {
                /// The configuration of authentification.
                /// A password implies a username is required.
                #[serde(default)]
                pub usrpwd: UserConf {
                    #[intkey(ZN_USER_KEY, into = string_to_cowstr, from = string_from_str)]
                    user: Option<String>,
                    #[intkey(ZN_PASSWORD_KEY, into = string_to_cowstr, from = string_from_str)]
                    password: Option<String>,
                    /// The path to a file containing the user password dictionary
                    #[intkey(ZN_USER_PASSWORD_DICTIONARY_KEY, into = string_to_cowstr, from = string_from_str)]
                    dictionary_file: Option<String>,
                } where (user_conf_validator),
                #[serde(default)]
                pub pubkey: PubKeyConf {
                    #[intkey(ZN_AUTH_RSA_PUBLIC_KEY_PEM_KEY, into = string_to_cowstr, from = string_from_str)]
                    public_key_pem: Option<String>,
                    #[intkey(ZN_AUTH_RSA_PRIVATE_KEY_PEM_KEY, into = string_to_cowstr, from = string_from_str)]
                    private_key_pem: Option<String>,
                    #[intkey(ZN_AUTH_RSA_PUBLIC_KEY_FILE_KEY, into = string_to_cowstr, from = string_from_str)]
                    public_key_file: Option<String>,
                    #[intkey(ZN_AUTH_RSA_PRIVATE_KEY_FILE_KEY, into = string_to_cowstr, from = string_from_str)]
                    private_key_file: Option<String>,
                    #[intkey(ZN_AUTH_RSA_KEY_SIZE_KEY, into = usize_to_cowstr, from = usize_from_str)]
                    key_size: Option<usize>,
                    #[intkey(ZN_AUTH_RSA_KNOWN_KEYS_FILE_KEY, into = string_to_cowstr, from = string_from_str)]
                    known_keys_file: Option<String>,
                },
            },
        },
        #[serde(default)]
        #[intkey(skip)]
        plugins_search_dirs: Vec<String>,
        #[intkey(skip)]
        #[serde(default)]
        #[validated(recursive_accessors)]
        plugins: PluginsConfig,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSearchDirs(Vec<String>);
impl Default for PluginSearchDirs {
    fn default() -> Self {
        Self(
            (*zenoh_util::LIB_DEFAULT_SEARCH_PATHS)
                .split(':')
                .map(|c| c.to_string())
                .collect(),
        )
    }
}

#[test]
fn config_deser() {
    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
        scouting: {
          multicast: {
            enabled: false,
            autoconnect: "router"
          }
        }
      }"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(*config.scouting().multicast().enabled(), Some(false))
}

impl Config {
    pub fn add_plugin_validator(&mut self, name: impl Into<String>, validator: ValidationFunction) {
        self.plugins.validators.insert(name.into(), validator);
    }
    pub fn insert_json<K: AsRef<str>>(
        &mut self,
        key: K,
        value: &str,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<serde_json::Error>,
    {
        self.insert(key.as_ref(), &mut serde_json::Deserializer::from_str(value))
    }

    pub fn insert_json5<K: AsRef<str>>(
        &mut self,
        key: K,
        value: &str,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<json5::Error>,
    {
        self.insert(key.as_ref(), &mut json5::Deserializer::from_str(value)?)
    }

    pub fn plugin(&self, name: &str) -> Option<&Value> {
        self.plugins.values.get(name)
    }

    pub fn sift_privates(&self) -> Self {
        let mut copy = self.clone();
        copy.plugins.sift_privates();
        copy
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
            ConfigOpenErr::IoError(e) => write!(f, "Couldn't open file : {}", e),
            ConfigOpenErr::JsonParseErr(e) => write!(f, "JSON5 parsing error {}", e),
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
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigOpenErr> {
        match std::fs::File::open(path) {
            Ok(mut f) => {
                let mut content = String::new();
                if let Err(e) = f.read_to_string(&mut content) {
                    return Err(ConfigOpenErr::IoError(e));
                }
                let mut deser = match json5::Deserializer::from_str(&content) {
                    Ok(d) => d,
                    Err(e) => return Err(ConfigOpenErr::JsonParseErr(e)),
                };
                match Config::from_deserializer(&mut deser) {
                    Ok(s) => Ok(s),
                    Err(e) => Err(match e {
                        Ok(c) => ConfigOpenErr::InvalidConfiguration(Box::new(c)),
                        Err(e) => ConfigOpenErr::JsonParseErr(e),
                    }),
                }
            }
            Err(e) => Err(ConfigOpenErr::IoError(e)),
        }
    }
    pub fn libloader(&self) -> LibLoader {
        LibLoader::new(&self.plugins_search_dirs, true)
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

#[test]
fn config_from_json() {
    use validated_struct::ValidatedMap;
    let from_str = serde_json::Deserializer::from_str;
    let mut config = Config::from_deserializer(&mut from_str(r#"{}"#)).unwrap();
    config
        .insert("transport/link/lease", &mut from_str("168"))
        .unwrap();
    dbg!(std::mem::size_of_val(&config));
    println!("{}", serde_json::to_string_pretty(&config).unwrap());
}

pub type Notification = Arc<str>;

struct NotifierInner<T> {
    inner: Mutex<T>,
    subscribers: Mutex<Vec<flume::Sender<Notification>>>,
}
pub struct Notifier<T> {
    inner: Arc<NotifierInner<T>>,
}
impl<T> Clone for Notifier<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<T: ValidatedMap> Notifier<T> {
    pub fn new(inner: T) -> Self {
        Notifier {
            inner: Arc::new(NotifierInner {
                inner: Mutex::new(inner),
                subscribers: Mutex::new(Vec::new()),
            }),
        }
    }
    pub fn subscribe(&self) -> flume::Receiver<Notification> {
        let (tx, rx) = flume::unbounded();
        {
            zlock!(self.inner.subscribers).push(tx);
        }
        rx
    }
    pub fn notify<K: AsRef<str>>(&self, key: K) {
        let key: Arc<str> = Arc::from(key.as_ref());
        let mut marked = Vec::new();
        let mut guard = zlock!(self.inner.subscribers);
        for (i, sub) in guard.iter().enumerate() {
            if sub.send(key.clone()).is_err() {
                marked.push(i)
            }
        }
        for i in marked.into_iter().rev() {
            guard.swap_remove(i);
        }
    }

    pub fn lock(&self) -> MutexGuard<T> {
        zlock!(self.inner.inner)
    }

    pub fn insert<'d, D: serde::Deserializer<'d>, K: AsRef<str>>(
        &self,
        key: K,
        value: D,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<D::Error>,
    {
        let key = key.as_ref();
        {
            let mut guard = zlock!(self.inner.inner);
            guard.insert(key, value)?;
        }
        self.notify(key);
        Ok(())
    }

    pub fn insert_json<K: AsRef<str>>(
        &self,
        key: K,
        value: &str,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<serde_json::Error>,
    {
        let key = key.as_ref();
        {
            let mut guard = zlock!(self.inner.inner);
            guard.insert(key, &mut serde_json::Deserializer::from_str(value))?;
        }
        self.notify(key);
        Ok(())
    }

    pub fn insert_json5<K: AsRef<str>>(
        &self,
        key: K,
        value: &str,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<json5::Error>,
    {
        let key = key.as_ref();
        {
            let mut guard = zlock!(self.inner.inner);
            guard.insert(key, &mut json5::Deserializer::from_str(value)?)?;
        }
        self.notify(key);
        Ok(())
    }

    pub fn get<'a, K: AsRef<str>>(&'a self, key: K) -> Result<GetGuard<'a, T>, GetError> {
        let guard: MutexGuard<'a, T> = zlock!(self.inner.inner);
        // Safety: MutexGuard pins the mutex behind which the value is held.
        let subref = guard.get(key.as_ref())? as *const _;
        Ok(GetGuard {
            _guard: guard,
            subref,
        })
    }
}

pub struct GetGuard<'a, T> {
    _guard: MutexGuard<'a, T>,
    subref: *const dyn Any,
}
use std::ops::Deref;
impl<'a, T> Deref for GetGuard<'a, T> {
    type Target = dyn Any;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.subref }
    }
}
impl<'a, T> AsRef<dyn Any> for GetGuard<'a, T> {
    fn as_ref(&self) -> &dyn Any {
        self.deref()
    }
}

impl<'a> From<&'a Config> for ConfigProperties {
    fn from(c: &'a Config) -> Self {
        let mut result = ConfigProperties::default();
        for key in c.ikeys() {
            if let Some(v) = c.iget(key) {
                result.insert(key, v.into_owned());
            }
        }
        result
    }
}

impl From<Config> for ConfigProperties {
    fn from(c: Config) -> Self {
        let mut result = ConfigProperties::default();
        for key in c.ikeys() {
            if let Some(v) = c.iget(key) {
                result.insert(key, v.into_owned());
            }
        }
        result
    }
}

fn user_conf_validator(u: &UserConf) -> bool {
    (u.password().is_none() && u.user().is_none()) || (u.password().is_some() && u.user().is_some())
}

fn whatami_to_cowstr(w: &Option<whatami::WhatAmI>) -> Option<Cow<str>> {
    w.map(|f| {
        match f {
            whatami::WhatAmI::Router => "router",
            whatami::WhatAmI::Peer => "peer",
            whatami::WhatAmI::Client => "client",
        }
        .into()
    })
}

fn whatami_from_str(w: &str) -> Option<Option<whatami::WhatAmI>> {
    match w {
        "router" => Some(Some(whatami::WhatAmI::Router)),
        "peer" => Some(Some(whatami::WhatAmI::Peer)),
        "client" => Some(Some(whatami::WhatAmI::Client)),
        "" => Some(None),
        _ => None,
    }
}

fn whatamimatcher_to_cowstr(w: &Option<whatami::WhatAmIMatcher>) -> Option<Cow<str>> {
    w.map(|f| f.to_str().into())
}

fn whatamimatcher_from_str(w: &str) -> Option<Option<whatami::WhatAmIMatcher>> {
    if w.is_empty() {
        Some(None)
    } else {
        match w.parse() {
            Ok(w) => Some(Some(w)),
            Err(_) => None,
        }
    }
}

fn locvec_to_cowstr(w: &[Locator]) -> Option<Cow<str>> {
    Some(
        w.iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .as_slice()
            .join(",")
            .into(),
    )
}

fn locvec_from_str(w: &str) -> Option<Vec<Locator>> {
    let mut result = Vec::new();
    for f in w.split(',') {
        result.push(f.parse().ok()?);
    }
    Some(result)
}

fn commastringvec_to_cowstr(w: &[String]) -> Option<Cow<str>> {
    Some(w.join(",").into())
}

fn commastringvec_from_str(w: &str) -> Option<Vec<String>> {
    Some(w.split(',').map(str::to_string).collect())
}

fn string_to_cowstr(s: &Option<String>) -> Option<Cow<str>> {
    s.as_ref().map(|s| Cow::Owned(s.clone()))
}

fn string_from_str(s: &str) -> Option<Option<String>> {
    Some(Some(s.to_string()))
}

fn bool_to_cowstr(s: &Option<bool>) -> Option<Cow<str>> {
    s.map(|s| Cow::Borrowed(if s { "true" } else { "false" }))
}

fn bool_from_str(s: &str) -> Option<Option<bool>> {
    match s {
        "true" => Some(Some(true)),
        "false" => Some(Some(false)),
        _ => None,
    }
}

fn usize_to_cowstr(s: &Option<usize>) -> Option<Cow<str>> {
    s.map(|s| format!("{}", s).into())
}

fn usize_from_str(s: &str) -> Option<Option<usize>> {
    s.parse().ok().map(Some)
}

fn u64_to_cowstr(s: &Option<u64>) -> Option<Cow<str>> {
    s.map(|s| format!("{}", s).into())
}

fn u64_from_str(s: &str) -> Option<Option<u64>> {
    s.parse().ok().map(Some)
}

fn u16_to_cowstr(s: &Option<u16>) -> Option<Cow<str>> {
    s.map(|s| format!("{}", s).into())
}

fn u16_from_str(s: &str) -> Option<Option<u16>> {
    s.parse().ok().map(Some)
}

fn f64_to_cowstr(s: &Option<f64>) -> Option<Cow<str>> {
    s.map(|s| format!("{:?}", s).into())
}

fn f64_from_str(s: &str) -> Option<Option<f64>> {
    s.parse().ok().map(Some)
}

fn addr_to_cowstr(s: &Option<SocketAddr>) -> Option<Cow<str>> {
    s.map(|s| format!("{}", s).into())
}

fn addr_from_str(s: &str) -> Option<Option<SocketAddr>> {
    s.parse().ok().map(Some)
}

#[derive(Clone)]
pub struct PluginsConfig {
    values: Value,
    validators: HashMap<String, zenoh_plugin_trait::ValidationFunction>,
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
pub struct PluginLoad {
    pub name: String,
    pub paths: Option<Vec<String>>,
    pub required: bool,
}
impl PluginsConfig {
    pub fn sift_privates(&mut self) {
        sift_privates(&mut self.values);
    }
    pub fn load_requests(&'_ self) -> impl Iterator<Item = PluginLoad> + '_ {
        self.values.as_object().unwrap().iter().map(|(name, value)| {
            let value = value.as_object().expect("Plugin configurations must be objects");
            let required = match value.get("__required__") {
                None => false,
                Some(Value::Bool(b)) => *b,
                _ => panic!("Plugin '{}' has an invalid '__required__' configuration property (must be a boolean)", name)
            };
            if let Some(paths) = value.get("__path__"){
                let paths = match paths {
                    Value::String(s) => vec![s.clone()],
                    Value::Array(a) => a.iter().map(|s| if let Value::String(s) = s {s.clone()} else {panic!("Plugin '{}' has an invalid '__path__' configuration property (must be either string or array of strings)", name)}).collect(),
                    _ => panic!("Plugin '{}' has an invalid '__path__' configuration property (must be either string or array of strings)", name)
                };
                PluginLoad {name: name.clone(), paths: Some(paths), required}
            } else {
                PluginLoad {name: name.clone(), paths: None, required}
            }
        })
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
            validators: Default::default(),
        }
    }
}
impl<'a> serde::Deserialize<'a> for PluginsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        Ok(PluginsConfig {
            validators: Default::default(),
            values: serde::Deserialize::deserialize(deserializer)?,
        })
    }
}
impl std::fmt::Debug for PluginsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.values)
    }
}
impl IntKeyMapLike for PluginsConfig {
    type Keys = Option<u64>;
    fn iget(&self, _: u64) -> Option<Cow<'_, str>> {
        None
    }
    fn iset<S: Into<String> + AsRef<str>>(&mut self, _: u64, _: S) -> Result<(), ()> {
        Err(())
    }
    fn ikeys(&self) -> Self::Keys {
        None
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
                "{} not found",
                path
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
        let validator = self.validators.get(plugin);
        let new_value: Value = serde::Deserialize::deserialize(deserializer)?;
        let value = self
            .values
            .as_object_mut()
            .unwrap()
            .entry(plugin)
            .or_insert(Value::Null);
        let mut new_value = value.clone().merge(key, new_value)?;
        if let Some(validator) = validator {
            match validator(
                key,
                value.as_object().unwrap(),
                new_value.as_object().unwrap(),
            ) {
                Ok(Some(val)) => new_value = Value::Object(val),
                Ok(None) => {}
                Err(e) => return Err(format!("{}", e).into()),
            }
        }
        *value = new_value;
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
}
