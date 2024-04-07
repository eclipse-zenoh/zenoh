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
use clap::Parser;
use std::time::Duration;
use zenoh::{
    config::Config, handlers::RingBuffer, key_expr::KeyExpr, prelude::r#async::*,
    session::SessionDeclarations,
};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr, size, interval) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");
    let subscriber = session
        .declare_subscriber(&key_expr)
        .with(RingBuffer::new(size))
        .res()
        .await
        .unwrap();

    println!(
        "Pulling data every {:#?} seconds. Press CTRL-C to quit...",
        interval
    );
    loop {
        match subscriber.recv() {
            Ok(Some(sample)) => {
                let payload = sample
                    .payload()
                    .deserialize::<String>()
                    .unwrap_or_else(|e| format!("{}", e));
                println!(
                    ">> [Subscriber] Pulled {} ('{}': '{}')",
                    sample.kind(),
                    sample.key_expr().as_str(),
                    payload,
                );
            }
            Ok(None) => {
                println!(
                    ">> [Subscriber] Pulled nothing... sleep for {:#?}",
                    interval
                );
                tokio::time::sleep(interval).await;
            }
            Err(e) => {
                println!(">> [Subscriber] Pull error: {e}");
                return;
            }
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    /// The size of the ringbuffer.
    #[arg(long, default_value = "3")]
    size: usize,
    /// The interval for pulling the ringbuffer.
    #[arg(long, default_value = "5.0")]
    interval: f32,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, usize, Duration) {
    let args = SubArgs::parse();
    let interval = Duration::from_secs_f32(args.interval);
    (args.common.into(), args.key, args.size, interval)
}
