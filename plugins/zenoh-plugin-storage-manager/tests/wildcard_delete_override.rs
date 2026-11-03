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

// Regression test for a retained wildcard Delete overriding a stale Put.
//
// Scenario:
//   1. PUT  a/k = V1 @ T1
//   2. DELETE a/** @ T3            (T3 > T1, registered as a retained wildcard delete)
//   3. PUT  a/k = V2 @ T2          (T1 < T2 < T3, delivered *after* the delete)
//   4. GET  a/k
//
// Because the late Put (T2) is older than the retained wildcard Delete (T3), the
// wildcard-delete tombstone must override it and the key must remain absent.
//
// Previously, `process_sample` dispatched on the *original* sample kind (Put) rather
// than on the kind of the override-resolved `sample_to_store` (Delete), so the stale
// Put was stored as a `put` of the override's empty payload at the delete's timestamp,
// resurrecting the key.

use std::{str::FromStr, thread::sleep};

use tokio::runtime::Runtime;
use zenoh::{query::Reply, sample::Sample, time::Timestamp, Config, Session};
use zenoh_plugin_trait::Plugin;

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

async fn test_wildcard_delete_overrides_stale_put() {
    let mut config = Config::default();
    config
        .insert_json5(
            "plugins/storage-manager",
            r#"{
                    storages: {
                        wild_delete_test: {
                            key_expr: "wilddelete/test/**",
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

    // Three explicit timestamps sharing the same source id, strictly increasing:
    // T1 < T2 < T3. We deliver them in the order T1 (put), T3 (wildcard delete),
    // T2 (stale put) to exercise the override path deterministically (no wall-clock
    // race).
    let t1 =
        Timestamp::from_str("7054123566570568799/BC779A06D7E049BD88C3FF3DB0C17FCC").unwrap();
    let t2 =
        Timestamp::from_str("7054123570865536095/BC779A06D7E049BD88C3FF3DB0C17FCC").unwrap();
    let t3 =
        Timestamp::from_str("7054123832858541151/BC779A06D7E049BD88C3FF3DB0C17FCC").unwrap();
    assert!(t1 < t2 && t2 < t3);

    // 1. PUT a/k = V1 @ T1
    println!("PUT wilddelete/test/a = V1 @ T1");
    session
        .put("wilddelete/test/a", "V1")
        .timestamp(t1)
        .await
        .unwrap();
    sleep(std::time::Duration::from_millis(50));

    // The key is present after the first put.
    let data = get_data(&session, "wilddelete/test/a").await;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].payload().try_to_string().unwrap(), "V1");

    // 2. DELETE a/** @ T3 (retained wildcard delete, newer than everything)
    println!("DELETE wilddelete/test/** @ T3");
    session
        .delete("wilddelete/test/**")
        .timestamp(t3)
        .await
        .unwrap();
    sleep(std::time::Duration::from_millis(50));

    // The wildcard delete removed the key.
    let data = get_data(&session, "wilddelete/test/a").await;
    assert_eq!(data.len(), 0);

    // 3. PUT a/k = V2 @ T2, delivered AFTER the delete but with an OLDER timestamp.
    println!("PUT wilddelete/test/a = V2 @ T2 (stale, after the wildcard delete)");
    session
        .put("wilddelete/test/a", "V2")
        .timestamp(t2)
        .await
        .unwrap();
    sleep(std::time::Duration::from_millis(50));

    // 4. GET a/k -- the retained wildcard delete (T3) is newer than the stale put
    // (T2), so the key must remain absent. The buggy code resurrected it (stored a
    // `put` of the override's empty payload at T3).
    let data = get_data(&session, "wilddelete/test/a").await;
    assert_eq!(
        data.len(),
        0,
        "stale Put (T2) must stay suppressed by the newer retained wildcard Delete (T3), \
         but the key was resurrected: {data:?}"
    );

    drop(storage);
}

#[test]
fn wildcard_delete_overrides_stale_put_test() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { test_wildcard_delete_overrides_stale_put().await });
}
