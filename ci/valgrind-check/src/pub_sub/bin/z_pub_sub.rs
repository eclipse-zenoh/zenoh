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

use zenoh::{config::Config, key_expr::KeyExpr};

#[tokio::main]
async fn main() {
    let _z = zenoh_runtime::ZRuntimePoolGuard;
    zenoh_util::init_log_test();

    let pub_key_expr = KeyExpr::try_from("test/valgrind/data").unwrap();
    let sub_key_expr = KeyExpr::try_from("test/valgrind/**").unwrap();

    println!("Declaring Publisher on '{pub_key_expr}'...");
    let pub_session = zenoh::open(Config::default()).await.unwrap();
    let publisher = pub_session.declare_publisher(&pub_key_expr).await.unwrap();

    println!("Declaring Subscriber on '{sub_key_expr}'...");
    let sub_session = zenoh::open(Config::default()).await.unwrap();
    let _subscriber = sub_session
        .declare_subscriber(&sub_key_expr)
        .callback(|sample| {
            println!(
                ">> [Subscriber] Received {} ('{}': '{}')",
                sample.kind(),
                sample.key_expr().as_str(),
                sample
                    .payload()
                    .try_to_string()
                    .unwrap_or_else(|e| e.to_string().into())
            );
        })
        .await
        .unwrap();

    for idx in 0..5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let buf = format!("[{idx:4}] data");
        println!("Putting Data ('{}': '{}')...", &pub_key_expr, buf);
        publisher.put(buf).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
}
