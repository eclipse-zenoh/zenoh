use derive_more::{AsMut, AsRef};
use serde_json::{Map, Value};
use std::convert::TryFrom;
use zenoh::Result as ZResult;
use zenoh_util::{bail, core::Error, zerror};

#[derive(Debug, Clone, AsMut, AsRef)]
pub struct PluginConfig {
    pub name: String,
    pub required: bool,
    pub backend_search_dirs: Option<Vec<String>>,
    pub backends: Vec<BackendConfig>,
    #[as_ref]
    #[as_mut]
    pub rest: Map<String, Value>,
}
#[derive(Debug, Clone, AsMut, AsRef)]
pub struct BackendConfig {
    pub name: String,
    pub paths: Option<Vec<String>>,
    pub storages: Vec<StorageConfig>,
    pub required: bool,
    #[as_ref]
    #[as_mut]
    pub rest: Map<String, Value>,
}
#[derive(Debug, Clone, PartialEq, AsMut, AsRef)]
pub struct StorageConfig {
    pub name: String,
    pub key_expr: String,
    pub strip_prefix: String,
    #[as_ref]
    #[as_mut]
    pub rest: Map<String, Value>,
}
pub enum ConfigDiff {
    DeleteBackend(BackendConfig),
    AddBackend(BackendConfig),
    DeleteStorage {
        backend_name: String,
        config: StorageConfig,
    },
    AddStorage {
        backend_name: String,
        config: StorageConfig,
    },
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
        let backends = match value.get("backends") {
            Some(backends) => BackendConfig::try_from(name.as_ref(), backends)?,
            None => Vec::new(),
        };
        Ok(PluginConfig {
            name: name.into(),
            required,
            backend_search_dirs,
            backends,
            rest: value
                .into_iter()
                .filter_map(|(k, v)| {
                    (!["__required__", "backend_search_dirs", "backends"].contains(&k.as_str()))
                        .then(|| (k.clone(), v.clone()))
                })
                .collect(),
        })
    }
}
impl ConfigDiff {
    pub fn diffs(old: PluginConfig, new: PluginConfig) -> Vec<ConfigDiff> {
        let mut diffs = Vec::new();
        for old in &old.backends {
            if let Some(new) = new.backends.iter().find(|&new| new == old) {
                for old_storage in &old.storages {
                    if !new.storages.contains(old_storage) {
                        diffs.push(ConfigDiff::DeleteStorage {
                            backend_name: old.name.clone(),
                            config: old_storage.clone(),
                        })
                    }
                }
            } else {
                diffs.extend(old.storages.iter().map(|config| ConfigDiff::DeleteStorage {
                    backend_name: old.name.clone(),
                    config: config.clone(),
                }));
                diffs.push(ConfigDiff::DeleteBackend(old.clone()));
            }
        }
        for mut new in new.backends {
            if let Some(old) = old.backends.iter().find(|&old| old == &new) {
                for new_storage in new.storages {
                    if !old.storages.contains(&new_storage) {
                        diffs.push(ConfigDiff::AddStorage {
                            backend_name: new.name.clone(),
                            config: new_storage,
                        })
                    }
                }
            } else {
                let mut new_storages = Vec::new();
                std::mem::swap(&mut new_storages, &mut new.storages);
                let name = new.name.clone();
                diffs.push(ConfigDiff::AddBackend(new));
                diffs.extend(
                    new_storages
                        .into_iter()
                        .map(|config| ConfigDiff::AddStorage {
                            backend_name: name.clone(),
                            config,
                        }),
                )
            }
        }
        diffs
    }
}
impl BackendConfig {
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
        result.insert(
            "storages".into(),
            Value::Array(self.storages.iter().map(|s| s.to_json_value()).collect()),
        );
        Value::Object(result)
    }
    fn try_from<V: AsObject>(plugin_name: &str, configs: &V) -> ZResult<Vec<Self>> {
        let configs = configs.as_object().ok_or_else(|| {
            zerror!(
                "Configuration for plugin `{}`'s `backends` field must be an object",
                plugin_name
            )
        })?;
        let mut backends = Vec::with_capacity(configs.len());
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
            let paths = match config.get("path") {
                None => None,
                Some(serde_json::Value::String(s)) => Some(vec![s.clone()]),
                Some(serde_json::Value::Array(a)) => {
                    let mut paths = Vec::with_capacity(a.len());
                    for path in a {
                        if let serde_json::Value::String(path) = path {
                            paths.push(path.clone());
                        } else {
                            bail!("`path` field of `{}`'s `{}` backend configuration must be a string or array of string", plugin_name, name)
                        }
                    }
                    Some(paths)
                }
                _ => bail!("`path` field of `{}`'s `{}` backend configuration must be a string or array of string", plugin_name, name)
            };
            let required = match config.get("required") {
                Some(serde_json::Value::Bool(b)) => *b,
                None => true,
                _ => todo!(),
            };
            let storages = match config.get("storages") {
                Some(serde_json::Value::Object(configs)) => {
                    let mut storages = Vec::with_capacity(configs.len());
                    for (name, config) in configs {
                        storages.push(StorageConfig::try_from(plugin_name, name, name, config)?)
                    }
                    storages
                }
                None => Vec::new(),
                _ => bail!(
                    "`storages` field of `{}`'s `{}` backend configuration must be an object",
                    plugin_name,
                    name
                ),
            };
            backends.push(BackendConfig {
                name: name.clone(),
                paths,
                storages,
                required,
                rest: config
                    .iter()
                    .filter_map(|(k, v)| {
                        (!["path", "required", "storages"].contains(&k.as_str()))
                            .then(|| (k.clone(), v.clone()))
                    })
                    .collect(),
            })
        }
        Ok(backends)
    }
}
impl StorageConfig {
    pub fn to_json_value(&self) -> Value {
        let mut result = self.rest.clone();
        result.insert("key_expr".into(), Value::String(self.key_expr.clone()));
        if !self.strip_prefix.is_empty() {
            result.insert(
                "strip_prefix".into(),
                Value::String(self.strip_prefix.clone()),
            );
        }
        Value::Object(result)
    }
    fn try_from<V: AsObject>(
        plugin_name: &str,
        backend_name: &str,
        storage_name: &str,
        config: &V,
    ) -> ZResult<Self> {
        let config = config.as_object().ok_or_else(|| {
            zerror!(
                "`storages` field of `{}`'s `{}` backend configuration must be an array of objects",
                plugin_name,
                backend_name
            )
        })?;
        let key_expr = if let Some(Value::String(s)) = config.get("key_expr") {
            s.clone()
        } else {
            bail!("elements of the `storages` field of `{}`'s `{}` backend configuration must be objects with at least a `key_expr` field",
            plugin_name,
            backend_name)
        };
        let strip_prefix = match config.get("strip_prefix") {
            Some(Value::String(s)) => s.clone(),
            None => String::new(),
            _ => bail!("`strip_prefix` field of elements of the `storage` field of `{}`'s `{}` backend configuration must be a string",
            plugin_name,
            backend_name)
        };
        Ok(StorageConfig {
            name: storage_name.into(),
            key_expr,
            strip_prefix,
            rest: config
                .into_iter()
                .filter_map(|(k, v)| {
                    (!["key_expr", "strip_prefix"].contains(&k.as_str()))
                        .then(|| (k.clone(), v.clone()))
                })
                .collect(),
        })
    }
}
impl PartialEq for BackendConfig {
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
                .map(|f| f.as_object().map(|f| f.get(key)).flatten())
                .flatten(),
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
