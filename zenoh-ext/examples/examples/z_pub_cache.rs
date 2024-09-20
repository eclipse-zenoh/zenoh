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

use clap::{arg, Parser};
use zenoh::{config::Config, key_expr::KeyExpr};
use zenoh_config::ModeDependentValue;
use zenoh_ext::*;
use zenoh_ext_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr, value, history, prefix, complete) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring PublicationCache on {}", &key_expr);
    let mut publication_cache_builder = session
        .declare_publication_cache(&key_expr)
        .history(history)
        .queryable_complete(complete);
    if let Some(prefix) = prefix {
        publication_cache_builder = publication_cache_builder.queryable_prefix(prefix);
    }
    let _publication_cache = publication_cache_builder.await.unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let buf = format!("[{idx:4}] {value}");
        println!("Put Data ('{}': '{}')", &key_expr, buf);
        session.put(&key_expr, buf).await.unwrap();
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-pub")]
    /// The key expression to publish.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Pub from Rust!")]
    /// The value to reply to queries.
    value: String,
    #[arg(short = 'i', long, default_value = "1")]
    /// The number of publications to keep in cache.
    history: usize,
    #[arg(short = 'o', long)]
    /// Set `complete` option to true. This means that this queryable is ultimate data source, no need to scan other queryables.
    complete: bool,
    #[arg(short = 'x', long)]
    /// An optional queryable prefix.
    prefix: Option<String>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (
    Config,
    KeyExpr<'static>,
    String,
    usize,
    Option<String>,
    bool,
) {
    let args = Args::parse();
    let mut config: Config = args.common.into();
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();
    (
        config,
        args.key,
        args.value,
        args.history,
        args.prefix,
        args.complete,
    )
}
