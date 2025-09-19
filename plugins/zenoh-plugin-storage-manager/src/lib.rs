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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
#![recursion_limit = "512"]

use std::{
    collections::HashMap,
    convert::TryFrom,
    str::FromStr,
    sync::{Arc, Mutex},
};

use memory_backend::MemoryBackend;
use storages_mgt::StorageMessage;
use tokio::sync::broadcast::Sender;
use zenoh::{
    internal::{
        bail,
        plugins::{Response, RunningPlugin, RunningPluginTrait, ZenohPlugin},
        runtime::DynamicRuntime,
        zlock, LibLoader,
    },
    key_expr::{keyexpr, KeyExpr, OwnedKeyExpr},
    session::Session,
    Result as ZResult, Wait,
};
use zenoh_backend_traits::{
    config::{ConfigDiff, PluginConfig, StorageConfig, VolumeConfig},
    VolumeInstance,
};
use zenoh_plugin_trait::{
    plugin_long_version, plugin_version, Plugin, PluginControl, PluginReport, PluginStatusRec,
};

mod memory_backend;
mod replication;
mod storages_mgt;
use storages_mgt::*;
use zenoh_util::ffi::JsonKeyValueMap;

const WORKER_THREAD_NUM: usize = 2;
const MAX_BLOCK_THREAD_NUM: usize = 50;
lazy_static::lazy_static! {
    // The global runtime is used in the zenohd case, which we can't get the current runtime
    static ref TOKIO_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
               .worker_threads(WORKER_THREAD_NUM)
               .max_blocking_threads(MAX_BLOCK_THREAD_NUM)
               .enable_all()
               .build()
               .expect("Unable to create runtime");
}

#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(StoragesPlugin);

pub struct StoragesPlugin {}
impl ZenohPlugin for StoragesPlugin {}
impl Plugin for StoragesPlugin {
    const DEFAULT_NAME: &'static str = "storage_manager";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    type StartArgs = DynamicRuntime;
    type Instance = RunningPlugin;

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::Instance> {
        zenoh::init_log_from_env_or("error");
        tracing::debug!("StorageManager plugin {}", Self::PLUGIN_VERSION);
        let config = {
            PluginConfig::try_from((name, &runtime.get_config().get_plugin_config(name).unwrap()))
        }?;
        Ok(Box::new(StorageRuntime::from(StorageRuntimeInner::new(
            runtime.clone(),
            config,
        )?)))
    }
}

type PluginsManager = zenoh_plugin_trait::PluginsManager<VolumeConfig, VolumeInstance>;

struct StorageRuntime(Arc<Mutex<StorageRuntimeInner>>);
struct StorageRuntimeInner {
    name: String,
    runtime: DynamicRuntime,
    session: Arc<Session>,
    storages: HashMap<String, HashMap<String, Sender<StorageMessage>>>,
    plugins_manager: PluginsManager,
}
impl StorageRuntimeInner {
    fn status_key(&self) -> String {
        format!(
            "@/{}/{}/status/plugins/{}",
            &self.runtime.zid(),
            &self.runtime.whatami().to_str(),
            &self.name
        )
    }
    fn new(runtime: DynamicRuntime, config: PluginConfig) -> ZResult<Self> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh::init_log_from_env_or("error");
        let PluginConfig {
            name,
            backend_search_dirs,
            volumes,
            storages,
            ..
        } = config;
        let lib_loader = LibLoader::new(backend_search_dirs);

        let mut plugins_manager = PluginsManager::dynamic(lib_loader.clone(), BACKEND_LIB_PREFIX);
        plugins_manager.declare_static_plugin::<MemoryBackend, &str>(MEMORY_BACKEND_NAME, true);

        let session = Arc::new(zenoh::session::init(runtime.clone()).wait()?);

