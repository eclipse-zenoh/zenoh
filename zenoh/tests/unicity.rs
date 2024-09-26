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

#![cfg(feature = "internal_config")]

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::runtime::Handle;
use zenoh::{config::WhatAmI, key_expr::KeyExpr, qos::CongestionControl, Session};
use zenoh_config::{EndPoint, ModeDependentValue};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_SIZE: [usize; 2] = [1_024, 100_000];

async fn open_p2p_sessions() -> (Session, Session, Session) {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:27447".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening s01 session");
    let s01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:27448".parse().unwrap()])
        .unwrap();
    config
        .connect
        .endpoints
        .set(vec!["tcp/127.0.0.1:27447".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Opening s02 session");
    let s02 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config
        .connect
        .endpoints
        .set(vec![
            "tcp/127.0.0.1:27447".parse().unwrap(),
            "tcp/127.0.0.1:27448".parse().unwrap(),
        ])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][03a] Opening s03 session");
    let s03 = ztimeout!(zenoh::open(config)).unwrap();

    (s01, s02, s03)
}

async fn open_router_session() -> Session {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Router)).unwrap();
    config
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:30447".parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][00a] Opening router session");
    ztimeout!(zenoh::open(config)).unwrap()
}

async fn close_router_session(s: Session) {
    println!("[  ][01d] Closing router session");
    ztimeout!(s.close()).unwrap();
}

async fn open_client_sessions() -> (Session, Session, Session) {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec!["tcp/127.0.0.1:30447"
            .parse::<EndPoint>()
            .unwrap()]))
        .unwrap();
    println!("[  ][01a] Opening s01 session");
    let s01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec!["tcp/127.0.0.1:30447"
            .parse::<EndPoint>()
            .unwrap()]))
        .unwrap();
    println!("[  ][02a] Opening s02 session");
    let s02 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config.set_mode(Some(WhatAmI::Client)).unwrap();
    config
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec!["tcp/127.0.0.1:30447"
            .parse::<EndPoint>()
            .unwrap()]))
        .unwrap();
    println!("[  ][03a] Opening s03 session");
    let s03 = ztimeout!(zenoh::open(config)).unwrap();

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
    let r = open_router_session().await;

    let (s01, s02, s03) = open_client_sessions().await;
    test_unicity_pubsub(&s01, &s02, &s03).await;
    test_unicity_qryrep(&s01, &s02, &s03).await;
    close_sessions(s01, s02, s03).await;

    close_router_session(r).await;
}
