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
use async_std::prelude::FutureExt;
use async_std::task;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use zenoh::prelude::r#async::*;
use zenoh_core::zasync_executor_init;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: [usize; 2] = [1_024, 100_000];

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

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
    println!("[  ][01d] Closing peer02 session");
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
                assert_eq!(sample.payload.len(), size);
                c_msgs.fetch_add(1, Ordering::Relaxed);
            })
            .res_async())
        .unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;

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
                    task::sleep(SLEEP).await;
                } else {
                    break;
                }
            }
        });

        // Wait for the messages to arrive
        task::sleep(SLEEP).await;

        println!("[PS][03b] Unsubscribing on peer01 session");
        ztimeout!(sub.undeclare().res_async()).unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;
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
            .callback(move |query| {
                c_msgs.fetch_add(1, Ordering::Relaxed);
                match query.parameters() {
                    "ok_put" => {
                        task::block_on(async {
                            ztimeout!(query
                                .reply(
                                    KeyExpr::try_from(key_expr).unwrap(),
                                    vec![0u8; size].to_vec()
                                )
                                .res_async())
                            .unwrap()
                        });
                    }
                    "ok_del" => {
                        task::block_on(async {
                            ztimeout!(query
                                .reply_del(KeyExpr::try_from(key_expr).unwrap())
                                .res_async())
                            .unwrap()
                        });
                    }
                    "err" => {
                        let rep = Value::from(vec![0u8; size]);
                        task::block_on(async {
                            ztimeout!(query.reply_err(rep).res_async()).unwrap()
                        });
                    }
                    _ => panic!("Unknown query parameter"),
                }
            })
            .res_async())
        .unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;

        // Get data
        println!("[QR][02c] Getting Ok(Put) on peer02 session. {msg_count} msgs.");
        let mut cnt = 0;
        for _ in 0..msg_count {
            let selector = format!("{}?ok_put", key_expr);
            let rs = ztimeout!(peer02.get(selector).res_async()).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                let s = s.sample.unwrap();
                assert_eq!(s.kind, SampleKind::Put);
                assert_eq!(s.payload.len(), size);
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
            let rs = ztimeout!(peer02.get(selector).res_async()).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                let s = s.sample.unwrap();
                assert_eq!(s.kind, SampleKind::Delete);
                assert_eq!(s.payload.len(), 0);
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
            let rs = ztimeout!(peer02.get(selector).res_async()).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                let e = s.sample.unwrap_err();
                assert_eq!(e.payload.len(), size);
                cnt += 1;
            }
        }
        println!("[QR][04c] Got on peer02 session. {cnt}/{msg_count} msgs.");
        assert_eq!(msgs.load(Ordering::Relaxed), msg_count);
        assert_eq!(cnt, msg_count);

        println!("[PS][03c] Unqueryable on peer01 session");
        ztimeout!(qbl.undeclare().res_async()).unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;
    }
}

#[test]
fn zenoh_session_unicast() {
    task::block_on(async {
        zasync_executor_init!();
        let _ = env_logger::try_init();

        let (peer01, peer02) = open_session_unicast(&["tcp/127.0.0.1:17447"]).await;
        test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
        test_session_qryrep(&peer01, &peer02, Reliability::Reliable).await;
        close_session(peer01, peer02).await;
    });
}

#[test]
fn zenoh_session_multicast() {
    task::block_on(async {
        zasync_executor_init!();
        let _ = env_logger::try_init();

        let (peer01, peer02) =
            open_session_multicast("udp/224.0.0.1:17448", "udp/224.0.0.1:17448").await;
        test_session_pubsub(&peer01, &peer02, Reliability::BestEffort).await;
        close_session(peer01, peer02).await;
    });
}
