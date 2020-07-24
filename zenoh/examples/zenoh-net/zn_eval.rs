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
use futures::select;
use zenoh::net::*;
use zenoh::net::queryable::EVAL;

//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> (Config, String, String) {
    let args = App::new("zenoh-net eval example")
        .arg(Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-p, --path=[PATH]        'The name of the resource to evaluate.'")
            .default_value("/demo/example/zenoh-rs-eval"))
        .arg(Arg::from_usage("-v, --value=[VALUE]      'The value to reply to queries.'")
            .default_value("Eval from Rust!"))
        .get_matches();

    let config = Config::default()
        .mode(args.value_of("mode").map(|m| Config::parse_mode(m)).unwrap().unwrap())
        .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
    let path    = args.value_of("path").unwrap().to_string();
    let value   = args.value_of("value").unwrap().to_string();

    (config, path, value)
}
#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, path, value) = parse_args();

    println!("Openning session...");
    let session = open(config, None).await.unwrap();

    println!("Declaring Queryable on {}", path);
    let mut queryable = session.declare_queryable(&path.clone().into(), EVAL).await.unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];
    loop {
        select!(
            query = queryable.next().fuse() => {
                let query = query.unwrap();
                println!(">> [Query handler] Handling '{}{}'", query.res_name, query.predicate);
                query.reply(Sample{
                    res_name: path.clone(),
                    payload: value.as_bytes().into(),
                    data_info: None,
                }).await;
            },
            
            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == 'q' as u8 {break}
            }
        );
    }

    session.undeclare_queryable(queryable).await.unwrap();
    session.close().await.unwrap();
}
