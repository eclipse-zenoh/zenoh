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

//! Closing a session must not wait for a connect still under way, nor for a listener waiting to
//! retry. A peer that accepts the TCP connection and never answers the handshake holds the connect
//! until `open_timeout`, and a listener that cannot bind waits out its retry period. Close used to
//! wait for both.

use std::time::{Duration, Instant};

use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const OPEN_TIMEOUT_MS: u64 = 20_000;
const RETRY_PERIOD_MS: u64 = 20_000;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_does_not_wait_for_a_pending_connect() {
    zenoh::init_log_from_env_or("error");

    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = silent.local_addr().unwrap().port();
    let accepted = tokio::spawn(async move { silent.accept().await.unwrap().0 });

    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config.insert_json5("listen/endpoints", "[]").unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("connect/endpoints", &format!(r#"["tcp/127.0.0.1:{port}"]"#))
        .unwrap();
    // Retried in the background, so open returns while the connect is still under way.
    config.insert_json5("connect/timeout_ms", "-1").unwrap();
    config
        .insert_json5("connect/exit_on_failure", "false")
        .unwrap();
    config
        .insert_json5(
            "transport/unicast/open_timeout",
            &OPEN_TIMEOUT_MS.to_string(),
        )
        .unwrap();

    let session = ztimeout!(zenoh::open(config)).unwrap();
    // The handshake has started and will not finish: the connect now waits on open_timeout.
    let _held = ztimeout!(accepted).unwrap();

    let started = Instant::now();
    ztimeout!(session.close()).unwrap();
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(OPEN_TIMEOUT_MS / 4),
        "close waited {took:?} for a connect that could only end at open_timeout"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_does_not_wait_for_a_listener_retry() {
    zenoh::init_log_from_env_or("error");

    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    // An address this host does not hold, so every attempt to listen on it fails.
    config
        .insert_json5("listen/endpoints", r#"["tcp/8.8.8.8:8"]"#)
        .unwrap();
    // Retried in the background, so open returns while the listener waits for its next attempt.
    config.insert_json5("listen/timeout_ms", "-1").unwrap();
    config
        .insert_json5("listen/exit_on_failure", "false")
        .unwrap();
    config
        .insert_json5(
            "listen/retry",
            &format!("{{ period_init_ms: {RETRY_PERIOD_MS}, period_max_ms: {RETRY_PERIOD_MS} }}"),
        )
        .unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();

    let session = ztimeout!(zenoh::open(config)).unwrap();

    let started = Instant::now();
    ztimeout!(session.close()).unwrap();
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(RETRY_PERIOD_MS / 4),
        "close waited {took:?} for a listener that could only retry after its period"
    );
}
