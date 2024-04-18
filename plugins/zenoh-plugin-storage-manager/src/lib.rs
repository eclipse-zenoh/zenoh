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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
#![recursion_limit = "512"]

use async_std::task;
use flume::Sender;
use memory_backend::MemoryBackend;
use zenoh::core::try_init_log_from_env;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::Mutex;
use storages_mgt::StorageMessage;
use zenoh::core::Result as ZResult;
use zenoh::core::SyncResolve;
use zenoh::internal::zlock;
use zenoh::internal::LibLoader;
use zenoh::key_expr::keyexpr;
use zenoh::plugins::{RunningPluginTrait, ZenohPlugin};
use zenoh::runtime::Runtime;
use zenoh::selector::Selector;
use zenoh::session::Session;
use zenoh_backend_traits::config::ConfigDiff;
use zenoh_backend_traits::config::PluginConfig;
use zenoh_backend_traits::config::StorageConfig;
use zenoh_backend_traits::config::VolumeConfig;
use zenoh_backend_traits::VolumeInstance;
use zenoh_plugin_trait::plugin_long_version;
use zenoh_plugin_trait::plugin_version;
use zenoh_plugin_trait::Plugin;
use zenoh_plugin_trait::PluginControl;
use zenoh_plugin_trait::PluginReport;
use zenoh_plugin_trait::PluginStatusRec;

mod backends_mgt;
use backends_mgt::*;
mod memory_backend;
mod replica;
mod storages_mgt;

#[cfg(feature = "no_mangle")]
zenoh_plugin_trait::declare_plugin!(StoragesPlugin);

pub struct StoragesPlugin {}
impl ZenohPlugin for StoragesPlugin {}
impl Plugin for StoragesPlugin {
    const DEFAULT_NAME: &'static str = "storage_manager";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    type StartArgs = Runtime;
    type Instance = zenoh::plugins::RunningPlugin;

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::Instance> {
        try_init_log_from_env();
        tracing::debug!("StorageManager plugin {}", Self::PLUGIN_VERSION);
        let config =
            { PluginConfig::try_from((name, runtime.config().lock().plugin(name).unwrap())) }?;
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
    runtime: Runtime,
    session: Arc<Session>,
    storages: HashMap<String, HashMap<String, Sender<StorageMessage>>>,
    plugins_manager: PluginsManager,
}
impl StorageRuntimeInner {
    fn status_key(&self) -> String {
        format!(
            "@/router/{}/status/plugins/{}",
            &self.runtime.zid(),
            &self.name
        )
    }
    fn new(runtime: Runtime, config: PluginConfig) -> ZResult<Self> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        try_init_log_from_env();
        let PluginConfig {
            name,
            backend_search_dirs,
            volumes,
            storages,
            ..
        } = config;
        let lib_loader = backend_search_dirs
            .map(|search_dirs| LibLoader::new(&search_dirs, false))
            .unwrap_or_default();

        let plugins_manager = PluginsManager::dynamic(lib_loader.clone(), BACKEND_LIB_PREFIX)
            .declare_static_plugin::<MemoryBackend>();

        let session = Arc::new(zenoh::session::init(runtime.clone()).res_sync()?);

        // After this moment result should be only Ok. Failure of loading of one voulme or storage should not affect others.

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
            async_std::task::block_on(futures::future::join_all(
                storages
                    .into_values()
                    .map(|s| async move { s.send(StorageMessage::Stop) }),
            ));
        }
        self.plugins_manager
            .started_plugin_mut(name)
            .ok_or(format!("Cannot find volume '{}' to stop it", name))?
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
        let declared = if let Some(declared) = self.plugins_manager.plugin_mut(volume_id) {
            declared
        } else if let Some(paths) = config.paths() {
            self.plugins_manager
                .declare_dynamic_plugin_by_paths(volume_id, paths)?
        } else {
            self.plugins_manager
                .declare_dynamic_plugin_by_name(volume_id, backend_name)?
        };
        let loaded = declared.load()?;
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
                let _ = storage.send(StorageMessage::Stop); // TODO: was previosuly spawning a task. do we need that?
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
        let stopper = async_std::task::block_on(create_and_start_storage(
            admin_key,
            storage.clone(),
            backend.instance(),
            self.session.clone(),
        ))?;
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
    fn plugins_status(&self, names: &keyexpr) -> Vec<PluginStatusRec> {
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
        old: &serde_json::Map<String, serde_json::Value>,
        new: &serde_json::Map<String, serde_json::Value>,
    ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>> {
        let name = { zlock!(self.0).name.clone() };
        let old = PluginConfig::try_from((&name, old))?;
        let new = PluginConfig::try_from((&name, new))?;
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
        selector: &'a Selector<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::plugins::Response>> {
        let mut responses = Vec::new();
        let mut key = String::from(plugin_status_key);
        // TODO: to be removed when "__version__" is implemented in admoin space
        with_extended_string(&mut key, &["/version"], |key| {
            if keyexpr::new(key.as_str())
                .unwrap()
                .intersects(selector.key_expr())
            {
                responses.push(zenoh::plugins::Response::new(
                    key.clone(),
                    StoragesPlugin::PLUGIN_VERSION.into(),
                ))
            }
        });
        let guard = self.0.lock().unwrap();
        with_extended_string(&mut key, &["/volumes/"], |key| {
            for plugin in guard.plugins_manager.started_plugins_iter() {
                with_extended_string(key, &[plugin.name()], |key| {
                    with_extended_string(key, &["/__path__"], |key| {
                        if keyexpr::new(key.as_str())
                            .unwrap()
                            .intersects(selector.key_expr())
                        {
                            responses.push(zenoh::plugins::Response::new(
                                key.clone(),
                                plugin.path().into(),
                            ))
                        }
                    });
                    if keyexpr::new(key.as_str())
                        .unwrap()
                        .intersects(selector.key_expr())
                    {
                        responses.push(zenoh::plugins::Response::new(
                            key.clone(),
                            plugin.instance().get_admin_status(),
                        ))
                    }
                });
            }
        });
        with_extended_string(&mut key, &["/storages/"], |key| {
            for storages in guard.storages.values() {
                for (storage, handle) in storages {
                    with_extended_string(key, &[storage], |key| {
                        if keyexpr::new(key.as_str())
                            .unwrap()
                            .intersects(selector.key_expr())
                        {
                            if let Ok(value) = task::block_on(async {
                                let (tx, rx) = async_std::channel::bounded(1);
                                let _ = handle.send(StorageMessage::GetStatus(tx));
                                rx.recv().await
                            }) {
                                responses.push(zenoh::plugins::Response::new(key.clone(), value))
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