        // NOTE: All storage **must** have a timestamp associated with a Sample. Considering that it is possible to make
        //       a publication without associating a timestamp, that means that the node managing the storage (be it a
        //       Zenoh client / peer / router) has to add it.
        //
        //       If the `timestamping` configuration setting is disabled then there is no HLC associated with the
        //       Session. That eventually means that no timestamp can be generated which goes against the previous
        //       requirement.
        //
        //       Hence, in that scenario, we refuse to start the storage manager and any storage.
        if session.hlc().is_none() {
            tracing::error!(
                "Cannot start storage manager (and thus any storage) without the 'timestamping' \
                 setting enabled in the Zenoh configuration"
            );
            bail!("Cannot start storage manager, 'timestamping' is disabled in the configuration");
        }

        // After this moment result should be only Ok. Failure of loading of one volume or storage should not affect others.

        let mut new_self = StorageRuntimeInner {
            name,
            runtime,
            session,
            storages: Default::default(),
            plugins_manager,
        };
        new_self
            .spawn_volume(&VolumeConfig {
                name: MEMORY_BACKEND_NAME.into(),
                backend: None,
                paths: None,
                required: false,
                rest: Default::default(),
            })
            .map_or_else(
                |e| {
                    tracing::error!(
                        "Cannot spawn static volume '{}': {}",
                        MEMORY_BACKEND_NAME,
                        e
                    )
                },
                |_| (),
            );
        for volume in &volumes {
            new_self.spawn_volume(volume).map_or_else(
                |e| tracing::error!("Cannot spawn volume '{}': {}", volume.name(), e),
                |_| (),
            );
        }
        for storage in &storages {
            new_self.spawn_storage(storage).map_or_else(
                |e| tracing::error!("Cannot spawn storage '{}': {}", storage.name(), e),
                |_| (),
            );
        }
        Ok(new_self)
    }
    fn update<I: IntoIterator<Item = ConfigDiff>>(&mut self, diffs: I) -> ZResult<()> {
        for ref diff in diffs {
            match diff {
                ConfigDiff::DeleteVolume(volume) => self.kill_volume(&volume.name)?,
                ConfigDiff::AddVolume(volume) => {
                    self.spawn_volume(volume)?;
                }
                ConfigDiff::DeleteStorage(config) => self.kill_storage(config),
                ConfigDiff::AddStorage(config) => self.spawn_storage(config)?,
            }
        }
        Ok(())
    }
    fn kill_volume<T: AsRef<str>>(&mut self, name: T) -> ZResult<()> {
        let name = name.as_ref();
        tracing::info!("Killing volume '{}'", name);
        if let Some(storages) = self.storages.remove(name) {
            tokio::task::block_in_place(|| {
                TOKIO_RUNTIME.block_on(futures::future::join_all(
                    storages
                        .into_values()
                        .map(|s| async move { s.send(StorageMessage::Stop) }),
                ))
            });
        }
        self.plugins_manager
            .started_plugin_mut(name)
            .ok_or(format!("Cannot find volume '{name}' to stop it"))?
            .stop();
        Ok(())
    }
    fn spawn_volume(&mut self, config: &VolumeConfig) -> ZResult<()> {
        let volume_id = config.name();
        let backend_name = config.backend();
        tracing::info!(
            "Spawning volume '{}' with backend '{}'",
            volume_id,
            backend_name
        );
        let declared = if let Some(declared) = self.plugins_manager.plugin_mut(backend_name) {
            declared
        } else if let Some(paths) = config.paths() {
            self.plugins_manager.declare_dynamic_plugin_by_paths(
                backend_name,
                volume_id,
                paths,
                true,
            )?
        } else {
            self.plugins_manager
                .declare_dynamic_plugin_by_name(volume_id, backend_name, true)?
        };
        let loaded = declared
            .load()?
            .expect("Volumes should not be loaded if the storage-manager plugin is not loaded");
        loaded.start(config)?;

        Ok(())
    }
    fn kill_storage(&mut self, config: &StorageConfig) {
        let volume = &config.volume_id;
        tracing::info!("Killing storage '{}' from volume '{}'", config.name, volume);
        if let Some(storages) = self.storages.get_mut(volume) {
            if let Some(storage) = storages.get_mut(&config.name) {
                tracing::debug!(
                    "Closing storage '{}' from volume '{}'",
                    config.name,
                    config.volume_id
                );
                // let _ = async_std::task::block_on(storage.send(StorageMessage::Stop));
                let _ = storage.send(StorageMessage::Stop); // TODO: was previously spawning a task. do we need that?
            }
        }
    }
    fn spawn_storage(&mut self, storage: &StorageConfig) -> ZResult<()> {
        let admin_key = self.status_key() + "/storages/" + &storage.name;
        let volume_id = storage.volume_id.clone();
        let backend = self
            .plugins_manager
            .started_plugin(&volume_id)
            .ok_or(format!(
                "Cannot find volume '{}' to spawn storage '{}'",
                volume_id, storage.name
            ))?;
        let storage_name = storage.name.clone();
        tracing::info!(
            "Spawning storage '{}' from volume '{}' with backend '{}'",
            storage_name,
            volume_id,
            backend.name()
        );
        let stopper = tokio::task::block_in_place(|| {
            TOKIO_RUNTIME.block_on(create_and_start_storage(
                admin_key,
                storage.clone(),
                backend.instance(),
                self.session.clone(),
            ))
        })?;
        self.storages
            .entry(volume_id)
            .or_default()
            .insert(storage_name, stopper);
        Ok(())
    }
}
impl From<StorageRuntimeInner> for StorageRuntime {
    fn from(inner: StorageRuntimeInner) -> Self {
        StorageRuntime(Arc::new(Mutex::new(inner)))
    }
}

