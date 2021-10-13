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
use clap::{Arg, ArgMatches};
use futures::prelude::*;
use libloading::Symbol;
use log::{debug, error, warn};
use std::collections::HashMap;
use zenoh::net::runtime::Runtime;
use zenoh::prelude::*;
use zenoh::Session;
use zenoh_backend_traits::{Backend, PROP_STORAGE_KEY_EXPR};
use zenoh_plugin_trait::prelude::*;
use zenoh_util::{zerror, LibLoader};

mod backends_mgt;
use backends_mgt::*;
mod memory_backend;
mod storages_mgt;

pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>> {
    lazy_static::lazy_static! {
        static ref BACKEND_SEARCH_DIR_USAGE: String = format!(
            "--backend-search-dir=[DIRECTORY]... \
        'A directory where to search for backends libraries to load. \
        Repeat this option to specify several search directories'. \
        By default, the backends libraries will be searched in: '{}' .",
            LibLoader::default_search_paths()
        );
    }

    vec![
        Arg::from_usage(
            "--no-backend \
            'If true, no backend (and thus no storage) are created at startup. \
             If false (default) the Memory backend it present at startup.'",
        ),
        Arg::from_usage(
            "--mem-storage=[KEY_EXPR]... \
            'A memory storage to be created at start-up. \
            Repeat this option to created several storages'",
        )
        .conflicts_with("no-backend"),
        Arg::from_usage(&BACKEND_SEARCH_DIR_USAGE),
    ]
}

zenoh_plugin_trait::declare_plugin!(StoragesPlugin);
pub struct StoragesPlugin {}
impl Plugin for StoragesPlugin {
    fn compatibility() -> zenoh_plugin_trait::PluginId {
        zenoh_plugin_trait::PluginId {
            uid: "zenoh-plugin-storages",
        }
    }

    type Requirements = Vec<Arg<'static, 'static>>;

