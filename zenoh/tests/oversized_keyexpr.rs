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

#![cfg(feature = "transport_tcp")]

//! A key expression that does not fit in the `u16` length prefix of the wire format must be
//! rejected with an error, before the session sends anything or delivers it to a local subscriber.
//! Otherwise the transport cannot encode the message and aborts the process.

use std::time::Duration;

use zenoh::{sample::Locality, Session};
use zenoh_core::ztimeout;
use zenoh_test::TestSessions;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MAX_LEN: usize = u16::MAX as usize;

async fn assert_session_usable(local: &Session, remote: &Session) {
    let subscriber = ztimeout!(remote.declare_subscriber("**")).unwrap();
    tokio::time::sleep(SLEEP).await;
    ztimeout!(local.put("test/oversized", "ok")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap(), "ok");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn put_with_oversized_keyexpr_is_rejected() {
    zenoh::init_log_from_env_or("error");

    let mut test_context = TestSessions::new();
    let (remote, local) = test_context.open_pairs().await;

    let remote_subscriber = ztimeout!(remote.declare_subscriber("**")).unwrap();
    let local_subscriber = ztimeout!(local.declare_subscriber("**")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let key_expr = "a".repeat(MAX_LEN + 1);
    assert!(ztimeout!(local.put(&key_expr, "x")).is_err());
    assert!(ztimeout!(local.delete(&key_expr)).is_err());

    tokio::time::sleep(SLEEP).await;
    assert!(remote_subscriber.try_recv().unwrap().is_none());
    assert!(local_subscriber.try_recv().unwrap().is_none());

    assert_session_usable(&local, &remote).await;
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn declare_subscriber_with_oversized_keyexpr_is_rejected() {
    zenoh::init_log_from_env_or("error");

    let mut test_context = TestSessions::new();
    let (remote, local) = test_context.open_pairs().await;

    let key_expr = "a".repeat(MAX_LEN + 1);
    assert!(ztimeout!(local.declare_subscriber(&key_expr)).is_err());

    assert_session_usable(&local, &remote).await;
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn other_operations_with_oversized_keyexpr_are_rejected() {
    zenoh::init_log_from_env_or("error");

    let mut test_context = TestSessions::new();
    let (remote, local) = test_context.open_pairs().await;

    let key_expr = "a".repeat(MAX_LEN + 1);
    assert!(ztimeout!(local.declare_keyexpr(&key_expr)).is_err());
    assert!(ztimeout!(local.declare_queryable(&key_expr)).is_err());
    assert!(ztimeout!(local.declare_publisher(&key_expr)).is_err());
    assert!(ztimeout!(local.declare_querier(&key_expr)).is_err());
    assert!(ztimeout!(local.get(&key_expr)).is_err());
    assert!(ztimeout!(local.liveliness().declare_token(&key_expr)).is_err());
    assert!(ztimeout!(local.liveliness().declare_subscriber(&key_expr)).is_err());
    assert!(ztimeout!(local.liveliness().get(&key_expr)).is_err());

    assert_session_usable(&local, &remote).await;
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reply_with_oversized_keyexpr_is_rejected() {
    zenoh::init_log_from_env_or("error");

    let mut test_context = TestSessions::new();
    let (remote, local) = test_context.open_pairs().await;

    let queryable = ztimeout!(remote.declare_queryable("**")).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(local.get("**")).unwrap();
    let query = ztimeout!(queryable.recv_async()).unwrap();
    let key_expr = "a".repeat(MAX_LEN + 1);
    assert!(ztimeout!(query.reply(&key_expr, "x")).is_err());
    drop(query);
    assert!(ztimeout!(replies.recv_async()).is_err());

    assert_session_usable(&local, &remote).await;
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_local_oversized_keyexpr_is_accepted() {
    zenoh::init_log_from_env_or("error");

    let mut test_context = TestSessions::new();
    let (remote, local) = test_context.open_pairs().await;

    // Nothing is sent on the wire, so the length prefix does not apply
    let key_expr = "a".repeat(MAX_LEN + 1);
    let subscriber = ztimeout!(local
        .declare_subscriber(&key_expr)
        .allowed_origin(Locality::SessionLocal))
    .unwrap();
    ztimeout!(local
        .put(&key_expr, "x")
        .allowed_destination(Locality::SessionLocal))
    .unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();
    assert_eq!(sample.key_expr().as_str(), key_expr);

    assert_session_usable(&local, &remote).await;
    test_context.close().await;
}

#[cfg(feature = "unstable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn put_with_keyexpr_oversized_by_namespace_is_rejected() {
    use zenoh_macros::nonwild_ke;

    zenoh::init_log_from_env_or("error");

    let mut test_context = TestSessions::new();
    let remote = test_context.open_listener().await;
    let mut config = test_context.get_connector_config();
    config.namespace = Some(nonwild_ke!("ns").to_owned());
    let local = test_context.open_connector_with_cfg(config).await;

    let _remote_subscriber = ztimeout!(remote.declare_subscriber("**")).unwrap();
    tokio::time::sleep(SLEEP).await;

    // Fits in the length prefix on its own, but not once the namespace is prepended
    let key_expr = "a".repeat(MAX_LEN);
    assert!(ztimeout!(local.put(&key_expr, "x")).is_err());

    assert_session_usable(&local, &remote).await;
    test_context.close().await;
}
