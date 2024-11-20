//
// Copyright (c) 2024 ZettaScale Technology
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
use zenoh::{key_expr::KeyExpr, query::QueryTarget, Config};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, keyexpr, payload, target, timeout) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Querier on '{keyexpr}'...");
    let querier = session
        .declare_querier(keyexpr)
        .target(target)
        .timeout(timeout)
        .await
        .unwrap();
    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(Duration::from_secs(1)).await;
        #[cfg(feature = "unstable")]
        println!(
            "Matching status: {}",
            querier.matching_status().await.unwrap().matching()
        );
        let buf = format!("[{idx:4}] {}", payload.clone().unwrap_or_default());
        println!(
            "Querying '{}' with payload: '{}')...",
            &querier.key_expr(),
            buf
        );
        let replies = querier
            .get()
            // // By default get receives replies from a FIFO.
            // // Uncomment this line to use a ring channel instead.
            // // More information on the ring channel are available in the z_pull example.
            // .with(zenoh::handlers::RingChannel::default())
            // Refer to z_bytes.rs to see how to serialize different types of message
            .payload(buf)
            .await
            .unwrap();
        while let Ok(reply) = replies.recv_async().await {
            match reply.result() {
                Ok(sample) => {
                    // Refer to z_bytes.rs to see how to deserialize different types of message
                    let payload = sample
                        .payload()
                        .try_to_string()
                        .unwrap_or_else(|e| e.to_string().into());
                    println!(
                        ">> Received ('{}': '{}')",
                        sample.key_expr().as_str(),
                        payload,
                    );
                }
                Err(err) => {
                    let payload = err
                        .payload()
                        .try_to_string()
                        .unwrap_or_else(|e| e.to_string().into());
                    println!(">> Received (ERROR: '{}')", payload);
                }
            }
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
#[value(rename_all = "SCREAMING_SNAKE_CASE")]
enum Qt {
    BestMatching,
    All,
    AllComplete,
}

#[derive(Parser, Clone, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The selection of resources to query
    key_expr: KeyExpr<'static>,
    #[arg(short, long)]
    /// An optional payload to put in the query.
    payload: Option<String>,
    #[arg(short, long, default_value = "BEST_MATCHING")]
    /// The target queryables of the query.
    target: Qt,
    #[arg(short = 'o', long, default_value = "10000")]
    /// The query timeout in milliseconds.
    timeout: u64,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (
    Config,
    KeyExpr<'static>,
    Option<String>,
    QueryTarget,
    Duration,
) {
    let args = Args::parse();
    (
        args.common.into(),
        args.key_expr,
        args.payload,
        match args.target {
            Qt::BestMatching => QueryTarget::BestMatching,
            Qt::All => QueryTarget::All,
            Qt::AllComplete => QueryTarget::AllComplete,
        },
        Duration::from_millis(args.timeout),
    )
}
