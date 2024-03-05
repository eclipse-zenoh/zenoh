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
use clap::arg;
use clap::Command;
use futures::prelude::*;
use futures::select;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh::query::ReplyKeyExpr;
use zenoh_ext::*;

#[tokio::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, query) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!(
        "Declaring QueryingSubscriber on {} with an initial query on {}",
        key_expr,
        query.as_ref().unwrap_or(&key_expr)
    );
    let subscriber = if let Some(selector) = query {
        session
            .declare_subscriber(key_expr)
            .querying()
            .query_selector(&selector)
            .query_accept_replies(ReplyKeyExpr::Any)
            .res()
            .await
            .unwrap()
    } else {
        session
            .declare_subscriber(key_expr)
            .querying()
            .res()
            .await
            .unwrap()
    };

    println!("Enter 'q' to quit...");
    let mut stdin = tokio::io::stdin();
    let mut input = [0_u8];
    loop {
        select!(
            sample = subscriber.recv_async() => {
                let sample = sample.unwrap();
                println!(">> [Subscriber] Received {} ('{}': '{}')",
                    sample.kind, sample.key_expr.as_str(), sample.value);
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                match input[0] {
                    b'q' => break,
                    0 => tokio::time::sleep(Duration::from_secs(1)).await,
                    _ => (),
                }
            }
        );
    }
}

fn parse_args() -> (Config, String, Option<String>) {
    let args = Command::new("zenoh-ext query sub example")
        .arg(
            arg!(-m --mode [MODE]  "The zenoh session mode (peer by default).")
                .value_parser(["peer", "client"]),
        )
        .arg(arg!(-e --connect [ENDPOINT]...   "Endpoints to connect to."))
        .arg(arg!(-l --listen [ENDPOINT]...   "Endpoints to listen on."))
        .arg(
            arg!(-k  --key [KEYEXPR] "The key expression to subscribe onto")
                .default_value("demo/example/**"),
        )
        .arg(
            arg!(-q --query [SELECTOR] "The selector to use for queries (by default it's same than 'selector' option)")
        )
        .arg(arg!(-c --config [FILE]      "A configuration file."))
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

    let key_expr = args.get_one::<String>("key").unwrap().to_string();
    let query = args.get_one::<String>("query").map(ToString::to_string);

    (config, key_expr, query)
}
