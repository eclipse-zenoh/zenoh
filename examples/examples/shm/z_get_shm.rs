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
use zenoh::{
    query::{QueryTarget, Selector},
    shm::{BlockOn, GarbageCollect, ShmProviderBuilder},
    Config, Wait,
};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, selector, mut payload, target, timeout) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Creating POSIX SHM provider...");
    // Create SHM provider with default backend
    // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
    let provider = ShmProviderBuilder::default_backend(1024 * 1024)
        .wait()
        .unwrap();

    // Allocate an SHM buffer
    // NOTE: For allocation API please check z_alloc_shm.rs example
    // NOTE: For buf's API please check z_bytes_shm.rs example
    println!("Allocating Shared Memory Buffer...");
    let mut sbuf = provider
        .alloc(1024)
        .with_policy::<BlockOn<GarbageCollect>>()
        .await
        .unwrap();

    let content = payload
        .take()
        .unwrap_or_else(|| "Get from Rust SHM!".to_string());
    sbuf[0..content.len()].copy_from_slice(content.as_bytes());

    println!("Sending Query '{selector}'...");
    let replies = session
        .get(&selector)
        .payload(sbuf)
        .target(target)
        .timeout(timeout)
        .await
        .unwrap();

    while let Ok(reply) = replies.recv_async().await {
        match reply.result() {
            Ok(sample) => {
                print!(">> Received ('{}': ", sample.key_expr().as_str());
                match sample.payload().as_shm() {
                    Some(payload) => println!("'{}')", String::from_utf8_lossy(payload)),
                    None => println!("'Not a ShmBufInner')"),
                }
            }
            Err(err) => {
                let payload = err
                    .payload()
                    .try_to_string()
                    .unwrap_or_else(|e| e.to_string().into());
                println!(">> Received (ERROR: '{payload}')");
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
    selector: Selector<'static>,
    /// The payload to publish.
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
    Selector<'static>,
    Option<String>,
    QueryTarget,
    Duration,
) {
    let args = Args::parse();
    (
        args.common.into(),
        args.selector,
        args.payload,
        match args.target {
            Qt::BestMatching => QueryTarget::BestMatching,
            Qt::All => QueryTarget::All,
            Qt::AllComplete => QueryTarget::AllComplete,
        },
        Duration::from_millis(args.timeout),
    )
}
