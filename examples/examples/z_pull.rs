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
use async_std::prelude::FutureExt;
use async_std::task::sleep;
use clap::{App, Arg};
use futures::prelude::*;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");

    let subscriber = session
        .declare_subscriber(&key_expr)
        .pull_mode()
        .res()
        .await
        .unwrap();

    println!("Press <enter> to pull data...");

    // Define the future to handle incoming samples of the subscription.
    let subs = async {
        while let Ok(sample) = subscriber.recv_async().await {
            println!(
                ">> [Subscriber] Received {} ('{}': '{}')",
                sample.kind,
                sample.key_expr.as_str(),
                sample.value,
            );
        }
    };

    // Define the future to handle keyboard's input.
    let keyb = async {
        let mut stdin = async_std::io::stdin();
        let mut input = [0_u8];
        loop {
            stdin.read_exact(&mut input).await.unwrap();
            match input[0] {
                b'q' => break,
                0 => sleep(Duration::from_secs(1)).await,
                _ => subscriber.pull().res().await.unwrap(),
            }
        }
    };

    // Execute both futures concurrently until one of them returns.
    subs.race(keyb).await;
}

fn parse_args() -> (Config, String) {
    let args = App::new("zenoh pull example")
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
            Arg::from_usage("-k, --key=[KEYEXPR] 'The key expression matching resources to pull'")
                .default_value("demo/example/**"),
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

    let key_expr = args.value_of("key").unwrap().to_string();

    (config, key_expr)
}
