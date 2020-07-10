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
#![feature(async_closure)]

use clap::{App, Arg};
use std::convert::TryInto;
use futures::prelude::*;
use zenoh::*;
use zenoh::net::Config;


#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let args = App::new("zenoh eval example")
        .arg(Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources the eval will respond for'")
            .default_value("/demo/example/eval/**"))
        .get_matches();

    let config = Config::new(args.value_of("mode").unwrap()).unwrap()
        .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
    let selector = args.value_of("selector").unwrap();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config, None).await.unwrap();
    
    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Register eval for {}'...\n", selector);
    workspace.register_eval(&selector.try_into().unwrap())
        .await.unwrap()
        .for_each( async move |get| {
            println!(">> [Eval listener] received get on {}", get.selector);

            // In this Eval function, we choosed to reply only 1 Path/Value per received get,
            // replacing each '*' (if any) in selector's path expression with a 'X'.
            let path: Path = get.selector.path_expr.to_string()
                .replace('*', "X")
                .try_into().unwrap();
            let value = Box::new(StringValue::from("Eval from Rust"));

            get.data_sender.send(Data { path, value }).await;
        }).await;

    zenoh.close().await.unwrap();
}
