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

use clap::{Arg, ArgMatches};
use futures::prelude::*;
use futures::select;
use log::{debug, info};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Arc,
};
use zenoh::net::runtime::Runtime;
use zenoh::prelude::*;
use zenoh::queryable::STORAGE;
use zenoh::utils::key_expr;
use zenoh_plugin_trait::prelude::*;
use zenoh_util::zerror2;

pub struct ExamplePlugin {}

zenoh_plugin_trait::declare_plugin!(ExamplePlugin);

pub struct ExamplePluginStopper {
    flag: Arc<AtomicBool>,
}

impl PluginStopper for ExamplePluginStopper {
    fn stop(&self) {
        self.flag.store(false, Relaxed);
    }
}

impl Plugin for ExamplePlugin {
    type Requirements = Vec<Arg<'static, 'static>>;
    type StartArgs = (Runtime, ArgMatches<'static>);
    fn compatibility() -> zenoh_plugin_trait::PluginId {
        zenoh_plugin_trait::PluginId {
            uid: "zenoh-example-plugin",
        }
    }

    fn get_requirements() -> Self::Requirements {
        vec![
            Arg::from_usage("--storage-expr 'The selection of resources to be stored'")
                .default_value("/demo/example/**"),
        ]
    }

    fn start(
        (runtime, args): &Self::StartArgs,
    ) -> Result<Box<dyn std::any::Any + Send + Sync>, Box<dyn std::error::Error>> {
        if let Some(selector) = args.value_of("storage-selector") {
            let flag = Arc::new(AtomicBool::new(true));
            let stopper = ExamplePluginStopper { flag: flag.clone() };
            async_std::task::spawn(run(runtime.clone(), selector.to_string().into(), flag));
            Ok(Box::new(stopper))
        } else {
            Err(Box::new(zerror2!(ZErrorKind::Other {
                descr: "storage-selector is a mandatory option for ExamplePlugin".into()
            })))
        }
    }
}

async fn run(runtime: Runtime, selector: KeyExpr<'_>, flag: Arc<AtomicBool>) {
    env_logger::init();

    let session = zenoh::Session::init(runtime, true, vec![], vec![]).await;

    let mut stored: HashMap<String, Sample> = HashMap::new();

    debug!("Run example-plugin with storage-selector={}", selector);

    debug!("Declare Subscriber on {}", selector);
    let mut sub = session.subscribe(&selector).await.unwrap();

    debug!("Declare Queryable on {}", selector);
    let mut queryable = session
        .queryable(&selector)
        .kind(STORAGE)
        .await
        .unwrap();

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
