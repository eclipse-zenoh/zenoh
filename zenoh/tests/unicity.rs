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
#![cfg(feature = "unstable")]

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::runtime::Handle;
use zenoh::{key_expr::KeyExpr, qos::CongestionControl, Session};
use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;

use zenoh_test::{get_locators_from_session, TestSessions};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_SIZE: [usize; 2] = [1_024, 100_000];

async fn open_p2p_sessions() -> (Session, Session, Session) {
    let test_context = TestSessions::new();

    // Open session 01 (create 1 listener)
    let s01_config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    println!("[  ][01a] Opening s01 session");
    let s01 = ztimeout!(zenoh::open(s01_config)).unwrap();

    // Open session 02 (create 1 listener and connect to session 01)
    let mut s02_config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    let mut locators = get_locators_from_session(&s01).await;
    s02_config.connect.endpoints.set(locators.clone()).unwrap();
    println!("[  ][02a] Opening s02 session");
    let s02 = ztimeout!(zenoh::open(s02_config)).unwrap();

    // Open session 03 (connect to session 01 and session02)
    locators.extend(get_locators_from_session(&s02).await);
    let s03_config = test_context.get_connector_config_with_endpoint(locators);
    println!("[  ][03a] Opening s03 session");
    let s03 = ztimeout!(zenoh::open(s03_config)).unwrap();

    (s01, s02, s03)
}

async fn close_sessions(s01: Session, s02: Session, s03: Session) {
    println!("[  ][01d] Closing s01 session");
    ztimeout!(s01.close()).unwrap();
    println!("[  ][02d] Closing s02 session");
    ztimeout!(s02.close()).unwrap();
    println!("[  ][03d] Closing s03 session");
    ztimeout!(s03.close()).unwrap();
}

async fn test_unicity_pubsub(s01: &Session, s02: &Session, s03: &Session) {
    let key_expr = "test/unicity";
    let msg_count = 1;
    let msgs1 = Arc::new(AtomicUsize::new(0));
    let msgs2 = Arc::new(AtomicUsize::new(0));

    for size in MSG_SIZE {
        msgs1.store(0, Ordering::Relaxed);
        msgs2.store(0, Ordering::Relaxed);

        // Subscribe to data
        println!("[PS][01b] Subscribing on s01 session");
        let c_msgs1 = msgs1.clone();
        let sub1 = ztimeout!(s01.declare_subscriber(key_expr).callback(move |sample| {
            assert_eq!(sample.payload().len(), size);
            c_msgs1.fetch_add(1, Ordering::Relaxed);
        }))
        .unwrap();

        // Subscribe to data
        println!("[PS][02b] Subscribing on s02 session");
        let c_msgs2 = msgs2.clone();
        let sub2 = ztimeout!(s02.declare_subscriber(key_expr).callback(move |sample| {
            assert_eq!(sample.payload().len(), size);
            c_msgs2.fetch_add(1, Ordering::Relaxed);
        }))
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // Put data
        println!("[PS][03b] Putting on s03 session. {msg_count} msgs of {size} bytes.");
        for _ in 0..msg_count {
            ztimeout!(s03
                .put(key_expr, vec![0u8; size])
                .congestion_control(CongestionControl::Block))
            .unwrap();
        }

        ztimeout!(async {
            loop {
                let cnt1 = msgs1.load(Ordering::Relaxed);
                let cnt2 = msgs2.load(Ordering::Relaxed);
                println!("[PS][01b] Received {cnt1}/{msg_count}.");
                println!("[PS][02b] Received {cnt2}/{msg_count}.");
                if cnt1 < msg_count || cnt2 < msg_count {
                    tokio::time::sleep(SLEEP).await;
                } else {
                    break;
                }
            }
        });

        tokio::time::sleep(SLEEP).await;

        let cnt1 = msgs1.load(Ordering::Relaxed);
        println!("[QR][01c] Got on s01 session. {cnt1}/{msg_count} msgs.");
        assert_eq!(cnt1, msg_count);
        let cnt2 = msgs1.load(Ordering::Relaxed);
        println!("[QR][02c] Got on s02 session. {cnt2}/{msg_count} msgs.");
        assert_eq!(cnt2, msg_count);

        println!("[PS][02b] Unsubscribing on s02 session");
        ztimeout!(sub2.undeclare()).unwrap();

        println!("[PS][01b] Unsubscribing on s01 session");
        ztimeout!(sub1.undeclare()).unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;
    }
}

