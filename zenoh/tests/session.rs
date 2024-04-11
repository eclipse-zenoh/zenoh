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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zenoh::prelude::r#async::*;
use zenoh::runtime::Runtime;
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: [usize; 2] = [1_024, 100_000];

async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
    // Open the sessions
    let mut config = config::peer();
    config.listen.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening peer01 session: {:?}", endpoints);
    let peer01 = ztimeout!(zenoh::open(config).res_async()).unwrap();

    let mut config = config::peer();
    config.connect.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Opening peer02 session: {:?}", endpoints);
    let peer02 = ztimeout!(zenoh::open(config).res_async()).unwrap();

    (peer01, peer02)
}

async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
    // Open the sessions
    let mut config = config::peer();
    config.listen.endpoints = vec![endpoint01.parse().unwrap()];
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    println!("[  ][01a] Opening peer01 session: {}", endpoint01);
    let peer01 = ztimeout!(zenoh::open(config).res_async()).unwrap();

    let mut config = config::peer();
    config.listen.endpoints = vec![endpoint02.parse().unwrap()];
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    println!("[  ][02a] Opening peer02 session: {}", endpoint02);
    let peer02 = ztimeout!(zenoh::open(config).res_async()).unwrap();

    (peer01, peer02)
}

async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close().res_async()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close().res_async()).unwrap();
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
        let sub = ztimeout!(peer01
            .declare_subscriber(key_expr)
            .callback(move |sample| {
                assert_eq!(sample.value.payload.len(), size);
                c_msgs.fetch_add(1, Ordering::Relaxed);
            })
            .res_async())
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // Put data
        println!("[PS][02b] Putting on peer02 session. {MSG_COUNT} msgs of {size} bytes.");
        for _ in 0..msg_count {
            ztimeout!(peer02
                .put(key_expr, vec![0u8; size])
                .congestion_control(CongestionControl::Block)
                .res_async())
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
        ztimeout!(sub.undeclare().res_async()).unwrap();

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
        let qbl = ztimeout!(peer01
            .declare_queryable(key_expr)
            .callback(move |sample| {
                c_msgs.fetch_add(1, Ordering::Relaxed);
                let rep = Sample::try_from(key_expr, vec![0u8; size]).unwrap();
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(async { ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap() })
                });
            })
            .res_async())
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // Get data
        println!("[QR][02c] Getting on peer02 session. {msg_count} msgs.");
        let mut cnt = 0;
        for _ in 0..msg_count {
            let rs = ztimeout!(peer02.get(key_expr).res_async()).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                assert_eq!(s.sample.unwrap().value.payload.len(), size);
                cnt += 1;
            }
        }
        println!("[QR][02c] Got on peer02 session. {cnt}/{msg_count} msgs.");
        assert_eq!(msgs.load(Ordering::Relaxed), msg_count);
        assert_eq!(cnt, msg_count);

        println!("[PS][03c] Unqueryable on peer01 session");
        ztimeout!(qbl.undeclare().res_async()).unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_session_unicast() {
    zenoh_util::init_log_from_env();
    let (peer01, peer02) = open_session_unicast(&["tcp/127.0.0.1:17447"]).await;
    test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
    test_session_qryrep(&peer01, &peer02, Reliability::Reliable).await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_session_multicast() {
    zenoh_util::init_log_from_env();
    let (peer01, peer02) =
        open_session_multicast("udp/224.0.0.1:17448", "udp/224.0.0.1:17448").await;
    test_session_pubsub(&peer01, &peer02, Reliability::BestEffort).await;
    close_session(peer01, peer02).await;
}

async fn open_session_unicast_runtime(endpoints: &[&str]) -> (Runtime, Runtime) {
    // Open the sessions
    let mut config = config::peer();
    config.listen.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Creating r1 session runtime: {:?}", endpoints);
    let r1 = Runtime::new(config).await.unwrap();

    let mut config = config::peer();
    config.connect.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Creating r2 session runtime: {:?}", endpoints);
    let r2 = Runtime::new(config).await.unwrap();

    (r1, r2)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_2sessions_1runtime_init() {
    let (r1, r2) = open_session_unicast_runtime(&["tcp/127.0.0.1:17449"]).await;
    println!("[RI][02a] Creating peer01 session from runtime 1");
    let peer01 = zenoh::init(r1.clone()).res_async().await.unwrap();
    println!("[RI][02b] Creating peer02 session from runtime 2");
    let peer02 = zenoh::init(r2.clone()).res_async().await.unwrap();
    println!("[RI][02c] Creating peer01a session from runtime 1");
    let peer01a = zenoh::init(r1.clone()).res_async().await.unwrap();
    println!("[RI][03c] Closing peer01a session");
    std::mem::drop(peer01a);
    test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
    close_session(peer01, peer02).await;
    println!("[  ][01e] Closing r1 runtime");
    ztimeout!(r1.close()).unwrap();
    println!("[  ][02e] Closing r2 runtime");
    ztimeout!(r2.close()).unwrap();
}
