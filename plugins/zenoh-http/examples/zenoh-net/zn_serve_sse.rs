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
use zenoh::net::*;
use zenoh::net::queryable::EVAL;
use zenoh_protocol::proto::{encoding, kind};

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

    let args = App::new("zenoh-net ssl server example")
      .arg(Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
          .possible_values(&["peer", "client"]).default_value("peer"))
      .arg(Arg::from_usage("-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'"))
      .get_matches();

    let config = Config::new()
      .mode(args.value_of("mode").map(|m| Config::into_mode(m)).unwrap().unwrap())
      .add_peers(args.values_of("peer").map(|p| p.collect()).or_else(|| Some(vec![])).unwrap());
    let path    = "/demo/sse";
    let value   = "Pub from sse server!";

    println!("Openning session...");
    let session = open(config, None).await.unwrap();

    println!("Declaring Queryable on {}", path);
    let queryable = session.declare_queryable(&path.into(), EVAL).await.unwrap();

    async_std::task::spawn(
        queryable.for_each(async move |request|{
            request.replies_sender.send(Sample {
                res_name: path.to_string(),
                payload: HTML.as_bytes().into(),
                data_info: None,
            }).await;
        })
    );

    let event_path = [path, "/event"].concat();

    print!("Declaring Resource {}", event_path);
    let rid = session.declare_resource(&event_path.into()).await.unwrap();
    println!(" => RId {}", rid);
    
    println!("Declaring Publisher on {}", rid);
    let _publ = session.declare_publisher(&rid.into()).await.unwrap();

    println!("Writing Data periodically ('{}': '{}')...", rid, value);

    println!("Data updates are accessible through HTML5 SSE at http://<hostname>:8000{}", path);
    loop {
        session.write_wo(&rid.into(), value.as_bytes().into(), encoding::TEXT_PLAIN, kind::PUT).await.unwrap();
        async_std::task::sleep(std::time::Duration::new(1, 0)).await;
    }
}
