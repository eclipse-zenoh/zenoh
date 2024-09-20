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
use std::{sync::Arc, time::Duration};

use clap::{arg, Parser};
use zenoh::config::Config;
use zenoh_ext::group::*;
use zenoh_ext_examples::CommonArgs;

#[tokio::main]
async fn main() {
    zenoh::init_log_from_env_or("error");

    let (config, group_name, id, size, timeout) = parse_args();

    let z = Arc::new(zenoh::open(config).await.unwrap());
    let member_id = id.unwrap_or_else(|| z.zid().to_string());
    let member = Member::new(member_id.as_str())
        .unwrap()
        .lease(Duration::from_secs(3));

    let group = Group::join(z.clone(), group_name.as_str(), member)
        .await
        .unwrap();
    println!(
        "Member {member_id} waiting for {size} members in group {group_name} for {timeout} seconds..."
    );
    if group
        .wait_for_view_size(size, Duration::from_secs(timeout))
        .await
    {
        println!("Established view size of {size} with members:");
        for m in group.view().await {
            println!(" - {}", m.id());
        }
    } else {
        println!("Failed to establish view size of {size}");
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "zgroup")]
    ///"The group name".
    group: String,
    #[arg(short, long, default_value = "3")]
    /// The expected group size. The example will wait for the group to reach this size.
    size: usize,
    #[arg(short, long, default_value = "15")]
    /// The duration (in seconds) this example will wait for the group to reach the expected size
    timeout: u64,
    #[arg(short, long)]
    /// The group member id (default is the zenoh ID)
    id: Option<String>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, String, Option<String>, usize, u64) {
    let args = Args::parse();
    (
        args.common.into(),
        args.group,
        args.id,
        args.size,
        args.timeout,
    )
}
