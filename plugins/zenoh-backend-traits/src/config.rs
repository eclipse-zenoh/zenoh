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
use derive_more::{AsMut, AsRef};
use serde_json::{Map, Value};
use std::convert::TryFrom;
use std::time::Duration;
use zenoh::{key_expr::keyexpr, prelude::OwnedKeyExpr, Result as ZResult};
use zenoh_result::{bail, zerror, Error};

#[derive(Debug, Clone, AsMut, AsRef)]
pub struct PluginConfig {
    pub name: String,
    pub required: bool,
    pub backend_search_dirs: Option<Vec<String>>,
    pub volumes: Vec<VolumeConfig>,
    pub storages: Vec<StorageConfig>,
    #[as_ref]
    #[as_mut]
    pub rest: Map<String, Value>,
}
#[derive(Debug, Clone, AsMut, AsRef)]
pub struct VolumeConfig {
    pub name: String,
    pub backend: Option<String>,
    pub paths: Option<Vec<String>>,
    pub required: bool,
    #[as_ref]
    #[as_mut]
    pub rest: Map<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    pub name: String,
    pub key_expr: OwnedKeyExpr,
    pub complete: bool,
    pub strip_prefix: Option<OwnedKeyExpr>,
    pub volume_id: String,
    pub volume_cfg: Value,
    pub garbage_collection_config: GarbageCollectionConfig,
    // Note: ReplicaConfig is optional. Alignment will be performed only if it is a replica
    pub replica_config: Option<ReplicaConfig>,
}
// Note: All parameters should be same for replicas, else will result on huge overhead
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaConfig {
    pub publication_interval: Duration,
    pub propagation_delay: Duration,
    pub delta: Duration,
}

impl Default for ReplicaConfig {
    fn default() -> Self {
        Self {
            // Publication interval indicates the frequency of digest publications
            // This will determine the time upto which replicas might be diverged
            // This can be different for each replica if not used to compute hot and warm
            publication_interval: Duration::from_secs(5),
            // This indicates the uncertainity due to the network
            // The messages might still be in transit in the network
            propagation_delay: Duration::from_millis(200),
            // This is the chunk that you would like your data to be divide into in time.
            // Higher the frequency of updates, lower the delta should be chosen
            // To be efficient, delta should be the time containing no more than 100,000 samples
            delta: Duration::from_millis(1000),
        }
    }
}

// The configuration for periodic garbage collection of metadata in storage manager
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GarbageCollectionConfig {
    // The duration between two garbage collection events
    // The garbage collection will be scheduled as a periodic event with this period
    pub period: Duration,
    // The metadata older than this parameter will be garbage collected
    pub lifespan: Duration,
}

impl Default for GarbageCollectionConfig {
    fn default() -> Self {
        Self {
            period: Duration::from_secs(30),
            lifespan: Duration::from_secs(86400),
        }
    }
}

