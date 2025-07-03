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
use clap::{arg, Parser};
use zenoh::config::Config;
use zenoh_ext::{AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig};
use zenoh_ext_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring AdvancedSubscriber on {key_expr}");
    let subscriber = session
        .declare_subscriber(key_expr)
        .history(HistoryConfig::default().detect_late_publishers())
        .recovery(RecoveryConfig::default().heartbeat())
        .subscriber_detection()
        .await
        .unwrap();

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
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, String) {
    let args = Args::parse();
    (args.common.into(), args.key)
}
