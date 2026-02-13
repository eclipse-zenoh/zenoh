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

#![cfg(feature = "unstable")]
use core::time::Duration;
use std::sync::{atomic::AtomicBool, Arc};

use zenoh::{handlers::CallbackDrop, Session};
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);

async fn create_peer_client_pair() -> (Session, Session) {
    // Create a session that listens on an auto assigned TCP port
    let config1 = {
        let mut config = zenoh::Config::default();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        config
            .listen
            .endpoints
            .set(vec!["tcp/127.0.0.1:0".parse().unwrap()])
            .unwrap();
        config
    };
    let session1 = zenoh::open(config1).await.unwrap();

    // Extract the actual TCP endpoint that session1 is listening on
    let locator = session1
        .info()
        .locators()
        .await
        .into_iter()
        .find(|l| l.protocol().as_str() == "tcp")
        .expect("Expected at least one TCP locator")
        .to_string();

    // Create a session that connects to the first session
    let mut config2 = zenoh::Config::default();
    config2.set_mode(Some(WhatAmI::Client)).unwrap();
    config2.scouting.multicast.set_enabled(Some(false)).unwrap();
    config2
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();

    let session2 = zenoh::open(config2).await.unwrap();
    (session1, session2)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cancellation_get() {
    zenoh::init_log_from_env_or("error");
    let (session1, session2) = ztimeout!(create_peer_client_pair());
    let queryable = ztimeout!(session1.declare_queryable("test/query_cancellation")).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let cancellation_token = zenoh::cancellation::CancellationToken::default();

    let replies = ztimeout!(session2
        .get("test/query_cancellation")
        .cancellation_token(cancellation_token.clone()))
    .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    cancellation_token.cancel().await.unwrap();
    assert!(replies.is_disconnected());
    ztimeout!(queryable
        .recv()
        .unwrap()
        .reply("test/query_cancellation", "ok"))
    .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let cancellation_token = zenoh::cancellation::CancellationToken::default();
    let n = Arc::new(AtomicBool::new(false));
    let n_clone = n.clone();
    ztimeout!(session2
        .get("test/query_cancellation")
        .cancellation_token(cancellation_token.clone())
        .callback(move |_| {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        }))
    .unwrap();
    ztimeout!(queryable
        .recv()
        .unwrap()
        .reply("test/query_cancellation", "ok"))
    .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    cancellation_token.cancel().await.unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check that cancelled token cancels operation automatically
    let n = Arc::new(AtomicBool::new(false));
    let n_clone = n.clone();
    let cb = CallbackDrop {
        callback: |_| {},
        drop: move || {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        },
    };
    assert!(cancellation_token.is_cancelled());
    assert!(ztimeout!(session2
        .get("test/query_cancellation")
        .cancellation_token(cancellation_token.clone())
        .with(cb))
    .is_err());
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cancellation_liveliness_get() {
    zenoh::init_log_from_env_or("error");
    let (session1, session2) = ztimeout!(create_peer_client_pair());
    let _token = ztimeout!(session1
        .liveliness()
        .declare_token("test/liveliness_query_cancellation"))
    .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let cancellation_token = zenoh::cancellation::CancellationToken::default();
    let n = Arc::new(AtomicBool::new(false));
    let n_clone = n.clone();

    ztimeout!(session2
        .liveliness()
        .get("test/liveliness_query_cancellation")
        .cancellation_token(cancellation_token.clone())
        .callback(move |_| {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        }))
    .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    cancellation_token.cancel().await.unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check that cancelled token cancels operation automatically
    let n = Arc::new(AtomicBool::new(false));
    let n_clone = n.clone();
    let cb = CallbackDrop {
        callback: |_| {},
        drop: move || {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        },
    };
    assert!(cancellation_token.is_cancelled());
    assert!(ztimeout!(session2
        .liveliness()
        .get("test/liveliness_query_cancellation")
        .cancellation_token(cancellation_token.clone())
        .with(cb))
    .is_err());
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cancellation_querier_get() {
    zenoh::init_log_from_env_or("error");
    let (session1, session2) = ztimeout!(create_peer_client_pair());
    let queryable = ztimeout!(session1.declare_queryable("test/querier_cancellation")).unwrap();

    let querier = ztimeout!(session2.declare_querier("test/querier_cancellation")).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let cancellation_token = zenoh::cancellation::CancellationToken::default();

    let replies = ztimeout!(querier.get().cancellation_token(cancellation_token.clone())).unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    cancellation_token.cancel().await.unwrap();
    assert!(replies.is_disconnected());
    ztimeout!(queryable
        .recv()
        .unwrap()
        .reply("test/querier_cancellation", "ok"))
    .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let cancellation_token = zenoh::cancellation::CancellationToken::default();
    let n = Arc::new(AtomicBool::new(false));
    let n_clone = n.clone();
    ztimeout!(querier
        .get()
        .cancellation_token(cancellation_token.clone())
        .callback(move |_| {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        }))
    .unwrap();
    ztimeout!(queryable
        .recv()
        .unwrap()
        .reply("test/querier_cancellation", "ok"))
    .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    cancellation_token.cancel().await.unwrap();
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));

    // check that cancelled token cancels operation automatically
    assert!(cancellation_token.is_cancelled());
    let n = Arc::new(AtomicBool::new(false));
    let n_clone = n.clone();
    let cb = CallbackDrop {
        callback: |_| {},
        drop: move || {
            std::thread::sleep(Duration::from_secs(5));
            n_clone.fetch_or(true, std::sync::atomic::Ordering::SeqCst);
        },
    };
    assert!(ztimeout!(querier
        .get()
        .cancellation_token(cancellation_token.clone())
        .with(cb))
    .is_err());
    assert!(n.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_cancellation_does_not_prevent_session_from_close() {
    zenoh::init_log_from_env_or("error");
    let (session1, session2) = ztimeout!(create_peer_client_pair());
    let cancellation_token = zenoh::cancellation::CancellationToken::default();

    let ke = "test/query_cancellation_does_not_prevent_session_from_close";
    let _queryable = ztimeout!(session1.declare_queryable(ke)).unwrap();

    let querier = ztimeout!(session2.declare_querier(ke)).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let replies = ztimeout!(session2
        .get(ke)
        .cancellation_token(cancellation_token.clone()))
    .unwrap();

    let replies2 = ztimeout!(querier.get().cancellation_token(cancellation_token.clone())).unwrap();

    std::mem::drop(session2);
    assert!(replies.is_disconnected());
    assert!(replies2.is_disconnected());

    ztimeout!(cancellation_token.cancel()).unwrap();
}