#[derive(Debug)]
pub enum ConfigDiff {
    DeleteVolume(VolumeConfig),
    AddVolume(VolumeConfig),
    DeleteStorage(StorageConfig),
    AddStorage(StorageConfig),
}
pub trait AsObject {
    fn as_object(&self) -> Option<&serde_json::Map<String, serde_json::Value>>;
}
impl AsObject for serde_json::Value {
    fn as_object(&self) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.as_object()
    }
}
impl AsObject for serde_json::Map<String, serde_json::Value> {
    fn as_object(&self) -> Option<&serde_json::Map<String, serde_json::Value>> {
        Some(self)
    }
}
impl<S: Into<String> + AsRef<str>, V: AsObject> TryFrom<(S, &V)> for PluginConfig {
    type Error = Error;
    fn try_from((name, value): (S, &V)) -> Result<Self, Self::Error> {
        let value = value.as_object().ok_or_else(|| {
            zerror!(
                "Configuration for plugin {} must be an object",
                name.as_ref()
            )
        })?;
        let required = value
            .get("__required__")
            .map(|r| {
                r.as_bool().ok_or_else(|| {
                    zerror!(
                        "`__required__` field of {}'s configuration must be a boolean",
                        name.as_ref()
                    )
                })
            })
            .unwrap_or(Ok(true))?;
        let backend_search_dirs = match value.get("backend_search_dirs") {
            Some(serde_json::Value::String(path)) => Some(vec![path.clone()]),
            Some(serde_json::Value::Array(paths)) => {
                let mut result = Vec::with_capacity(paths.len());
                for path in paths {
                    let path = if let serde_json::Value::String(path) = path {path} else {bail!("`backend_search_dirs` field of {}'s configuration must be a string or array of strings", name.as_ref())};
                    result.push(path.clone());
                }
                Some(result)
            }
            None => None,
            _ => bail!("`backend_search_dirs` field of {}'s configuration must be a string or array of strings", name.as_ref())
        };
        let volumes = match value.get("volumes") {
            Some(configs) => VolumeConfig::try_from(name.as_ref(), configs)?,
            None => Vec::new(),
        };
        let storages = match value.get("storages") {
            Some(serde_json::Value::Object(configs)) => {
                let mut storages = Vec::with_capacity(configs.len());
                for (storage_name, config) in configs {
                    storages.push(StorageConfig::try_from(
                        name.as_ref(),
                        storage_name,
                        config,
                    )?)
                }
                storages
            }
            None => Vec::new(),
            _ => bail!(
                "`storages` field of `{}`'s configuration must be an object",
                name.as_ref()
            ),
        };
        Ok(PluginConfig {
            name: name.into(),
            required,
            backend_search_dirs,
            volumes,
            storages,
            rest: value
                .into_iter()
                .filter_map(|(k, v)| {
                    (!["__required__", "backend_search_dirs", "volumes", "storages"]
                        .contains(&k.as_str()))
                    .then(|| (k.clone(), v.clone()))
                })
                .collect(),
        })
    }
}
impl ConfigDiff {
    pub fn diffs(old: PluginConfig, new: PluginConfig) -> Vec<ConfigDiff> {
        let mut diffs = Vec::new();
        for old in &old.storages {
            if !new.storages.contains(old) {
                diffs.push(ConfigDiff::DeleteStorage(old.clone()))
            }
        }
        for old in &old.volumes {
            if !new.volumes.contains(old) {
                diffs.push(ConfigDiff::DeleteVolume(old.clone()))
            }
        }
        for new in new.volumes {
            if !old.volumes.contains(&new) {
                diffs.push(ConfigDiff::AddVolume(new))
            }
        }
        for new in new.storages {
            if !old.storages.contains(&new) {
                diffs.push(ConfigDiff::AddStorage(new))
            }
        }
        diffs
    }
}
pub enum BackendSearchMethod<'a> {
    ByPaths(&'a [String]),
    ByName(&'a str),
}
impl VolumeConfig {
    pub fn to_json_value(&self) -> Value {
        let mut result = self.rest.clone();
        if let Some(paths) = &self.paths {
            result.insert(
                "__path__".into(),
                Value::Array(paths.iter().map(|p| Value::String(p.into())).collect()),
            );
        }
        if !self.required {
            result.insert("__required__".into(), Value::Bool(false));
        }
        Value::Object(result)
    }
    pub fn backend_search_method(&self) -> BackendSearchMethod {
        match &self.paths {
            None => BackendSearchMethod::ByName(self.backend.as_deref().unwrap_or(&self.name)),
            Some(paths) => BackendSearchMethod::ByPaths(paths),
        }
    }
    fn try_from<V: AsObject>(plugin_name: &str, configs: &V) -> ZResult<Vec<Self>> {
        let configs = configs.as_object().ok_or_else(|| {
            zerror!(
                "Configuration for plugin `{}`'s `volumes` field must be an object",
                plugin_name
            )
        })?;
        let mut volumes = Vec::with_capacity(configs.len());
        for (name, config) in configs {
            let config = if let Value::Object(config) = config {
                config
            } else {
                bail!(
                    "`{}`'s `{}` backend configuration must be an object",
                    plugin_name,
                    name
                );
            };
            let backend = match config.get("backend") {
                None => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                _ => bail!(
                    "`backend` field of `{}`'s `{}` volume configuration must be a string",
                    plugin_name,
                    name
                ),
            };
            let paths = match config.get("__path__") {
                None => None,
                Some(serde_json::Value::String(s)) => Some(vec![s.clone()]),
                Some(serde_json::Value::Array(a)) => {
                    let mut paths = Vec::with_capacity(a.len());
                    for path in a {
                        if let serde_json::Value::String(path) = path {
                            paths.push(path.clone());
                        } else {
                            bail!("`path` field of `{}`'s `{}` volume configuration must be a string or array of string", plugin_name, name)
                        }
                    }
                    Some(paths)
                }
                _ => bail!("`path` field of `{}`'s `{}` volume configuration must be a string or array of string", plugin_name, name)
            };
            let required = match config.get("__required__") {
                Some(serde_json::Value::Bool(b)) => *b,
                None => true,
                _ => todo!(),
            };
            volumes.push(VolumeConfig {
                name: name.clone(),
                backend,
                paths,
                required,
                rest: config
                    .iter()
                    .filter_map(|(k, v)| {
                        (!["__path__", "__required__"].contains(&k.as_str()))
                            .then(|| (k.clone(), v.clone()))
                    })
                    .collect(),
            })
        }
        Ok(volumes)
    }
}
impl StorageConfig {
    pub fn to_json_value(&self) -> Value {
        let mut result = serde_json::Map::new();
        result.insert("key_expr".into(), Value::String(self.key_expr.to_string()));
        if let Some(s) = &self.strip_prefix {
            result.insert("strip_prefix".into(), Value::String(s.to_string()));
        }
        result.insert(
            "volume".into(),
            match &self.volume_cfg {
                Value::Null => Value::String(self.volume_id.clone()),
                Value::Object(v) => {
                    let mut v = v.clone();
                    v.insert("id".into(), self.volume_id.clone().into());
                    Value::Object(v)
                }
                _ => unreachable!(),
            },
        );
        Value::Object(result)
    }
    fn try_from<V: AsObject>(plugin_name: &str, storage_name: &str, config: &V) -> ZResult<Self> {
        let config = config.as_object().ok_or_else(|| {
            zerror!(
                "`storages` field of `{}`'s configuration must be an array of objects",
                plugin_name,
            )
        })?;
        let key_expr = match config.get("key_expr").and_then(|x| x.as_str()) {
            Some(s) => match keyexpr::new(s) {
                Ok(ke) => ke.to_owned(),
                Err(e) => bail!("key_expr='{}' is not a valid key-expression: {}", s, e),
            },
            None => {
                bail!("elements of the `storages` field of `{}`'s configuration must be objects with at least a `key_expr` string-typed field",
            plugin_name,)
            }
        };
        let complete = match config.get("complete").and_then(|x| x.as_str()) {
            Some(s) => {
                match s {
                    "true" => true,
                    "false" => false,
                    e => {
                        bail!("complete='{}' is not a valid value. Accepted values: ['true', 'false']", e)
                    }
                }
            }
            None => false,
        };
        let strip_prefix: Option<OwnedKeyExpr> = match config.get("strip_prefix") {
            Some(Value::String(s)) => {
                if !key_expr.starts_with(s) {
                    bail!(
                        r#"The specified "strip_prefix={}" is not a prefix of "key_expr={}""#,
                        s,
                        key_expr
                    )
                }
                match keyexpr::new(s.as_str()) {
                    Ok(ke) => {
                        if ke.is_wild() {
                            bail!(
                                r#"The specified "strip_prefix={}" contains wildcard characters (it shouldn't)"#,
                                ke
                            )
                        }
                        Some(ke.to_owned())
                    }
                    Err(e) => bail!("strip_prefix='{}' is not a valid key-expression: {}", s, e),
                }
            }
            None => None,
            _ => bail!(
                "Invalid type for field `strip_prefix` of storage `{}`. Only strings are accepted.",
                storage_name
            ),
        };
        let (volume_id, volume_cfg) = match config.get("volume") {
            Some(Value::String(volume_id)) => (volume_id.clone(), Value::Null),
            Some(Value::Object(volume)) => {
                let mut volume_id = None;
                let mut volume_cfg = serde_json::Map::new();
                for (key, value) in volume {
                    match (key.as_str(), value) {
                        ("id", Value::String(id)) => volume_id = Some(id.to_owned()),
                        ("id", _) => {}
                        _ => {
                            volume_cfg.insert(key.clone(), value.clone());
                        }
                    }
                }
                (volume_id.ok_or_else(|| zerror!("`volume` value for storage `{}` is an object, but misses mandatory string-typed field `id`", storage_name))?, volume_cfg.into())
            }
            None => bail!(
                "`volume` field missing for storage `{}`. This field is mandatory and accepts strings or objects with at least the `id` field",
                storage_name
            ),
            _ => bail!("Invalid type for field `volume` of storage `{}`. Only strings or objects with at least the `id` field are accepted.", storage_name)
        };
        let garbage_collection_config = match config.get("garbage_collection") {
            Some(s) => {
                let mut garbage_collection_config = GarbageCollectionConfig::default();
                if let Some(period) = s.get("period") {
                    let period = period.to_string().parse::<u64>();
                    if let Ok(period) = period {
                        garbage_collection_config.period = Duration::from_secs(period)
                    } else {
                        bail!("Invalid type for field `period` in `garbage_collection` of storage `{}`. Only integer values are accepted.", plugin_name)
                    }
                }
                if let Some(lifespan) = s.get("lifespan") {
                    let lifespan = lifespan.to_string().parse::<u64>();
                    if let Ok(lifespan) = lifespan {
                        garbage_collection_config.lifespan = Duration::from_secs(lifespan)
                    } else {
                        bail!("Invalid type for field `lifespan` in `garbage_collection` of storage `{}`. Only integer values are accepted.", plugin_name)
                    }
                }
                garbage_collection_config
            }
            None => GarbageCollectionConfig::default(),
        };
        let replica_config = match config.get("replica_config") {
            Some(s) => {
                let mut replica_config = ReplicaConfig::default();
                // TODO: Discuss what to do in case of wrong configuration - exit or use default
                if let Some(p) = s.get("publication_interval") {
                    let p = p.to_string().parse::<u64>();
                    if let Ok(p) = p {
                        replica_config.publication_interval = Duration::from_secs(p)
                    } else {
                        bail!("Invalid type for field `publication_interval` in `replica_config` of storage `{}`. Only integer values are accepted.", plugin_name)
                    }
                }
                if let Some(p) = s.get("propagation_delay") {
                    let p = p.to_string().parse::<u64>();
                    if let Ok(p) = p {
                        replica_config.propagation_delay = Duration::from_millis(p)
                    } else {
                        bail!("Invalid type for field `propagation_delay` in `replica_config` of storage `{}`. Only integer values are accepted.", plugin_name)
                    }
                }
                if let Some(d) = s.get("delta") {
                    let d = d.to_string().parse::<u64>();
                    if let Ok(d) = d {
                        replica_config.delta = Duration::from_millis(d)
                    } else {
                        bail!("Invalid type for field `delta` in `replica_config` of storage `{}`. Only integer values are accepted.", plugin_name)
                    }
                }
                Some(replica_config)
            }
            None => None,
        };
        Ok(StorageConfig {
            name: storage_name.into(),
            key_expr,
            complete,
            strip_prefix,
            volume_id,
            volume_cfg,
            garbage_collection_config,
            replica_config,
        })
    }
}
impl PartialEq for VolumeConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.paths == other.paths && self.rest == other.rest
    }
}
pub enum PrivacyGetResult<T> {
    NotFound,
    Private(T),
    Public(T),
    Both { public: T, private: T },
}
pub trait PrivacyTransparentGet<T> {
    fn get_private(&self, key: &str) -> PrivacyGetResult<&T>;
}
impl PrivacyTransparentGet<serde_json::Value> for serde_json::Map<String, serde_json::Value> {
    fn get_private(&self, key: &str) -> PrivacyGetResult<&serde_json::Value> {
        use PrivacyGetResult::*;
        match (
            self.get(key),
            self.get("private")
                .and_then(|f| f.as_object().and_then(|f| f.get(key))),
        ) {
            (None, None) => NotFound,
            (Some(a), None) => Public(a),
            (None, Some(a)) => Private(a),
            (Some(a), Some(b)) => Both {
                public: a,
                private: b,
            },
        }
    }
}
