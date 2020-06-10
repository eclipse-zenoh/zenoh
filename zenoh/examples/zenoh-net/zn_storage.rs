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

use clap::App;
use std::collections::HashMap;
use futures::prelude::*;
use futures::select;
use async_std::task;
use zenoh::net::*;
use zenoh::net::queryable::STORAGE;

fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();

        let args = App::new("zenoh-net storage example")
            .arg("-l, --locator=[LOCATOR]   'Sets the locator used to initiate the zenoh session'")
            .arg("-s, --selector=[SELECTOR] 'Sets the selection of resources to store'")
            .get_matches();

        let locator  = args.value_of("locator").unwrap_or("").to_string();
        let selector = args.value_of("selector").unwrap_or("/demo/example/**").to_string();

        let mut stored: HashMap<String, (RBuf, Option<RBuf>)> = HashMap::new();

        println!("Openning session...");
        let session = open(&locator, None).await.unwrap();

        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None
        };

        println!("Declaring Subscriber on {}", selector);
        let mut sub = session.declare_subscriber(&selector.clone().into(), &sub_info).await.unwrap();

        println!("Declaring Queryable on {}", selector);
        let mut queryable = session.declare_queryable(&selector.into(), STORAGE).await.unwrap();


        let mut stdin = async_std::io::stdin();
        let mut input = [0u8];
        loop {
            select!(
                sample = sub.next().fuse() => {
                    let (res_name, payload, data_info) = sample.unwrap();
                    println!(">> [Subscription listener] Received ('{}': '{}')", res_name, String::from_utf8_lossy(&payload.to_vec()));
                    stored.insert(res_name.into(), (payload, data_info));
                },

                query = queryable.next().fuse() => {
                    let (res_name, predicate, replies_sender) = query.unwrap();
                    println!(">> [Query handler        ] Handling '{}?{}'", res_name, predicate);
                    for (rname, (data, data_info)) in stored.iter() {
                        if rname_intersect(&res_name, rname) {
                            replies_sender.send((rname.clone(), data.clone(), data_info.clone())).await;
                        }
                    }
                },
                
                _ = stdin.read_exact(&mut input).fuse() => {
                    if input[0] == 'q' as u8 {break}
                }
            );
        }

        session.undeclare_queryable(queryable).await.unwrap();
        session.undeclare_subscriber(sub).await.unwrap();
        session.close().await.unwrap();
    })
}
