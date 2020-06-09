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
use clap::App;
use std::collections::HashMap;
use async_std::prelude::*;
use async_std::task;
use async_std::sync::Arc;
use spin::RwLock;
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

        // Create a HashMap to store the keys/values receveid in data_handler closure.
        // As this map has to be used also in query_handler closure, we need to wrap it
        // in a Arc<RwLock<T>>. Each closure will own a copy of this Arc.
        let stored: Arc<RwLock<HashMap<String, (RBuf, Option<RBuf>)>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let stored_shared = stored.clone();

        let data_handler = move |res_name: &str, payload: RBuf, data_info: Option<RBuf>| {
            println!(">> [Subscription listener] Received ('{}': '{}')", res_name, String::from_utf8_lossy(&payload.to_vec()));
            stored.write().insert(res_name.into(), (payload, data_info));
        };

        let query_handler = move |res_name: &str, predicate: &str, replies_sender: &RepliesSender, query_handle: QueryHandle| {
            println!(">> [Query handler   ] Handling '{}?{}'", res_name, predicate);
            let mut result: Vec<(String, RBuf, Option<RBuf>)> = Vec::new();
            let ref st = stored_shared.read();
            for (rname, (data, data_info)) in st.iter() {
                if rname_intersect(res_name, rname) {
                    result.push((rname.to_string(), data.clone(), data_info.clone()));
                }
            }
            (*replies_sender)(query_handle, result);
        };


        println!("Openning session...");
        let session = open(&locator, None).await.unwrap();

        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None
        };

        println!("Declaring Subscriber on {}", selector);
        let sub = session.declare_subscriber(&selector.clone().into(), &sub_info, data_handler).await.unwrap();

        println!("Declaring Queryable on {}", selector);
        let queryable = session.declare_queryable(&selector.into(), STORAGE, query_handler).await.unwrap();

        let mut stdin = async_std::io::stdin();
        let mut input = [0u8];
        while input[0] != 'q' as u8 {
            stdin.read_exact(&mut input).await.unwrap();
        }

        session.undeclare_queryable(queryable).await.unwrap();
        session.undeclare_subscriber(sub).await.unwrap();
        session.close().await.unwrap();
    })
}
