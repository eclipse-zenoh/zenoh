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

use clap::{App, Arg};
use futures::prelude::*;
use zenoh::prelude::*;
use zenoh::publication::CongestionControl;
use zenoh::queryable::EVAL;

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
    let key = "/demo/sse";
    let value = "Pub from sse server!";

    println!("Open session");
    let session = zenoh::open(config).await.unwrap();

    println!("Declare Queryable on {}", key);
    let mut queryable = session.queryable(key).kind(EVAL).await.unwrap();

    async_std::task::spawn(
        queryable
            .receiver()
            .clone()
            .for_each(move |request| async move {
                request
                    .reply_async(Sample::new(key.to_string(), HTML))
                    .await;
            }),
    );

    let event_key = [key, "/event"].concat();

    print!("Declare key expression {}", event_key);
    let expr_id = session.declare_expr(&event_key).await.unwrap();
    println!(" => ExprId {}", expr_id);

    println!("Declare publication on {}", expr_id);
    session.declare_publication(expr_id).await.unwrap();

    println!("Put Data periodically ('{}': '{}')...", expr_id, value);

    println!(
        "Data updates are accessible through HTML5 SSE at http://<hostname>:8000{}",
        key
    );
    loop {
        session
            .put(expr_id, value)
            .encoding(Encoding::TEXT_PLAIN)
            .congestion_control(CongestionControl::Block)
            .await
            .unwrap();
        async_std::task::sleep(std::time::Duration::new(1, 0)).await;
    }
}

fn parse_args() -> Properties {
    let args = App::new("zenoh ssl server example")
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