impl PluginControl for StorageRuntime {
    fn report(&self) -> PluginReport {
        PluginReport::default()
    }
    fn plugins_status(&self, names: &keyexpr) -> Vec<PluginStatusRec<'_>> {
        let guard = self.0.lock().unwrap();
        guard
            .plugins_manager
            .plugins_status(names)
            .into_iter()
            .map(PluginStatusRec::into_owned)
            .collect()
    }
}

impl RunningPluginTrait for StorageRuntime {
    fn config_checker(
        &self,
        _: &str,
        old: &JsonKeyValueMap,
        new: &JsonKeyValueMap,
    ) -> ZResult<Option<JsonKeyValueMap>> {
        let old: serde_json::Map<String, serde_json::Value> = old.into();
        let new: serde_json::Map<String, serde_json::Value> = new.into();
        let name = { zlock!(self.0).name.clone() };
        let old = PluginConfig::try_from((&name, &old))?;
        let new = PluginConfig::try_from((&name, &new))?;
        tracing::debug!("config change requested for plugin '{}'", name);
        tracing::debug!("old config: {:?}", &old);
        tracing::debug!("new config: {:?}", &new);
        let diffs = ConfigDiff::diffs(old, new);
        tracing::debug!("applying diff: {:?}", &diffs);
        { zlock!(self.0).update(diffs) }?;
        tracing::debug!("applying diff done");
        Ok(None)
    }

    fn adminspace_getter<'a>(
        &'a self,
        key_expr: &'a KeyExpr<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<Response>> {
        let mut responses = Vec::new();
        let mut key = String::from(plugin_status_key);
        // TODO: to be removed when "__version__" is implemented in admin space
        with_extended_string(&mut key, &["/version"], |key| {
            if keyexpr::new(key.as_str()).unwrap().intersects(key_expr) {
                responses.push(Response::new(
                    key.clone(),
                    StoragesPlugin::PLUGIN_VERSION.into(),
                ))
            }
        });
        let guard = self.0.lock().unwrap();
        with_extended_string(&mut key, &["/volumes/"], |key| {
            for plugin in guard.plugins_manager.started_plugins_iter() {
                with_extended_string(key, &[plugin.id()], |key| {
                    with_extended_string(key, &["/__path__"], |key| {
                        if keyexpr::new(key.as_str()).unwrap().intersects(key_expr) {
                            responses.push(Response::new(key.clone(), plugin.path().into()))
                        }
                    });
                    if keyexpr::new(key.as_str()).unwrap().intersects(key_expr) {
                        responses.push(Response::new(
                            key.clone(),
                            plugin.instance().get_admin_status().into(),
                        ))
                    }
                });
            }
        });
        with_extended_string(&mut key, &["/storages/"], |key| {
            for storages in guard.storages.values() {
                for (storage, handle) in storages {
                    with_extended_string(key, &[storage], |key| {
                        if keyexpr::new(key.as_str()).unwrap().intersects(key_expr) {
                            if let Some(value) = tokio::task::block_in_place(|| {
                                TOKIO_RUNTIME.block_on(async {
                                    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
                                    let _ = handle.send(StorageMessage::GetStatus(tx));
                                    rx.recv().await
                                })
                            }) {
                                responses.push(Response::new(key.clone(), value))
                            }
                        }
                    })
                }
            }
        });
        Ok(responses)
    }
}

