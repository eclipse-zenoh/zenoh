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

#[cfg(feature = "internal")]
use zenoh::internal::runtime::{Runtime, RuntimeBuilder};
#[cfg(feature = "unstable")]
use zenoh::qos::Reliability;
use zenoh::{key_expr::KeyExpr, qos::CongestionControl, sample::SampleKind, Session};
use zenoh_core::ztimeout;
#[cfg(not(feature = "unstable"))]
use zenoh_protocol::core::Reliability;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: [usize; 2] = [1_024, 100_000];

async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(
            endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening peer01 session: {:?}", endpoints);
    let peer01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config
        .connect
        .endpoints
        .set(
            endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Opening peer02 session: {:?}", endpoints);
    let peer02 = ztimeout!(zenoh::open(config)).unwrap();

    (peer01, peer02)
}

async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec![endpoint01.parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    println!("[  ][01a] Opening peer01 session: {}", endpoint01);
    let peer01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec![endpoint02.parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    println!("[  ][02a] Opening peer02 session: {}", endpoint02);
    let peer02 = ztimeout!(zenoh::open(config)).unwrap();

    (peer01, peer02)
}

async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

async fn test_session_pubsub(peer01: &Session, peer02: &Session, reliability: Reliability) {
    let key_expr = "test/session";
    let msg_count = match reliability {
        Reliability::Reliable => MSG_COUNT,
        Reliability::BestEffort => 1,
    };
    let msgs = Arc::new(AtomicUsize::new(0));

    for size in MSG_SIZE {
        msgs.store(0, Ordering::SeqCst);

        // Subscribe to data
        println!("[PS][01b] Subscribing on peer01 session");
        let c_msgs = msgs.clone();
        let sub = ztimeout!(peer01.declare_subscriber(key_expr).callback(move |sample| {
            assert_eq!(sample.payload().len(), size);
            c_msgs.fetch_add(1, Ordering::Relaxed);
        }))
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // Put data
        println!("[PS][02b] Putting on peer02 session. {MSG_COUNT} msgs of {size} bytes.");
        for _ in 0..msg_count {
            ztimeout!(peer02
                .put(key_expr, vec![0u8; size])
                .congestion_control(CongestionControl::Block))
            .unwrap();
        }

        ztimeout!(async {
            loop {
                let cnt = msgs.load(Ordering::Relaxed);
                println!("[PS][03b] Received {cnt}/{msg_count}.");
                if cnt < msg_count {
                    tokio::time::sleep(SLEEP).await;
                } else {
                    break;
                }
            }
        });

        // Wait for the messages to arrive
        tokio::time::sleep(SLEEP).await;

        println!("[PS][03b] Unsubscribing on peer01 session");
        ztimeout!(sub.undeclare()).unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;
    }
}

async fn test_session_qryrep(peer01: &Session, peer02: &Session, reliability: Reliability) {
    let key_expr = "test/session";
    let msg_count = match reliability {
        Reliability::Reliable => MSG_COUNT,
        Reliability::BestEffort => 1,
    };
    let msgs = Arc::new(AtomicUsize::new(0));

    for size in MSG_SIZE {
        msgs.store(0, Ordering::Relaxed);

        // Queryable to data
        println!("[QR][01c] Queryable on peer01 session");
        let c_msgs = msgs.clone();
        let qbl = ztimeout!(peer01.declare_queryable(key_expr).callback(move |query| {
            c_msgs.fetch_add(1, Ordering::Relaxed);
            match query.parameters().as_str() {
                "ok_put" => {
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            ztimeout!(query.reply(
                                KeyExpr::try_from(key_expr).unwrap(),
                                vec![0u8; size].to_vec()
                            ))
                            .unwrap()
                        })
                    });
                }
                "ok_del" => {
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(async { ztimeout!(query.reply_del(key_expr)).unwrap() })
                    });
                }
                "err" => {
                    let rep = vec![0u8; size];
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(async { ztimeout!(query.reply_err(rep)).unwrap() })
                    });
                }
                _ => panic!("Unknown query parameter"),
            }
        }))
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // Get data
        println!("[QR][02c] Getting Ok(Put) on peer02 session. {msg_count} msgs.");
        let mut cnt = 0;
        for _ in 0..msg_count {
            let selector = format!("{}?ok_put", key_expr);
            let rs = ztimeout!(peer02.get(selector)).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                let s = s.result().unwrap();
                assert_eq!(s.kind(), SampleKind::Put);
                assert_eq!(s.payload().len(), size);
                cnt += 1;
            }
        }
        println!("[QR][02c] Got on peer02 session. {cnt}/{msg_count} msgs.");
        assert_eq!(msgs.load(Ordering::Relaxed), msg_count);
        assert_eq!(cnt, msg_count);

        msgs.store(0, Ordering::Relaxed);

        println!("[QR][03c] Getting Ok(Delete) on peer02 session. {msg_count} msgs.");
        let mut cnt = 0;
        for _ in 0..msg_count {
            let selector = format!("{}?ok_del", key_expr);
            let rs = ztimeout!(peer02.get(selector)).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                let s = s.result().unwrap();
                assert_eq!(s.kind(), SampleKind::Delete);
                assert_eq!(s.payload().len(), 0);
                cnt += 1;
            }
        }
        println!("[QR][03c] Got on peer02 session. {cnt}/{msg_count} msgs.");
        assert_eq!(msgs.load(Ordering::Relaxed), msg_count);
        assert_eq!(cnt, msg_count);

        msgs.store(0, Ordering::Relaxed);

        println!("[QR][04c] Getting Err() on peer02 session. {msg_count} msgs.");
        let mut cnt = 0;
        for _ in 0..msg_count {
            let selector = format!("{}?err", key_expr);
            let rs = ztimeout!(peer02.get(selector)).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                let e = s.result().unwrap_err();
                assert_eq!(e.payload().len(), size);
                cnt += 1;
            }
        }
        println!("[QR][04c] Got on peer02 session. {cnt}/{msg_count} msgs.");
        assert_eq!(msgs.load(Ordering::Relaxed), msg_count);
        assert_eq!(cnt, msg_count);

        println!("[PS][03c] Unqueryable on peer01 session");
        ztimeout!(qbl.undeclare()).unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_session_unicast() {
    zenoh::init_log_from_env_or("error");
    let (peer01, peer02) = open_session_unicast(&["tcp/127.0.0.1:17447"]).await;
    test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
    test_session_qryrep(&peer01, &peer02, Reliability::Reliable).await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_session_multicast() {
    zenoh::init_log_from_env_or("error");
    let (peer01, peer02) =
        open_session_multicast("udp/224.0.0.1:17448", "udp/224.0.0.1:17448").await;
    test_session_pubsub(&peer01, &peer02, Reliability::BestEffort).await;
    close_session(peer01, peer02).await;
}

#[cfg(feature = "internal")]
async fn open_session_unicast_runtime(endpoints: &[&str]) -> (Runtime, Runtime) {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(
            endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Creating r1 session runtime: {:?}", endpoints);
    let mut r1 = RuntimeBuilder::new(config).build().await.unwrap();
    r1.start().await.unwrap();

    let mut config = zenoh::Config::default();
    config
        .connect
        .endpoints
        .set(
            endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Creating r2 session runtime: {:?}", endpoints);
    let mut r2 = RuntimeBuilder::new(config).build().await.unwrap();
    r2.start().await.unwrap();

    (r1, r2)
}

#[cfg(feature = "internal")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_2sessions_1runtime_init() {
    let (r1, r2) = open_session_unicast_runtime(&["tcp/127.0.0.1:17449"]).await;
    println!("[RI][02a] Creating peer01 session from runtime 1");
    let peer01 = zenoh::session::init(r1.clone()).await.unwrap();
    println!("[RI][02b] Creating peer02 session from runtime 2");
    let peer02 = zenoh::session::init(r2.clone()).await.unwrap();
    println!("[RI][02c] Creating peer01a session from runtime 1");
    let peer01a = zenoh::session::init(r1.clone()).await.unwrap();
    println!("[RI][03c] Closing peer01a session");
    drop(peer01a);
    test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
    close_session(peer01, peer02).await;
    println!("[  ][01e] Closing r1 runtime");
    ztimeout!(r1.close()).unwrap();
    println!("[  ][02e] Closing r2 runtime");
    ztimeout!(r2.close()).unwrap();
}
