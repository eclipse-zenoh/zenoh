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
#![recursion_limit="256"]

use log::{debug, info};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use futures::prelude::*;
use futures::select;
use clap::{Arg, ArgMatches};
use zenoh_router::runtime::Runtime;
use zenoh::net::*;
use zenoh::net::queryable::STORAGE;


#[no_mangle]
pub fn get_expected_args<'a, 'b>() -> Vec<Arg<'a, 'b>>
{
    vec![
        Arg::from_usage("--storage-selector \
            'The selector under which the storage will be declared as subscriber and queryable'")
        .default_value("/demo/example/**")
    ]
}

#[no_mangle]
pub fn start<'a>(runtime: Runtime, args: &'a ArgMatches<'a>) -> Pin<Box<dyn Future<Output=()> + 'a>>
{
    // NOTES: the Future cannot be returned as such to the caller of this plugin.
    // Otherwise Rust complains it cannot move it as its size is not known.
    // We need to wrap it in a pinned Box.
    // See https://stackoverflow.com/questions/61167939/return-an-async-function-from-a-function-in-rust
    Box::pin(run(runtime, args))
}

async fn run(runtime: Runtime, args: &ArgMatches<'_>) {
    env_logger::init();

    let session = Session::init(runtime).await;

    let mut stored: HashMap<String, (RBuf, Option<RBuf>)> = HashMap::new();

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None
    };

    let selector: ResKey = args.value_of("storage-selector").unwrap().into();
    debug!("Run example-plugin with storage-selector={}", selector);

    debug!("Declaring Subscriber on {}", selector);
    let mut sub = session.declare_subscriber(&selector, &sub_info).await.unwrap();

    debug!("Declaring Queryable on {}", selector);
    let mut queryable = session.declare_queryable(&selector, STORAGE).await.unwrap();
    
    loop {
        select!(
            sample = sub.next().fuse() => {
                let (res_name, payload, data_info) = sample.unwrap();
                info!("Received data ('{}': '{}')", res_name, payload);
                stored.insert(res_name.into(), (payload, data_info));
            },

            query = queryable.next().fuse() => {
                let (res_name, predicate, replies_sender) = query.unwrap();
                info!("Handling query '{}?{}'", res_name, predicate);
                for (rname, (data, data_info)) in stored.iter() {
                    if rname_intersect(&res_name, rname) {
                        replies_sender.send((rname.clone(), data.clone(), data_info.clone())).await;
                    }
                }
            }
        );
    }
}

