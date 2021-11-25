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
#![recursion_limit = "256"]

use anyhow::anyhow;
use futures::prelude::*;
use futures::select;
use log::{debug, info};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Arc, Mutex,
};
use zenoh::net::runtime::Runtime;
use zenoh::prelude::*;
use zenoh::queryable::STORAGE;
use zenoh::utils::key_expr;
use zenoh_plugin_trait::prelude::*;
use zenoh_plugin_trait::RunningPluginTrait;
use zenoh_plugin_trait::ValidationFunction;
use zenoh_util::zerror2;
use zenoh_util::zlock;

pub struct ExamplePlugin {}

zenoh_plugin_trait::declare_plugin!(ExamplePlugin);

const DEFAULT_SELECTOR: &str = "/demo/example/**";
impl Plugin for ExamplePlugin {
    type StartArgs = Runtime;
    fn compatibility() -> zenoh_plugin_trait::PluginId {
        zenoh_plugin_trait::PluginId {
            uid: "zenoh-example-plugin",
        }
    }
    const STATIC_NAME: &'static str = "example";

    fn start(
        name: &str,
        runtime: &Self::StartArgs,
    ) -> Result<Box<dyn RunningPluginTrait>, Box<dyn std::error::Error>> {
        let config = runtime.config.lock();
        let self_cfg = config.plugin(name).unwrap().as_object().unwrap();
        let selector;
        match self_cfg.get("storage-selector") {
            Some(serde_json::Value::String(s)) => {
                selector = s.clone();
            }
            None => selector = DEFAULT_SELECTOR.into(),
            _ => {
                return Err(Box::new(zerror2!(ZErrorKind::Other {
                    descr: format!("storage-selector is a mandatory option for {}", name)
                })))
            }
        }
        std::mem::drop(config);
        let flag = Arc::new(AtomicBool::new(true));
        async_std::task::spawn(run(runtime.clone(), selector.into(), flag.clone()));
        Ok(Box::new(RunningPlugin(Arc::new(Mutex::new(
            RunningPluginInner {
                flag,
                name: name.into(),
                runtime: runtime.clone(),
            },
        )))))
    }
}
struct RunningPluginInner {
    flag: Arc<AtomicBool>,
    name: String,
    runtime: Runtime,
}
#[derive(Clone)]
struct RunningPlugin(Arc<Mutex<RunningPluginInner>>);
impl RunningPluginTrait for RunningPlugin {
    fn config_checker(&self) -> ValidationFunction {
        let guard = zlock!(&self.0);
        let name = guard.name.clone();
        std::mem::drop(guard);
        let plugin = self.clone();
        Arc::new(move |path, old, new| {
            const STORAGE_SELECTOR: &str = "storage-selector";
            if path == STORAGE_SELECTOR || path.is_empty() {
                match (old.get(STORAGE_SELECTOR), new.get(STORAGE_SELECTOR)) {
                    (Some(serde_json::Value::String(os)), Some(serde_json::Value::String(ns)))
                        if os == ns => {}
                    (_, Some(serde_json::Value::String(selector))) => {
                        let mut guard = zlock!(&plugin.0);
                        guard.flag.store(false, Relaxed);
                        guard.flag = Arc::new(AtomicBool::new(true));
                        async_std::task::spawn(run(
                            guard.runtime.clone(),
                            selector.clone().into(),
                            guard.flag.clone(),
                        ));
                        return Ok(None);
                    }
                    (_, None) => {
                        let guard = zlock!(&plugin.0);
                        guard.flag.store(false, Relaxed);
                    }
                    _ => {
                        return Err(
                            anyhow!("storage-selector for {} must be a string", &name).into()
                        )
                    }
                }
            }
            Err(anyhow!("unknown option {} for {}", path, &name).into())
        })
    }
}
impl Drop for RunningPlugin {
    fn drop(&mut self) {
        zlock!(self.0).flag.store(false, Relaxed);
    }
}

async fn run(runtime: Runtime, selector: KeyExpr<'_>, flag: Arc<AtomicBool>) {
    env_logger::init();

    let session = zenoh::Session::init(runtime, true, vec![], vec![]).await;

    let mut stored: HashMap<String, Sample> = HashMap::new();

    debug!("Run example-plugin with storage-selector={}", selector);

    debug!("Create Subscriber on {}", selector);
    let mut sub = session.subscribe(&selector).await.unwrap();

    debug!("Create Queryable on {}", selector);
    let mut queryable = session.queryable(&selector).kind(STORAGE).await.unwrap();

    while flag.load(Relaxed) {
        select!(
            sample = sub.receiver().next() => {
                let sample = sample.unwrap();
                info!("Received data ('{}': '{}')", sample.key_expr, sample.value);
                stored.insert(sample.key_expr.to_string(), sample);
            },

            query = queryable.receiver().next() => {
                let query = query.unwrap();
                info!("Handling query '{}'", query.selector());
                for (key_expr, sample) in stored.iter() {
                    if key_expr::intersect(query.selector().key_selector.as_str(), key_expr) {
                        query.reply_async(sample.clone()).await;
                    }
                }
            }
        );
    }
}
