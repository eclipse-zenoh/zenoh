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

use std::{sync::Arc, time::Duration};

use zenoh::{
    config::WhatAmI,
    query::ConsolidationMode,
    timestamp_stack::{
        GetTimestampCallback, InstrumentationTimestamp, InterceptionPoint,
        TimestampInstrumentation, TimestampInstrumentationBuilder,
    },
};
use zenoh_core::ztimeout;
use zenoh_test::TestSessions;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

// ─── Helpers ───────────────────────────────────────────────────────────────

fn make_instrumentation(send: bool, route: bool, receive: bool) -> TimestampInstrumentation {
    TimestampInstrumentationBuilder::new()
        .set_send(send)
        .set_route(route)
        .set_receive(receive)
        .build()
        .unwrap()
}

async fn open_peer_sessions(test_context: &mut TestSessions) -> (zenoh::Session, zenoh::Session) {
    let mut config_peer1 = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    config_peer1.set_mode(Some(WhatAmI::Peer)).unwrap();
    let peer1 = test_context.open_listener_with_cfg(config_peer1).await;

    let mut config_peer2 = test_context.get_connector_config();
    config_peer2.set_mode(Some(WhatAmI::Peer)).unwrap();
    let peer2 = test_context.open_connector_with_cfg(config_peer2).await;

    tokio::time::sleep(SLEEP).await;
    (peer1, peer2)
}

async fn open_routed_sessions(
    test_context: &mut TestSessions,
) -> (zenoh::Session, zenoh::Session, zenoh::Session) {
    let mut config_router = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    config_router.set_mode(Some(WhatAmI::Router)).unwrap();
    let router = test_context.open_listener_with_cfg(config_router).await;
    tokio::time::sleep(SLEEP).await;

    let mut config_client1 = test_context.get_connector_config();
    config_client1.set_mode(Some(WhatAmI::Client)).unwrap();
    let client1 = test_context.open_connector_with_cfg(config_client1).await;

    let mut config_client2 = test_context.get_connector_config();
    config_client2.set_mode(Some(WhatAmI::Client)).unwrap();
    let client2 = test_context.open_connector_with_cfg(config_client2).await;

    tokio::time::sleep(SLEEP).await;
    (client1, client2, router)
}

fn assert_record(
    record: &zenoh::timestamp_stack::TimestampStackRecord,
    expected_point: InterceptionPoint,
    expected_custom: bool,
) {
    assert_eq!(record.point(), expected_point);
    assert_eq!(record.is_custom(), expected_custom);
    match record.timestamp() {
        InstrumentationTimestamp::Custom(ts_bytes) => {
            assert!(!ts_bytes.is_empty(), "timestamp bytes should not be empty")
        }
        InstrumentationTimestamp::UHLC(_timestamp) => (),
    }
}

fn assert_custom_ts_record(
    record: &zenoh::timestamp_stack::InstrumentationTimestamp,
    content: &[u8],
) {
    match record {
        InstrumentationTimestamp::UHLC(_) => panic!("expected custom timestamp, found UHLC"),
        InstrumentationTimestamp::Custom(ts) => assert_eq!(ts, content),
    }
}

// ─── Builder & Unit Tests ───────────────────────────────────────────────

#[test]
fn builder_no_points_active() {
    let result = TimestampInstrumentationBuilder::new().build();
    assert!(result.is_err());
}

#[test]
fn builder_send_only() {
    let instr = TimestampInstrumentationBuilder::new()
        .set_send(true)
        .build()
        .unwrap();
    assert!(instr.is_instrumented(InterceptionPoint::Send));
    assert!(!instr.is_instrumented(InterceptionPoint::Route));
    assert!(!instr.is_instrumented(InterceptionPoint::Receive));
}

#[test]
fn builder_route_only() {
    let instr = TimestampInstrumentationBuilder::new()
        .set_route(true)
        .build()
        .unwrap();
    assert!(!instr.is_instrumented(InterceptionPoint::Send));
    assert!(instr.is_instrumented(InterceptionPoint::Route));
    assert!(!instr.is_instrumented(InterceptionPoint::Receive));
}

