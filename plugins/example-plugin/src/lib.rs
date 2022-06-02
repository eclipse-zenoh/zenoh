//
// Copyright (c) 2022 ZettaScale Technology
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
#![recursion_limit = "256"]

use futures::select;
use log::{debug, info};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Arc, Mutex,
};
use zenoh::net::runtime::Runtime;
use zenoh::plugins::{Plugin, RunningPluginTrait, ValidationFunction, ZenohPlugin};
use zenoh::prelude::*;
use zenoh_core::AsyncResolve;
use zenoh_core::SyncResolve;
use zenoh_core::{bail, zlock, Result as ZResult};

pub struct ExamplePlugin {}

zenoh_plugin_trait::declare_plugin!(ExamplePlugin);

const DEFAULT_SELECTOR: &str = "demo/example/**";
impl ZenohPlugin for ExamplePlugin {}
impl Plugin for ExamplePlugin {
    type StartArgs = Runtime;
    type RunningPlugin = zenoh::plugins::RunningPlugin;
    const STATIC_NAME: &'static str = "example";

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::RunningPlugin> {
        let config = runtime.config.lock();
        let self_cfg = config.plugin(name).unwrap().as_object().unwrap();
        let selector: KeyExpr = match self_cfg.get("storage-selector") {
            Some(serde_json::Value::String(s)) => KeyExpr::try_from(s)?,
            None => KeyExpr::try_from(DEFAULT_SELECTOR).unwrap(),
            _ => {
                bail!("storage-selector is a mandatory option for {}", name)
            }
        }
        .clone()
        .into_owned();
        std::mem::drop(config);
        let flag = Arc::new(AtomicBool::new(true));
        async_std::task::spawn(run(runtime.clone(), selector, flag.clone()));
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
                        match KeyExpr::try_from(selector.clone()) {
                            Err(e) => log::error!("{}", e),
                            Ok(selector) => {
                                async_std::task::spawn(run(
                                    guard.runtime.clone(),
                                    selector,
                                    guard.flag.clone(),
                                ));
                            }
                        }
                        return Ok(None);
                    }
                    (_, None) => {
                        let guard = zlock!(&plugin.0);
                        guard.flag.store(false, Relaxed);
                    }
                    _ => {
                        bail!("storage-selector for {} must be a string", &name)
                    }
                }
            }
            bail!("unknown option {} for {}", path, &name)
        })
    }
    fn adminspace_getter<'a>(
        &'a self,
        _selector: &'a Selector<'a>,
        _plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::plugins::Response>> {
        Ok(Vec::new())
    }
}
impl Drop for RunningPlugin {
    fn drop(&mut self) {
        zlock!(self.0).flag.store(false, Relaxed);
    }
}

async fn run(runtime: Runtime, selector: KeyExpr<'_>, flag: Arc<AtomicBool>) {
    env_logger::init();

    let session = zenoh::Session::init(runtime, true, vec![], vec![])
        .res_async()
        .await;

    let mut stored: HashMap<String, Sample> = HashMap::new();

    debug!("Run example-plugin with storage-selector={}", selector);

    debug!("Create Subscriber on {}", selector);
    let sub = session
        .declare_subscriber(&selector)
        .res_async()
        .await
        .unwrap();

    debug!("Create Queryable on {}", selector);
    let queryable = session.declare_queryable(&selector).res_sync().unwrap();

    while flag.load(Relaxed) {
        select!(
            sample = sub.recv_async() => {
                let sample = sample.unwrap();
                info!("Received data ('{}': '{}')", sample.key_expr, sample.value);
                stored.insert(sample.key_expr.to_string(), sample);
            },

            query = queryable.recv_async() => {
                let query = query.unwrap();
                info!("Handling query '{}'", query.selector());
                for (key_expr, sample) in stored.iter() {
                    if query.selector().key_expr.intersects(unsafe{keyexpr::from_str_unchecked(key_expr)}) {
                        query.reply(Ok(sample.clone())).res_async().await.unwrap();
                    }
                }
            }
        );
    }
}
