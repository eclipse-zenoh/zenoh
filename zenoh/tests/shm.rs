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
#![cfg(all(
    feature = "unstable",
    feature = "shared-memory",
    feature = "internal_config"
))]
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use zenoh::{
    qos::{CongestionControl, Reliability},
    shm::{
        AllocAlignment, BlockOn, GarbageCollect, MemoryLayout, PosixShmProviderBackend,
        ShmProviderBuilder,
    },
    Session, Wait,
};
use zenoh_buffers::ZBuf;
use zenoh_core::ztimeout;
use zenoh_shm::api::buffer::traits::OwnedShmBuf;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_00;
const MSG_SIZE: [usize; 2] = [1_024, 100_000];

async fn open_session_unicast<const NO_SHM_FOR_SECOND_PEER: bool>(
    endpoints: &[&str],
) -> (Session, Session) {
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
    config
        .transport
        .shared_memory
        .transport_optimization
        .set_message_size_threshold(1)
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening peer01 session: {endpoints:?}");
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
    config
        .transport
        .shared_memory
        .set_enabled(!NO_SHM_FOR_SECOND_PEER)
        .unwrap();
    config
        .transport
        .shared_memory
        .transport_optimization
        .set_message_size_threshold(1)
        .unwrap();
    println!("[  ][02a] Opening peer02 session: {endpoints:?}");
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
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][01a] Opening peer01 session: {endpoint01}");
    let peer01 = ztimeout!(zenoh::open(config)).unwrap();

    let mut config = zenoh::Config::default();
    config
        .listen
        .endpoints
        .set(vec![endpoint02.parse().unwrap()])
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    println!("[  ][02a] Opening peer02 session: {endpoint02}");
    let peer02 = ztimeout!(zenoh::open(config)).unwrap();

    (peer01, peer02)
}

async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer02 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResizeBuffer {
    // Do not change the buffer size
    NoResize,
    // Halve the buffer size after allocation
    Resize,
    // Halve the buffer size and change the layout after allocation
    Relayout,
    // Make buffer non-shm buffer
    NonSHM,
}

async fn test_session_pubsub<const NO_SHM_FOR_SECOND_PEER: bool>(
    peer01: &Session,
    peer02: &Session,
    reliability: Reliability,
    resize_buffer: ResizeBuffer,
) {
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
                let expected_size = match resize_buffer {
                    ResizeBuffer::NoResize => size,
                    ResizeBuffer::Resize => size / 2,
                    ResizeBuffer::Relayout => size / 2,
                    ResizeBuffer::NonSHM => size,
                };

                let len = sample.payload().len();
                assert_eq!(len, expected_size);

                if NO_SHM_FOR_SECOND_PEER {
                    assert!(sample.payload().as_shm().is_none());
                } else {
                    assert!(sample.payload().as_shm().is_some());
                }
                c_msgs.fetch_add(1, Ordering::Relaxed);
            }))
        .unwrap();

        // Wait for the declaration to propagate
        tokio::time::sleep(SLEEP).await;

        // create SHM backend...
        let backend = PosixShmProviderBackend::builder(size * MSG_COUNT / 10)
            .wait()
            .unwrap();
        // ...and SHM provider
        let shm01 = ShmProviderBuilder::backend(backend).wait();

        // remember segment size that was allocated
        let shm_segment_size = shm01.available();

        // Prepare a layout for allocations
        let layout = shm01.alloc_layout(size).unwrap();

        // Put data
        println!("[PS][03b] Putting on peer02 session. {MSG_COUNT} msgs of {size} bytes.");
        for c in 0..msg_count {
            // Allocate new message
            let mut sbuf =
                ztimeout!(layout.alloc().with_policy::<BlockOn<GarbageCollect>>()).unwrap();
            match resize_buffer {
                ResizeBuffer::Relayout => sbuf
                    .try_relayout(MemoryLayout::new(size / 2, AllocAlignment::default()).unwrap())
                    .unwrap(),
                ResizeBuffer::Resize => sbuf.try_resize((size / 2).try_into().unwrap()).unwrap(),
                ResizeBuffer::NoResize => {}
                ResizeBuffer::NonSHM => {}
            }
            println!("{c} created");

            // convert SHM to non shm if set up by config
            let buf = if resize_buffer == ResizeBuffer::NonSHM {
                let r = sbuf.as_ref().to_vec();
                ZBuf::from(r)
            } else {
                ZBuf::from(sbuf)
            };

            // Publish this message
            ztimeout!(peer02
                .put(&key_expr, buf)
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
fn zenoh_shm_startup_init() {
    // Open the sessions
    let mut config = zenoh::Config::default();
    config
        .transport
        .shared_memory
        .set_mode(zenoh_config::ShmInitMode::Init)
        .unwrap();
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let _session = ztimeout!(zenoh::open(config)).unwrap();
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<false>(&["tcp/127.0.0.1:19447"]).await;
    test_session_pubsub::<false>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::NoResize,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast_to_non_shm() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<true>(&["tcp/127.0.0.1:19448"]).await;
    test_session_pubsub::<true>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::NoResize,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast_implicit_optimization() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<false>(&["tcp/127.0.0.1:19453"]).await;

    {
        let key = "warmup";
        let shm_works = Arc::new(AtomicBool::new(false));
        let c_shm_works = shm_works.clone();
        let _sub = peer01
            .declare_subscriber(key)
            .callback(move |sample| {
                if sample.payload().as_shm().is_some() {
                    c_shm_works.store(true, Ordering::Relaxed);
                }
            })
            .wait()
            .unwrap();

        while !shm_works.load(Ordering::Relaxed) {
            peer02.put(key, "test").wait().unwrap();
            // Wait for implicit SHM to init
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    test_session_pubsub::<false>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::NonSHM,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast_with_buffer_shrink_relayout() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<false>(&["tcp/127.0.0.1:19449"]).await;
    test_session_pubsub::<false>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::Relayout,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast_with_buffer_shrink_resize() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<false>(&["tcp/127.0.0.1:19450"]).await;
    test_session_pubsub::<false>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::Resize,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast_with_buffer_shrink_to_non_shm_relayout() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<true>(&["tcp/127.0.0.1:19451"]).await;
    test_session_pubsub::<true>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::Relayout,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_unicast_with_buffer_shrink_to_non_shm_resize() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) = open_session_unicast::<true>(&["tcp/127.0.0.1:19452"]).await;
    test_session_pubsub::<true>(
        &peer01,
        &peer02,
        Reliability::Reliable,
        ResizeBuffer::Resize,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_multicast() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) =
        open_session_multicast("udp/224.0.0.1:19448", "udp/224.0.0.1:19448").await;
    test_session_pubsub::<false>(
        &peer01,
        &peer02,
        Reliability::BestEffort,
        ResizeBuffer::NoResize,
    )
    .await;
    close_session(peer01, peer02).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zenoh_shm_multicast_with_buffer_shrink_relayout() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (peer01, peer02) =
        open_session_multicast("udp/224.0.0.1:19449", "udp/224.0.0.1:19449").await;
    test_session_pubsub::<false>(
        &peer01,
        &peer02,
        Reliability::BestEffort,
        ResizeBuffer::Relayout,
    )
    .await;
    close_session(peer01, peer02).await;
}
