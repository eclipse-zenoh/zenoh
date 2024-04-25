// Copyright (c) 2024 ZettaScale Technology
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

use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;

#[tokio::main]
async fn main() {
    zenoh_util::init_log_test();

    let pub_key_expr = KeyExpr::try_from("test/valgrind/data").unwrap();
    let sub_key_expr = KeyExpr::try_from("test/valgrind/**").unwrap();

    println!("Declaring Publisher on '{pub_key_expr}'...");
    let pub_session = zenoh::open(Config::default()).res().await.unwrap();
    let publisher = pub_session
        .declare_publisher(&pub_key_expr)
        .res()
        .await
        .unwrap();

    println!("Declaring Subscriber on '{sub_key_expr}'...");
    let sub_session = zenoh::open(Config::default()).res().await.unwrap();
    let _subscriber = sub_session
        .declare_subscriber(&sub_key_expr)
        .callback(|sample| {
            println!(
                ">> [Subscriber] Received {} ('{}': '{}')",
                sample.kind,
                sample.key_expr.as_str(),
                sample.value
            );
        })
        .res()
        .await
        .unwrap();

    for idx in 0..5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let buf = format!("[{idx:4}] data");
        println!("Putting Data ('{}': '{}')...", &pub_key_expr, buf);
        publisher.put(buf).res().await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
}
