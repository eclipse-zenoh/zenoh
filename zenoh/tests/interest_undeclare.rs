//
// Copyright (c) 2026 ZettaScale Technology
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

//! A client's publisher declares an interest in its key's subscribers. Undeclaring the publisher
//! must not leave the client believing that interest is still finalized: the router no longer
//! sends it subscriber declarations, so later puts on the key would be dropped.

use std::time::Duration;

use zenoh::Session;
use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;
use zenoh_test::TestSessions;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(200);

async fn routed_clients(test_context: &mut TestSessions) -> (Session, Session) {
    let mut router_config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    router_config.set_mode(Some(WhatAmI::Router)).unwrap();
    test_context.open_listener_with_cfg(router_config).await;

    let mut clients = Vec::new();
    for _ in 0..2 {
        let mut config = test_context.get_connector_config();
        config.set_mode(Some(WhatAmI::Client)).unwrap();
        clients.push(test_context.open_connector_with_cfg(config).await);
    }
    let publisher_session = clients.remove(0);
    (publisher_session, clients.remove(0))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn put_reaches_new_subscriber_after_publisher_undeclared() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();
    let (a, b) = routed_clients(&mut test_context).await;
    let key = "test/interest_undeclare";

    let first_subscriber = ztimeout!(b.declare_subscriber(key)).unwrap();
    tokio::time::sleep(SLEEP).await;

    // the router answers the publisher's subscriber interest (DeclareFinal) before it is undeclared
    let publisher = ztimeout!(a.declare_publisher(key)).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.undeclare()).unwrap();

    ztimeout!(first_subscriber.undeclare()).unwrap();
    let subscriber = ztimeout!(b.declare_subscriber(key)).unwrap();
    tokio::time::sleep(SLEEP).await;

    ztimeout!(a.put(key, "hello")).unwrap();
    let sample = tokio::time::timeout(Duration::from_secs(5), subscriber.recv_async())
        .await
        .expect("the put never reached the subscriber")
        .unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap(), "hello");

    test_context.close().await;
}
