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
use libloading::Library;
use memory_backend::create_memory_backend;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;
use storages_mgt::StorageMessage;
use zenoh::plugins::{Plugin, RunningPluginTrait, ValidationFunction, ZenohPlugin};
use zenoh::prelude::sync::*;
use zenoh::runtime::Runtime;
use zenoh::Session;
use zenoh_backend_traits::CreateVolume;
use zenoh_backend_traits::CREATE_VOLUME_FN_NAME;
use zenoh_backend_traits::{config::*, Volume};
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
            { PluginConfig::try_from((name, runtime.config.lock().plugin(name).unwrap())) }?;
        Ok(Box::new(StorageRuntime::from(StorageRuntimeInner::new(
            runtime.clone(),
            config,
        )?)))
    }
}
struct StorageRuntime(Arc<Mutex<StorageRuntimeInner>>);
struct StorageRuntimeInner {
    name: String,
    runtime: Runtime,
    session: Arc<Session>,
    lib_loader: LibLoader,
    volumes: HashMap<String, VolumeHandle>,
    storages: HashMap<String, HashMap<String, Sender<StorageMessage>>>,
}
impl StorageRuntimeInner {
    fn status_key(&self) -> String {
        format!(
            "@/router/{}/status/plugins/{}",
            &self.runtime.zid, &self.name
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
        let mut new_self = StorageRuntimeInner {
            name,
            runtime,
            session,
            lib_loader,
            volumes: Default::default(),
            storages: Default::default(),
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
                ConfigDiff::DeleteVolume(volume) => self.kill_volume(volume),
                ConfigDiff::AddVolume(volume) => {
                    self.spawn_volume(volume)?;
                }
                ConfigDiff::DeleteStorage(config) => self.kill_storage(config),
                ConfigDiff::AddStorage(config) => self.spawn_storage(config)?,
            }
        }
        Ok(())
    }
    fn kill_volume(&mut self, volume: VolumeConfig) {
        if let Some(storages) = self.storages.remove(&volume.name) {
            async_std::task::block_on(futures::future::join_all(
                storages
                    .into_values()
                    .map(|s| async move { s.send(StorageMessage::Stop) }),
            ));
        }
        std::mem::drop(self.volumes.remove(&volume.name));
    }
    fn spawn_volume(&mut self, config: VolumeConfig) -> ZResult<()> {
        let volume_id = config.name.clone();
        if volume_id == MEMORY_BACKEND_NAME {
            match create_memory_backend(config) {
                Ok(backend) => {
                    self.volumes.insert(
                        volume_id,
                        VolumeHandle::new(backend, None, "<static-memory>".into()),
                    );
                }
                Err(e) => bail!("{}", e),
            }
        } else {
            match config.backend_search_method() {
                BackendSearchMethod::ByPaths(paths) => {
                    for path in paths {
                        unsafe {
                            if let Ok((lib, path)) = LibLoader::load_file(path) {
                                self.loaded_backend_from_lib(
                                    &volume_id,
                                    config.clone(),
                                    lib,
                                    path,
                                )?;
                                break;
                            }
                        }
                    }
                    bail!(
                        "Failed to find a suitable library for volume {} from paths: {:?}",
                        volume_id,
                        paths
                    );
                }
                BackendSearchMethod::ByName(backend_name) => unsafe {
                    let backend_filename = format!("{}{}", BACKEND_LIB_PREFIX, &backend_name);
                    if let Ok((lib, path)) = self.lib_loader.search_and_load(&backend_filename) {
                        self.loaded_backend_from_lib(&volume_id, config.clone(), lib, path)?;
                    } else {
                        bail!(
                            "Failed to find a suitable library for volume {} (was looking for <lib>{}<.so/.dll/.dylib>)",
                            volume_id,
                            &backend_filename
                        );
                    }
                },
            };
        };
        Ok(())
    }
    unsafe fn loaded_backend_from_lib(
        &mut self,
        volume_id: &str,
        config: VolumeConfig,
        lib: Library,
        lib_path: PathBuf,
    ) -> ZResult<()> {
        if let Ok(create_backend) = lib.get::<CreateVolume>(CREATE_VOLUME_FN_NAME) {
            match create_backend(config) {
                Ok(backend) => {
                    self.volumes.insert(
                        volume_id.to_string(),
                        VolumeHandle::new(
                            backend,
                            Some(lib),
                            lib_path.to_string_lossy().into_owned(),
                        ),
                    );
                    Ok(())
                }
                Err(e) => bail!(
                    "Failed to load Backend {} from {}: {}",
                    volume_id,
                    lib_path.display(),
                    e
                ),
            }
        } else {
            bail!(
                "Failed to instantiate volume {} from {}: function {}(VolumeConfig) not found in lib",
                volume_id,
                lib_path.display(),
                String::from_utf8_lossy(CREATE_VOLUME_FN_NAME)
            );
        }
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
        if let Some(backend) = self.volumes.get_mut(&volume_id) {
            let storage_name = storage.name.clone();
            let in_interceptor = backend.backend.incoming_data_interceptor();
            let out_interceptor = backend.backend.outgoing_data_interceptor();
            let stopper = async_std::task::block_on(create_and_start_storage(
                admin_key,
                storage,
                &mut backend.backend,
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
struct VolumeHandle {
    backend: Box<dyn Volume>,
    _lib: Option<Library>,
    lib_path: String,
    stopper: Arc<AtomicBool>,
}
impl VolumeHandle {
    fn new(backend: Box<dyn Volume>, lib: Option<Library>, lib_path: String) -> Self {
        VolumeHandle {
            backend,
            _lib: lib,
            lib_path,
            stopper: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }
}
impl Drop for VolumeHandle {
    fn drop(&mut self) {
        self.stopper
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
impl From<StorageRuntimeInner> for StorageRuntime {
    fn from(inner: StorageRuntimeInner) -> Self {
        StorageRuntime(Arc::new(Mutex::new(inner)))
    }
}

impl RunningPluginTrait for StorageRuntime {
    fn config_checker(&self) -> ValidationFunction {
        let name = { zlock!(self.0).name.clone() };
        let runtime = self.0.clone();
        Arc::new(move |_path, old, new| {
            let old = PluginConfig::try_from((&name, old))?;
            let new = PluginConfig::try_from((&name, new))?;
            log::info!("old: {:?}", &old);
            log::info!("new: {:?}", &new);
            let diffs = ConfigDiff::diffs(old, new);
            log::info!("diff: {:?}", &diffs);
            { zlock!(runtime).update(diffs) }?;
            Ok(None)
        })
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
            for (volume_id, volume) in &guard.volumes {
                with_extended_string(key, &[volume_id], |key| {
                    with_extended_string(key, &["/__path__"], |key| {
                        if keyexpr::new(key.as_str())
                            .unwrap()
                            .intersects(&selector.key_expr)
                        {
                            responses.push(zenoh::plugins::Response::new(
                                key.clone(),
                                volume.lib_path.clone().into(),
                            ))
                        }
                    });
                    if keyexpr::new(key.as_str())
                        .unwrap()
                        .intersects(&selector.key_expr)
                    {
                        responses.push(zenoh::plugins::Response::new(
                            key.clone(),
                            volume.backend.get_admin_status(),
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
