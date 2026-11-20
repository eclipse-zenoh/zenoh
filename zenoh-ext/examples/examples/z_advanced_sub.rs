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
use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    time::Duration,
};

use clap::Parser;
use zenoh::config::Config;
use zenoh_ext::{AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig};
use zenoh_ext_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, args) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    let mut recovery = RecoveryConfig::default().heartbeat();
    if let Some(delay) = args.fragment_recovery_delay {
        recovery = recovery.fragments_recovery_delay(Duration::from_millis(delay.get()));
    }

    println!("Declaring AdvancedSubscriber on {}", args.key);
    let mut subscriber = session
        .declare_subscriber(args.key)
        .history(HistoryConfig::default().detect_late_publishers())
        .recovery(recovery)
        .subscriber_detection();
    if let Some(max) = args.max_fragments {
        subscriber = subscriber.max_fragments(max.get());
    }
    if let Some(max) = args.max_pending_samples {
        subscriber = subscriber.max_pending_samples(max.get());
    }
    let subscriber = subscriber.await.unwrap();

    let miss_listener = subscriber.sample_miss_listener().await.unwrap();

    println!("Press CTRL-C to quit...");
    loop {
        tokio::select! {
            sample = subscriber.recv_async() => {
                if let Ok(sample) = sample {
                    let payload = sample
                        .payload()
                        .try_to_string()
                        .unwrap_or_else(|e| e.to_string().into());
                    println!(
                        ">> [Subscriber] Received {} ('{}': '{}')",
                        sample.kind(),
                        sample.key_expr().as_str(),
                        payload
                    );
                }
            },
            miss = miss_listener.recv_async() => {
                if let Ok(miss) = miss {
                    println!(
                        ">> [Subscriber] Missed {} samples from {:?} !!!",
                        miss.nb(),
                        miss.source()
                    );
                }
            },
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The key expression to subscribe onto.
    key: String,
    #[arg(long, value_name = "COUNT")]
    /// Maximum fragments per publication (default: 4096).
    max_fragments: Option<NonZeroU32>,
    #[arg(long, value_name = "COUNT")]
    /// Maximum publications buffered per source for reordering and reassembly (default: 100).
    max_pending_samples: Option<NonZeroUsize>,
    #[arg(long, value_name = "MS")]
    /// Fragment check and retry delay in milliseconds (default: 1000).
    fragment_recovery_delay: Option<NonZeroU64>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, Args) {
    let args = Args::parse();
    ((&args.common).into(), args)
}
