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
use futures::prelude::*;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::handlers::PullCache;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr, capacity) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");

    let subscriber = session
        .declare_subscriber(&key_expr)
        .with(PullCache::new(capacity))
        .res()
        .await
        .unwrap();

    // Handle keyboard's input.
    println!("Press <enter> to pull data...");
    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        stdin.read_exact(&mut input).await.unwrap();
        match input[0] {
            b'q' => break,
            0 => sleep(Duration::from_secs(1)).await,
            _ => match subscriber.pull() {
                Ok(Some(sample)) => {
                    println!(
                        ">> [Subscriber] Received {} ('{}': '{}')",
                        sample.kind,
                        sample.key_expr.as_str(),
                        sample.value,
                    )
                }
                Ok(None) => {
                    println!(">> [Subscriber] Nothing to pull")
                }
                Err(e) => {
                    println!(">> [Subscriber] Pull error: {e}")
                }
            },
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    /// The size of the cache.
    #[arg(long, default_value = "3")]
    cache: usize,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, usize) {
    let args = SubArgs::parse();
    (args.common.into(), args.key, args.cache)
}
