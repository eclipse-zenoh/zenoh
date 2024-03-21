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
use zenoh::handlers::RingBuffer;
use zenoh::{config::Config, prelude::r#async::*};
use zenoh_examples::CommonArgs;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr, cache, interval) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");
    let subscriber = session
        .declare_subscriber(&key_expr)
        .with(RingBuffer::new(cache))
        .res()
        .await
        .unwrap();

    println!("Pulling data every {:#?} seconds", interval);
    loop {
        print!(">> [Subscriber] Pulling ");
        match subscriber.recv() {
            Ok(Some(sample)) => {
                let payload = sample
                    .payload()
                    .deserialize::<String>()
                    .unwrap_or_else(|e| format!("{}", e));
                println!(
                    "{} ('{}': '{}')",
                    sample.kind(),
                    sample.key_expr().as_str(),
                    payload,
                );
            }
            Ok(None) => {
                println!(">> [Subscriber] nothing... sleep for {:#?}", interval);
                sleep(interval).await;
            }
            Err(e) => {
                println!(">> [Subscriber] Pull error: {e}")
            }
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    /// The size of the cache.
    #[arg(long, default_value = "3")]
    cache: usize,
    /// The interval for pulling the cache.
    #[arg(long, default_value = "5.0")]
    interval: f32,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, usize, Duration) {
    let args = SubArgs::parse();
    let interval = Duration::from_secs_f32(args.interval);
    (args.common.into(), args.key, args.cache, interval)
}
