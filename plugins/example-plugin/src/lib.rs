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
use runtime::Runtime;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Arc,
};
use zenoh::net::queryable::STORAGE;
use zenoh::net::utils::resource_name;
use zenoh::net::*;
use zenoh_plugin_trait::prelude::*;
use zenoh_util::zerror2;

pub struct ExamplePlugin {
    selector: ResKey,
}

zenoh_plugin_trait::declare_plugin!(ExamplePlugin);

pub struct ExamplePluginStopper {
    flag: Arc<AtomicBool>,
}

impl PluginStopper for ExamplePluginStopper {
    fn stop(self) {
        self.flag.store(false, Relaxed);
    }
}

impl Plugin for ExamplePlugin {
    fn compatibility() -> zenoh_plugin_trait::Compatibility {
        zenoh_plugin_trait::Compatibility {
            uid: "zenoh-example-plugin",
        }
    }

    fn get_expected_args() -> Vec<Arg<'static, 'static>> {
        vec![
            Arg::from_usage("--storage-selector 'The selection of resources to be stored'")
                .default_value("/demo/example/**"),
        ]
    }

    fn init(args: &ArgMatches) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(selector) = args.value_of("storage-selector") {
            Ok(ExamplePlugin {
                selector: selector.into(),
            })
        } else {
            Err(Box::new(zerror2!(ZErrorKind::Other {
                descr: "storage-selector is a mandatory option for ExamplePlugin".into()
            })))
        }
    }
}

impl PluginLaunch for ExamplePlugin {
    fn start(self, runtime: Runtime) -> Box<dyn PluginStopper> {
        let flag = Arc::new(AtomicBool::new(true));
        let stopper = ExamplePluginStopper { flag: flag.clone() };
        async_std::task::spawn(run(runtime, self.selector, flag));
        Box::new(stopper)
    }
}

async fn run(runtime: Runtime, selector: ResKey, flag: Arc<AtomicBool>) {
    env_logger::init();

    let session = Session::init(runtime, true, vec![], vec![]).await;

    let mut stored: HashMap<String, (ZBuf, Option<DataInfo>)> = HashMap::new();

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };

    debug!("Run example-plugin with storage-selector={}", selector);

    debug!("Declaring Subscriber on {}", selector);
    let mut sub = session
        .declare_subscriber(&selector, &sub_info)
        .await
        .unwrap();

    debug!("Declaring Queryable on {}", selector);
    let mut queryable = session.declare_queryable(&selector, STORAGE).await.unwrap();

    while flag.load(Relaxed) {
        select!(
            sample = sub.receiver().next().fuse() => {
                let sample = sample.unwrap();
                info!("Received data ('{}': '{}')", sample.res_name, sample.payload);
                stored.insert(sample.res_name, (sample.payload, sample.data_info));
            },

            query = queryable.receiver().next().fuse() => {
                let query = query.unwrap();
                info!("Handling query '{}{}'", query.res_name, query.predicate);
                for (rname, (data, data_info)) in stored.iter() {
                    if resource_name::intersect(&query.res_name, rname) {
                        query.reply_async(Sample{
                            res_name: rname.clone(),
                            payload: data.clone(),
                            data_info: data_info.clone(),
                        }).await;
                    }
                }
            }
        );
    }
}