const BACKEND_LIB_PREFIX: &str = "zenoh_backend_";
const MEMORY_BACKEND_NAME: &str = "memory";

fn with_extended_string<R, F: FnMut(&mut String) -> R>(
    prefix: &mut String,
    suffixes: &[&str],
    mut closure: F,
) -> R {
    let prefix_len = prefix.len();
    for suffix in suffixes {
        prefix.push_str(suffix);
    }
    let result = closure(prefix);
    prefix.truncate(prefix_len);
    result
}

/// Returns the key expression stripped of the provided prefix.
///
/// If no prefix is provided this function returns the key expression untouched.
///
/// If `None` is returned, it indicates that the key expression is equal to the prefix.
///
/// This function will internally call [strip_prefix], see its documentation for possible outcomes.
///
/// # Errors
///
/// This function will return an error if:
/// - The provided prefix contains a wildcard.
///   NOTE: The configuration of a Storage is checked and will reject any prefix that contains a
///   wildcard. In theory, this error should never occur.
/// - The key expression is not prefixed by the provided prefix.
/// - The resulting stripped key is not a valid key expression (this should, in theory, never
///   happen).
///
/// [strip_prefix]: zenoh::key_expr::keyexpr::strip_prefix()
pub fn strip_prefix(
    maybe_prefix: Option<&OwnedKeyExpr>,
    key_expr: &KeyExpr<'_>,
) -> ZResult<Option<OwnedKeyExpr>> {
    match maybe_prefix {
        None => Ok(Some(key_expr.clone().into())),
        Some(prefix) => {
            if prefix.is_wild() {
                bail!(
                    "Prefix < {} > contains a wild character (\"**\" or \"*\")",
                    prefix
                );
            }

            // `key_expr.strip_prefix` returns empty vec if `key_expr == prefix`,
            // but also returns empty vec if `prefix` is not a prefix to `key_expr`.
            // First case needs to be handled before calling `key_expr.strip_prefix`
            if key_expr.as_str().eq(prefix.as_str()) {
                return Ok(None);
            }

            match key_expr.strip_prefix(prefix).as_slice() {
                // NOTE: `stripped_key_expr.is_empty()` should be impossible as "" is not a valid key expression
                [stripped_key_expr] => OwnedKeyExpr::from_str(stripped_key_expr).map(Some),
                _ => bail!("Failed to strip prefix < {} > from: {}", prefix, key_expr),
            }
        }
    }
}

/// Returns the key with an additional prefix, if both were provided.
///
/// If no prefix is provided, this function returns `maybe_stripped_key`.
///
/// If no key is provided, this function returns the `maybe_prefix`.
///
/// If a prefix is provided, this function returns the concatenation of both.
///
/// # Error
///
/// This function will return an error if both `maybe_prefix` and `maybe_stripped_key` are equal to
/// `None`. This situation can happen if (i) the "backend" associated to a Storage is first started
/// with a `strip_prefix` set to some value, (ii) a key equal to the `strip_prefix` is published
/// (hence storing `None`) then (iii) the Storage is stopped and the "backend" is associated to
/// another Storage without a `strip_prefix` configured.
pub fn prefix(
    maybe_prefix: Option<&OwnedKeyExpr>,
    maybe_stripped_key: Option<&OwnedKeyExpr>,
) -> ZResult<OwnedKeyExpr> {
    match (maybe_prefix, maybe_stripped_key) {
        (Some(prefix), Some(stripped_key)) => Ok(prefix / stripped_key),
        (Some(prefix), None) => Ok(prefix.clone()),
        (None, Some(key)) => Ok(key.clone()),
        (None, None) => bail!("Fatal internal error: empty prefix with empty key"),
    }
}
