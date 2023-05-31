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

//! Configuration to pass to `zenoh::open()` and `zenoh::scout()` functions and associated constants.
pub mod defaults;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Serialize,
};
use serde_json::Value;
use std::{
    any::Any,
    collections::HashMap,
    fmt,
    io::Read,
    marker::PhantomData,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};
use validated_struct::ValidatedMapAssociatedTypes;
pub use validated_struct::{GetError, ValidatedMap};
pub use zenoh_cfg_properties::config::*;
use zenoh_core::zlock;
use zenoh_protocol::core::{
    key_expr::OwnedKeyExpr,
    whatami::{WhatAmIMatcher, WhatAmIMatcherVisitor},
};
pub use zenoh_protocol::core::{whatami, EndPoint, Locator, Priority, WhatAmI, ZenohId};
use zenoh_result::{bail, zerror, ZResult};
use zenoh_util::LibLoader;

pub type ValidationFunction = std::sync::Arc<
    dyn Fn(
            &str,
            &serde_json::Map<String, serde_json::Value>,
            &serde_json::Map<String, serde_json::Value>,
        ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>>
        + Send
        + Sync,
>;
type ZInt = u64;

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
    config
        .connect
        .endpoints
        .extend(peers.into_iter().map(|t| t.into()));
    config
}

#[test]
fn config_keys() {
    use validated_struct::ValidatedMap;
    let c = Config::default();
    dbg!(c.keys());
}

fn treat_error_as_none<'a, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::de::Deserialize<'a>,
    D: serde::de::Deserializer<'a>,
{
    let value: Value = serde::de::Deserialize::deserialize(deserializer)?;
    Ok(T::deserialize(value).ok())
}

