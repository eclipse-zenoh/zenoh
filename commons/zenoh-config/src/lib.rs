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

//! Properties to pass to [`open`](zenoh::open) and [`scout`](zenoh::scout) functions as configuration
//! and associated constants.

use serde_json::Value;
use std::{
    any::Any,
    collections::HashMap,
    io::Read,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};
use validated_struct::{GetError, ValidatedMap};
pub use zenoh_cfg_properties::config::*;
use zenoh_core::{bail, zerror, zlock, Result as ZResult};
pub use zenoh_protocol_core::{whatami, EndPoint, Locator, WhatAmI};
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
    #[recursive_attrs]
    #[derive(serde::Deserialize, serde::Serialize, Clone, Debug, Default)]
    #[serde(default)]
    #[serde(deny_unknown_fields)]
    Config {
        /// The Zenoh ID of the instance. This ID MUST be unique throughout your Zenoh infrastructure and cannot exceed 16 bytes of length. If left unset, a random UUIDv4 will be generated.
        id: Option<String>,
        /// The node's mode ("router" (default value in `zenohd`), "peer" or "client").
        mode: Option<whatami::WhatAmI>,
        /// Which zenoh nodes to connect to.
        pub connect: ConnectConfig {
            pub endpoints: Vec<EndPoint>,
        },
        /// Which endpoints to listen on. `zenohd` will add `tcp/0.0.0.0:7447` to these locators if left empty.
        pub listen: ListenConfig {
            pub endpoints: Vec<EndPoint>,
        },
        /// Actions taken by the Zenoh instance upon startup.
        pub startup: JoinConfig {
            /// A list of key-expressions to subscribe to upon startup.
            subscribe: Vec<String>,
            /// A list of key-expressions to declare publications onto upon startup.
            declare_publications: Vec<String>,
        },
        pub scouting: ScoutingConf {
            /// In client mode, the period dedicated to scouting for a router before failing. In milliseconds.
            timeout: Option<u64>,
            /// In peer mode, the period dedicated to scouting remote peers before attempting other operations. In milliseconds.
            delay: Option<u64>,
            /// How multicast should behave.
            pub multicast: ScoutingMulticastConf {
                /// Whether multicast scouting is enabled or not. If left empty, `zenohd` will set it according to the presence of the `--no-multicast-scouting` argument.
                enabled: Option<bool>,
                /// The socket which should be used for multicast scouting. `zenohd` will use `224.0.0.224:7447` by default if none is provided.
                address: Option<SocketAddr>,
                /// The network interface which should be used for multicast scouting. `zenohd` will automatically select an interface if none is provided.
                interface: Option<String>,
                /// Which type of Zenoh instances to automatically establish sessions with upon discovery through multicast scouting.
                #[serde(deserialize_with = "treat_error_as_none")]
                autoconnect: Option<whatami::WhatAmIMatcher>,
            },
            pub gossip: GossipConf {
                /// Whether the link state protocol (gossip scouting) should be enabled. Defaults to `true` if left empty.
                enabled: Option<bool>,
                /// Which type of Zenoh instances to automatically establish sessions with upon discovery through gossip scouting.
                #[serde(deserialize_with = "treat_error_as_none")]
                autoconnect: Option<whatami::WhatAmIMatcher>,
            },
            /// If set to `false`, peers will never automatically establish sessions between each-other.
            peers_autoconnect: Option<bool>,
        },
        /// Whether data messages should be timestamped. If left empty, `zenohd` will set it according to the presence of the `--no-timestamp` argument.
        add_timestamp: Option<bool>,
        /// Whether local writes/queries should reach local subscribers/queryables.
        local_routing: Option<bool>,
        /// The default timeout to apply to queries in milliseconds.
        queries_default_timeout: Option<ZInt>,
        pub transport: TransportConf {
            /// If set to `false`, the shared-memory transports will be disabled. (default `true`).
            shared_memory: Option<bool>,
            /// The largest value allowed for Zenoh message sequence numbers (wrappring to 0 when reached). When establishing a session with another Zenoh instance, the lowest value of the two instances will be used.
            /// Defaults to 2^28.
            sequence_number_resolution: Option<ZInt>,
            /// Enables QOS (traffic priorities) (default false).
            qos: Option<bool>,
            pub unicast: TransportUnicastConf {
                /// Timeout in milliseconds when opening a link (default: 10000).
                accept_timeout: Option<ZInt>,
                /// Number of links that may stay pending during accept phase (default: 100).
                accept_pending: Option<usize>,
                /// Maximum number of unicast sessions (default: 1024)
                max_sessions: Option<usize>,
                /// Maximum number of unicast incoming links per transport session (default: 1)
                max_links: Option<usize>,
            },
            pub multicast: TransportMulticastConf {
                /// Link keep-alive duration in milliseconds (default: 2500)
                join_interval: Option<ZInt>,
                /// Maximum number of multicast sessions (default: 1024)
                max_sessions: Option<usize>,
            },
            pub link: TransportLinkConf {
                /// Zenoh's MTU equivalent (default: 2^16-1)
                batch_size: Option<u16>,
                /// Link lease duration in milliseconds (default: 10000)
                lease: Option<ZInt>,
                /// Link keep-alive duration in milliseconds (default: 2500)
                keep_alive: Option<ZInt>,
                /// Receiving buffer size for each link (default: 16MiB, this default is currently under investigation)
                rx_buffer_size: Option<usize>,
                /// Maximum size of the defragmentation buffer at receiver end (default: 1GiB).
                /// Fragmented messages that are larger than the configured size will be dropped.
                defrag_buffer_size: Option<usize>,
                pub tls: TLSConf {
                    root_ca_certificate: Option<String>,
                    server_private_key: Option<String>,
                    server_certificate: Option<String>,
                    client_auth: Option<bool>,
                    client_private_key: Option<String>,
                    client_certificate: Option<String>,
                },
            },
            pub auth: AuthConf {
                /// The configuration of authentification.
                /// A password implies a username is required.
                pub usrpwd: UserConf {
                    user: Option<String>,
                    password: Option<String>,
                    /// The path to a file containing the user password dictionary, a file containing "<user>:<password>"
                    dictionary_file: Option<String>,
                } where (user_conf_validator),
                pub pubkey: PubKeyConf {
                    public_key_pem: Option<String>,
                    private_key_pem: Option<String>,
                    public_key_file: Option<String>,
                    private_key_file: Option<String>,
                    key_size: Option<usize>,
                    known_keys_file: Option<String>,
                },
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
    assert_eq!(*config.scouting().multicast().enabled(), Some(false));
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
    dbg!(Config::from_file("../../EXAMPLE_CONFIG.json5").unwrap());
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
        let key = key.as_ref();
        let key = key.strip_prefix('/').unwrap_or(key);
        self.insert(key, &mut serde_json::Deserializer::from_str(value))
    }

    pub fn insert_json5<K: AsRef<str>>(
        &mut self,
        key: K,
        value: &str,
    ) -> Result<(), validated_struct::InsertionError>
    where
        validated_struct::InsertionError: From<json5::Error>,
    {
        let key = key.as_ref();
        let key = key.strip_prefix('/').unwrap_or(key);
        self.insert(key, &mut json5::Deserializer::from_str(value)?)
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
    pub fn from_file<P: AsRef<Path>>(path: P) -> ZResult<Self> {
        let path = path.as_ref();
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
impl Notifier<Config> {
    pub fn remove<K: AsRef<str>>(&self, key: K) -> ZResult<()> {
        let key = key.as_ref();
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
///         // If none is specified, `zenohd` will search for a `<dylib_prefix>zplugin_<plugin_name>.<dylib_suffix>` file in the search directories.
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