#[test]
fn builder_receive_only() {
    let instr = TimestampInstrumentationBuilder::new()
        .set_receive(true)
        .build()
        .unwrap();
    assert!(!instr.is_instrumented(InterceptionPoint::Send));
    assert!(!instr.is_instrumented(InterceptionPoint::Route));
    assert!(instr.is_instrumented(InterceptionPoint::Receive));
}

#[test]
fn builder_all_points() {
    let instr = TimestampInstrumentationBuilder::new()
        .set_send(true)
        .set_route(true)
        .set_receive(true)
        .build()
        .unwrap();
    assert!(instr.is_instrumented(InterceptionPoint::Send));
    assert!(instr.is_instrumented(InterceptionPoint::Route));
    assert!(instr.is_instrumented(InterceptionPoint::Receive));
}

#[test]
fn builder_set_unset() {
    let instr = TimestampInstrumentationBuilder::new()
        .set_send(true)
        .set_send(false)
        .set_route(true)
        .build()
        .unwrap();
    assert!(!instr.is_instrumented(InterceptionPoint::Send));
    assert!(instr.is_instrumented(InterceptionPoint::Route));
    assert!(!instr.is_instrumented(InterceptionPoint::Receive));
}

#[test]
fn interception_point_try_from_valid() {
    use zenoh_protocol::network::timestamp_stack::interception_point;
    let send: InterceptionPoint = interception_point::SEND.try_into().unwrap();
    assert_eq!(send, InterceptionPoint::Send);
    let route: InterceptionPoint = interception_point::ROUTE.try_into().unwrap();
    assert_eq!(route, InterceptionPoint::Route);
    let receive: InterceptionPoint = interception_point::RECEIVE.try_into().unwrap();
    assert_eq!(receive, InterceptionPoint::Receive);
}

#[test]
fn interception_point_try_from_invalid() {
    let result: Result<InterceptionPoint, _> = 0xFFu8.try_into();
    assert!(result.is_err());
}

// ─── Records Content Verification ───────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn records_non_empty_timestamp_bytes() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/records/non_empty_ts";
    let instr = make_instrumentation(true, false, false);

    let session = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    for record in ts_stack.records() {
        assert!(
            matches!(record.timestamp(), InstrumentationTimestamp::UHLC(_)),
            "UHLC timestamp should be valid"
        );
    }

    session.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn records_order_matters() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/records/order";
    let instr = make_instrumentation(true, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 5);
    // Verify exact order: Send → Route → Route → Route → Receive
    assert_eq!(ts_stack.records()[0].point(), InterceptionPoint::Send);
    for i in 1..4 {
        assert_eq!(ts_stack.records()[i].point(), InterceptionPoint::Route);
    }
    assert_eq!(ts_stack.records()[4].point(), InterceptionPoint::Receive);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn instrumentation_config_preserved() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/records/config_preserved";
    let instr = make_instrumentation(true, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    let preserved_instr = ts_stack.instrumentation();
    assert!(preserved_instr.is_instrumented(InterceptionPoint::Send));
    assert!(preserved_instr.is_instrumented(InterceptionPoint::Route));
    assert!(preserved_instr.is_instrumented(InterceptionPoint::Receive));

    test_context.close().await;
}

// ─── Pub/Sub — Same Session ─────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_same_session_send_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/same_session/send_only";
    let instr = make_instrumentation(true, false, false);

    let session = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);

    session.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_same_session_receive_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/same_session/receive_only";
    let instr = make_instrumentation(false, false, true);

    let session = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Receive, false);

    session.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_same_session_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/same_session/send_receive";
    let instr = make_instrumentation(true, false, true);

    let session = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    session.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_same_session_no_instrumentation() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/same_session/no_instr";

    let session = ztimeout!(zenoh::open(zenoh::Config::default())).unwrap();
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert!(
        sample.timestamp_stack().is_none(),
        "timestamp_stack should be None when no instrumentation"
    );

    session.close().await.unwrap();
}

