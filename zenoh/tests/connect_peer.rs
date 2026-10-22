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

//! Tests for `Session::connect_peer`: dialing a peer by locator after open.
#![cfg(feature = "unstable")]

use std::time::{Duration, Instant};

use zenoh::{config::WhatAmI, session::ZenohId, Session, Wait};
use zenoh_config::{Config, EndPoint};
use zenoh_core::ztimeout;
use zenoh_test::{get_tcp_locator, TestSessions};

const TIMEOUT: Duration = Duration::from_secs(60);
const RECV_TIMEOUT: Duration = Duration::from_secs(5);
const SLEEP: Duration = Duration::from_millis(100);

/// A peer with no listener, no connect endpoints and no scouting: the only way
/// it gets a transport is through `connect_peer`.
fn isolated_peer_config() -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.listen.endpoints.set(vec![]).unwrap();
    config
}

async fn peer_zids(session: &Session) -> Vec<ZenohId> {
    session.info().peers_zid().await.collect()
}

async fn wait_for_peer(session: &Session, zid: ZenohId) {
    tokio::time::timeout(RECV_TIMEOUT, async {
        while !peer_zids(session).await.contains(&zid) {
            tokio::time::sleep(SLEEP).await;
        }
    })
    .await
    .expect("timed out waiting for the peer transport");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_peer_dials_a_listener_by_locator() {
    zenoh::init_log_from_env_or("error");
    let mut sessions = TestSessions::new();
    let listener = sessions.open_listener().await;
    let endpoint = get_tcp_locator(&listener).await;
    let dialer = sessions
        .open_connector_with_cfg(isolated_peer_config())
        .await;
    assert!(peer_zids(&dialer).await.is_empty());

    let zid = ztimeout!(dialer
        .connect_peer(endpoint.clone())
        .expected_zid(listener.zid())
        .timeout(Duration::from_secs(5)))
    .unwrap();
    assert_eq!(zid, listener.zid());
    wait_for_peer(&dialer, listener.zid()).await;
    wait_for_peer(&listener, dialer.zid()).await;

    // Data flows over the new transport.
    let subscriber = ztimeout!(listener.declare_subscriber("test/connect_peer")).unwrap();
    tokio::time::sleep(SLEEP * 5).await;
    ztimeout!(dialer.put("test/connect_peer", "hello")).unwrap();
    let sample = tokio::time::timeout(RECV_TIMEOUT, subscriber.recv_async())
        .await
        .expect("no sample received over the dialed transport")
        .unwrap();
    assert_eq!(sample.payload().try_to_string().unwrap(), "hello");

    // With an expected id, an existing transport satisfies the call without a new link.
    let links_before = dialer.info().links().await.count();
    let again = ztimeout!(dialer.connect_peer(endpoint).expected_zid(listener.zid())).unwrap();
    assert_eq!(again, listener.zid());
    assert_eq!(dialer.info().links().await.count(), links_before);

    sessions.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_peer_without_expected_zid_reports_the_remote_id() {
    zenoh::init_log_from_env_or("error");
    let mut sessions = TestSessions::new();
    let listener = sessions.open_listener().await;
    let endpoint = get_tcp_locator(&listener).await;
    let dialer = sessions
        .open_connector_with_cfg(isolated_peer_config())
        .await;

    let zid = ztimeout!(dialer.connect_peer(endpoint)).unwrap();
    assert_eq!(zid, listener.zid());

    sessions.close().await;
}

#[test]
fn connect_peer_wait_dials_a_listener() {
    zenoh::init_log_from_env_or("error");
    let mut sessions = TestSessions::new();
    let listener =
        sessions.open_listener_with_cfg_sync(sessions.get_listener_config("tcp/127.0.0.1:0", 1));
    let endpoint = zenoh_test::get_locators_from_session_sync(&listener)
        .into_iter()
        .find(|e| e.protocol().as_str() == "tcp")
        .unwrap();
    let dialer = sessions.open_connector_with_cfg_sync(isolated_peer_config());

    let zid = dialer
        .connect_peer(endpoint)
        .expected_zid(listener.zid())
        .wait()
        .unwrap();
    assert_eq!(zid, listener.zid());

    sessions.close_sync();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_peer_refuses_a_different_peer() {
    zenoh::init_log_from_env_or("error");
    let mut sessions = TestSessions::new();
    let listener = sessions.open_listener().await;
    let endpoint = get_tcp_locator(&listener).await;
    let dialer = sessions
        .open_connector_with_cfg(isolated_peer_config())
        .await;
    let other = sessions
        .open_connector_with_cfg(isolated_peer_config())
        .await;

    let res = ztimeout!(dialer.connect_peer(endpoint).expected_zid(other.zid()));
    assert!(res.is_err());
    tokio::time::sleep(SLEEP * 5).await;
    assert!(peer_zids(&dialer).await.is_empty());

    sessions.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_peer_timeout_bounds_one_attempt() {
    zenoh::init_log_from_env_or("error");
    // Accepts the TCP connection but never answers the handshake.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", silent.local_addr().unwrap().port())
        .parse()
        .unwrap();
    let mut config = isolated_peer_config();
    config.transport.unicast.set_open_timeout(30_000).unwrap();
    let mut sessions = TestSessions::new();
    let dialer = sessions.open_connector_with_cfg(config).await;

    let attempt = Duration::from_millis(300);
    let start = Instant::now();
    let res = ztimeout!(dialer.connect_peer(endpoint).timeout(attempt));
    let elapsed = start.elapsed();
    assert!(res.is_err());
    assert!(elapsed >= attempt, "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");

    // No background connector is left behind: the silent listener sees no new
    // connection attempts after the call returns.
    silent.set_nonblocking(true).unwrap();
    while silent.accept().is_ok() {}
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(silent.accept().is_err());

    sessions.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_peer_on_a_closed_session_fails() {
    zenoh::init_log_from_env_or("error");
    let session = ztimeout!(zenoh::open(isolated_peer_config())).unwrap();
    ztimeout!(session.close()).unwrap();
    let endpoint: EndPoint = "tcp/127.0.0.1:1".parse().unwrap();

    assert!(ztimeout!(session.connect_peer(endpoint)).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connect_peer_is_refused_in_client_mode() {
    zenoh::init_log_from_env_or("error");
    let mut sessions = TestSessions::new();
    let listener = sessions.open_listener().await;
    let endpoint = get_tcp_locator(&listener).await;
    let mut config = isolated_peer_config();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
        .connect
        .endpoints
        .set(vec![endpoint.clone().into()])
        .unwrap();
    let client = sessions.open_connector_with_cfg(config).await;

    let err = ztimeout!(client.connect_peer(endpoint)).unwrap_err();
    assert!(err.to_string().contains("client mode"), "{err}");

    sessions.close().await;
}