    type StartArgs = (Runtime, ArgMatches<'static>);

    fn get_requirements() -> Self::Requirements {
        get_expected_args()
    }

    fn start(
        (runtime, args): &Self::StartArgs,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, Box<dyn std::error::Error>> {
        async_std::task::spawn(run(runtime.clone(), args.to_owned()));
        Ok(Box::new(()))
    }
}

const BACKEND_LIB_PREFIX: &str = "zbackend_";
const MEMORY_BACKEND_NAME: &str = "memory";
const MEMORY_STORAGE_NAME: &str = "mem-storage";

async fn run(runtime: Runtime, args: ArgMatches<'_>) {
    // Try to initiate login.
    // Required in case of dynamic lib, otherwise no logs.
    // But cannot be done twice in case of static link.
    let _ = env_logger::try_init();

    let lib_loader = if let Some(values) = args.values_of("backend-search-dir") {
        LibLoader::new(&values.collect::<Vec<&str>>(), false)
    } else {
        LibLoader::default()
    };

    let backends_prefix = format!(
        "/@/router/{}/plugin/storages/backend",
        runtime.get_pid_str()
    );

    let zenoh = Arc::new(zenoh::init(runtime).await.unwrap());

    // Map owning handles on alive backends. Once dropped, a handle will release/stop the backend.
    let mut backend_handles: HashMap<String, Sender<bool>> = HashMap::new();

    // Start Memory Backend and storages if configured via args
    if !args.is_present("no-backend") {
        debug!("Memory backend enabled");
        let mem_backend = memory_backend::create_backend(Properties::default()).unwrap();
        let mem_backend_key = format!("{}/{}", backends_prefix, MEMORY_BACKEND_NAME);
        let handle = start_backend(mem_backend, mem_backend_key.clone(), zenoh.clone())
            .await
            .unwrap();
        backend_handles.insert(mem_backend_key.clone(), handle);

        if let Some(values) = args.values_of("mem-storage") {
            let mut i: u32 = 1;
            for key_expr in values {
                debug!(
                    "Add memory storage {}-{} on {}",
                    MEMORY_STORAGE_NAME, i, key_expr
                );
                let storage_admin_key =
                    format!("{}/storage/{}-{}", mem_backend_key, MEMORY_STORAGE_NAME, i)
                        .to_string();
                let props = Properties::from([(PROP_STORAGE_KEY_EXPR, key_expr)].as_ref());
                zenoh
                    .put(&storage_admin_key, props)
                    .encoding(Encoding::APP_PROPERTIES)
                    .await
                    .unwrap();
                i += 1
            }
        }
    }

    // subscribe to PUT/DELETE on 'backends_prefix'/*
    let backends_admin_selector = format!("{}/*", backends_prefix).to_string();
    if let Ok(mut backends_admin) = zenoh.subscribe(&backends_admin_selector).await {
        while let Some(sample) = backends_admin.receiver().next().await {
            debug!("Received sample: {:?}", sample);
            let key = sample.res_key.as_str();
            match sample.kind {
                SampleKind::Put => {
                    #[allow(clippy::map_entry)]
                    // Disable clippy check because no way to log the warn using map.entry().or_insert()
                    if !backend_handles.contains_key(key) {
                        match load_and_start_backend(key, sample.value, zenoh.clone(), &lib_loader)
                            .await
                        {
                            Ok(handle) => {
                                let _ = backend_handles.insert(key.to_string(), handle);
                            }
                            Err(e) => warn!("{}", e),
                        }
                    } else {
                        warn!("Backend {} already exists", key);
                    }
                }
                SampleKind::Delete => {
                    debug!("Delete backend {}", key);
                    let _ = backend_handles.remove(key);
                }
                SampleKind::Patch => warn!("PATCH not supported on {}", key),
            }
        }
    } else {
        error!("Failed to subscribe on {}", backends_admin_selector);
    };
}

/// Signature of the `create_backend` operation to be implemented in the library as an entrypoint.
const CREATE_BACKEND_FN_NAME: &[u8; 15] = b"create_backend\0";
type CreateBackend<'lib> =
    Symbol<'lib, unsafe extern "C" fn(&Properties) -> ZResult<Box<dyn Backend>>>;

async fn load_and_start_backend(
    key: &str,
    mut value: Value,
    zenoh: Arc<Session>,
    lib_loader: &LibLoader,
) -> ZResult<Sender<bool>> {
    if value.encoding == Encoding::APP_PROPERTIES {
        if let Ok(props) = String::from_utf8(value.payload.read_vec()).map(Properties::from) {
            let name = match key.rfind('/') {
                Some(i) => &key[i + 1..],
                None => key,
            };
            let (lib, lib_path) = unsafe {
                if let Some(filename) = props.get("lib") {
                    LibLoader::load_file(filename)?
                } else {
                    lib_loader.search_and_load(&format!("{}{}", BACKEND_LIB_PREFIX, name))?
                }
            };

            debug!("Create backend {} using {}", name, lib_path.display());
            unsafe {
                match lib.get::<CreateBackend>(CREATE_BACKEND_FN_NAME) {
                    Ok(create_backend) => match create_backend(&props) {
                        Ok(backend) => start_backend(backend, key.to_string(), zenoh).await,
                        Err(err) => zerror!(
                            ZErrorKind::Other {
                                descr: format!(
                                    "Failed to create Backend {} from {}: {}",
                                    name,
                                    lib_path.display(),
                                    err
                                ),
                            },
                            err
                        ),
                    },
                    Err(err) => zerror!(ZErrorKind::Other {
                        descr: format!(
                            "Failed to create Backend {} from {}: {}",
                            name,
                            lib_path.display(),
                            err
                        )
                    }),
                }
            }
        } else {
            zerror!(ZErrorKind::Other {
                descr: format!(
                    "Received a PUT on {}, unable to decode properties from value: {:?}",
                    key, value
                )
            })
        }
    } else {
        zerror!(ZErrorKind::Other {
            descr: format!("Received a PUT on {} with invalid value: {:?}", key, value)
        })
    }
}
