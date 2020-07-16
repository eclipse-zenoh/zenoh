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
use std::convert::TryFrom;
use futures::prelude::*;
use zenoh::*;
use zenoh::net::Config;


//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> (Config, String)  {
    let args = App::new("zenoh eval example")
        .arg(Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-p, --path=[PATH] 'The path the eval will respond for'")
            .default_value("/demo/example/eval"))
        .get_matches();

    let config = Config::default()
        .mode(args.value_of("mode").map(|m| Config::parse_mode(m)).unwrap().unwrap())
        .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
    let path = args.value_of("path").unwrap().to_string();

    (config, path)
}

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, path) = parse_args();

    // NOTE: in this example we choosed to register the eval for a single Path,
    // and to send replies with this same Path.
    // But we could also register an eval for a PathExpr. In this case,
    // the eval implementation should choose the Path(s) for reply(ies) that
    // are coherent with the Selector received in GetRequest.
    let ref path = Path::try_from(path).unwrap();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config, None).await.unwrap();
    
    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Register eval for {}'...\n", path);
    let mut get_stream = workspace.register_eval(&path.into()).await.unwrap();
    while let Some(get_request) = get_stream.next().await {
        println!(">> [Eval listener] received get on {}", get_request.selector);

        // The returned Value is a StringValue with a 'name' part which is set in 3 possible ways,
        // depending the properties specified in the selector. For example, with the
        // following selectors:
        // - "/zenoh/example/eval" : no properties are set, a default value is used for the name
        // - "/zenoh/example/eval?(name=Bob)" : "Bob" is used for the name
        // - "/zenoh/example/eval?(name=/zenoh/example/name)" : the Eval function does a GET 
        //      on "/zenoh/example/name" an uses the 1st result for the name
        let mut name = get_request.selector.properties.get("name").cloned().unwrap_or("Rust!".to_string());
        println!("  >> name = {}", name);
        if name.starts_with('/') {
            println!("  >> get name from  {}", name);
            if let Ok(selector) = Selector::try_from(name.as_str()) {
                if let Some(data) = workspace.get(&selector).await.unwrap().next().await {
                    name = String::from_utf8_lossy(&data.value.as_rbuf().to_vec()).to_string();
                } else {
                    println!("Failed to get value from '{}' : not found", name);
                }
            } else {
                println!("Failed to get value from '{}' : this is not a valid Selector", name);
            }

        }

        let value = Box::new(StringValue::from(format!("Eval from {}", name)));

        get_request.data_sender.send(Data { path: path.clone(), value }).await;
    }

    zenoh.close().await.unwrap();
}
