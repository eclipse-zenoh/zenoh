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

use anyhow::{anyhow, bail};
use async_std::channel::Sender;
use async_std::sync::Arc;
use futures::StreamExt;
use memory_backend::create_memory_backend;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem::MaybeUninit;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use zenoh::net::runtime::Runtime;
use zenoh::prelude::*;
use zenoh::Session;
// use zenoh_backend_traits::Storage;
use zenoh_backend_traits::{config::*, Backend};
use zenoh_plugin_trait::prelude::*;
use zenoh_plugin_trait::RunningPluginTrait;
use zenoh_plugin_trait::ValidationFunction;
use zenoh_util::zlock;
use zenoh_util::LibLoader;
use zenoh_util::LIB_SUFFIX;

mod backends_mgt;
use backends_mgt::*;
mod memory_backend;
mod storages_mgt;

zenoh_plugin_trait::declare_plugin!(StoragesPlugin);
pub struct StoragesPlugin {}
impl Plugin for StoragesPlugin {
    fn compatibility() -> zenoh_plugin_trait::PluginId {
        zenoh_plugin_trait::PluginId {
            uid: "zenoh-plugin-storages",
        }
    }
    const STATIC_NAME: &'static str = "storages";

    type StartArgs = Runtime;

    fn start(
        name: &str,
        runtime: &Self::StartArgs,
    ) -> Result<Box<dyn RunningPluginTrait>, Box<dyn std::error::Error>> {
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
    storages: HashMap<String, HashMap<String, Sender<()>>>,
}
impl StorageRuntimeInner {
    fn status_key(&self) -> String {
        format!(
            "/@/router/{}/status/plugins/{}",
            &self.runtime.pid, &self.name
        )
    }
    fn new(runtime: Runtime, config: PluginConfig) -> Result<Self, Box<dyn std::error::Error>> {
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
    fn update<I: IntoIterator<Item = ConfigDiff>>(
        &mut self,
        diffs: I,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        if let Some(storages) = self.storages.remove(&backend.name) {
            async_std::task::block_on(futures::future::join_all(
                storages
                    .into_iter()
                    .map(|(_, s)| async move { s.send(()).await }),
            ));
        }
        std::mem::drop(self.backends.remove(&backend.name));
    }
    fn spawn_backend(&mut self, backend: BackendConfig) -> anyhow::Result<()> {
        let backend_name = backend.name.clone();
        let (backend, lib_path) = if backend_name == MEMORY_BACKEND_NAME {
            match create_memory_backend(backend) {
                Ok(backend) => (backend, "<static-memory>".into()),
                Err(e) => bail!("{}", e),
            }
        } else {
            let mut loaded_backend = Err(anyhow!("file not found"));
            let mut lib_path = MaybeUninit::uninit();
            match &backend.paths {
                Some(paths) => {
                    for path in paths {
                        unsafe {
                            if let Ok((lib, path)) = LibLoader::load_file(path) {
                                if let Ok(create_backend) =
                                    lib.get::<CreateBackend>(CREATE_BACKEND_FN_NAME)
                                {
                                    loaded_backend = create_backend(backend.clone())
                                        .map_err(|e| anyhow!("{}", e));
                                    if loaded_backend.is_ok() {
                                        lib_path = MaybeUninit::new(path);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                None => unsafe {
                    if let Ok((lib, path)) = self.lib_loader.search_and_load(&format!(
                        "{}{}{}",
                        BACKEND_LIB_PREFIX,
                        &backend_name,
                        LIB_SUFFIX.as_str()
                    )) {
                        if let Ok(create_backend) = lib.get::<CreateBackend>(CREATE_BACKEND_FN_NAME)
                        {
                            loaded_backend =
                                create_backend(backend.clone()).map_err(|e| anyhow!("{}", e));
                            lib_path = MaybeUninit::new(path);
                        }
                    }
                },
            };
            match loaded_backend {
                Ok(loaded_backend) => (loaded_backend, unsafe {
                    lib_path.assume_init().to_string_lossy().into_owned()
                }),
                Err(e) => bail!("Couldn't load `{}` backend: {}", &backend_name, e),
            }
        };
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
        async_std::task::spawn({
            let session = self.session.clone();
            let flag = flag.clone();
            let keyexpr = self.status_key() + "/" + &backend_name + "/path";
            async move {
                let mut queryable = match session.register_queryable(&keyexpr).await {
                    Ok(q) => q,
                    Err(e) => {
                        log::error!("Couldn't spawn {}", e);
                        return;
                    }
                };
                let receiver = queryable.receiver();
                while flag.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Some(query) = receiver.next().await {
                        query
                            .reply_async(Sample::new(keyexpr.clone(), lib_path.clone()))
                            .await;
                    } else {
                        break;
                    }
                }
            }
        });
        self.backends
            .insert(backend_name, BackendHandle::new(backend, flag));
        Ok(())
    }
    fn kill_storage(&mut self, backend: &str, storage: StorageConfig) {
        if let Some(storages) = self.storages.get_mut(backend) {
            if let Some(storage) = storages.get_mut(&storage.key_expr) {
                let _ = async_std::task::block_on(storage.send(()));
            }
        }
    }
    fn spawn_storage(&mut self, backend_name: &str, storage: StorageConfig) -> anyhow::Result<()> {
        let admin_key = self.status_key() + backend_name + "/storages/" + &storage.name;
        if let Some(backend) = self.backends.get_mut(backend_name) {
            let storage_name = storage.key_expr.clone();
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
    stopper: Arc<AtomicBool>,
}
impl BackendHandle {
    fn new(backend: Box<dyn Backend>, stopper: Arc<AtomicBool>) -> Self {
        BackendHandle { backend, stopper }
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
impl RunningPluginTrait for StorageRuntime {
    fn config_checker(&self) -> ValidationFunction {
        let name = { zlock!(self.0).name.clone() };
        let runtime = self.0.clone();
        Arc::new(move |_path, old, new| {
            let old = match PluginConfig::try_from((&name, old)) {
                Ok(v) => v,
                Err(e) => return Err(e.into()),
            };
            let new = match PluginConfig::try_from((&name, new)) {
                Ok(v) => v,
                Err(e) => return Err(e.into()),
            };
            let diffs = ConfigDiff::diffs(old, new);
            zlock!(runtime).update(diffs)
        })
    }
}

const BACKEND_LIB_PREFIX: &str = "zbackend_";
const MEMORY_BACKEND_NAME: &str = "memory";

/// Signature of the `create_backend` operation to be implemented in the library as an entrypoint.
const CREATE_BACKEND_FN_NAME: &[u8] = b"create_backend";
type CreateBackend = unsafe extern "C" fn(BackendConfig) -> ZResult<Box<dyn Backend>>;
