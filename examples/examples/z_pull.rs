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
use tokio::io::AsyncReadExt;
use clap::Parser;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

#[tokio::main]
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
        let mut stdin = tokio::io::stdin();
        let mut input = [0_u8];
        loop {
            stdin.read_exact(&mut input).await.unwrap();
            match input[0] {
                b'q' => break,
                0 => tokio::time::sleep(Duration::from_secs(1)).await,
                _ => subscriber.pull().res().await.unwrap(),
            }
        }
    };

    // Execute both futures concurrently until one of them returns.
    tokio::select! {
        _ = subs => {},
        _ = keyb => {},
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
