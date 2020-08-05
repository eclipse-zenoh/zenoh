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
#![recursion_limit = "256"]

use clap::{App, Arg};
use futures::prelude::*;
use futures::select;
use std::collections::HashMap;
use zenoh::net::queryable::STORAGE;
use zenoh::net::utils::resource_name;
use zenoh::net::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    let mut stored: HashMap<String, (RBuf, Option<RBuf>)> = HashMap::new();

    println!("Opening session...");
    let session = open(config, None).await.unwrap();

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };

    println!("Declaring Subscriber on {}", selector);
    let mut sub = session
        .declare_subscriber(&selector.clone().into(), &sub_info)
        .await
        .unwrap();

    println!("Declaring Queryable on {}", selector);
    let mut queryable = session
        .declare_queryable(&selector.into(), STORAGE)
        .await
        .unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];
    loop {
        select!(
            sample = sub.next().fuse() => {
                let sample = sample.unwrap();
                println!(">> [Subscription listener] Received ('{}': '{}')",
                    sample.res_name, String::from_utf8_lossy(&sample.payload.to_vec()));
                stored.insert(sample.res_name.into(), (sample.payload, sample.data_info));
            },

            query = queryable.next().fuse() => {
                let query = query.unwrap();
                println!(">> [Query handler        ] Handling '{}{}'", query.res_name, query.predicate);
                for (stored_name, (data, data_info)) in stored.iter() {
                    if resource_name::intersect(&query.res_name, stored_name) {
                        query.reply(Sample{
                            res_name: stored_name.clone(),
                            payload: data.clone(),
                            data_info: data_info.clone(),
                        }).await;
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
}

fn parse_args() -> (Config, String) {
    let args = App::new("zenoh-net storage example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to store'")
                .default_value("/demo/example/**"),
        )
        .get_matches();

    let config = Config::default()
        .mode(
            args.value_of("mode")
                .map(|m| Config::parse_mode(m))
                .unwrap()
                .unwrap(),
        )
        .add_peers(
            args.values_of("peer")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        )
        .add_listeners(
            args.values_of("listener")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        );
    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector)
}
