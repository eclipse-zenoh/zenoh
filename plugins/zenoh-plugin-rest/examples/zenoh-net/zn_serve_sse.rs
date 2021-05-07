//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
#![feature(async_closure)]

use clap::{App, Arg};
use futures::prelude::*;
use zenoh::net::queryable::EVAL;
use zenoh::net::*;
use zenoh::Properties;

const HTML: &str = r#"
<div id="result"></div>
<script>
if(typeof(EventSource) !== "undefined") {
  var source = new EventSource("/demo/sse/event");
  source.addEventListener("PUT", function(e) {
    document.getElementById("result").innerHTML += e.data + "<br>";
  }, false);
} else {
  document.getElementById("result").innerHTML = "Sorry, your browser does not support server-sent events...";
}
</script>"#;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let config = parse_args();
    let path = "/demo/sse";
    let value = "Pub from sse server!";

    println!("Opening session...");
    let session = open(config.into()).await.unwrap();

    println!("Declaring Queryable on {}", path);
    let mut queryable = session.declare_queryable(&path.into(), EVAL).await.unwrap();

    async_std::task::spawn(queryable.stream().clone().for_each(async move |request| {
        request
            .reply(Sample {
                res_name: path.to_string(),
                payload: HTML.as_bytes().into(),
                data_info: None,
            })
            .await;
    }));

    let event_path = [path, "/event"].concat();

    print!("Declaring Resource {}", event_path);
    let rid = session.declare_resource(&event_path.into()).await.unwrap();
    println!(" => RId {}", rid);

    println!("Declaring Publisher on {}", rid);
    let _publ = session.declare_publisher(&rid.into()).await.unwrap();

    println!("Writing Data periodically ('{}': '{}')...", rid, value);

    println!(
        "Data updates are accessible through HTML5 SSE at http://<hostname>:8000{}",
        path
    );
    loop {
        session
            .write_ext(
                &rid.into(),
                value.as_bytes().into(),
                encoding::TEXT_PLAIN,
                data_kind::PUT,
                CongestionControl::Block,
            )
            .await
            .unwrap();
        async_std::task::sleep(std::time::Duration::new(1, 0)).await;
    }
}

fn parse_args() -> Properties {
    let args = App::new("zenoh-net ssl server example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Properties::from(std::fs::read_to_string(conf_file).unwrap())
    } else {
        Properties::default()
    };
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    config
}
