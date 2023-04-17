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
const MSG_SIZE: [usize; 2] = [1_024, 131_072];

macro_rules! ztimeout {
    ($f:expr) => {
        $f.timeout(TIMEOUT).await.unwrap()
    };
}

async fn open_session(endpoints: &[&str]) -> (Session, Session) {
    // Open the sessions
    let mut config = config::peer();
    config.listen.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening peer01 session");
    let peer01 = ztimeout!(zenoh::open(config).res_async()).unwrap();

    let mut config = config::peer();
    config.connect.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Opening peer02 session");
    let peer02 = ztimeout!(zenoh::open(config).res_async()).unwrap();

    (peer01, peer02)
}

async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer02 session");
    ztimeout!(peer01.close().res_async()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close().res_async()).unwrap();
}

async fn test_session_pubsub(peer01: &Session, peer02: &Session) {
    let key_expr = "test/session";

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
                c_msgs.fetch_add(1, Ordering::SeqCst);
            })
            .res_async())
        .unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;

        // Put data
        println!("[PS][02b] Putting on peer02 session. {MSG_COUNT} msgs of {size} bytes.");
        for _ in 0..MSG_COUNT {
            ztimeout!(peer02
                .put(key_expr, vec![0u8; size])
                .congestion_control(CongestionControl::Block)
                .res_async())
            .unwrap();
        }

        ztimeout!(async {
            loop {
                let cnt = msgs.load(Ordering::SeqCst);
                println!("[PS][03b] Received {cnt}/{MSG_COUNT}.");
                if cnt < MSG_COUNT {
                    task::sleep(SLEEP).await;
                } else {
                    break;
                }
            }
        });

        println!("[PS][03b] Unsubscribing on peer01 session");
        ztimeout!(sub.undeclare().res_async()).unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;
    }
}

async fn test_session_qryrep(peer01: &Session, peer02: &Session) {
    let key_expr = "test/session";

    let msgs = Arc::new(AtomicUsize::new(0));

    for size in MSG_SIZE {
        msgs.store(0, Ordering::SeqCst);

        // Queryable to data
        println!("[QR][01c] Queryable on peer01 session");
        let c_msgs = msgs.clone();
        let qbl = ztimeout!(peer01
            .declare_queryable(key_expr)
            .callback(move |sample| {
                c_msgs.fetch_add(1, Ordering::SeqCst);
                let rep = Sample::try_from(key_expr, vec![0u8; size]).unwrap();
                task::block_on(async { ztimeout!(sample.reply(Ok(rep)).res_async()).unwrap() });
            })
            .res_async())
        .unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;

        // Get data
        println!("[QR][02c] Getting on peer02 session. {MSG_COUNT} msgs.");
        let mut cnt = 0;
        for _ in 0..MSG_COUNT {
            let rs = ztimeout!(peer02.get(key_expr).res_async()).unwrap();
            while let Ok(s) = ztimeout!(rs.recv_async()) {
                assert_eq!(s.sample.unwrap().value.payload.len(), size);
                cnt += 1;
            }
        }
        println!("[QR][02c] Got on peer02 session. {cnt}/{MSG_COUNT} msgs.");
        assert_eq!(msgs.load(Ordering::SeqCst), MSG_COUNT);
        assert_eq!(cnt, MSG_COUNT);

        println!("[PS][03c] Unqueryable on peer01 session");
        ztimeout!(qbl.undeclare().res_async()).unwrap();

        // Wait for the declaration to propagate
        task::sleep(SLEEP).await;
    }
}

#[test]
fn zenoh_session() {
    task::block_on(async {
        zasync_executor_init!();
        let _ = env_logger::try_init();

        let (peer01, peer02) = open_session(&["tcp/127.0.0.1:17447"]).await;
        test_session_pubsub(&peer01, &peer02).await;
        test_session_qryrep(&peer01, &peer02).await;
        close_session(peer01, peer02).await;
    });
}
