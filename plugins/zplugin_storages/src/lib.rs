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

use clap::{Arg, ArgMatches};
use futures::prelude::*;
use futures::select;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use zenoh::net::queryable::STORAGE;
use zenoh::net::utils::resource_name;
use zenoh::net::{DataInfo, RBuf, Sample, WBuf};
use zenoh::{Path, Properties, Selector, Value, ZError, ZErrorKind, ZResult, Zenoh};
use zenoh_backend_core::STORAGE_PATH_EXPR_PROPERTY;
use zenoh_router::runtime::Runtime;
use zenoh_util::{zerror, zerror2};

mod backend;
use backend::Backend;
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

struct State {
    admin_prefix: Path,
    backends: HashMap<String, Backend>,
    pub(crate) admin_space: HashMap<String, Value>,
}

impl State {
    fn new(admin_prefix: Path) -> Self {
        State {
            admin_prefix,
            backends: HashMap::new(),
            admin_space: HashMap::new(),
        }
    }

    fn properties_to_json_value(props: &Properties) -> Value {
        let json_map = props
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect::<serde_json::map::Map<String, serde_json::Value>>();
        let json_val = serde_json::Value::Object(json_map);
        Value::Json(json_val.to_string())
    }

    fn add_backend(&mut self, beid: String, backend: Backend) -> ZResult<()> {
        let path = format!("{}/backend/{}", self.admin_prefix, beid);
        debug!("Add backend: {}", path);
        if !self.backends.contains_key(&beid) {
            self.admin_space
                .insert(path, Self::properties_to_json_value(&backend.properties()));
            self.backends.insert(beid, backend);
            Ok(())
        } else {
            zerror!(ZErrorKind::Other {
                descr: format!("Backend '{}' already exists", beid)
            })
        }
    }

    async fn create_storage(&mut self, stid: String, props: Properties, beid: &str) -> ZResult<()> {
        let path = format!("{}/backend/{}/storage/{}", self.admin_prefix, beid, stid);
        debug!("Create storage {} with props: {}", path, props);
        let backend = self.backends.get_mut(beid).ok_or_else(|| {
            zerror2!(ZErrorKind::Other {
                descr: format!("Backend '{}' doesn't exist", beid)
            })
        })?;

        backend.create_storage(stid.clone(), props).await?;
        self.admin_space.insert(
            path,
            Self::properties_to_json_value(&backend.get_storage_properties(&stid).await.unwrap()),
        );
        Ok(())
    }
}

async fn run(runtime: Runtime, args: &'static ArgMatches<'_>) {
    env_logger::init();

    let admin_prefix_str = format!(
        "/@/router/{}/plugin/storages",
        runtime.get_pid_str().await.to_string()
    );
    let admin_path_sel = Selector::try_from(format!("{}/**", admin_prefix_str)).unwrap();
    let admin_prefix = Path::new(admin_prefix_str).unwrap();

    let zenoh = Zenoh::init(runtime).await;
    let workspace = zenoh.workspace(Some(admin_prefix.clone())).await.unwrap();

    let mut sub = workspace.subscribe(&admin_path_sel).await.unwrap();
    let mut eval = workspace
        .register_eval(&admin_path_sel.path_expr)
        .await
        .unwrap();

    let mut state = State::new(admin_prefix.clone());

    // Address argguments
    if !args.is_present("no-backend") {
        let mem_backend = memory_backend::create_backend(Properties::default()).unwrap();
        state
            .add_backend(
                MEMORY_BACKEND_NAME.to_string(),
                Backend::new(mem_backend, zenoh.session().clone()),
            )
            .unwrap();

        if let Some(values) = args.values_of("mem-storage") {
            let mut i: u32 = 1;
            for path_expr in values {
                let props = Properties::from([(STORAGE_PATH_EXPR_PROPERTY, path_expr)].as_ref());
                state
                    .create_storage(
                        format!("{}-{}", MEMORY_STORAGE_NAME, i),
                        props,
                        MEMORY_BACKEND_NAME,
                    )
                    .await
                    .unwrap();
                i += 1
            }
        }
    }

    loop {
        select!(
            change = sub.next().fuse() => {
                let change = change.unwrap();
                debug!("Received change for {}", change.path);
                if let Some(sub_path) = change.path.strip_prefix(&admin_prefix) {
                    match sub_path.as_str().split('/').collect::<Vec<&str>>()[..] {
                        [ "backend", "auto"] => info!("ADD BACKEND AUTO"),
                        [ "backend", beid] => info!("ADD BACKEND {}", beid),
                        [ "backend", "auto", "storage", stid] => info!("ADD STORAGE on AUTO: {}", stid),
                        [ "backend", beid, "storage", stid] => info!("ADD STORAGE on {} : {}", beid, stid),
                        _ => warn!("Invalid path on admin space: {}", change.path)
                    }
                } else {
                    warn!("Received publication for a non-subscribed path: {}", change.path);
                }
            },

            get = eval.next().fuse() => {
                let get = get.unwrap();
                debug!("Received get for {}", get.selector);
                if get.selector.path_expr.is_a_path() {
                    if let Some(value) = state.admin_space.get(get.selector.path_expr.as_str()) {
                        get.reply(Path::try_from(get.selector.path_expr.to_string()).unwrap(), value.clone()).await;
                    }
                } else {
                    for (path, value) in state.admin_space.iter() {
                        if resource_name::intersect(&get.selector.path_expr.as_str(), path) {
                            get.reply(Path::try_from(path.as_ref()).unwrap(), value.clone()).await;

                        }
                    }
                }
            }
        );
    }
}
