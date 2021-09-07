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
use zenoh::net::runtime::Runtime;
use zenoh::queryable::STORAGE;
use zenoh::utils::resource_name;
use zenoh::*;

#[no_mangle]
pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>> {
    vec![
        Arg::from_usage("--storage-selector 'The selection of resources to be stored'")
            .default_value("/demo/example/**"),
    ]
}

#[no_mangle]
pub fn start(runtime: Runtime, args: &'static ArgMatches<'_>) {
    async_std::task::spawn(run(runtime, args));
}

async fn run(runtime: Runtime, args: &'static ArgMatches<'_>) {
    env_logger::init();

    let session = Session::init(runtime, true, vec![], vec![]).await;

    let mut stored: HashMap<String, Sample> = HashMap::new();

    let selector: ResKey = args.value_of("storage-selector").unwrap().into();
    debug!("Run example-plugin with storage-selector={}", selector);

    debug!("Declaring Subscriber on {}", selector);
    let mut sub = session.subscribe(&selector).await.unwrap();

    debug!("Declaring Queryable on {}", selector);
    let mut queryable = session
        .register_queryable(&selector)
        .kind(STORAGE)
        .await
        .unwrap();

    loop {
        select!(
            sample = sub.receiver().next().fuse() => {
                let sample = sample.unwrap();
                info!("Received data ('{}': '{}')", sample.res_name, sample.value);
                stored.insert(sample.res_name.clone(), sample);
            },

            query = queryable.receiver().next().fuse() => {
                let query = query.unwrap();
                info!("Handling query '{}'", query.selector());
                for (rname, sample) in stored.iter() {
                    if resource_name::intersect(&query.selector().key_selector, rname) {
                        query.reply_async(sample.clone()).await;
                    }
                }
            }
        );
    }
}
