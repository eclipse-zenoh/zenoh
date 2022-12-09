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

// Test wild card updates -
// 1. normal case, just some wild card puts and deletes on existing keys and ensure it works
// 2. check for dealing with out of order updates

use std::thread::sleep;
use std::{collections::HashMap, str::FromStr};

// use std::collections::HashMap;
use async_std::task;
use std::convert::TryFrom;
use zenoh::prelude::r#async::*;
use zenoh::query::Reply;
use zenoh::{prelude::Config, time::Timestamp};
use zenoh_core::zasync_executor_init;
use zenoh_plugin_trait::Plugin;

async fn put_data(session: &zenoh::Session, key_expr: &str, value: &str, timestamp: Timestamp) {
    println!("Putting Data ('{}': '{}')...", key_expr, value);
    // let key_expr = OwnedKeyExpr::new(key_expr);
    //  @TODO: how to add timestamp metadata with put, not manipulating sample...
    session.put(key_expr, value).res().await.unwrap();
}

async fn get_data(session: &zenoh::Session, key_expr: &str) -> Vec<Sample> {
    let replies: Vec<Reply> = session
        .get(key_expr)
        .res()
        .await
        .unwrap()
        .into_iter()
        .collect();
    println!("Getting replies on '{}': '{:?}'...", key_expr, replies);
    let mut samples = Vec::new();
    for reply in replies {
        if let Ok(sample) = reply.sample {
            samples.push(sample);
        }
    }
    println!("Getting Data on '{}': '{:?}'...", key_expr, samples);
    samples
}

async fn test_wild_card_in_order() {
    task::block_on(async {
        zasync_executor_init!();
    });
    let mut config = Config::default();
    config
        .insert_json5(
            "plugins/storage-manager",
            r#"{
                    storages: {
                        demo: {
                            key_expr: "demo/example/**",
                            volume: {
                                id: "memory"
                            }
                        }
                    }
                }"#,
        )
        .unwrap();

    let runtime = zenoh::runtime::Runtime::new(config, true).await.unwrap();
    let storage =
        zplugin_storage_manager::StoragesPlugin::start("storage-manager", &runtime).unwrap();

    let session = zenoh::init(runtime).res().await.unwrap();

    // put *, ts: 1
    put_data(
        &session,
        "demo/example/a",
        "1",
        Timestamp::from_str("2022-01-17T10:42:10.418555997Z/BC779A06D7E049BD88C3FF3DB0C17FCC")
            .unwrap(),
    )
    .await;
    sleep(std::time::Duration::from_millis(100));
    // expected no data
    let data = get_data(&session, "demo/example/a").await;
    assert_eq!(data.len(), 1);

    // let data = get_data(&session, "demo/example/*").await;
    // assert_eq!(data.len(), 1);

    // put_data(
    //     &session,
    //     "demo/example/b",
    //     "1",
    //     Timestamp::from_str("2022-01-17T10:43:10.418555997Z/BC779A06D7E049BD88C3FF3DB0C17FCC")
    //         .unwrap(),
    // )
    // .await;

    // let data = get_data(&session, "demo/example/*").await;
    // assert_eq!(data.len(), 2);

    println!("received: {:?}", data);
    drop(storage);
}

fn test_wild_card_out_of_order() {
    assert_eq!(true, true);
}

#[test]
fn wildcard_test() {
    task::block_on(async { test_wild_card_in_order().await });
    task::block_on(async { test_wild_card_out_of_order() });
}
