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
use clap::{arg, Command};
use std::time::Duration;
use zenoh::config::{Config, ModeDependentValue};
use zenoh::prelude::r#async::*;
use zenoh_ext::*;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh_util::init_log();

    let (config, key_expr, value, history, prefix, complete) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring PublicationCache on {}", &key_expr);
    let mut publication_cache_builder = session
        .declare_publication_cache(&key_expr)
        .history(history)
        .queryable_complete(complete);
    if let Some(prefix) = prefix {
        publication_cache_builder = publication_cache_builder.queryable_prefix(prefix);
    }
    let _publication_cache = publication_cache_builder.res().await.unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let buf = format!("[{idx:4}] {value}");
        println!("Put Data ('{}': '{}')", &key_expr, buf);
        session.put(&key_expr, buf).res().await.unwrap();
    }
}

fn parse_args() -> (Config, String, String, usize, Option<String>, bool) {
    let args = Command::new("zenoh-ext pub cache example")
        .arg(
            arg!(-m --mode [MODE] "The zenoh session mode (peer by default)")
                .value_parser(["peer", "client"]),
        )
        .arg(arg!(-e --connect [ENDPOINT]...  "Endpoints to connect to."))
        .arg(arg!(-l --listen [ENDPOINT]...   "Endpoints to listen on."))
        .arg(
            arg!(-k --key [KEYEXPR]        "The key expression to publish.")
                .default_value("demo/example/zenoh-rs-pub"),
        )
        .arg(arg!(-v --value [VALUE]      "The value to publish.").default_value("Pub from Rust!"))
        .arg(
            arg!(-i --history [SIZE] "The number of publications to keep in cache")
                .default_value("1"),
        )
        .arg(arg!(-x --prefix [STRING] "An optional queryable prefix"))
        .arg(arg!(-c --config [FILE]      "A configuration file."))
        .arg(arg!(-o --complete  "Set `complete` option to true. This means that this queryable is ulitmate data source, no need to scan other queryables."))
        .arg(arg!(--"no-multicast-scouting" "Disable the multicast-based scouting mechanism."))
        .get_matches();

    let mut config = if let Some(conf_file) = args.get_one::<&String>("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.get_one::<&String>("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.get_many::<&String>("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.get_many::<&String>("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.get_flag("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    // Timestamping of publications is required for publication cache
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();

    let key_expr = args.get_one::<String>("key").unwrap().to_string();
    let value = args.get_one::<String>("value").unwrap().to_string();
    let history: usize = args.get_one::<String>("history").unwrap().parse().unwrap();
    let prefix = args.get_one::<String>("prefix").map(|s| (*s).to_owned());
    let complete = args.get_flag("complete");

    (config, key_expr, value, history, prefix, complete)
}
