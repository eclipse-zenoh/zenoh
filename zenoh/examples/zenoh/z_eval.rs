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
use clap::{App, Arg};
use futures::prelude::*;
use std::convert::TryFrom;
use zenoh::*;

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
    let path = &Path::try_from(path).unwrap();

    println!("New zenoh...");
    let zenoh = Zenoh::new(config.into()).await.unwrap();

    println!("New workspace...");
    let workspace = zenoh.workspace(None).await.unwrap();

    println!("Register eval for {}'...\n", path);
    let mut get_stream = workspace.register_eval(&path.into()).await.unwrap();
    while let Some(get_request) = get_stream.next().await {
        println!(
            ">> [Eval listener] received get with selector: {}",
            get_request.selector
        );

        // The returned Value is a StringValue with a 'name' part which is set in 3 possible ways,
        // depending the properties specified in the selector. For example, with the
        // following selectors:
        // - "/zenoh/example/eval" : no properties are set, a default value is used for the name
        // - "/zenoh/example/eval?(name=Bob)" : "Bob" is used for the name
        // - "/zenoh/example/eval?(name=/zenoh/example/name)" : the Eval function does a GET
        //      on "/zenoh/example/name" an uses the 1st result for the name
        let mut name = get_request
            .selector
            .properties
            .get("name")
            .cloned()
            .unwrap_or_else(|| "Rust!".to_string());
        if name.starts_with('/') {
            println!("   >> Get name to use from path: {}", name);
            if let Ok(selector) = Selector::try_from(name.as_str()) {
                match workspace.get(&selector).await.unwrap().next().await {
                    Some(Data {
                        path: _,
                        value: Value::StringUtf8(s),
                        timestamp: _,
                    }) => name = s,
                    Some(_) => println!("Failed to get name from '{}' : not a UTF-8 String", name),
                    None => println!("Failed to get name from '{}' : not found", name),
                }
            } else {
                println!(
                    "Failed to get value from '{}' : this is not a valid Selector",
                    name
                );
            }
        }
        let s = format!("Eval from {}", name);
        println!(r#"   >> Returning string: "{}""#, s);
        get_request.reply(path.clone(), s.into()).await;
    }

    get_stream.close().await.unwrap();
    zenoh.close().await.unwrap();
}

fn parse_args() -> (Properties, String) {
    let args = App::new("zenoh eval example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(
            Arg::from_usage("-p, --path=[PATH] 'The path the eval will respond for'")
                .default_value("/demo/example/eval"),
        )
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Properties::try_from(std::path::Path::new(conf_file)).unwrap()
    } else {
        Properties::default()
    };
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let path = args.value_of("path").unwrap().to_string();

    (config, path)
}