// ─── Pub/Sub — Local + Remote Delivery (destination=Any) ────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_local_and_remote_no_duplicate_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/pubsub/local_and_remote";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    // Publisher on peer1 with destination=Any (default)
    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    // Local subscriber on peer1
    let local_sub = ztimeout!(peer1.declare_subscriber(ke)).unwrap();
    // Remote subscriber on peer2
    let remote_sub = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();

    // Local subscriber should get exactly 1 RECEIVE (not duplicated)
    let local_sample = ztimeout!(local_sub.recv_async()).unwrap();
    let local_ts_stack = local_sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        local_ts_stack.records().len(),
        2,
        "local subscriber should have exactly SEND + RECEIVE (no duplicate RECEIVE)"
    );
    assert_record(&local_ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(
        &local_ts_stack.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    // Remote subscriber should get exactly 1 RECEIVE
    let remote_sample = ztimeout!(remote_sub.recv_async()).unwrap();
    let remote_ts_stack = remote_sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        remote_ts_stack.records().len(),
        2,
        "remote subscriber should have exactly SEND + RECEIVE"
    );
    assert_record(
        &remote_ts_stack.records()[0],
        InterceptionPoint::Send,
        false,
    );
    assert_record(
        &remote_ts_stack.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_local_only_destination() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/pubsub/local_only_destination";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    // Publisher on peer1 with destination=SessionLocal
    let publisher = ztimeout!(peer1
        .declare_publisher(ke)
        .allowed_destination(zenoh::sample::Locality::SessionLocal))
    .unwrap();
    // Local subscriber on peer1
    let local_sub = ztimeout!(peer1.declare_subscriber(ke)).unwrap();
    // Remote subscriber on peer2 (should NOT receive)
    let remote_sub = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();

    // Local subscriber should get exactly 1 RECEIVE
    let local_sample = ztimeout!(local_sub.recv_async()).unwrap();
    let local_ts_stack = local_sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        local_ts_stack.records().len(),
        2,
        "local subscriber should have exactly SEND + RECEIVE"
    );
    assert_record(&local_ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(
        &local_ts_stack.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    // Remote subscriber should NOT receive anything
    let remote_result = tokio::time::timeout(Duration::from_secs(2), remote_sub.recv_async()).await;
    assert!(
        remote_result.is_err(),
        "remote subscriber should not receive when destination=SessionLocal"
    );

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_remote_only_destination() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/pubsub/remote_only_destination";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    // Publisher on peer1 with destination=Remote
    let publisher = ztimeout!(peer1
        .declare_publisher(ke)
        .allowed_destination(zenoh::sample::Locality::Remote))
    .unwrap();
    // Local subscriber on peer1 (should NOT receive)
    let local_sub = ztimeout!(peer1.declare_subscriber(ke)).unwrap();
    // Remote subscriber on peer2
    let remote_sub = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();

    // Remote subscriber should get exactly 1 RECEIVE
    let remote_sample = ztimeout!(remote_sub.recv_async()).unwrap();
    let remote_ts_stack = remote_sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        remote_ts_stack.records().len(),
        2,
        "remote subscriber should have exactly SEND + RECEIVE"
    );
    assert_record(
        &remote_ts_stack.records()[0],
        InterceptionPoint::Send,
        false,
    );
    assert_record(
        &remote_ts_stack.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    // Local subscriber should NOT receive anything
    let local_result = tokio::time::timeout(Duration::from_secs(2), local_sub.recv_async()).await;
    assert!(
        local_result.is_err(),
        "local subscriber should not receive when destination=Remote"
    );

    test_context.close().await;
}

// ─── Pub/Sub — Peer-Peer ─────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_p2p_send_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/peer_peer/send_only";
    let instr = make_instrumentation(true, false, false);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_p2p_receive_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/peer_peer/receive_only";
    let instr = make_instrumentation(false, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_p2p_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/peer_peer/send_receive";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_p2p_route_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/peer_peer/route_only";
    let instr = make_instrumentation(false, true, false);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    // In peer-peer, message goes through routing layer on both peers → 2 ROUTE records
    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert!(ts_stack
        .instrumentation()
        .is_instrumented(InterceptionPoint::Route));
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Route, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Route, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_p2p_no_instrumentation() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/peer_peer/no_instr";

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert!(
        sample.timestamp_stack().is_none(),
        "timestamp_stack should be None when no instrumentation"
    );

    test_context.close().await;
}

// ─── Pub/Sub — Routed Client-Router-Client ──────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_send_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/send_only";
    let instr = make_instrumentation(true, false, false);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_route_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/route_only";
    let instr = make_instrumentation(false, true, false);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 3);
    for r in ts_stack.records() {
        assert_record(r, InterceptionPoint::Route, false);
    }

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_receive_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/receive_only";
    let instr = make_instrumentation(false, false, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_send_route() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/send_route";
    let instr = make_instrumentation(true, true, false);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 4);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    for i in 1..4 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/send_receive";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_route_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/route_receive";
    let instr = make_instrumentation(false, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 4);
    for i in 0..3 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[3], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_send_route_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/send_route_receive";
    let instr = make_instrumentation(true, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 5);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    for i in 1..4 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[4], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pubsub_routed_no_instrumentation() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/routed/no_instr";

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload")).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert!(
        sample.timestamp_stack().is_none(),
        "timestamp_stack should be None when no instrumentation"
    );

    test_context.close().await;
}

