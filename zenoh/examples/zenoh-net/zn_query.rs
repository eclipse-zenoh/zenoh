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
use futures::prelude::*;
use zenoh::net::*;

//
// Argument parsing -- look at the main for the zenoh-related code
//
fn parse_args() -> (Config, String)  {
    let args = App::new("zenoh-net query example")
        .arg(Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to query'")
            .default_value("/demo/example/**"))
        .get_matches();

    let config = Config::default()
        .mode(args.value_of("mode").map(|m| Config::parse_mode(m)).unwrap().unwrap())
        .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
        
    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector.to_string())
}

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    println!("Openning session...");
    let session = open(config, None).await.unwrap();

    println!("Sending Query '{}'...", selector);
    session.query(
        &selector.into(), "",
        QueryTarget::default(),
        QueryConsolidation::default()
    ).await.unwrap().for_each( async move |reply| 
        println!(">> [Reply handler] received reply data {:?} : {}",
            reply.data.res_name, String::from_utf8_lossy(&reply.data.payload.to_vec()))
    ).await;

    session.close().await.unwrap();
}
