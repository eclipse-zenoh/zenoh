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
use clap::{App, Arg, Values};
use futures::prelude::*;
use futures::select;
use zenoh::net::queryable::EVAL;
use zenoh::net::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, path, value) = parse_args();

    println!("Opening session...");
    let session = open(config).await.unwrap();

    println!("Declaring Queryable on {}", path);
    let mut queryable = session
        .declare_queryable(&path.clone().into(), EVAL)
        .await
        .unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];
    loop {
        select!(
            query = queryable.stream().next().fuse() => {
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
}

fn parse_args() -> (Properties, String, String) {
    let args = App::new("zenoh-net eval example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(
            Arg::from_usage("-p, --path=[PATH]        'The name of the resource to evaluate.'")
                .default_value("/demo/example/zenoh-rs-eval"),
        )
        .arg(
            Arg::from_usage("-v, --value=[VALUE]      'The value to reply to queries.'")
                .default_value("Eval from Rust!"),
        )
        .get_matches();

    let mut config = config::empty();
    config.push((
        config::ZN_MODE_KEY,
        args.value_of("mode").unwrap().as_bytes().to_vec(),
    ));
    for peer in args
        .values_of("peer")
        .or_else(|| Some(Values::default()))
        .unwrap()
    {
        config.push((config::ZN_PEER_KEY, peer.as_bytes().to_vec()));
    }
    for listener in args
        .values_of("listener")
        .or_else(|| Some(Values::default()))
        .unwrap()
    {
        config.push((config::ZN_LISTENER_KEY, listener.as_bytes().to_vec()));
    }

    let path = args.value_of("path").unwrap().to_string();
    let value = args.value_of("value").unwrap().to_string();

    (config, path, value)
}
