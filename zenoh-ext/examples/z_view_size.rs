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
use clap::{App, Arg};
use std::sync::Arc;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_ext::group::*;

#[async_std::main]
async fn main() {
    env_logger::init();

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
    let args = App::new("zenoh-ext group view size example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...  'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "-g, --group=[STRING] 'The group name'",
        ).default_value("zgroup"))
        .arg(Arg::from_usage(
            "-i, --id=[STRING] 'The group member id (default is the zenoh ID)'",
        ))
        .arg(Arg::from_usage(
            "-s, --size=[INT] 'The expected group size. The example will wait for the group to reach this size'",
        ).default_value("3"))
        .arg(Arg::from_usage(
            "-t, --timeout=[SEC] 'The duration (in seconds) this example will wait for the group to reach the expected size.'",
        ).default_value("15"))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.value_of("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }

    let group = args.value_of("group").unwrap().to_string();
    let id = args.value_of("id").map(String::from);
    let size: usize = args.value_of("size").unwrap().parse().unwrap();
    let timeout: u64 = args.value_of("timeout").unwrap().parse().unwrap();

    (config, group, id, size, timeout)
}
