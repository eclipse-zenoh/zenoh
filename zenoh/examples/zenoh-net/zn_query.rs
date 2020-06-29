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

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let args = App::new("zenoh-net query example")
        .arg(Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode.")
            .possible_values(&["peer", "client"]).default_value("peer"))
        .arg(Arg::from_usage("-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'"))
        .arg(Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to query'")
            .default_value("/demo/example/**"))
        .get_matches();

    let config = Config::new(args.value_of("mode").unwrap()).unwrap()
        .add_peers(args.values_of("peer").map(|p| p.collect()).or(Some(vec![])).unwrap());
    let selector = args.value_of("selector").unwrap().to_string();

    println!("Openning session...");
    let session = open(config, None).await.unwrap();

    println!("Sending Query '{}'...", selector);
    session.query(
        &selector.into(), "",
        QueryTarget::default(),
        QueryConsolidation::default()
    ).await.unwrap().for_each(
        async move |reply: Reply| {
            match reply {
                Reply::ReplyData {reskey, payload, ..} => {println!(">> [Reply handler] received reply data {:?} : {}", 
                                                            reskey, String::from_utf8_lossy(&payload.to_vec()))}
                Reply::SourceFinal {..} => {println!(">> [Reply handler] received source final.")}
                Reply::ReplyFinal => {println!(">> [Reply handler] received reply final.")}
            }
        }
    ).await;

    session.close().await.unwrap();
}
