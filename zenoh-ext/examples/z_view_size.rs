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
use clap::{arg, Command};
use std::sync::Arc;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_ext::group::*;

#[tokio::main]
async fn main() {
    zenoh_util::init_log_from_env();

    let (config, group_name, id, size, timeout) = parse_args();

    let z = Arc::new(zenoh::open(config).res().await.unwrap());
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

fn parse_args() -> (Config, String, Option<String>, usize, u64) {
    let args = Command::new("zenoh-ext group view size example")
        .arg(
            arg!(-m --mode [MODE] "The zenoh session mode (peer by default).")
                .value_parser(["peer", "client"]),
        )
        .arg(arg!(
            -e --connect [ENDPOINT]...  "Endpoints to connect to."
        ))
        .arg(arg!(
            -l --listen [ENDPOINT]...   "Endpoints to listen on."
        ))
        .arg(arg!(
            -c --config [FILE]      "A configuration file."
        ))
        .arg(arg!(
            -g --group [STRING] "The group name"
        ).default_value("zgroup"))
        .arg(arg!(
            -i --id [STRING] "The group member id (default is the zenoh ID)"
        ))
        .arg(arg!(
            -s --size [INT] "The expected group size. The example will wait for the group to reach this size"
        ).default_value("3"))
        .arg(arg!(
            -t --timeout [SEC] "The duration (in seconds) this example will wait for the group to reach the expected size."
        ).default_value("15"))
        .get_matches();

    let mut config = if let Some(conf_file) = args.get_one::<&String>("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.get_one::<&String>("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.get_many::<&String>("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.get_many::<&String>("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }

    let group = args.get_one::<String>("group").unwrap().to_string();
    let id = args.get_one::<String>("id").map(|v| (*v).to_owned());
    let size: usize = args.get_one::<String>("size").unwrap().parse().unwrap();
    let timeout: u64 = args.get_one::<String>("timeout").unwrap().parse().unwrap();

    (config, group, id, size, timeout)
}
