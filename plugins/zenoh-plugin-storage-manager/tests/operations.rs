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

// Test wild card updates -
// 1. normal case, just some wild card puts and deletes on existing keys and ensure it works
// 2. check for dealing with out of order updates

use std::{str::FromStr, thread::sleep};

use tokio::runtime::Runtime;
use zenoh::{
    internal::zasync_executor_init, query::Reply, sample::Sample, time::Timestamp, Config, Session,
};
use zenoh_plugin_trait::Plugin;

async fn put_data(session: &Session, key_expr: &str, value: &str, _timestamp: Timestamp) {
    println!("Putting Data ('{key_expr}': '{value}')...");
    //  @TODO: how to add timestamp metadata with put, not manipulating sample...
    session.put(key_expr, value).await.unwrap();
}

async fn delete_data(session: &Session, key_expr: &str, _timestamp: Timestamp) {
    println!("Deleting Data '{key_expr}'...");
    //  @TODO: how to add timestamp metadata with delete, not manipulating sample...
    session.delete(key_expr).await.unwrap();
}

async fn get_data(session: &Session, key_expr: &str) -> Vec<Sample> {
    let replies: Vec<Reply> = session.get(key_expr).await.unwrap().into_iter().collect();
    println!("Getting replies on '{key_expr}': '{replies:?}'...");
    let mut samples = Vec::new();
    for reply in replies {
        if let Ok(sample) = reply.into_result() {
            samples.push(sample);
        }
    }
    println!("Getting Data on '{key_expr}': '{samples:?}'...");
    samples
}

async fn test_updates_in_order() {
    async {
        zasync_executor_init!();
    }
    .await;
    let mut config = Config::default();
    config
        .insert_json5(
            "plugins/storage-manager",
            r#"{
                    storages: {
                        operation_test: {
                            key_expr: "operation/test/**",
                            volume: {
                                id: "memory"
                            }
                        }
                    }
                }"#,
        )
        .unwrap();
    config
        .insert_json5(
            "timestamping",
            r#"{
                    enabled: {
                        router: true,
                        peer: true,
                        client: true
                    }
                }"#,
        )
        .unwrap();

    let runtime = zenoh::internal::runtime::RuntimeBuilder::new(config)
        .build()
        .await
        .unwrap()
        .into();
    let storage =
        zenoh_plugin_storage_manager::StoragesPlugin::start("storage-manager", &runtime).unwrap();

    let session = zenoh::session::init(runtime).await.unwrap();

    sleep(std::time::Duration::from_secs(1));

    put_data(
        &session,
        "operation/test/a",
        "1",
        Timestamp::from_str("7054123566570568799/BC779A06D7E049BD88C3FF3DB0C17FCC").unwrap(),
    )
    .await;

    sleep(std::time::Duration::from_millis(10));

    // expects exactly one sample
    let data = get_data(&session, "operation/test/a").await;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].payload().try_to_string().unwrap(), "1");

    put_data(
        &session,
        "operation/test/b",
        "2",
        Timestamp::from_str("7054123824268606559/BC779A06D7E049BD88C3FF3DB0C17FCC").unwrap(),
    )
    .await;

    sleep(std::time::Duration::from_millis(10));

    // expects exactly one sample
    let data = get_data(&session, "operation/test/b").await;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].payload().try_to_string().unwrap(), "2");

    delete_data(
        &session,
        "operation/test/a",
        Timestamp::from_str("7054123824268606559/BC779A06D7E049BD88C3FF3DB0C17FCC").unwrap(),
    )
    .await;

    sleep(std::time::Duration::from_millis(10));

    // expects zero sample
    let data = get_data(&session, "operation/test/a").await;
    assert_eq!(data.len(), 0);

    // expects exactly one sample
    let data = get_data(&session, "operation/test/b").await;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].payload().try_to_string().unwrap(), "2");
    assert_eq!(data[0].key_expr().as_str(), "operation/test/b");

    drop(storage);
}

#[test]
fn updates_test() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { test_updates_in_order().await });
}
