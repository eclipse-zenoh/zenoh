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
use std::env;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use futures::prelude::*;
use futures::select;
use zenoh::net::*;
use zenoh::net::queryable::STORAGE;

#[no_mangle]
pub fn start<'a>() -> Pin<Box<dyn Future<Output=()> + 'a>>
{
    // NOTES: the Future cannot be returned as such to the caller of this plugin.
    // Otherwise Rust complains it cannot move it as its size is not known.
    // We need to wrap it in a pinned Box.
    // See https://stackoverflow.com/questions/61167939/return-an-async-function-from-a-function-in-rust
    Box::pin(run())
}

async fn run() {
    env_logger::init();
    debug!("Start_async zenoh-plugin-http");

    let mut args: Vec<String> = env::args().collect();
    debug!("args: {:?}", args);

    let mut options = args.drain(1..);
    let locator = options.next().unwrap_or_else(|| "".to_string());
    
    let mut ps = Properties::new();
    ps.insert(ZN_USER_KEY, b"user".to_vec());
    ps.insert(ZN_PASSWD_KEY, b"password".to_vec());

    debug!("Openning session...");
    let session = open(&locator, Some(ps)).await.unwrap();

    let info = session.info();
    debug!("LOCATOR :  \"{}\"", String::from_utf8_lossy(info.get(&ZN_INFO_PEER_KEY).unwrap()));
    debug!("PID :      {:02x?}", info.get(&ZN_INFO_PID_KEY).unwrap());
    debug!("PEER PID : {:02x?}", info.get(&ZN_INFO_PEER_PID_KEY).unwrap());

    let mut stored: HashMap<String, (RBuf, Option<RBuf>)> = HashMap::new();

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None
    };

    let selector = "/demo/example/**".to_string();
    
    debug!("Declaring Subscriber on {}", selector);
    let mut sub = session.declare_subscriber(&selector.clone().into(), &sub_info).await.unwrap();

    debug!("Declaring Queryable on {}", selector);
    let mut queryable = session.declare_queryable(&selector.into(), STORAGE).await.unwrap();
    
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

