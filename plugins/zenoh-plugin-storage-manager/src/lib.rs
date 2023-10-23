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
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use std::sync::Mutex;
use storages_mgt::StorageMessage;
use zenoh::plugins::{Plugin, RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::sync::*;
use zenoh::runtime::Runtime;
use zenoh::Session;
use zenoh_backend_traits::VolumePlugin;
use zenoh_core::zlock;
use zenoh_result::{bail, ZResult};
use zenoh_util::LibLoader;

mod backends_mgt;
use backends_mgt::*;
mod memory_backend;
mod replica;
mod storages_mgt;

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
lazy_static::lazy_static! {
    static ref LONG_VERSION: String = format!("{} built with {}", GIT_VERSION, env!("RUSTC_VERSION"));
}

zenoh_plugin_trait::declare_plugin!(StoragesPlugin);
pub struct StoragesPlugin {}
impl ZenohPlugin for StoragesPlugin {}
impl Plugin for StoragesPlugin {
    const STATIC_NAME: &'static str = "storage_manager";

    type StartArgs = Runtime;
    type RunningPlugin = zenoh::plugins::RunningPlugin;

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::RunningPlugin> {
        std::mem::drop(env_logger::try_init());
        log::debug!("StorageManager plugin {}", LONG_VERSION.as_str());
        let config =
            { PluginConfig::try_from((name, runtime.config().lock().plugin(name).unwrap())) }?;
        Ok(Box::new(StorageRuntime::from(StorageRuntimeInner::new(
            runtime.clone(),
            config,
        )?)))
    }
}

type PluginsManager = zenoh_plugin_trait::loading::PluginsManager<VolumeConfig, VolumePlugin>;

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
        let _ = env_logger::try_init();
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

        let session = Arc::new(zenoh::init(runtime.clone()).res_sync().unwrap());

        let plugins_manager = PluginsManager::dynamic(lib_loader.clone(), BACKEND_LIB_PREFIX).add_static::<MemoryBackend>();

        let mut new_self = StorageRuntimeInner {
            name,
            runtime,
            session,
            storages: Default::default(),
            plugins_manager
        };
        new_self.spawn_volume(VolumeConfig {
            name: MEMORY_BACKEND_NAME.into(),
            backend: None,
            paths: None,
            required: false,
            rest: Default::default(),
        })?;
        new_self.update(
            volumes
                .into_iter()
                .map(ConfigDiff::AddVolume)
                .chain(storages.into_iter().map(ConfigDiff::AddStorage)),
        )?;
        Ok(new_self)
    }
    fn update<I: IntoIterator<Item = ConfigDiff>>(&mut self, diffs: I) -> ZResult<()> {
        for diff in diffs {
            match diff {
                ConfigDiff::DeleteVolume(volume) => self.kill_volume(&volume.name),
                ConfigDiff::AddVolume(volume) => {
                    self.spawn_volume(volume)?;
                }
                ConfigDiff::DeleteStorage(config) => self.kill_storage(config),
                ConfigDiff::AddStorage(config) => self.spawn_storage(config)?,
            }
        }
        Ok(())
    }
    fn kill_volume<T: AsRef<str>>(&mut self, name: T) {
        let name = name.as_ref();
        if let Some(storages) = self.storages.remove(name) {
            async_std::task::block_on(futures::future::join_all(
                storages
                    .into_values()
                    .map(|s| async move { s.send(StorageMessage::Stop) }),
            ));
        }
        self.plugins_manager.stop(name);
    }
    fn spawn_volume(&mut self, config: VolumeConfig) -> ZResult<()> {
        let volume_id = config.name();
        let backend_name = config.backend();
        if let Some(paths) = config.paths() {
            self.plugins_manager
                .load_plugin_by_paths(volume_id, paths)?;
        } else {
            self.plugins_manager
                .load_plugin_by_backend_name(volume_id, backend_name)?;
        }
        self.plugins_manager.start(volume_id, &config)?;
        Ok(())
    }
    fn kill_storage(&mut self, config: StorageConfig) {
        let volume = &config.volume_id;
        if let Some(storages) = self.storages.get_mut(volume) {
            if let Some(storage) = storages.get_mut(&config.name) {
                log::debug!(
                    "Closing storage {} from volume {}",
                    config.name,
                    config.volume_id
                );
                // let _ = async_std::task::block_on(storage.send(StorageMessage::Stop));
                let _ = storage.send(StorageMessage::Stop); // TODO: was previosuly spawning a task. do we need that?
            }
        }
    }
    fn spawn_storage(&mut self, storage: StorageConfig) -> ZResult<()> {
        let admin_key = self.status_key() + "/storages/" + &storage.name;
        let volume_id = storage.volume_id.clone();
        if let Some(backend) = self.plugins_manager.plugin(&volume_id) {
            let storage_name = storage.name.clone();
            let in_interceptor = backend.incoming_data_interceptor();
            let out_interceptor = backend.outgoing_data_interceptor();
            let stopper = async_std::task::block_on(create_and_start_storage(
                admin_key,
                storage,
                &backend,
                in_interceptor,
                out_interceptor,
                self.session.clone(),
            ))?;
            self.storages
                .entry(volume_id)
                .or_default()
                .insert(storage_name, stopper);
            Ok(())
        } else {
            bail!(
                "`{}` volume doesn't support the required storage configuration",
                volume_id
            )
        }
    }
}
impl From<StorageRuntimeInner> for StorageRuntime {
    fn from(inner: StorageRuntimeInner) -> Self {
        StorageRuntime(Arc::new(Mutex::new(inner)))
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
        log::info!("old: {:?}", &old);
        log::info!("new: {:?}", &new);
        let diffs = ConfigDiff::diffs(old, new);
        log::info!("diff: {:?}", &diffs);
        { zlock!(self.0).update(diffs) }?;
        Ok(None)
    }

    fn adminspace_getter<'a>(
        &'a self,
        selector: &'a Selector<'a>,
        plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::plugins::Response>> {
        let mut responses = Vec::new();
        let mut key = String::from(plugin_status_key);
        with_extended_string(&mut key, &["/version"], |key| {
            if keyexpr::new(key.as_str())
                .unwrap()
                .intersects(&selector.key_expr)
            {
                responses.push(zenoh::plugins::Response::new(
                    key.clone(),
                    GIT_VERSION.into(),
                ))
            }
        });
        let guard = self.0.lock().unwrap();
        with_extended_string(&mut key, &["/volumes/"], |key| {
            for (volume_id, (lib_path, volume)) in guard.plugins_manager.running_plugins() {
                with_extended_string(key, &[volume_id], |key| {
                    with_extended_string(key, &["/__path__"], |key| {
                        if keyexpr::new(key.as_str())
                            .unwrap()
                            .intersects(&selector.key_expr)
                        {
                            responses.push(zenoh::plugins::Response::new(
                                key.clone(),
                                lib_path.into(),
                            ))
                        }
                    });
                    if keyexpr::new(key.as_str())
                        .unwrap()
                        .intersects(&selector.key_expr)
                    {
                        responses.push(zenoh::plugins::Response::new(
                            key.clone(),
                            volume.get_admin_status(),
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
                            .intersects(&selector.key_expr)
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