async fn test_unicity_qryrep(s01: &Session, s02: &Session, s03: &Session) {
    let key_expr = KeyExpr::new("test/unicity").unwrap();
    let msg_count = 1;
    let msgs1 = Arc::new(AtomicUsize::new(0));
    let msgs2 = Arc::new(AtomicUsize::new(0));

    for size in MSG_SIZE {
        msgs1.store(0, Ordering::Relaxed);
        msgs2.store(0, Ordering::Relaxed);

        // Queryable to data
        println!("[QR][01c] Queryable on s01 session");
        let cke = key_expr.clone();
        let c_msgs1 = msgs1.clone();
        let qbl1 = ztimeout!(s01.declare_queryable(cke.clone()).callback(move |sample| {
            c_msgs1.fetch_add(1, Ordering::Relaxed);
            tokio::task::block_in_place({
                let cke2 = cke.clone();
                move || {
                    Handle::current().block_on(async move {
                        ztimeout!(sample.reply(cke2.clone(), vec![0u8; size])).unwrap()
                    });
                }
            });
        }))
        .unwrap();

        // Queryable to data
        println!("[QR][02c] Queryable on s02 session");
        let cke = key_expr.clone();
        let c_msgs2 = msgs2.clone();
        let qbl2 = ztimeout!(s02.declare_queryable(cke.clone()).callback(move |sample| {
            c_msgs2.fetch_add(1, Ordering::Relaxed);
            tokio::task::block_in_place({
                let cke2 = cke.clone();
                move || {
                    Handle::current().block_on(async move {
                        ztimeout!(sample.reply(cke2.clone(), vec![0u8; size])).unwrap()
                    });
                }
            });
        }))
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // Get data
        println!("[QR][03c] Getting on s03 session. {msg_count} msgs.");
        let cke = key_expr.clone();
        let mut cnt = 0;
        for _ in 0..msg_count {
            let rs = ztimeout!(s03.get(cke.clone())).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                assert_eq!(s.result().unwrap().payload().len(), size);
                cnt += 1;
            }
        }
        let cnt1 = msgs1.load(Ordering::Relaxed);
        println!("[QR][01c] Got on s01 session. {cnt1}/{msg_count} msgs.");
        assert_eq!(cnt1, msg_count);
        let cnt2 = msgs1.load(Ordering::Relaxed);
        println!("[QR][02c] Got on s02 session. {cnt2}/{msg_count} msgs.");
        assert_eq!(cnt2, msg_count);
        println!("[QR][03c] Got on s03 session. {cnt}/{msg_count} msgs.");
        assert_eq!(cnt, msg_count);

        println!("[PS][01c] Unqueryable on s01 session");
        ztimeout!(qbl1.undeclare()).unwrap();

        println!("[PS][02c] Unqueryable on s02 session");
        ztimeout!(qbl2.undeclare()).unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_unicity_p2p() {
    zenoh::init_log_from_env_or("error");

    let (s01, s02, s03) = open_p2p_sessions().await;
    test_unicity_pubsub(&s01, &s02, &s03).await;
    test_unicity_qryrep(&s01, &s02, &s03).await;
    close_sessions(s01, s02, s03).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_unicity_brokered() {
    zenoh::init_log_from_env_or("error");
    let mut test_context = TestSessions::new();

    // Create a router session
    let mut config = test_context.get_listener_config("tcp/127.0.0.1:0", 1);
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    let _r = test_context.open_listener_with_cfg(config).await;

    // Create 3 client session
    let mut config = test_context.get_connector_config();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    let s01 = test_context.open_connector_with_cfg(config.clone()).await;
    let s02 = test_context.open_connector_with_cfg(config.clone()).await;
    let s03 = test_context.open_connector_with_cfg(config.clone()).await;

    // Test
    test_unicity_pubsub(&s01, &s02, &s03).await;
    test_unicity_qryrep(&s01, &s02, &s03).await;

    test_context.close().await;
}
