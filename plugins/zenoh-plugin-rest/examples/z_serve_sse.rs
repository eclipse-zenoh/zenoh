//
// Copyright (c) 2022 ZettaScale Technology
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
use futures::prelude::*;
use zenoh::config::Config;
use zenoh::prelude::*;
use zenoh::publication::CongestionControl;

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

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Creating Queryable on '{}'...", key);
    let mut queryable = session.queryable(key).await.unwrap();

    async_std::task::spawn(
        queryable
            .receiver()
            .clone()
            .for_each(move |request| async move {
                request
                    .reply(Ok(Sample::new(key.to_string(), HTML)))
                    .await
                    .unwrap();
            }),
    );

    let event_key = [key, "/event"].concat();

    print!("Declaring key expression '{}'...", event_key);
    let expr_id = session.declare_expr(&event_key).await.unwrap();
    println!(" => ExprId {}", expr_id);

    println!("Declaring publication on '{}'...", expr_id);
    session.declare_publication(expr_id).await.unwrap();

    println!("Putting Data periodically ('{}': '{}')...", expr_id, value);

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

fn parse_args() -> Config {
    let args = App::new("zenoh ssl server example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
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
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    match args.value_of("mode").map(|m| m.parse()) {
        Some(Ok(mode)) => {
            config.set_mode(Some(mode)).unwrap();
        }
        Some(Err(e)) => panic!("Invalid mode: {}", e),
        None => {}
    };
    if let Some(values) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listeners") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    config
}
