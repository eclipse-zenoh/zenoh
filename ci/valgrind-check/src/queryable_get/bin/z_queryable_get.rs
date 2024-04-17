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
use std::convert::TryFrom;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;

#[tokio::main]
async fn main() {
    zenoh_util::init_log_test();

    let _z = zenoh_runtime::ZRuntimePoolGuard;

    let queryable_key_expr = KeyExpr::try_from("test/valgrind/data").unwrap();
    let get_selector = Selector::try_from("test/valgrind/**").unwrap();

    println!("Declaring Queryable on '{queryable_key_expr}'...");
    let queryable_session = zenoh::open(Config::default()).res().await.unwrap();
    let _queryable = queryable_session
        .declare_queryable(&queryable_key_expr.clone())
        .callback(move |query| {
            println!(">> Handling query '{}'", query.selector());
            zenoh_runtime::ZRuntime::Application.block_in_place(async move {
                query
                    .reply(
                        queryable_key_expr.clone(),
                        query.value().unwrap().payload().clone(),
                    )
                    .res()
                    .await
                    .unwrap();
            });
        })
        .complete(true)
        .res()
        .await
        .unwrap();

    println!("Declaring Get session for '{get_selector}'...");
    let get_session = zenoh::open(Config::default()).res().await.unwrap();

    for idx in 0..5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!("Sending Query '{get_selector}'...");
        let replies = get_session
            .get(&get_selector)
            .value(idx)
            .target(QueryTarget::All)
            .res()
            .await
            .unwrap();
        while let Ok(reply) = replies.recv_async().await {
            match reply.result() {
                Ok(sample) => println!(
                    ">> Received ('{}': '{}')",
                    sample.key_expr().as_str(),
                    sample
                        .payload()
                        .deserialize::<String>()
                        .unwrap_or_else(|e| format!("{}", e))
                ),
                Err(err) => println!(
                    ">> Received (ERROR: '{}')",
                    err.payload()
                        .deserialize::<String>()
                        .unwrap_or_else(|e| format!("{}", e))
                ),
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
}
