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
pub use crate::net::protocol::core::{whatami, WhatAmI};
use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    convert::TryFrom,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};
use validated_struct::{GetError, ValidatedMap};
pub use zenoh_util::properties::config::*;
use zenoh_util::properties::IntKeyProperties;

/// A set of Key/Value (`u64`/`String`) pairs to pass to [`open`](super::open)  
/// to configure the zenoh [`Session`](crate::Session).
///
/// Multiple values are coma separated.
///
/// The [`IntKeyProperties`](zenoh_util::properties::IntKeyProperties) can be built from (`String`/`String`)
/// [`Properties`](crate::properties::Properties) and reverse.

/// Creates an empty zenoh net Session configuration.
pub fn empty() -> ConfigProperties {
    ConfigProperties::default()
}

/// Creates a default zenoh net Session configuration.
///
/// The returned configuration contains :
///  - `(ZN_MODE_KEY, "peer")`
pub fn default() -> ConfigProperties {
    peer()
}

/// Creates a default `'peer'` mode zenoh net Session configuration.
///
/// The returned configuration contains :
///  - `(ZN_MODE_KEY, "peer")`
pub fn peer() -> ConfigProperties {
    let mut props = ConfigProperties::default();
    props.insert(ZN_MODE_KEY, "peer".to_string());
    props
}

