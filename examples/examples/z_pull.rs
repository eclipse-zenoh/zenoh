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
use clap::Parser;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

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
        .callback(|sample| {
            println!(
                ">> [Subscriber] Received {} ('{}': '{}')",
                sample.kind,
                sample.key_expr.as_str(),
                sample.value,
            );
        })
        .res()
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        sleep(Duration::from_secs(1)).await;
        println!("[{idx:4}] Pulling...");
        subscriber.pull().res().await.unwrap();
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>) {
    let args = SubArgs::parse();
    (args.common.into(), args.key)
}
