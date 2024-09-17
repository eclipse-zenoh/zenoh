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
use std::{
    any::Any,
    env,
    error::Error,
    fmt,
    ops::{self, Deref},
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use zenoh_result::ZResult;

/// Zenoh configuration.
///
/// Most options are optional as a way to keep defaults flexible. Some of the options have different
/// default values depending on the rest of the configuration.
///
/// To construct a configuration, we advise that you use a configuration file (JSON, JSON5 and YAML
/// are currently supported, please use the proper extension for your format as the deserializer
/// will be picked according to it).
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Config(pub(crate) zenoh_config::Config);

impl Config {
    /// Default environment variable containing the file path used in [`Config::from_env`].
    pub const DEFAULT_CONFIG_PATH_ENV: &'static str = "ZENOH_CONFIG";

    /// Load configuration from the file path specified in the [`Self::DEFAULT_CONFIG_PATH_ENV`]
    /// environment variable.
    pub fn from_env() -> ZResult<Self> {
        let path = env::var(Self::DEFAULT_CONFIG_PATH_ENV)?;
        Ok(Config(zenoh_config::Config::from_file(Path::new(&path)?)?))
    }

    /// Load configuration from the file at `path`.
    pub fn from_file<P: AsRef<Path>>(path: P) -> ZResult<Self> {
        Ok(Config(zenoh_config::Config::from_file(path)?))
    }

    /// Load configuration from the JSON5 string `input`.
    pub fn from_json5(input: &str) -> ZResult<Config> {
        match zenoh_config::Config::from_deserializer(&mut json5::Deserializer::from_str(input)?) {
            Ok(config) => Ok(Config(config)),
            Err(Ok(config)) => {
                Err(zerror!("The config was correctly deserialized yet it's invalid").into())
            }
            Err(Err(err)) => Err(err.into()),
        }
    }

    /// Inserts configuration value `value` at path `key`.
    pub fn insert_json5(&mut self, key: &str, value: &str) -> Result<(), InsertionError> {
        self.0.insert_json5(key, value).map_err(InsertionError)
    }

    #[zenoh_macros::unstable]
    pub fn get_json(&self, key: &str) -> Result<String, LookupError> {
        self.0.get_json(key).map_err(LookupError)
    }

    // REVIEW(fuzzypixelz): the error variant of the Result is a Result because this does
    // deserialization AND validation.
    #[zenoh_macros::unstable]
    pub fn from_deserializer<'d, D: serde::Deserializer<'d>>(
        d: D,
    ) -> Result<Self, Result<Self, D::Error>>
    where
        Self: serde::Deserialize<'d>,
    {
        match zenoh_config::Config::from_deserializer(d) {
            Ok(config) => Ok(Config(config)),
            Err(result) => match result {
                Ok(config) => Err(Ok(Config(config))),
                Err(err) => Err(Err(err)),
            },
        }
    }
}

#[derive(Debug)]
pub struct InsertionError(validated_struct::InsertionError);

impl fmt::Display for InsertionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for InsertionError {}

#[derive(Debug)]
pub struct LookupError(validated_struct::GetError);

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for LookupError {}

#[zenoh_macros::unstable_config]
impl std::ops::Deref for Config {
    type Target = zenoh_config::Config;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[zenoh_macros::unstable_config]
impl std::ops::DerefMut for Config {
    fn deref_mut(&mut self) -> &mut <Self as std::ops::Deref>::Target {
        &mut self.0
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
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
    pub fn new(inner: Config) -> Self {
        Notifier {
            inner: Arc::new(NotifierInner {
                inner: Mutex::new(inner),
                subscribers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn subscribe(&self) -> flume::Receiver<Notification> {
        let (tx, rx) = flume::unbounded();
        self.lock_subscribers().push(tx);
        rx
    }

    pub fn notify<K: AsRef<str>>(&self, key: K) {
        let key = key.as_ref();
        let key: Arc<str> = Arc::from(key);
        let mut marked = Vec::new();
        let mut subscribers = self.lock_subscribers();

        for (i, sub) in subscribers.iter().enumerate() {
            if sub.send(key.clone()).is_err() {
                marked.push(i)
            }
        }

        for i in marked.into_iter().rev() {
            subscribers.swap_remove(i);
        }
    }

    pub fn lock(&self) -> MutexGuard<Config> {
        self.lock_config()
    }

    fn lock_subscribers(&self) -> MutexGuard<Vec<flume::Sender<Notification>>> {
        self.inner
            .subscribers
            .lock()
            .expect("acquiring Notifier's subscribers Mutex should not fail")
    }

    fn lock_config(&self) -> MutexGuard<Config> {
        self.inner
            .inner
            .lock()
            .expect("acquiring Notifier's Config Mutex should not fail")
    }

    pub fn remove<K: AsRef<str>>(&self, key: K) -> ZResult<()> {
        self.lock_config().0.remove(key.as_ref())?;
        self.notify(key);
        Ok(())
    }

    pub fn insert_json5(&self, key: &str, value: &str) -> Result<(), InsertionError> {
        self.lock_config().insert_json5(key, value)
    }

    #[allow(dead_code)]
    pub fn get<'a>(&'a self, key: &str) -> Result<LookupGuard<'a, Config>, LookupError> {
        let config = self.lock_config();
        // SAFETY: MutexGuard pins the mutex behind which the value is held.
        let subref = config.0.get(key.as_ref()).map_err(LookupError)? as *const _;
        Ok(LookupGuard {
            _guard: config,
            subref,
        })
    }
}

pub struct LookupGuard<'a, T> {
    _guard: MutexGuard<'a, T>,
    subref: *const dyn Any,
}

impl<'a, T> ops::Deref for LookupGuard<'a, T> {
    type Target = dyn Any;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.subref }
    }
}

impl<'a, T> AsRef<dyn Any> for LookupGuard<'a, T> {
    fn as_ref(&self) -> &dyn Any {
        self.deref()
    }
}
