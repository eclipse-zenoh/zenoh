//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use async_std::task::sleep;
use clap::{App, Arg};
use futures::prelude::*;
use futures::select;
use std::convert::TryFrom;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr, value, complete) = parse_args();

    let key_expr = KeyExpr::try_from(key_expr).unwrap();
    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        .complete(complete)
        .res()
        .await
        .unwrap();

    println!("Enter 'q' to quit...");
    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        select!(
            query = queryable.recv_async() => {
                let query = query.unwrap();
                match query.value() {
                    None => println!(">> [Queryable ] Received Query '{}'", query.selector()),
                    Some(value) => println!(">> [Queryable ] Received Query '{}' with value '{}'", query.selector(), value),
                }
                query
                    .reply(Ok(Sample::new(key_expr.clone(), value.clone())))
                    .res()
                    .await
                    .unwrap_or_else(|e| println!(">> [Queryable ] Error sending reply: {e}"));
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                match input[0] {
                    b'q' => break,
                    0 => sleep(Duration::from_secs(1)).await,
                    _ => (),
                }
            }
        );
    }
}

fn parse_args() -> (Config, String, String, bool) {
    let args = App::new("zenoh queryable example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...  'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(
            Arg::from_usage(
                "-k, --key=[KEYEXPR]        'The key expression matching queries to reply to.'",
            )
            .default_value("demo/example/zenoh-rs-queryable"),
        )
        .arg(
            Arg::from_usage("-v, --value=[VALUE]      'The value to reply to queries.'")
                .default_value("Queryable from Rust!"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .arg(Arg::from_usage(
            "--complete 'Declare the queryable as complete w.r.t. the key expression.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.value_of("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }
    let key_expr = args.value_of("key").unwrap().to_string();
    let value = args.value_of("value").unwrap().to_string();
    let complete = args.is_present("complete");

    (config, key_expr, value, complete)
}