// ─── Delete Operations ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_p2p_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/delete/peer_peer";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let publisher = ztimeout!(peer1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(peer2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.delete().timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.kind(), zenoh::sample::SampleKind::Delete);
    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_routed_send_route_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/delete/routed";
    let instr = make_instrumentation(true, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let publisher = ztimeout!(client1.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(client2.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.delete().timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    assert_eq!(sample.kind(), zenoh::sample::SampleKind::Delete);
    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 5);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    for i in 1..4 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[4], InterceptionPoint::Receive, false);

    test_context.close().await;
}

// ─── Query/Reply — Local + Remote Delivery ──────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_local_and_remote_no_duplicate_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/queryreply/local_and_remote";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    // Queryable on peer2
    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    // Local querier on peer1 (queries peer2)
    let local_replies = ztimeout!(peer1.get(ke).timestamp_instrumentation(instr)).unwrap();

    // Receive query on peer2
    let query = ztimeout!(queryable.recv_async()).unwrap();
    let query_ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        query_ts_stack.records().len(),
        2,
        "query should have exactly SEND + RECEIVE"
    );
    assert_record(&query_ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(
        &query_ts_stack.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    // Reply
    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    // Receive reply on peer1
    let reply = ztimeout!(local_replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    let reply_sample = reply.result().unwrap();
    let reply_ts_stack = reply_sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        reply_ts_stack.records().len(),
        4,
        "reply sample should have exactly SEND + RECEIVE + SEND + RECEIVE"
    );
    assert_record(&reply_ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(
        &reply_ts_stack.records()[1],
        InterceptionPoint::Receive,
        false,
    );
    assert_record(&reply_ts_stack.records()[2], InterceptionPoint::Send, false);
    assert_record(
        &reply_ts_stack.records()[3],
        InterceptionPoint::Receive,
        false,
    );
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_local_and_remote_queryable_no_duplicate_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/queryreply/local_and_remote_queryable";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    // Two queryables: one local on peer1, one remote on peer2
    let local_queryable = ztimeout!(peer1.declare_queryable(ke)).unwrap();
    let remote_queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    // Querier on peer1 with destination=Any (default)
    let replies = ztimeout!(peer1
        .get(ke)
        // Disable consolidation to receive both replies
        .consolidation(ConsolidationMode::None)
        .timestamp_instrumentation(instr))
    .unwrap();

    // Both queryables should receive the query
    let local_query = ztimeout!(local_queryable.recv_async()).unwrap();
    let remote_query = ztimeout!(remote_queryable.recv_async()).unwrap();

    // Local query should have exactly 1 RECEIVE (not duplicated)
    let local_query_ts = local_query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        local_query_ts.records().len(),
        2,
        "local queryable should receive query with exactly SEND + RECEIVE (no duplicate RECEIVE)"
    );
    assert_record(&local_query_ts.records()[0], InterceptionPoint::Send, false);
    assert_record(
        &local_query_ts.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    // Remote query should have exactly 1 RECEIVE
    let remote_query_ts = remote_query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        remote_query_ts.records().len(),
        2,
        "remote queryable should receive query with exactly SEND + RECEIVE"
    );
    assert_record(
        &remote_query_ts.records()[0],
        InterceptionPoint::Send,
        false,
    );
    assert_record(
        &remote_query_ts.records()[1],
        InterceptionPoint::Receive,
        false,
    );

    // Both reply
    ztimeout!(remote_query.reply(ke, "remote_data")).unwrap();
    ztimeout!(local_query.reply(ke, "local_data")).unwrap();
    std::mem::drop(remote_query);
    std::mem::drop(local_query);

    // Receive the replies
    let reply1 = ztimeout!(replies.recv_async()).unwrap();
    let reply2 = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply1.result().is_ok());
    assert!(reply2.result().is_ok());
    for r in [reply1, reply2] {
        let ts_stack = r
            .result()
            .unwrap()
            .timestamp_stack()
            .expect("timestamp_stack should be Some");
        assert_eq!(
            ts_stack.records().len(),
            4,
            "reply ts_stack should have exactly SEND + RECEIVE + SEND + RECEIVE"
        );
        assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
        assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);
        assert_record(&ts_stack.records()[2], InterceptionPoint::Send, false);
        assert_record(&ts_stack.records()[3], InterceptionPoint::Receive, false);
    }

    test_context.close().await;
}

