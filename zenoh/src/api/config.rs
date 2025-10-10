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
    env, fmt,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use zenoh_result::{bail, ZResult};

/// Zenoh configuration.
///
/// The zenoh configuration is unstable, so no direct access to the fields is provided.
/// The only way to change the configuration is to load the JSON configuration from a file or a string,
/// with [`Config::from_file`](crate::config::Config::from_file) or
/// [`Config::from_json5`](crate::config::Config::from_json5),
/// or to use the [`Config::insert_json5`](crate::config::Config::insert_json5)
/// and [`Config::remove`](crate::config::Config::remove) methods to modify the configuration tree.
///
/// Example configuration file:
#[doc = concat!(
    "```json5\n",
    include_str!("../../DEFAULT_CONFIG.json5"),
    "\n```"
)]
///
/// Most options are optional as a way to keep defaults flexible. Some of the options have different
/// default values depending on the rest of the configuration.
///
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Config(pub(crate) zenoh_config::Config);

impl Config {
    /// Default environment variable containing the file path used in [`Config::from_env`].
    pub const DEFAULT_CONFIG_PATH_ENV: &'static str = "ZENOH_CONFIG";

    /// Load configuration from the file path specified in the [`Self::DEFAULT_CONFIG_PATH_ENV`]
    /// environment variable.
    pub fn from_env() -> ZResult<Self> {
        let path = env::var(Self::DEFAULT_CONFIG_PATH_ENV)?;
        Ok(Config(zenoh_config::Config::from_file(Path::new(&path))?))
    }

    /// Load configuration from the file at `path`.
    pub fn from_file<P: AsRef<Path>>(path: P) -> ZResult<Self> {
        Ok(Config(zenoh_config::Config::from_file(path)?))
    }

    /// Load configuration from the JSON5 string `input`.
    pub fn from_json5(input: &str) -> ZResult<Config> {
        match zenoh_config::Config::from_deserializer(&mut json5::Deserializer::from_str(input)?) {
            Ok(config) => Ok(Config(config)),
            Err(Ok(_)) => {
                Err(zerror!("The config was correctly deserialized, but it is invalid").into())
            }
            Err(Err(err)) => Err(err.into()),
        }
    }

    pub fn remove<K: AsRef<str>>(&mut self, key: K) -> ZResult<()> {
        match key.as_ref().split_once("=") {
            None => self.0.remove(key.as_ref()),
            Some((prefix, id_value)) => {
                let (key, id_key) = prefix.rsplit_once("/").ok_or("missing id")?;
                let current = serde_json::from_str::<serde_json::Value>(&self.get_json(key)?)?;
                let serde_json::Value::Array(mut list) = current else {
                    bail!("not an array")
                };
                let prev_len = list.len();
                list.retain(|item| match item {
                    serde_json::Value::Object(map) => {
                        map.get(id_key).and_then(|v| v.as_str()) != Some(id_value)
                    }
                    _ => true,
                });
                if list.len() != prev_len {
                    self.0.insert_json5(key, &serde_json::to_string(&list)?)?;
                }
                Ok(())
            }
        }
    }

    /// Inserts configuration value `value` at `key`.
    pub fn insert_json5(&mut self, key: &str, value: &str) -> ZResult<()> {
        match key.split_once("=") {
            None => self.0.insert_json5(key, value),
            Some((prefix, id_value)) => {
                let (key, id_key) = prefix.rsplit_once("/").ok_or("missing id")?;
                let new_item = json5::from_str::<serde_json::Value>(value)?;
                if new_item
                    .as_object()
                    .and_then(|map| map.get(id_key))
                    .and_then(|v| v.as_str())
                    != Some(id_value)
                {
                    bail!("id mismatch");
                }
                let current = serde_json::from_str::<serde_json::Value>(&self.get_json(key)?)?;
                let serde_json::Value::Array(mut list) = current else {
                    bail!("not an array")
                };
                let mut new_item = Some(new_item);
                for item in list.iter_mut() {
                    let serde_json::Value::Object(map) = item else {
                        bail!("array item is not an object");
                    };
                    if map.get(id_key).and_then(|v| v.as_str()) == Some(id_value) {
                        *item = new_item.take().unwrap();
                        break;
                    }
                }
                if let Some(new_item) = new_item {
                    list.push(new_item);
                }
                self.0.insert_json5(key, &serde_json::to_string(&list)?)
            }
        }
        .map_err(|err| zerror!("{err}").into())
    }

    /// Returns a JSON string containing the configuration at `key`.
    pub fn get_json(&self, key: &str) -> ZResult<String> {
        self.0.get_json(key).map_err(|err| zerror!("{err}").into())
    }