/// Creates a default `'client'` mode zenoh net Session configuration.
///
/// The returned configuration contains :
///  - `(ZN_MODE_KEY, "client")`
///
/// If the given peer locator is not `None`, the returned configuration also contains :
///  - `(ZN_PEER_KEY, <peer>)`
pub fn client(peer: Option<String>) -> ConfigProperties {
    let mut props = ConfigProperties::default();
    props.insert(ZN_MODE_KEY, "client".to_string());
    if let Some(peer) = peer {
        props.insert(ZN_PEER_KEY, peer);
    }
    props
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
        /// The node's mode (router, peer or client)
        #[intkey(ZN_MODE_KEY, into = whatami_to_cowstr, from = whatami_from_str)]
        mode: Option<whatami::Type> where (whatami_validator),
        /// Which locators to connect to.
        #[serde(default)]
        #[intkey(ZN_PEER_KEY, into = locvec_to_cowstr, from = locvec_from_str)]
        pub peers: Vec<Locator>,
        /// Which locators to listen on.
        #[serde(default)]
        #[intkey(ZN_LISTENER_KEY, into = locvec_to_cowstr, from = locvec_from_str)]
        pub listeners: Vec<Locator>,
        /// The configuration of authentification.
        /// A password implies a username is required.
        #[serde(default)]
        user: UserConf {
            #[intkey(ZN_USER_KEY, into = string_to_cowstr, from = string_from_str)]
            name: Option<String>,
            #[intkey(ZN_PASSWORD_KEY, into = string_to_cowstr, from = string_from_str)]
            password: Option<String>,
        } where (user_conf_validator),
        /// How multicast should behave
        #[serde(default)]
        pub multicast: MulticastConf {
            /// Whether multicast scouting is enabled or not
            #[intkey(ZN_MULTICAST_SCOUTING_KEY, into = bool_to_cowstr, from = bool_from_str)]
            scouting: Option<bool>,
            /// The socket which should be used for multicast scouting
            #[intkey(ZN_MULTICAST_IPV4_ADDRESS_KEY, into = addr_to_cowstr, from = addr_from_str)]
            address: Option<SocketAddr>,
            /// The network interface which should be used for multicast scouting
            #[intkey(ZN_MULTICAST_INTERFACE_KEY, into = string_to_cowstr, from = string_from_str)]
            interface: Option<String>,
        },
        #[serde(default)]
        pub scouting: ScoutingConf {
            /// In client mode, the period dedicated to scouting for a router before failing
            #[intkey(ZN_SCOUTING_TIMEOUT_KEY, into = f64_to_cowstr, from = f64_from_str)]
            timeout: Option<f64>,
            /// In peer mode, the period dedicated to scouting remote peers before attempting other operations
            #[intkey(ZN_SCOUTING_DELAY_KEY, into = f64_to_cowstr, from = f64_from_str)]
            delay: Option<f64>,
        },
        /// Whether data messages should be timestamped
        #[intkey(ZN_ADD_TIMESTAMP_KEY, into = bool_to_cowstr, from = bool_from_str)]
        add_timestamp: Option<bool>,
        /// Whether the link state protocol should be enabled
        #[intkey(ZN_LINK_STATE_KEY, into = bool_to_cowstr, from = bool_from_str)]
        link_state: Option<bool>,
        /// The path to a file containing the user password dictionary
        #[intkey(ZN_USER_PASSWORD_DICTIONARY_KEY, into = string_to_cowstr, from = string_from_str)]
        user_password_dictionary: Option<String>,
        /// Whether peers should connect to each other upon discovery (through multicast or gossip)
        #[intkey(ZN_PEERS_AUTOCONNECT_KEY, into = bool_to_cowstr, from = bool_from_str)]
        peers_autoconnect: Option<bool>,
        #[serde(default)]
        pub tls: TLSConf {
            #[intkey(ZN_TLS_SERVER_PRIVATE_KEY_KEY, into = string_to_cowstr, from = string_from_str)]
            server_private_key: Option<String>,
            #[intkey(ZN_TLS_SERVER_CERTIFICATE_KEY, into = string_to_cowstr, from = string_from_str)]
            server_certificate: Option<String>,
            #[intkey(ZN_TLS_ROOT_CA_CERTIFICATE_KEY, into = string_to_cowstr, from = string_from_str)]
            root_ca_certificate: Option<String>,
        },
        #[intkey(ZN_SHM_KEY, into = bool_to_cowstr, from = bool_from_str)]
        zero_copy: Option<bool>,
        #[serde(default)]
        pub routers_autoconnect: RoutersAutoconnectConf {
            /// Whether routers should connect to each other when discovered through multicast
            #[intkey(ZN_ROUTERS_AUTOCONNECT_MULTICAST_KEY, into = bool_to_cowstr, from = bool_from_str)]
            multicast: Option<bool>,
            /// Whether routers should connect to each other when discovered through gossip
            #[intkey(ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY, into = bool_to_cowstr, from = bool_from_str)]
            gossip: Option<bool>,
        },
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
        #[serde(default)]
        pub link: LinkConf {
            /// Link lease duration in milliseconds, into = usize_to_cowstr, from = usize_from_str
            #[intkey(ZN_LINK_LEASE_KEY, into = u64_to_cowstr, from = u64_from_str)]
            lease: Option<u64>,
            /// Link keep-alive duration in milliseconds
            #[intkey(ZN_LINK_KEEP_ALIVE_KEY, into = u64_to_cowstr, from = u64_from_str)]
            keep_alive: Option<u64>,
            /// Timeout in milliseconds when opening a link
            #[intkey(ZN_OPEN_TIMEOUT_KEY, into = u64_to_cowstr, from = u64_from_str)]
            open_timeout: Option<u64>,
            /// Receiving buffer size for each link
            #[intkey(ZN_LINK_RX_BUFF_SIZE_KEY, into = usize_to_cowstr, from = usize_from_str)]
            rx_buff_size: Option<usize>,
            #[intkey(ZN_MAX_LINKS_KEY, into = usize_to_cowstr, from = usize_from_str)]
            max_number: Option<usize>
        },
        #[intkey(ZN_SEQ_NUM_RESOLUTION_KEY, into = u64_to_cowstr, from = u64_from_str)]
        sequence_number_resolution: Option<u64>,
        #[intkey(ZN_OPEN_INCOMING_PENDING_KEY, into = usize_to_cowstr, from = usize_from_str)]
        open_pending: Option<usize>,
        #[intkey(ZN_MAX_SESSIONS_KEY, into = usize_to_cowstr, from = usize_from_str)]
        max_sessions: Option<usize>,
        #[intkey(ZN_VERSION_KEY, into = u8_to_cowstr, from = u8_from_str)]
        version: Option<u8>,
        #[intkey(ZN_PEER_ID_KEY, into = string_to_cowstr, from = string_from_str)]
        peer_id: Option<String>,
        #[intkey(ZN_BATCH_SIZE_KEY, into = u16_to_cowstr, from = u16_from_str)]
        batch_size: Option<u16>,
        /// Maximum size of the defragmentation buffer at receiver end.
        /// Fragmented messages that are larger than the configured size will be dropped.
        #[intkey(ZN_DEFRAG_BUFF_SIZE_KEY, into = usize_to_cowstr, from = usize_from_str)]
        defrag_buffer_size: Option<usize>,
        #[intkey(ZN_QOS_KEY, into = bool_to_cowstr, from = bool_from_str)]
        qos: Option<bool>,
    }
}
#[derive(Debug)]
pub enum ConfigOpenErr {
    CouldNotOpenFile(std::io::Error),
    JsonParseErr(serde_json::Error),
    InvalidConfiguration(Box<Config>),
}
impl std::fmt::Display for ConfigOpenErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigOpenErr::CouldNotOpenFile(e) => write!(f, "Couldn't open file : {}", e),
            ConfigOpenErr::JsonParseErr(e) => write!(f, "JSON parsing error {}", e),
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
            Ok(f) => match Config::from_deserializer(&mut serde_json::Deserializer::from_reader(f))
            {
                Ok(s) => Ok(s),
                Err(e) => Err(match e {
                    Ok(c) => ConfigOpenErr::InvalidConfiguration(Box::new(c)),
                    Err(e) => ConfigOpenErr::JsonParseErr(e),
                }),
            },
            Err(e) => Err(ConfigOpenErr::CouldNotOpenFile(e)),
        }
    }
}