// ─── Query/Reply — Peer-Peer ────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_p2p_query_send_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/query/peer/send_only";
    let instr = make_instrumentation(true, false, false);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(peer1.get(ke).timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    let ts_stack = reply
        .result()
        .unwrap()
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        ts_stack.records().len(),
        2,
        "reply ts_stack should have exactly SEND + SEND"
    );
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Send, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_p2p_query_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/query/peer/send_receive";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(peer1.get(ke).timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());

    let ts_stack = reply
        .result()
        .unwrap()
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(
        ts_stack.records().len(),
        4,
        "reply ts_stack should have exactly SEND + RECEIVE + SEND + RECEIVE"
    );
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);
    assert_record(&ts_stack.records()[2], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[3], InterceptionPoint::Receive, false);

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_p2p_query_no_instrumentation() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/query/peer/no_instr";

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(peer1.get(ke)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    assert!(
        query.timestamp_stack().is_none(),
        "timestamp_stack should be None when no instrumentation"
    );

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_p2p_reply_no_instrumentation() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/reply/peer/no_instr";

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(peer1.get(ke)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    let sample = reply.result().unwrap();

    assert!(
        sample.timestamp_stack().is_none(),
        "timestamp_stack should be None when no instrumentation on query"
    );

    test_context.close().await;
}

// ─── Query/Reply — Routed ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_routed_query_send_route_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/query/routed/send_route_receive";
    let instr = make_instrumentation(true, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let queryable = ztimeout!(client2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(client1.get(ke).timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 5);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    for i in 1..4 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[4], InterceptionPoint::Receive, false);

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    let ts_stack = reply
        .result()
        .unwrap()
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 10);

    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    for i in 1..4 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[4], InterceptionPoint::Receive, false);

    assert_record(&ts_stack.records()[5], InterceptionPoint::Send, false);
    for i in 6..9 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[9], InterceptionPoint::Receive, false);
    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_routed_query_route_only() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/query/routed/route_only";
    let instr = make_instrumentation(false, true, false);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let queryable = ztimeout!(client2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(client1.get(ke).timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 3);
    for r in ts_stack.records() {
        assert_record(r, InterceptionPoint::Route, false);
    }

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    let ts_stack = reply
        .result()
        .unwrap()
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 6);

    for r in ts_stack.records() {
        assert_record(r, InterceptionPoint::Route, false);
    }

    test_context.close().await;
}

