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
#![recursion_limit = "512"]

use async_std::channel::Sender;
use async_std::sync::Arc;
use async_std::task;
use libloading::Library;
use log::debug;
use memory_backend::create_memory_backend;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use storages_mgt::StorageMessage;
use zenoh::net::runtime::Runtime;
use zenoh::plugins::{Plugin, RunningPluginTrait, ValidationFunction, ZenohPlugin};
use zenoh::prelude::*;
use zenoh::Session;
use zenoh_backend_traits::CreateBackend;
use zenoh_backend_traits::CREATE_BACKEND_FN_NAME;
use zenoh_backend_traits::{config::*, Backend};
use zenoh_util::core::Result as ZResult;
use zenoh_util::LibLoader;
use zenoh_util::{bail, zlock};

mod backends_mgt;
use backends_mgt::*;
mod memory_backend;
mod storages_mgt;

zenoh_plugin_trait::declare_plugin!(StoragesPlugin);
pub struct StoragesPlugin {}
impl ZenohPlugin for StoragesPlugin {}
impl Plugin for StoragesPlugin {
    const STATIC_NAME: &'static str = "storages";

    type StartArgs = Runtime;
    type RunningPlugin = zenoh::plugins::RunningPlugin;

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::RunningPlugin> {
        std::mem::drop(env_logger::try_init());
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
    backends: HashMap<String, BackendHandle>,
    storages: HashMap<String, HashMap<String, Sender<StorageMessage>>>,
}
impl StorageRuntimeInner {
    fn status_key(&self) -> String {
        format!(
            "/@/router/{}/status/plugins/{}",
            &self.runtime.pid, &self.name
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
            backends,
            ..
        } = config;
        let lib_loader = backend_search_dirs
            .map(|search_dirs| LibLoader::new(&search_dirs, false))
            .unwrap_or_default();

        let session = Arc::new(zenoh::init(runtime.clone()).wait().unwrap());
        let mut new_self = StorageRuntimeInner {
            name,
            runtime,
            session,
            lib_loader,
            backends: Default::default(),
            storages: Default::default(),
        };
        new_self.update(backends.into_iter().map(ConfigDiff::AddBackend))?;
        Ok(new_self)
    }
    fn update<I: IntoIterator<Item = ConfigDiff>>(&mut self, diffs: I) -> ZResult<()> {
        for diff in diffs {
            match diff {
                ConfigDiff::DeleteBackend(backend) => self.kill_backend(backend),
                ConfigDiff::AddBackend(mut backend) => {
                    let name = backend.name.clone();
                    let mut storages = Vec::new();
                    std::mem::swap(&mut storages, &mut backend.storages);
                    self.spawn_backend(backend)?;
                    for storage in storages {
                        self.spawn_storage(&name, storage)?;
                    }
                }
                ConfigDiff::DeleteStorage {
                    backend_name,
                    config,
                } => self.kill_storage(&backend_name, config),
                ConfigDiff::AddStorage {
                    backend_name,
                    config,
                } => self.spawn_storage(&backend_name, config)?,
            }
        }
        Ok(())
    }
    fn kill_backend(&mut self, backend: BackendConfig) {
        debug!("Close backend {} and all its storages", backend.name);
        if let Some(storages) = self.storages.remove(&backend.name) {
            async_std::task::block_on(futures::future::join_all(
                storages
                    .into_iter()
                    .map(|(_, s)| async move { s.send(StorageMessage::Stop).await }),
            ));
        }
        std::mem::drop(self.backends.remove(&backend.name));
    }
    fn spawn_backend(&mut self, config: BackendConfig) -> ZResult<()> {
        let backend_name = config.name.clone();
        if backend_name == MEMORY_BACKEND_NAME {
            match create_memory_backend(config) {
                Ok(backend) => {
                    self.backends.insert(
                        backend_name,
                        BackendHandle::new(backend, None, "<static-memory>".into()),
                    );
                }
                Err(e) => bail!("{}", e),
            }
        } else {
            match &config.paths {
                Some(paths) => {
                    for path in paths {
                        unsafe {
                            if let Ok((lib, path)) = LibLoader::load_file(path) {
                                self.loaded_backend_from_lib(
                                    &backend_name,
                                    config.clone(),
                                    lib,
                                    path,
                                )?;
                                break;
                            }
                        }
                    }
                    bail!(
                        "Failed to find a suitable library for Backend {} from paths: {:?}",
                        backend_name,
                        paths
                    );
                }
                None => unsafe {
                    if let Ok((lib, path)) = self
                        .lib_loader
                        .search_and_load(&format!("{}{}", BACKEND_LIB_PREFIX, &backend_name))
                    {
                        self.loaded_backend_from_lib(&backend_name, config.clone(), lib, path)?;
                    } else {
                        bail!(
                            "Failed to find a suitable library for Backend {}",
                            backend_name
                        );
                    }
                },
            };
        };
        Ok(())
    }
    unsafe fn loaded_backend_from_lib(
        &mut self,
        backend_name: &str,
        config: BackendConfig,
        lib: Library,
        lib_path: PathBuf,
    ) -> ZResult<()> {
        if let Ok(create_backend) = lib.get::<CreateBackend>(CREATE_BACKEND_FN_NAME) {
            match create_backend(config) {
                Ok(backend) => {
                    self.backends.insert(
                        backend_name.to_string(),
                        BackendHandle::new(
                            backend,
                            Some(lib),
                            lib_path.to_string_lossy().into_owned(),
                        ),
                    );
                    Ok(())
                }
                Err(e) => bail!(
                    "Failed to load Backend {} from {} : {}",
                    backend_name,
                    lib_path.display(),
                    e
                ),
            }
        } else {
            bail!(
                "Failed to load Backend {} from {} : function {}(BackendConfig) not found in lib",
                backend_name,
                lib_path.display(),
                String::from_utf8_lossy(CREATE_BACKEND_FN_NAME)
            );
        }
    }
    fn kill_storage(&mut self, backend: &str, config: StorageConfig) {
        if let Some(storages) = self.storages.get_mut(backend) {
            if let Some(storage) = storages.get_mut(&config.name) {
                debug!("Close storage {} from backend {}", config.name, backend);
                let _ = async_std::task::block_on(storage.send(StorageMessage::Stop));
            }
        }
    }
    fn spawn_storage(&mut self, backend_name: &str, storage: StorageConfig) -> ZResult<()> {
        let admin_key =
            self.status_key() + "/backends/" + backend_name + "/storages/" + &storage.name;
        if let Some(backend) = self.backends.get_mut(backend_name) {
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
                .entry(backend_name.into())
                .or_default()
                .insert(storage_name, stopper);
            Ok(())
        } else {
            bail!("`{}` backend not found", backend_name)
        }
    }
}
struct BackendHandle {
    backend: Box<dyn Backend>,
    _lib: Option<Library>,
    lib_path: String,
    stopper: Arc<AtomicBool>,
}
impl BackendHandle {
    fn new(backend: Box<dyn Backend>, lib: Option<Library>, lib_path: String) -> Self {
        BackendHandle {
            backend,
            _lib: lib,
            lib_path,
            stopper: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }
}
impl Drop for BackendHandle {
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

const GIT_VERSION: &str = git_version::git_version!(prefix = "v", cargo_prefix = "v");
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
        let key_selector = selector.key_selector.as_str();
        with_extended_string(&mut key, &["/version"], |key| {
            if zenoh::utils::key_expr::intersect(key, key_selector) {
                responses.push(zenoh::plugins::Response {
                    key: key.clone(),
                    value: GIT_VERSION.into(),
                })
            }
        });
        let guard = self.0.lock().unwrap();
        with_extended_string(&mut key, &["/backends/"], |key| {
            for (backend_name, backend) in &guard.backends {
                with_extended_string(key, &[backend_name], |key| {
                    with_extended_string(key, &["/__path__"], |key| {
                        if zenoh::utils::key_expr::intersect(key, key_selector) {
                            responses.push(zenoh::plugins::Response {
                                key: key.clone(),
                                value: backend.lib_path.clone().into(),
                            })
                        }
                    });
                    if zenoh::utils::key_expr::intersect(key, key_selector) {
                        responses.push(zenoh::plugins::Response {
                            key: key.clone(),
                            value: backend.backend.get_admin_status(),
                        })
                    }
                    with_extended_string(key, &["/storages/"], |key| {
                        let storages = if let Some(s) = guard.storages.get(backend_name) {
                            s
                        } else {
                            return;
                        };
                        for (storage, handle) in storages {
                            with_extended_string(key, &[storage], |key| {
                                if zenoh::utils::key_expr::intersect(key, key_selector) {
                                    if let Ok(value) = task::block_on(async {
                                        let (tx, rx) = async_std::channel::bounded(1);
                                        let _ = handle.send(StorageMessage::GetStatus(tx)).await;
                                        rx.recv().await
                                    }) {
                                        responses.push(zenoh::plugins::Response {
                                            key: key.clone(),
                                            value,
                                        })
                                    }
                                }
                            })
                        }
                    });
                });
            }
        });
        Ok(responses)
    }
}

const BACKEND_LIB_PREFIX: &str = "zbackend_";
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
