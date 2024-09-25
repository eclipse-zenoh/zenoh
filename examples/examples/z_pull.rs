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
use std::time::Duration;

use clap::Parser;
use zenoh::{handlers::RingChannel, key_expr::KeyExpr, Config};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr, size, interval) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");
    let subscriber = session
        .declare_subscriber(&key_expr)
        .with(RingChannel::new(size))
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");

    // Blocking recv. If the ring is empty, wait for the first sample to arrive.
    loop {
        // Use .recv() for the synchronous version.
        match subscriber.recv_async().await {
            Ok(sample) => {
                let payload = sample
                    .payload()
                    .try_to_string()
                    .unwrap_or_else(|e| e.to_string().into());
                println!(
                    ">> [Subscriber] Pulled {} ('{}': '{}')... performing a computation of {:#?}",
                    sample.kind(),
                    sample.key_expr().as_str(),
                    payload,
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

    // Non-blocking recv. This can be usually used to implement a polling mechanism.
    // loop {
    //     match subscriber.try_recv() {
    //         Ok(Some(sample)) => {
    //             let payload = sample
    //                 .payload()
    //                 .try_to_string()
    //                 .unwrap_or_else(|e| e.to_string().into());
    //             println!(
    //                 ">> [Subscriber] Pulled {} ('{}': '{}')",
    //                 sample.kind(),
    //                 sample.key_expr().as_str(),
    //                 payload,
    //             );
    //         }
    //         Ok(None) => {
    //             println!(
    //                 ">> [Subscriber] Pulled nothing... sleep for {:#?}",
    //                 interval
    //             );
    //             tokio::time::sleep(interval).await;
    //         }
    //         Err(e) => {
    //             println!(">> [Subscriber] Pull error: {e}");
    //             return;
    //         }
    //     }
    // }
}

#[derive(clap::Parser, Clone, PartialEq, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    /// The size of the ringbuffer.
    #[arg(short, long, default_value = "3")]
    size: usize,
    /// The interval for pulling the ringbuffer.
    #[arg(short, long, default_value = "5.0")]
    interval: f32,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, usize, Duration) {
    let args = SubArgs::parse();
    let interval = Duration::from_secs_f32(args.interval);
    (args.common.into(), args.key, args.size, interval)
}