// ─── Reply Error ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queryreply_reply_err_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/reply_err/send_receive";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(peer1.get(ke).timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    ztimeout!(query.reply_err("error payload")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_err());
    let reply_err = reply.result().unwrap_err();

    let ts_stack = reply_err
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 4);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);
    assert_record(&ts_stack.records()[2], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[3], InterceptionPoint::Receive, false);

    test_context.close().await;
}

// ─── Custom Timestamp Callback ──────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_callback_pub_sub() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/custom_callback/pub_sub";
    let custom_bytes = b"custom_ts_123".to_vec();
    let cb: GetTimestampCallback = Arc::new(move |_ctx| custom_bytes.clone());

    let instr = make_instrumentation(true, false, false);

    let session =
        ztimeout!(zenoh::open(zenoh::Config::default()).with_timestamp_callback(cb)).unwrap();
    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let sample = ztimeout!(subscriber.recv_async()).unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 1);
    let record = &ts_stack.records()[0];
    assert_eq!(record.point(), InterceptionPoint::Send);
    assert!(
        record.is_custom(),
        "is_custom should be true when using custom callback"
    );
    assert_custom_ts_record(record.timestamp(), b"custom_ts_123");

    session.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_callback_query_reply() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/custom_callback/query_reply";
    let custom_bytes_query = b"custom_query_ts".to_vec();
    let custom_bytes_reply = b"custom_reply_ts".to_vec();

    let cb_query: GetTimestampCallback = Arc::new(move |_ctx| custom_bytes_query.clone());
    let cb_reply: GetTimestampCallback = Arc::new(move |_ctx| custom_bytes_reply.clone());

    let instr = make_instrumentation(true, false, true);

    let mut config = zenoh::Config::default();
    config
        .listen
        .set_endpoints(zenoh_config::ModeDependentValue::Unique(
            ["tcp/127.0.0.1:0".parse().unwrap()].to_vec(),
        ))
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    let session1 = ztimeout!(zenoh::open(config).with_timestamp_callback(cb_query)).unwrap();

    let locators = session1.info().locators().await;
    let mut config2 = zenoh::Config::default();
    config2.scouting.multicast.set_enabled(Some(false)).unwrap();
    let endpoints: Vec<zenoh_config::EndPoints> = locators
        .into_iter()
        .map(|l| l.to_endpoint().into())
        .collect();
    config2
        .connect
        .set_endpoints(zenoh_config::ModeDependentValue::Unique(endpoints))
        .unwrap();
    config2.set_mode(Some(WhatAmI::Peer)).unwrap();
    let session2 = ztimeout!(zenoh::open(config2).with_timestamp_callback(cb_reply)).unwrap();

    let queryable = ztimeout!(session2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let replies = ztimeout!(session1.get(ke).timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    // Query side uses session1's callback for SEND, session2's callback for RECEIVE
    assert!(ts_stack.records()[0].is_custom());
    assert_custom_ts_record(ts_stack.records()[0].timestamp(), b"custom_query_ts");
    assert!(ts_stack.records()[1].is_custom());
    // RECEIVE is pushed on session2's side, so it uses session2's callback
    assert_custom_ts_record(ts_stack.records()[1].timestamp(), b"custom_reply_ts");

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());
    let sample = reply.result().unwrap();

    let ts_stack = sample
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 4);
    // Query side is the same
    assert!(ts_stack.records()[0].is_custom());
    assert_custom_ts_record(ts_stack.records()[0].timestamp(), b"custom_query_ts");
    assert!(ts_stack.records()[1].is_custom());
    assert_custom_ts_record(ts_stack.records()[1].timestamp(), b"custom_reply_ts");

    // Reply side uses session2's callback for SEND, session1's callback for RECEIVE
    assert!(ts_stack.records()[2].is_custom());
    assert_custom_ts_record(ts_stack.records()[2].timestamp(), b"custom_reply_ts");
    assert!(ts_stack.records()[3].is_custom());
    // RECEIVE is pushed on session1's side, so it uses session1's callback
    assert_custom_ts_record(ts_stack.records()[3].timestamp(), b"custom_query_ts");

    session1.close().await.unwrap();
    session2.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_callback_context() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/custom_callback/context";

    use std::sync::Mutex;
    #[derive(Clone)]
    struct ContextCapture {
        contexts: Arc<Mutex<Vec<(zenoh::session::ZenohId, WhatAmI, InterceptionPoint)>>>,
    }
    let capture = ContextCapture {
        contexts: Arc::new(Mutex::new(Vec::new())),
    };
    let capture_clone = capture.clone();

    let cb: GetTimestampCallback = Arc::new(move |ctx| {
        capture_clone
            .contexts
            .lock()
            .unwrap()
            .push((ctx.zid, ctx.whatami, ctx.interception_point));
        b"ctx_ts".to_vec()
    });

    let instr = make_instrumentation(true, false, true);

    let session =
        ztimeout!(zenoh::open(zenoh::Config::default()).with_timestamp_callback(cb)).unwrap();
    let expected_zid = session.zid();

    let publisher = ztimeout!(session.declare_publisher(ke)).unwrap();
    let subscriber = ztimeout!(session.declare_subscriber(ke)).unwrap();

    tokio::time::sleep(SLEEP).await;
    ztimeout!(publisher.put("payload").timestamp_instrumentation(instr)).unwrap();
    let _sample = ztimeout!(subscriber.recv_async()).unwrap();
    {
        let contexts = capture.contexts.lock().unwrap();
        assert_eq!(
            contexts.len(),
            2,
            "Expected 2 context captures (SEND + RECEIVE)"
        );

        // SEND context
        assert_eq!(contexts[0].0, expected_zid);
        assert_eq!(contexts[0].1, WhatAmI::Peer);
        assert_eq!(contexts[0].2, InterceptionPoint::Send);

        // RECEIVE context
        assert_eq!(contexts[1].0, expected_zid);
        assert_eq!(contexts[1].1, WhatAmI::Peer);
        assert_eq!(contexts[1].2, InterceptionPoint::Receive);
    }
    session.close().await.unwrap();
}

