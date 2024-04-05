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
#![recursion_limit = "256"]

use futures::select;
use log::{debug, info};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Arc, Mutex,
};
use zenoh::key_expr::{keyexpr, KeyExpr};
use zenoh::plugins::{RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::runtime::Runtime;
use zenoh_core::zlock;
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};
use zenoh_result::{bail, ZResult};

// The struct implementing the ZenohPlugin and ZenohPlugin traits
pub struct ExamplePlugin {}

// declaration of the plugin's VTable for zenohd to find the plugin's functions to be called
#[cfg(feature = "no_mangle")]
zenoh_plugin_trait::declare_plugin!(ExamplePlugin);

// A default selector for this example of storage plugin (in case the config doesn't set it)
// This plugin will subscribe to this selector and declare a queryable with this selector
const DEFAULT_SELECTOR: &str = "demo/example/**";

impl ZenohPlugin for ExamplePlugin {}
impl Plugin for ExamplePlugin {
    type StartArgs = Runtime;
    type Instance = zenoh::plugins::RunningPlugin;

    // A mandatory const to define, in case of the plugin is built as a standalone executable
    const DEFAULT_NAME: &'static str = "example";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    // The first operation called by zenohd on the plugin
    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::Instance> {
        let config = runtime.config().lock();
        let self_cfg = config.plugin(name).unwrap().as_object().unwrap();
        // get the plugin's config details from self_cfg Map (here the "storage-selector" property)
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

        // a flag to end the plugin's loop when the plugin is removed from the config
        let flag = Arc::new(AtomicBool::new(true));
        // spawn the task running the plugin's loop
        async_std::task::spawn(run(runtime.clone(), selector, flag.clone()));
        // return a RunningPlugin to zenohd
        Ok(Box::new(RunningPlugin(Arc::new(Mutex::new(
            RunningPluginInner {
                flag,
                name: name.into(),
                runtime: runtime.clone(),
            },
        )))))
    }
}

// An inner-state for the RunningPlugin
struct RunningPluginInner {
    flag: Arc<AtomicBool>,
    name: String,
    runtime: Runtime,
}
// The RunningPlugin struct implementing the RunningPluginTrait trait
#[derive(Clone)]
struct RunningPlugin(Arc<Mutex<RunningPluginInner>>);

impl PluginControl for RunningPlugin {}

impl RunningPluginTrait for RunningPlugin {
    fn config_checker(
        &self,
        path: &str,
        old: &serde_json::Map<String, serde_json::Value>,
        new: &serde_json::Map<String, serde_json::Value>,
    ) -> ZResult<Option<serde_json::Map<String, serde_json::Value>>> {
        let mut guard = zlock!(&self.0);
        const STORAGE_SELECTOR: &str = "storage-selector";
        if path == STORAGE_SELECTOR || path.is_empty() {
            match (old.get(STORAGE_SELECTOR), new.get(STORAGE_SELECTOR)) {
                (Some(serde_json::Value::String(os)), Some(serde_json::Value::String(ns)))
                    if os == ns => {}
                (_, Some(serde_json::Value::String(selector))) => {
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
                    guard.flag.store(false, Relaxed);
                }
                _ => {
                    bail!("storage-selector for {} must be a string", &guard.name)
                }
            }
        }
        bail!("unknown option {} for {}", path, guard.name)
    }
}

// If the plugin is dropped, set the flag to false to end the loop
impl Drop for RunningPlugin {
    fn drop(&mut self) {
        zlock!(self.0).flag.store(false, Relaxed);
    }
}

async fn run(runtime: Runtime, selector: KeyExpr<'_>, flag: Arc<AtomicBool>) {
    env_logger::init();

    // create a zenoh Session that shares the same Runtime than zenohd
    let session = zenoh::session::init(runtime).res().await.unwrap();

    // the HasMap used as a storage by this example of storage plugin
    let mut stored: HashMap<String, Sample> = HashMap::new();

    debug!("Run example-plugin with storage-selector={}", selector);

    // This storage plugin subscribes to the selector and will store in HashMap the received samples
    debug!("Create Subscriber on {}", selector);
    let sub = session.declare_subscriber(&selector).res().await.unwrap();

    // This storage plugin declares a Queryable that will reply to queries with the samples stored in the HashMap
    debug!("Create Queryable on {}", selector);
    let queryable = session.declare_queryable(&selector).res().await.unwrap();

    // Plugin's event loop, while the flag is true
    while flag.load(Relaxed) {
        select!(
            // on sample received by the Subscriber
            sample = sub.recv_async() => {
                let sample = sample.unwrap();
                let payload = sample.payload().deserialize::<Cow<str>>().unwrap_or_else(|e| Cow::from(e.to_string()));
                info!("Received data ('{}': '{}')", sample.key_expr(), payload);
                stored.insert(sample.key_expr().to_string(), sample);
            },
            // on query received by the Queryable
            query = queryable.recv_async() => {
                let query = query.unwrap();
                info!("Handling query '{}'", query.selector());
                for (key_expr, sample) in stored.iter() {
                    if query.selector().key_expr.intersects(unsafe{keyexpr::from_str_unchecked(key_expr)}) {
                        query.reply_sample(sample.clone()).res().await.unwrap();
                    }
                }
            }
        );
    }
}