#[test]
fn config_from_json() {
    use validated_struct::ValidatedMap;
    let from_str = |s| serde_json::Deserializer::from_str(s);
    let mut config = Config::from_deserializer(&mut from_str(r#"{}"#)).unwrap();
    config.insert("link/lease", &mut from_str("168")).unwrap();
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
            self.inner.subscribers.lock().unwrap().push(tx);
        }
        rx
    }
    pub fn notify<K: AsRef<str>>(&self, key: K) {
        let key: Arc<str> = Arc::from(key.as_ref());
        let mut marked = Vec::new();
        let mut guard = self.inner.subscribers.lock().unwrap();
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
        self.inner.inner.lock().unwrap()
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
            let mut guard = self.inner.inner.lock().unwrap();
            guard.insert(key, value)?;
        }
        self.notify(key);
        Ok(())
    }

    pub fn get<'a, K: AsRef<str>>(&'a self, key: K) -> Result<GetGuard<'a, T>, GetError> {
        let guard: MutexGuard<'a, T> = self.inner.inner.lock().unwrap();
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

impl<'a> TryFrom<&'a HashMap<u64, String>> for Config {
    type Error = Config;

    fn try_from(value: &'a HashMap<u64, String>) -> Result<Self, Self::Error> {
        let mut s: Self = Default::default();
        let mut merge_error = false;
        for (key, value) in value {
            s.iset(*key, value).is_err().then(|| merge_error = true);
        }
        if merge_error || s.validate_rec() {
            Ok(s)
        } else {
            Err(s)
        }
    }
}

impl<'a> TryFrom<&'a IntKeyProperties<ConfigTranscoder>> for Config {
    type Error = Config;

    fn try_from(value: &'a IntKeyProperties<ConfigTranscoder>) -> Result<Self, Self::Error> {
        let mut s: Self = Default::default();
        let mut merge_error = false;
        for (key, value) in &value.0 {
            s.iset(*key, value).is_err().then(|| merge_error = true);
        }
        if merge_error || s.validate_rec() {
            Ok(s)
        } else {
            Err(s)
        }
    }
}

impl TryFrom<IntKeyProperties<ConfigTranscoder>> for Config {
    type Error = Config;

    fn try_from(value: IntKeyProperties<ConfigTranscoder>) -> Result<Self, Self::Error> {
        let mut s: Self = Default::default();
        let mut merge_error = false;
        for (key, value) in value.0 {
            s.iset(key, value).is_err().then(|| merge_error = true);
        }
        if merge_error || s.validate_rec() {
            Ok(s)
        } else {
            Err(s)
        }
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

fn whatami_validator(w: &Option<whatami::Type>) -> bool {
    use whatami::*;
    if let Some(w) = *w {
        w == ROUTER || w == PEER || w == CLIENT
    } else {
        true
    }
}
fn user_conf_validator(u: &UserConf) -> bool {
    !(u.password().is_some() && u.name().is_none())
}

fn whatami_to_cowstr(w: &Option<whatami::Type>) -> Option<Cow<str>> {
    use whatami::*;
    w.map(|f| {
        match f {
            ROUTER => "router",
            PEER => "peer",
            CLIENT => "client",
            _ => panic!("{} should be an impossible value for whatami::Type", f),
        }
        .into()
    })
}

fn whatami_from_str(w: &str) -> Option<Option<whatami::Type>> {
    use whatami::*;
    match w {
        "router" => Some(Some(ROUTER)),
        "peer" => Some(Some(PEER)),
        "client" => Some(Some(CLIENT)),
        "" => Some(None),
        _ => None,
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

fn u8_to_cowstr(s: &Option<u8>) -> Option<Cow<str>> {
    s.map(|s| format!("{}", s).into())
}

fn u8_from_str(s: &str) -> Option<Option<u8>> {
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
