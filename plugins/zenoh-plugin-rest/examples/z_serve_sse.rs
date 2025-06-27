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

use clap::{arg, Command};
use zenoh::{config::Config, key_expr::keyexpr, qos::CongestionControl};

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

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let config = parse_args();
    let key = keyexpr::new("demo/sse").unwrap();
    let value = "Pub from sse server!";

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Queryable on '{key}'...");
    let queryable = session.declare_queryable(key).await.unwrap();

    tokio::task::spawn({
        let receiver = queryable.handler().clone();
        async move {
            while let Ok(request) = receiver.recv_async().await {
                request.reply(key, HTML).await.unwrap();
            }
        }
    });

    let event_key = [key, "/event"].concat();

    println!("Declaring Publisher on '{event_key}'...");
    let publisher = session
        .declare_publisher(&event_key)
        .congestion_control(CongestionControl::Block)
        .await
        .unwrap();

    println!(
        "Putting Data periodically ('{}': '{}')...",
        &event_key, value
    );

    println!("Data updates are accessible through HTML5 SSE at http://<hostname>:8000/{key}");
    loop {
        publisher.put(value).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn parse_args() -> Config {
    let args = Command::new("zenoh ssl server example")
        .arg(
            arg!(-m --mode [MODE] "The zenoh session mode (peer by default).")
                .value_parser(["peer", "client"]),
        )
        .arg(arg!(-e --connect [ENDPOINT]...  "Endpoints to connect to."))
        .arg(arg!(-l --listen [ENDPOINT]...  "Endpoints to listen on."))
        .arg(arg!(-c --config [FILE] "A configuration file."))
        .arg(
            arg!(--"no-multicast-scouting" "Disable the multicast-based scouting mechanism."
            ),
        )
        .get_matches();

    let mut config = if let Some(conf_file) = args.get_one::<&String>("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    match args.get_one::<&String>("mode").map(|m| m.parse()) {
        Some(Ok(mode)) => {
            config.set_mode(Some(mode)).unwrap();
        }
        Some(Err(e)) => panic!("Invalid mode: {e}"),
        None => {}
    };
    if let Some(values) = args.get_many::<&String>("connect") {
        config
            .connect
            .endpoints
            .set(values.into_iter().map(|v| v.parse().unwrap()).collect())
            .unwrap();
    }
    if let Some(values) = args.get_many::<&String>("listen") {
        config
            .listen
            .endpoints
            .set(values.into_iter().map(|v| v.parse().unwrap()).collect())
            .unwrap();
    }
    if args.get_flag("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    config
}
