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

use async_std::sync::Sender;
use clap::{Arg, ArgMatches};
use futures::prelude::*;
use log::{debug, error, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use zenoh::{ChangeKind, Path, Properties, Selector, Value, Workspace, ZResult, Zenoh};
use zenoh_router::runtime::Runtime;

mod backend;
use backend::*;
mod memory_backend;

#[no_mangle]
pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>> {
    vec![
        Arg::from_usage(
            "--no-backend \
            'If true, no backend (and thus no storage) are created at startup. \
             If false (default) the Memory backend it present at startup.'",
        ),
        Arg::from_usage(
            "--mem-storage=[PATH_EXPR]... \
            'A memory storage to be created at start-up. \
            Repeat this option to created several storages'",
        )
        .conflicts_with("no-backend"),
    ]
}

#[no_mangle]
pub fn start(runtime: Runtime, args: &'static ArgMatches<'_>) {
    async_std::task::spawn(run(runtime, args));
}

const MEMORY_BACKEND_NAME: &str = "memory";
const MEMORY_STORAGE_NAME: &str = "mem-storage";

async fn run(runtime: Runtime, args: &'static ArgMatches<'_>) {
    env_logger::init();

    let backends_prefix = format!(
        "/@/router/{}/plugin/storages/backend",
        runtime.get_pid_str().await
    );

    let zenoh = Zenoh::init(runtime).await;
    let workspace = zenoh
        .workspace(Some(Path::try_from(backends_prefix.clone()).unwrap()))
        .await
        .unwrap();

    // Map owning handles on alive backends. Once dropped, a handle will release/stop the backend.
    let mut backend_handles: HashMap<Path, Sender<bool>> = HashMap::new();

    // Start Memory Backend and storages if configured via args
    if !args.is_present("no-backend") {
        let mem_backend = memory_backend::create_backend(Properties::default()).unwrap();
        let mem_backend_path =
            Path::try_from(format!("{}/{}", backends_prefix, MEMORY_BACKEND_NAME)).unwrap();
        let handle = start_backend(mem_backend, mem_backend_path.clone(), workspace.clone())
            .await
            .unwrap();
        backend_handles.insert(mem_backend_path.clone(), handle);

        if let Some(values) = args.values_of("mem-storage") {
            let mut i: u32 = 1;
            for path_expr in values {
                let storage_admin_path = Path::try_from(format!(
                    "{}/storage/{}-{}",
                    mem_backend_path, MEMORY_STORAGE_NAME, i
                ))
                .unwrap();
                let props = Properties::from([(STORAGE_PATH_EXPR_PROPERTY, path_expr)].as_ref());
                workspace
                    .put(&storage_admin_path, Value::Properties(props))
                    .await
                    .unwrap();
                i += 1
            }
        }
    }

    // subscribe to PUT/DELETE on 'backends_prefix'/*
    let backends_admin_selector = Selector::try_from(format!("{}/*", backends_prefix)).unwrap();
    if let Ok(mut backends_admin) = workspace.subscribe(&backends_admin_selector).await {
        while let Some(change) = backends_admin.next().await {
            debug!("Received change: {:?}", change);
            match change.kind {
                ChangeKind::PUT => {
                    #[allow(clippy::map_entry)]
                    // Disable clippy check because no way to log the warn using map.entry().or_insert()
                    if !backend_handles.contains_key(&change.path) {
                        if let Some(value) = change.value {
                            match load_and_start_backend(&change.path, value, &workspace).await {
                                Ok(handle) => {
                                    let _ = backend_handles.insert(change.path, handle);
                                }
                                Err(e) => warn!("{}", e),
                            }
                        } else {
                            warn!("Received a PUT on {} without value", change.path);
                        }
                    } else {
                        warn!("Backend {} already exists", change.path);
                    }
                }
                ChangeKind::DELETE => {
                    debug!("Delete backend {}", change.path);
                    let _ = backend_handles.remove(&change.path);
                }
                ChangeKind::PATCH => warn!("PATCH not supported on {}", change.path),
            }
        }
    } else {
        error!("Failed to subscribe on {}", backends_admin_selector);
    };
}

async fn load_and_start_backend(
    path: &Path,
    _value: Value,
    workspace: &Workspace,
) -> ZResult<Sender<bool>> {
    // TODO: find and load appropriate BACKEND depending to properties in "value"
    let mem_backend = memory_backend::create_backend(Properties::default()).unwrap();
    start_backend(mem_backend, path.clone(), (*workspace).clone()).await
}
