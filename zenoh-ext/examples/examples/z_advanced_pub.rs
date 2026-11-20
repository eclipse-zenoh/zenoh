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
use std::{num::NonZeroUsize, time::Duration};

use clap::Parser;
use zenoh::{config::Config, key_expr::KeyExpr};
use zenoh_config::ModeDependentValue;
use zenoh_ext::{AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};
use zenoh_ext_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, args) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring AdvancedPublisher on {}", args.key);
    let mut publisher = session
        .declare_publisher(&args.key)
        .cache(CacheConfig::default().max_samples(args.history))
        .sample_miss_detection(MissDetectionConfig::default().heartbeat(Duration::from_millis(500)))
        .publisher_detection();
    if let Some(size) = args.fragment {
        publisher = publisher.fragmentation(size.get());
    }
    let publisher = publisher.await.unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let buf = format!("[{idx:4}] {}", args.value);
        println!("Put Data ('{}': '{}')", args.key, buf);
        publisher.put(buf).await.unwrap();
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-pub")]
    /// The key expression to publish.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Pub from Rust!")]
    /// The value to publish.
    value: String,
    #[arg(short = 'i', long, default_value = "1")]
    /// The number of publications to keep in cache.
    history: usize,
    #[arg(long, value_name = "BYTES")]
    /// Fragment payloads larger than this size in bytes (disabled by default).
    fragment: Option<NonZeroUsize>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, Args) {
    let args = Args::parse();
    let mut config: Config = (&args.common).into();
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();
    (config, args)
}