validated_struct::validator! {
    /// The main configuration structure for Zenoh.
    ///
    /// Most fields are optional as a way to keep defaults flexible. Some of the fields have different default values depending on the rest of the configuration.
    ///
    /// To construct a configuration, we advise that you use a configuration file (JSON, JSON5 and YAML are currently supported, please use the proper extension for your format as the deserializer will be picked according to it).
    #[derive(Default)]
    #[recursive_attrs]
    #[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
    #[serde(default)]
    #[serde(deny_unknown_fields)]
    Config {
        /// The Zenoh ID of the instance. This ID MUST be unique throughout your Zenoh infrastructure and cannot exceed 16 bytes of length. If left unset, a random u128 will be generated.
        id: ZenohId,
        /// The node's mode ("router" (default value in `zenohd`), "peer" or "client").
        mode: Option<whatami::WhatAmI>,
        /// Which zenoh nodes to connect to.
        pub connect: #[derive(Default)]
        ConnectConfig {
            pub endpoints: Vec<EndPoint>,
        },
        /// Which endpoints to listen on. `zenohd` will add `tcp/[::]:7447` to these locators if left empty.
        pub listen: #[derive(Default)]
        ListenConfig {
            pub endpoints: Vec<EndPoint>,
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
                /// Which type of Zenoh instances to automatically establish sessions with upon discovery through UDP multicast.
                #[serde(deserialize_with = "treat_error_as_none")]
                autoconnect: Option<ModeDependentValue<WhatAmIMatcher>>,
                /// Whether or not to listen for scout messages on UDP multicast and reply to them.
                listen: Option<ModeDependentValue<bool>>,
            },
            /// The gossip scouting configuration.
            pub gossip: #[derive(Default)]
            GossipConf {
                /// Whether gossip scouting is enabled or not.
                enabled: Option<bool>,
                /// When true, gossip scouting informations are propagated multiple hops to all nodes in the local network.
                /// When false, gossip scouting informations are only propagated to the next hop.
                /// Activating multihop gossip implies more scouting traffic and a lower scalability.
                /// It mostly makes sense when using "linkstate" routing mode where all nodes in the subsystem don't have
                /// direct connectivity with each other.
                multihop: Option<bool>,
                /// Which type of Zenoh instances to automatically establish sessions with upon discovery through gossip.
                #[serde(deserialize_with = "treat_error_as_none")]
                autoconnect: Option<ModeDependentValue<WhatAmIMatcher>>,
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
        queries_default_timeout: Option<ZInt>,

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
            },
            /// The routing strategy to use in peers and it's configuration.
            pub peer: #[derive(Default)]
            PeerRoutingConf {
                /// The routing strategy to use in peers. ("peer_to_peer" or "linkstate").
                mode: Option<String>,
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
        pub transport: #[derive(Default)]
        TransportConf {
            pub unicast: TransportUnicastConf {
                /// Timeout in milliseconds when opening a link (default: 10000).
                accept_timeout: Option<ZInt>,
                /// Number of links that may stay pending during accept phase (default: 100).
                accept_pending: Option<usize>,
                /// Maximum number of unicast sessions (default: 1000)
                max_sessions: Option<usize>,
                /// Maximum number of unicast incoming links per transport session (default: 1)
                max_links: Option<usize>,
            },
            pub multicast: TransportMulticastConf {
                /// Link join interval duration in milliseconds (default: 2500)
                join_interval: Option<ZInt>,
                /// Maximum number of multicast sessions (default: 1000)
                max_sessions: Option<usize>,
            },
            pub qos: QoSConf {
                /// Whether QoS is enabled or not.
                /// If set to `false`, the QoS will be disabled. (default `true`).
                enabled: bool
            },
            pub link: #[derive(Default)]
            TransportLinkConf {
                // An optional whitelist of protocols to be used for accepting and opening sessions.
                // If not configured, all the supported protocols are automatically whitelisted.
                pub protocols: Option<Vec<String>>,
                pub tx: LinkTxConf {
                    /// The largest value allowed for Zenoh message sequence numbers (wrappring to 0 when reached). When establishing a session with another Zenoh instance, the lowest value of the two instances will be used.
                    /// Defaults to 2^28.
                    sequence_number_resolution: Option<ZInt>,
                    /// Link lease duration in milliseconds (default: 10000)
                    lease: Option<ZInt>,
                    /// Number fo keep-alive messages in a link lease duration (default: 4)
                    keep_alive: Option<usize>,
                    /// Zenoh's MTU equivalent (default: 2^16-1)
                    batch_size: Option<u16>,
                    pub queue: QueueConf {
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
                        /// The initial exponential backoff time in nanoseconds to allow the batching to eventually progress.
                        /// Higher values lead to a more aggressive batching but it will introduce additional latency.
                        backoff: Option<ZInt>
                    },
                    // Number of threads used for TX
                    threads: Option<usize>,
                },
                pub rx: LinkRxConf {
                    /// Receiving buffer size in bytes for each link
                    /// The default the rx_buffer_size value is the same as the default batch size: 65335.
                    /// For very high throughput scenarios, the rx_buffer_size can be increased to accomodate
                    /// more in-flight data. This is particularly relevant when dealing with large messages.
                    /// E.g. for 16MiB rx_buffer_size set the value to: 16777216.
                    buffer_size: Option<usize>,
                    /// Maximum size of the defragmentation buffer at receiver end (default: 1GiB).
                    /// Fragmented messages that are larger than the configured size will be dropped.
                    max_message_size: Option<usize>,
                },
                pub tls: #[derive(Default)]
                TLSConf {
                    root_ca_certificate: Option<String>,
                    server_private_key: Option<String>,
                    server_certificate: Option<String>,
                    client_auth: Option<bool>,
                    client_private_key: Option<String>,
                    client_certificate: Option<String>,
                },
                pub compression: #[derive(Default)]
                /// **Experimental** compression feature.
                /// Will compress the batches hop to hop (as opposed to end to end). May cause errors when
                /// the batches's complexity is too high, causing the resulting compression to be bigger in
                /// size than the MTU.
                /// You must use the features "transport_compression" and "unstable" to enable this.
                CompressionConf {
                    /// When enabled is true, batches will be sent compressed. It does not affect the
                    /// reception, which always expects compressed batches when built with thes features
                    /// "transport_compression" and "unstable".
                    enabled: bool,
                }
            },
            pub shared_memory: SharedMemoryConf {
                /// Whether shared memory is enabled or not.
                /// If set to `true`, the shared-memory transport will be enabled. (default `false`).
                enabled: bool,
            },
            pub auth: #[derive(Default)]
            AuthConf {
                /// The configuration of authentification.
                /// A password implies a username is required.
                pub usrpwd: #[derive(Default)]
                UserConf {
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
        /// A list of directories where plugins may be searched for if no `__path__` was specified for them.
        /// The executable's current directory will be added to the search paths.
        plugins_search_dirs: Vec<String>, // TODO (low-prio): Switch this String to a PathBuf? (applies to other paths in the config as well)
        #[validated(recursive_accessors)]
        /// The configuration for plugins.
        ///
        /// Please refer to [`PluginsConfig`]'s documentation for further details.
        plugins: PluginsConfig,
    }
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
            autoconnect: "peer|router"
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
        Some(&WhatAmIMatcher::try_from(131).unwrap())
    );
    assert_eq!(
        config.scouting().multicast().autoconnect().peer(),
        Some(&WhatAmIMatcher::try_from(131).unwrap())
    );
    assert_eq!(
        config.scouting().multicast().autoconnect().client(),
        Some(&WhatAmIMatcher::try_from(131).unwrap())
    );
    let config = Config::from_deserializer(
        &mut json5::Deserializer::from_str(
            r#"{
        scouting: {
          multicast: {
            enabled: false,
            autoconnect: {router: "", peer: "peer|router"}
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
        Some(&WhatAmIMatcher::try_from(128).unwrap())
    );
    assert_eq!(
        config.scouting().multicast().autoconnect().peer(),
        Some(&WhatAmIMatcher::try_from(131).unwrap())
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
    dbg!(Config::from_file("../../DEFAULT_CONFIG.json5").unwrap());
}

impl Config {
    pub fn add_plugin_validator(&mut self, name: impl Into<String>, validator: ValidationFunction) {
        self.plugins.validators.insert(name.into(), validator);
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
        self._remove(key)
    }

    fn _remove(&mut self, key: &str) -> ZResult<()> {
        let key = key.strip_prefix('/').unwrap_or(key);
        if !key.starts_with("plugins/") {
            bail!(
                "Removal of values from Config is only supported for keys starting with `plugins/`"
            )
        }
        self.plugins.remove(&key["plugins/".len()..])
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
    pub fn from_env() -> ZResult<Self> {
        let path = std::env::var(defaults::ENV)
            .map_err(|e| zerror!("Invalid ENV variable ({}): {}", defaults::ENV, e))?;
        Self::from_file(path.as_str())
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> ZResult<Self> {
        let path = path.as_ref();
        Self::_from_file(path)
    }

    fn _from_file(path: &Path) -> ZResult<Config> {
        match std::fs::File::open(path) {
            Ok(mut f) => {
                let mut content = String::new();
                if let Err(e) = f.read_to_string(&mut content) {
                    bail!(e)
                }
                match path
                    .extension()
                    .map(|s| s.to_str().unwrap())
                {
                    Some("json") | Some("json5") => match json5::Deserializer::from_str(&content) {
                        Ok(mut d) => Config::from_deserializer(&mut d).map_err(|e| match e {
                            Ok(c) => zerror!("Invalid configuration: {}", c).into(),
                            Err(e) => zerror!("JSON error: {}", e).into(),
                        }),
                        Err(e) => bail!(e),
                    },
                    Some("yaml") => Config::from_deserializer(serde_yaml::Deserializer::from_str(&content)).map_err(|e| match e {
                        Ok(c) => zerror!("Invalid configuration: {}", c).into(),
                        Err(e) => zerror!("YAML error: {}", e).into(),
                    }),
                    Some(other) => bail!("Unsupported file type '.{}' (.json, .json5 and .yaml are supported)", other),
                    None => bail!("Unsupported file type. Configuration files must have an extension (.json, .json5 and .yaml supported)")
                }
            }
            Err(e) => bail!(e),
        }
    }

    pub fn libloader(&self) -> LibLoader {
        if self.plugins_search_dirs.is_empty() {
            LibLoader::default()
        } else {
            LibLoader::new(&self.plugins_search_dirs, true)
        }
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
        .insert("transport/link/tx/lease", &mut from_str("168"))
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
impl Notifier<Config> {
    pub fn remove<K: AsRef<str>>(&self, key: K) -> ZResult<()> {
        let key = key.as_ref();
        self._remove(key)
    }

    fn _remove(&self, key: &str) -> ZResult<()> {
        {
            let mut guard = zlock!(self.inner.inner);
            guard.remove(key)?;
        }
        self.notify(key);
        Ok(())
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
        let key = key.as_ref();
        self._notify(key);
    }
    fn _notify(&self, key: &str) {
        let key: Arc<str> = Arc::from(key);
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
}

impl<'a, T: 'a> ValidatedMapAssociatedTypes<'a> for Notifier<T> {
    type Accessor = GetGuard<'a, T>;
}
impl<'a, T: 'a> ValidatedMapAssociatedTypes<'a> for &Notifier<T> {
    type Accessor = GetGuard<'a, T>;
}
impl<T: ValidatedMap + 'static> ValidatedMap for Notifier<T>
where
    T: for<'a> ValidatedMapAssociatedTypes<'a, Accessor = &'a dyn Any>,
{
    fn insert<'d, D: serde::Deserializer<'d>>(
        &mut self,
        key: &str,
        value: D,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<D::Error>,
    {
        {
            let mut guard = zlock!(self.inner.inner);
            guard.insert(key, value)?;
        }
        self.notify(key);
        Ok(())
    }
    fn get<'a>(
        &'a self,
        key: &str,
    ) -> Result<<Self as validated_struct::ValidatedMapAssociatedTypes<'a>>::Accessor, GetError>
    {
        let guard: MutexGuard<'a, T> = zlock!(self.inner.inner);
        // Safety: MutexGuard pins the mutex behind which the value is held.
        let subref = guard.get(key.as_ref())? as *const _;
        Ok(GetGuard {
            _guard: guard,
            subref,
        })
    }
    fn get_json(&self, key: &str) -> Result<String, GetError> {
        self.lock().get_json(key)
    }
    type Keys = T::Keys;
    fn keys(&self) -> Self::Keys {
        self.lock().keys()
    }
}
impl<T: ValidatedMap + 'static> ValidatedMap for &Notifier<T>
where
    T: for<'a> ValidatedMapAssociatedTypes<'a, Accessor = &'a dyn Any>,
{
    fn insert<'d, D: serde::Deserializer<'d>>(
        &mut self,
        key: &str,
        value: D,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<D::Error>,
    {
        {
            let mut guard = zlock!(self.inner.inner);
            guard.insert(key, value)?;
        }
        self.notify(key);
        Ok(())
    }
    fn get<'a>(
        &'a self,
        key: &str,
    ) -> Result<<Self as validated_struct::ValidatedMapAssociatedTypes<'a>>::Accessor, GetError>
    {
        let guard: MutexGuard<'a, T> = zlock!(self.inner.inner);
        // Safety: MutexGuard pins the mutex behind which the value is held.
        let subref = guard.get(key.as_ref())? as *const _;
        Ok(GetGuard {
            _guard: guard,
            subref,
        })
    }
    fn get_json(&self, key: &str) -> Result<String, GetError> {
        self.lock().get_json(key)
    }
    type Keys = T::Keys;
    fn keys(&self) -> Self::Keys {
        self.lock().keys()
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

fn user_conf_validator(u: &UserConf) -> bool {
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
    validators: HashMap<String, ValidationFunction>,
}
pub fn sift_privates(value: &mut serde_json::Value) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        Value::Array(a) => a.iter_mut().for_each(sift_privates),
        Value::Object(o) => {
            o.remove("private");
            o.values_mut().for_each(sift_privates);
        }
    }
}
#[derive(Debug, Clone)]
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
    pub fn remove(&mut self, key: &str) -> ZResult<()> {
        let mut split = key.split('/');
        let plugin = split.next().unwrap();
        let mut current = match split.next() {
            Some(first_in_plugin) => first_in_plugin,
            None => {
                self.values.as_object_mut().unwrap().remove(plugin);
                self.validators.remove(plugin);
                return Ok(());
            }
        };
        let validator = self.validators.get(plugin);
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
                    Some(v) => unsafe { remove_from = std::mem::transmute(v) },
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
        let new_conf = if let Some(validator) = validator {
            match validator(
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
                Err(e) => return Err(format!("{e}").into()),
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

pub trait ModeDependent<T> {
    fn router(&self) -> Option<&T>;
    fn peer(&self) -> Option<&T>;
    fn client(&self) -> Option<&T>;
    #[inline]
    fn get(&self, whatami: WhatAmI) -> Option<&T> {
        match whatami {
            WhatAmI::Router => self.router(),
            WhatAmI::Peer => self.peer(),
            WhatAmI::Client => self.client(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModeValues<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    router: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peer: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client: Option<T>,
}

impl<T> ModeDependent<T> for ModeValues<T> {
    #[inline]
    fn router(&self) -> Option<&T> {
        self.router.as_ref()
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        self.peer.as_ref()
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        self.client.as_ref()
    }
}

#[derive(Clone, Debug)]
pub enum ModeDependentValue<T> {
    Unique(T),
    Dependent(ModeValues<T>),
}

impl<T> ModeDependent<T> for ModeDependentValue<T> {
    #[inline]
    fn router(&self) -> Option<&T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.router(),
        }
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.peer(),
        }
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        match self {
            Self::Unique(v) => Some(v),
            Self::Dependent(o) => o.client(),
        }
    }
}

impl<T> serde::Serialize for ModeDependentValue<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ModeDependentValue::Unique(value) => value.serialize(serializer),
            ModeDependentValue::Dependent(options) => options.serialize(serializer),
        }
    }
}
impl<'a> serde::Deserialize<'a> for ModeDependentValue<bool> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<bool>> {
            type Value = ModeDependentValue<bool>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("bool or mode dependent bool")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ModeDependentValue::Unique(value))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                ModeValues::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(ModeDependentValue::Dependent)
            }
        }
        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl<'a> serde::Deserialize<'a> for ModeDependentValue<WhatAmIMatcher> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct UniqueOrDependent<U>(PhantomData<fn() -> U>);

        impl<'de> Visitor<'de> for UniqueOrDependent<ModeDependentValue<WhatAmIMatcher>> {
            type Value = ModeDependentValue<WhatAmIMatcher>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("WhatAmIMatcher or mode dependent WhatAmIMatcher")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                WhatAmIMatcherVisitor {}
                    .visit_str(value)
                    .map(ModeDependentValue::Unique)
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                ModeValues::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(ModeDependentValue::Dependent)
            }
        }
        deserializer.deserialize_any(UniqueOrDependent(PhantomData))
    }
}

impl<T> ModeDependent<T> for Option<ModeDependentValue<T>> {
    #[inline]
    fn router(&self) -> Option<&T> {
        match self {
            Some(ModeDependentValue::Unique(v)) => Some(v),
            Some(ModeDependentValue::Dependent(o)) => o.router(),
            None => None,
        }
    }

    #[inline]
    fn peer(&self) -> Option<&T> {
        match self {
            Some(ModeDependentValue::Unique(v)) => Some(v),
            Some(ModeDependentValue::Dependent(o)) => o.peer(),
            None => None,
        }
    }

    #[inline]
    fn client(&self) -> Option<&T> {
        match self {
            Some(ModeDependentValue::Unique(v)) => Some(v),
            Some(ModeDependentValue::Dependent(o)) => o.client(),
            None => None,
        }
    }
}

#[macro_export]
macro_rules! unwrap_or_default {
    ($val:ident$(.$field:ident($($param:ident)?))*) => {
        $val$(.$field($($param)?))*.clone().unwrap_or(zenoh_config::defaults$(::$field$(($param))?)*.into())
    };
}
