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
use clap::{App, Arg};
use std::convert::TryFrom;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector, value, target, timeout) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Sending Query '{selector}'...");
    let replies = match value {
        Some(value) => session.get(&selector).with_value(value),
        None => session.get(&selector),
    }
    .target(target)
    .timeout(timeout)
    .res()
    .await
    .unwrap();
    while let Ok(reply) = replies.recv_async().await {
        match reply.sample {
            Ok(sample) => println!(
                ">> Received ('{}': '{}')",
                sample.key_expr.as_str(),
                sample.value,
            ),
            Err(err) => println!(">> Received (ERROR: '{}')", String::try_from(&err).unwrap()),
        }
    }
}

fn parse_args() -> (Config, String, Option<String>, QueryTarget, Duration) {
    let args = App::new("zenoh query example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode (peer by default).")
                .possible_values(["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...   'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to query'")
                .default_value("demo/example/**"),
        )
        .arg(Arg::from_usage(
            "-v, --value=[VALUE]      'An optional value to put in the query.'",
        ))
        .arg(
            Arg::from_usage("-t, --target=[TARGET] 'The target queryables of the query'")
                .possible_values(["BEST_MATCHING", "ALL", "ALL_COMPLETE"])
                .default_value("BEST_MATCHING"),
        )
        .arg(
            Arg::from_usage("-o, --timeout=[TIME] 'The query timeout in milliseconds'")
                .default_value("10000"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
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
    if let Some(values) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    let selector = args.value_of("selector").unwrap().to_string();

    let value = args.value_of("value").map(ToOwned::to_owned);

    let target = match args.value_of("target") {
        Some("BEST_MATCHING") => QueryTarget::BestMatching,
        Some("ALL") => QueryTarget::All,
        Some("ALL_COMPLETE") => QueryTarget::AllComplete,
        _ => QueryTarget::default(),
    };

    let timeout = Duration::from_millis(args.value_of("timeout").unwrap().parse::<u64>().unwrap());

    (config, selector, value, target, timeout)
}
