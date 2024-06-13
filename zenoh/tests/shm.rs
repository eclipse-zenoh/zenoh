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
#![cfg(all(feature = "unstable", feature = "shared-memory"))]
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use zenoh::{
    config,
    prelude::*,
    publisher::CongestionControl,
    shm::{
        BlockOn, GarbageCollect, PosixShmProviderBackend, ShmProviderBuilder, POSIX_PROTOCOL_ID,
    },
    subscriber::Reliability,
    Session,
};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_00;
const MSG_SIZE: [usize; 2] = [1_024, 100_000];

async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
    // Open the sessions
    let mut config = config::peer();
    config.listen.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.transport.shared_memory.set_enabled(true).unwrap();
    println!("[  ][01a] Opening peer01 session: {:?}", endpoints);
    let peer01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = config::peer();
    config.connect.endpoints = endpoints
        .iter()
        .map(|e| e.parse().unwrap())
        .collect::<Vec<_>>();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.transport.shared_memory.set_enabled(true).unwrap();
    println!("[  ][02a] Opening peer02 session: {:?}", endpoints);
    let peer02 = ztimeout!(zenoh::open(config)).unwrap();

    (peer01, peer02)
}

async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
    // Open the sessions
    let mut config = config::peer();
    config.listen.endpoints = vec![endpoint01.parse().unwrap()];
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    config.transport.shared_memory.set_enabled(true).unwrap();
    println!("[  ][01a] Opening peer01 session: {}", endpoint01);
    let peer01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = config::peer();
    config.listen.endpoints = vec![endpoint02.parse().unwrap()];
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    config.transport.shared_memory.set_enabled(true).unwrap();
    println!("[  ][02a] Opening peer02 session: {}", endpoint02);
    let peer02 = ztimeout!(zenoh::open(config)).unwrap();

    (peer01, peer02)
}

async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer02 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

async fn test_session_pubsub(peer01: &Session, peer02: &Session, reliability: Reliability) {
    let msg_count = match reliability {
        Reliability::Reliable => MSG_COUNT,
        Reliability::BestEffort => 1,
    };
    let msgs = Arc::new(AtomicUsize::new(0));

    for size in MSG_SIZE {
        let key_expr = format!("shm{size}");

        msgs.store(0, Ordering::SeqCst);

        // Subscribe to data
        println!("[PS][01b] Subscribing on peer01 session");
        let c_msgs = msgs.clone();
        let _sub = ztimeout!(peer01
            .declare_subscriber(&key_expr)
            .callback(move |sample| {
                assert_eq!(sample.payload().len(), size);
                c_msgs.fetch_add(1, Ordering::Relaxed);
            }))
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // create SHM backend...
        let backend = PosixShmProviderBackend::builder()
            .with_size(size * MSG_COUNT / 10)
            .unwrap()
            .res()
            .unwrap();
        // ...and SHM provider
        let shm01 = ShmProviderBuilder::builder()
            .protocol_id::<POSIX_PROTOCOL_ID>()
            .backend(backend)
            .res();

        // remember segment size that was allocated
        let shm_segment_size = shm01.available();

        // Prepare a layout for allocations
        let layout = shm01.alloc(size).into_layout().unwrap();

        // Put data
        println!("[PS][03b] Putting on peer02 session. {MSG_COUNT} msgs of {size} bytes.");
        for c in 0..msg_count {
            // Allocate new message
            let sbuf = ztimeout!(layout.alloc().with_policy::<BlockOn<GarbageCollect>>()).unwrap();
            println!("{c} created");

            // Publish this message
            ztimeout!(peer02
                .put(&key_expr, sbuf)
                .congestion_control(CongestionControl::Block))
            .unwrap();
            println!("{c} putted");
        }

        // wat for all messages received
        ztimeout!(async {
            loop {
                let cnt = msgs.load(Ordering::Relaxed);
                println!("[PS][03b] Received {cnt}/{msg_count}.");
                if cnt != msg_count {
                    tokio::time::sleep(SLEEP).await;
                } else {
                    break;
                }
            }
        });

        // wat for all memory reclaimed
        ztimeout!(async {
            loop {
                shm01.garbage_collect();
                let available = shm01.available();
                println!("[PS][03b] SHM available {available}/{shm_segment_size}");
                if available != shm_segment_size {
                    tokio::time::sleep(SLEEP).await;
                } else {
                    break;
                }
            }
        });
    }
}

#[test]
fn zenoh_shm_unicast() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        // Initiate logging
        zenoh::try_init_log_from_env();

        let (peer01, peer02) = open_session_unicast(&["tcp/127.0.0.1:19447"]).await;
        test_session_pubsub(&peer01, &peer02, Reliability::Reliable).await;
        close_session(peer01, peer02).await;
    });
}

#[test]
fn zenoh_shm_multicast() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        // Initiate logging
        zenoh::try_init_log_from_env();

        let (peer01, peer02) =
            open_session_multicast("udp/224.0.0.1:19448", "udp/224.0.0.1:19448").await;
        test_session_pubsub(&peer01, &peer02, Reliability::BestEffort).await;
        close_session(peer01, peer02).await;
    });
}