    // REVIEW(fuzzypixelz): the error variant of the Result is a Result because this does
    // deserialization AND validation.
    #[zenoh_macros::unstable]
    // TODO(yellowhatter): clippy says that Error here is extremely large (1k)
    #[allow(clippy::result_large_err)]
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

#[zenoh_macros::internal_config]
impl std::ops::Deref for Config {
    type Target = zenoh_config::Config;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[zenoh_macros::internal_config]
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

/// The wrapper for a [`Config`] that allows to subscribe to changes.
/// This type is returned by [`Session::config`](crate::Session::config) and allows
/// the `Session` to immediately react to changes applied to the configuration.
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

    #[cfg(feature = "plugins")]
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

    pub fn lock(&self) -> MutexGuard<'_, Config> {
        self.lock_config()
    }

    fn lock_subscribers(&self) -> MutexGuard<'_, Vec<flume::Sender<Notification>>> {
        self.inner
            .subscribers
            .lock()
            .expect("acquiring Notifier's subscribers Mutex should not fail")
    }

    fn lock_config(&self) -> MutexGuard<'_, Config> {
        self.inner
            .inner
            .lock()
            .expect("acquiring Notifier's Config Mutex should not fail")
    }

    pub fn remove<K: AsRef<str>>(&self, key: K) -> ZResult<()> {
        self.lock_config().remove(key.as_ref())?;
        self.notify(key);
        Ok(())
    }

    pub fn insert_json5(&self, key: &str, value: &str) -> ZResult<()> {
        if !key.starts_with("plugins/") {
            bail!(
                "Error inserting conf value {} : updating config is only \
                    supported for keys starting with `plugins/`",
                key
            );
        }
        self.lock_config().insert_json5(key, value)?;
        self.notify(key);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn get<'a>(&'a self, key: &str) -> ZResult<LookupGuard<'a, Config>> {
        let config = self.lock_config();
        let subref = config.0.get(key.as_ref()).map_err(|err| zerror!("{err}"))? as *const _;
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

impl<T> Deref for LookupGuard<'_, T> {
    type Target = dyn Any;

    fn deref(&self) -> &Self::Target {
        // SAFETY: MutexGuard pins the mutex behind which the value is held.
        unsafe { &*self.subref }
    }
}

impl<T> AsRef<dyn Any> for LookupGuard<'_, T> {
    fn as_ref(&self) -> &dyn Any {
        self.deref()
    }
}

#[cfg(test)]
mod tests {
    use zenoh_config::{InterceptorFlow, QosOverwriteItemConf};

    use crate::Config;

    #[test]
    fn insert_remove_list_item() {
        let mut config = Config::default();

        let item1 = r#"{
            id: "item1",
            messages: ["put"],
            key_exprs: ["**"],
            overwrite: {
                priority: "real_time",
            },
            flows: ["egress"]
        }"#;
        config.insert_json5("qos/network/id=item1", item1).unwrap();
        let items = serde_json::from_str::<Vec<QosOverwriteItemConf>>(
            &config.get_json("qos/network").unwrap(),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.as_ref().unwrap(), "item1");
        assert_eq!(
            *items[0].flows.as_ref().unwrap().first(),
            InterceptorFlow::Egress
        );

        let item1 = r#"{
            id: "item1",
            messages: ["put"],
            key_exprs: ["**"],
            overwrite: {
                priority: "real_time",
            },
            flows: ["ingress"]
        }"#;
        config.insert_json5("qos/network/id=item1", item1).unwrap();
        let items = serde_json::from_str::<Vec<QosOverwriteItemConf>>(
            &config.get_json("qos/network").unwrap(),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.as_ref().unwrap(), "item1");
        assert_eq!(
            *items[0].flows.as_ref().unwrap().first(),
            InterceptorFlow::Ingress
        );

        let item2 = r#"{
            id: "item2",
            messages: ["put"],
            key_exprs: ["**"],
            overwrite: {
                priority: "real_time",
            },
            flows: ["egress"]
        }"#;
        config.insert_json5("qos/network/id=item2", item2).unwrap();
        let items = serde_json::from_str::<Vec<QosOverwriteItemConf>>(
            &config.get_json("qos/network").unwrap(),
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id.as_ref().unwrap(), "item1");
        assert_eq!(
            *items[0].flows.as_ref().unwrap().first(),
            InterceptorFlow::Ingress
        );
        assert_eq!(items[1].id.as_ref().unwrap(), "item2");
        assert_eq!(
            *items[1].flows.as_ref().unwrap().first(),
            InterceptorFlow::Egress
        );

        config.remove("qos/network/id=item2").unwrap();
        let items = serde_json::from_str::<Vec<QosOverwriteItemConf>>(
            &config.get_json("qos/network").unwrap(),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id.as_ref().unwrap(), "item1");
        assert_eq!(
            *items[0].flows.as_ref().unwrap().first(),
            InterceptorFlow::Ingress
        );

        config.remove("qos/network/id=item1").unwrap();
        let items = serde_json::from_str::<Vec<QosOverwriteItemConf>>(
            &config.get_json("qos/network").unwrap(),
        )
        .unwrap();
        assert_eq!(items.len(), 0);
    }
}
