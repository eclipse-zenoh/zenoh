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
use async_std::task::sleep;
use clap::{App, Arg};
use std::time::Duration;
use zenoh::config::Config;
use zenoh_ext::*;

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, value, history, prefix) = parse_args();

    println!("Openning session...");
    let session = zenoh::open(config).await.unwrap();

    print!("Declare key expression {}", key_expr);
    let expr_id = session.declare_expr(&key_expr).await.unwrap();
    println!(" => ExprId {}", expr_id);

    println!("Creating PublicationCache on {}", expr_id);
    let mut publication_cache_builder = session.publication_cache(expr_id).history(history);
    if let Some(prefix) = prefix {
        publication_cache_builder = publication_cache_builder.queryable_prefix(prefix);
    }
    let _publication_cache = publication_cache_builder.await.unwrap();

    for idx in 0..u32::MAX {
        sleep(Duration::from_secs(1)).await;
        let buf = format!("[{:4}] {}", idx, value);
        println!("Put Data ('{}': '{}')", expr_id, buf);
        session.put(expr_id, buf).await.unwrap();
    }
}

fn parse_args() -> (Config, String, String, usize, Option<String>) {
    let args = App::new("zenoh-ext pub cache example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-c, --connect=[ENDPOINT]...  'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(
            Arg::from_usage("-k, --key=[KEYEXPR]        'The key expression to publish.'")
                .default_value("/demo/example/zenoh-rs-pub"),
        )
        .arg(
            Arg::from_usage("-v, --value=[VALUE]      'The value to publish.'")
                .default_value("Pub from Rust!"),
        )
        .arg(
            Arg::from_usage("-h, --history=[SIZE] 'The number of publications to keep in cache'")
                .default_value("1"),
        )
        .arg(Arg::from_usage(
            "-x, --prefix=[STRING] 'An optional queryable prefix'",
        ))
        .arg(Arg::from_usage(
            "-f, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
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
    if let Some(values) = args.values_of("listeners") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    // Timestamping of publications is required for publication cache
    config.set_add_timestamp(Some(true)).unwrap();

    let key_expr = args.value_of("key").unwrap().to_string();
    let value = args.value_of("value").unwrap().to_string();
    let history: usize = args.value_of("history").unwrap().parse().unwrap();
    let prefix = args.value_of("prefix").map(String::from);

    (config, key_expr, value, history, prefix)
}