// ─── Querier API ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn querier_get_send_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/querier/send_receive";
    let instr = make_instrumentation(true, false, true);

    let mut test_context = TestSessions::new();
    let (peer1, peer2) = ztimeout!(open_peer_sessions(&mut test_context));

    let queryable = ztimeout!(peer2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let querier = ztimeout!(peer1.declare_querier(ke)).unwrap();
    let replies = ztimeout!(querier.get().timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 2);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    assert_record(&ts_stack.records()[1], InterceptionPoint::Receive, false);

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());

    test_context.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn querier_get_routed_send_route_receive() {
    zenoh_util::init_log_from_env_or("error");
    let ke = "test/ts_instr/querier/routed/send_route_receive";
    let instr = make_instrumentation(true, true, true);

    let mut test_context = TestSessions::new();
    let (client1, client2, _router) = ztimeout!(open_routed_sessions(&mut test_context));

    let queryable = ztimeout!(client2.declare_queryable(ke)).unwrap();
    tokio::time::sleep(SLEEP).await;

    let querier = ztimeout!(client1.declare_querier(ke)).unwrap();
    let replies = ztimeout!(querier.get().timestamp_instrumentation(instr)).unwrap();

    let query = ztimeout!(queryable.recv_async()).unwrap();
    let ts_stack = query
        .timestamp_stack()
        .expect("timestamp_stack should be Some");
    assert_eq!(ts_stack.records().len(), 5);
    assert_record(&ts_stack.records()[0], InterceptionPoint::Send, false);
    for i in 1..4 {
        assert_record(&ts_stack.records()[i], InterceptionPoint::Route, false);
    }
    assert_record(&ts_stack.records()[4], InterceptionPoint::Receive, false);

    ztimeout!(query.reply(ke, "data")).unwrap();
    std::mem::drop(query);

    let reply = ztimeout!(replies.recv_async()).unwrap();
    assert!(reply.result().is_ok());

    test_context.close().await;
}
