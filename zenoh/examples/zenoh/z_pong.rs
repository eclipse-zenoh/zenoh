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
use async_std::future;
use async_std::stream::StreamExt;
use clap::{App, Arg};
use std::convert::TryInto;
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let config = parse_args();

    let zenoh = Zenoh::new(config.into()).await.unwrap();
    let workspace = zenoh.workspace(None).await.unwrap();
    let mut sub = workspace
        .subscribe(&"/test/ping/".to_string().try_into().unwrap())
        .await
        .unwrap();

    while let Some(change) = sub.next().await {
        match change.value.unwrap() {
            Value::Raw(_, payload) => {
                workspace
                    .put(&"/test/pong".try_into().unwrap(), payload.into())
                    .await
                    .unwrap();
            }
            _ => panic!("Invalid value"),
        }
    }

    // Stop forever
    future::pending::<()>().await;
}

fn parse_args() -> Properties {
    let args = App::new("zenoh-net delay sub example")
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
        .get_matches();

    let mut config = Properties::default();
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }

    config
}
