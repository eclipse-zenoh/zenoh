//
// Copyright (c) 2025 ZettaScale Technology
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

#![cfg(all(feature = "unstable", feature = "internal_config"))]
use core::time::Duration;

use zenoh::{sample::SourceInfo, Session};
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);

async fn create_peer_client_pair(locator: &str) -> (Session, Session) {
    let config1 = {
        let mut config = zenoh::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .listen
            .endpoints
            .set(vec![locator.parse().unwrap()])
            .unwrap();
        config
    };
    let mut config2 = zenoh::Config::default();
    config2.set_mode(Some(WhatAmI::Client)).unwrap();
    config2.scouting.multicast.set_enabled(Some(false)).unwrap();
    config2
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();

    let session1 = zenoh::open(config1).await.unwrap();
    let session2 = zenoh::open(config2).await.unwrap();
    (session1, session2)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_source_info_pub_sub() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/source_info";
    let (session1, session2) = ztimeout!(create_peer_client_pair("tcp/127.0.0.1:51001"));
    let publisher = ztimeout!(session1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let id = session1.id();
    let sn = 15;
    ztimeout!(publisher.put("data").source_info(SourceInfo::new(id, sn))).unwrap();

    let sample = ztimeout!(subscriber.recv_async()).unwrap();
    assert!(sample.source_info().is_some());
    assert_eq!(sample.source_info().unwrap().source_id(), &id);
    assert_eq!(sample.source_info().unwrap().source_sn(), sn);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_source_info_pub_sub_no_source_info() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/no_source_info";
    let (session1, session2) = ztimeout!(create_peer_client_pair("tcp/127.0.0.1:51002"));
    let publisher = ztimeout!(session1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    ztimeout!(publisher.put("data")).unwrap();

    let sample = ztimeout!(subscriber.recv_async()).unwrap();
    assert!(sample.source_info().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_source_info_query_reply() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/source_info";
    let (session1, session2) = ztimeout!(create_peer_client_pair("tcp/127.0.0.1:51003"));
    let queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let id = session1.id();
    let sn = 15;
    let replies = ztimeout!(session1.get(ke).source_info(SourceInfo::new(id, sn))).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();

    assert!(query.source_info().is_some());
    assert_eq!(query.source_info().unwrap().source_id(), &id);
    assert_eq!(query.source_info().unwrap().source_sn(), sn);

    let id = session2.id();
    let sn = 115;
    ztimeout!(query.reply(ke, "data").source_info(SourceInfo::new(id, sn))).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    assert!(reply.result().unwrap().source_info().is_some());
    assert_eq!(
        reply.result().unwrap().source_info().unwrap().source_id(),
        &id
    );
    assert_eq!(
        reply.result().unwrap().source_info().unwrap().source_sn(),
        sn
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_source_info_query_reply_no_source_info() {
    zenoh::init_log_from_env_or("error");
    let ke = "test/no_source_info";
    let (session1, session2) = ztimeout!(create_peer_client_pair("tcp/127.0.0.1:51004"));
    let queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let replies = ztimeout!(session1.get(ke)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();

    assert!(query.source_info().is_none());

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    assert!(reply.result().unwrap().source_info().is_none());
}
