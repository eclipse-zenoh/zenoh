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
#![cfg(feature = "transport_tcp")]

use std::{
    convert::TryFrom,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use zenoh_core::ztimeout;
use zenoh_link::EndPoint;
use zenoh_protocol::{
    core::{CongestionControl, Priority, WhatAmI, ZenohIdProto},
    network::{push::ext::QoSType, NetworkMessage, Push},
};
use zenoh_result::ZResult;
use zenoh_test::get_free_tcp_port;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, DummyTransportPeerEventHandler,
    TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
    TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);

/// How long a non droppable message is allowed to block before the transport
/// is considered unresponsive and closed.
const WAIT_BEFORE_CLOSE: Duration = Duration::from_secs(1);

#[derive(Default)]
struct SHDummy;

impl TransportEventHandler for SHDummy {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(DummyTransportPeerEventHandler))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

/// Spawns a TCP proxy from `proxy_addr` to `target_addr` that stops reading from
/// both sides as soon as `stalled` is set to `true`, simulating a peer whose TCP
/// connection is alive but which does not consume incoming data anymore
/// (zero-window situation, e.g. a frozen or abruptly powered off host).
async fn spawn_stallable_proxy(proxy_addr: String, target_addr: String, stalled: Arc<AtomicBool>) {
    let listener = TcpListener::bind(proxy_addr).await.unwrap();
    tokio::spawn(async move {
        let (downstream, _) = listener.accept().await.unwrap();
        let upstream = TcpStream::connect(target_addr).await.unwrap();
        let (down_rx, down_tx) = downstream.into_split();
        let (up_rx, up_tx) = upstream.into_split();

        async fn pump(
            mut rx: tokio::net::tcp::OwnedReadHalf,
            mut tx: tokio::net::tcp::OwnedWriteHalf,
            stalled: Arc<AtomicBool>,
        ) {
            let mut buf = [0u8; 16384];
            loop {
                // Once stalled, stop reading. The socket stays open so the sender
                // side keeps its connection established, its kernel send buffer
                // fills up, and TCP flow control blocks any further transmission.
                if stalled.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                match rx.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }

        tokio::join!(
            pump(down_rx, up_tx, stalled.clone()),
            pump(up_rx, down_tx, stalled),
        );
    });
}

fn make_message(payload_size: usize) -> NetworkMessage {
    NetworkMessage::from(Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
        ..Push::from(vec![0u8; payload_size])
    })
}

/// When a non droppable message cannot be pushed within `wait_before_close`, the
/// transport is closed. Any subsequent push to this transport must fail fast
/// instead of blocking for another `wait_before_close` period: the thread pushing
/// the message may be an RX runtime worker holding routing locks, and blocking it
/// again would starve the very runtime that has to execute the close task,
/// deadlocking the whole session (see eclipse-zenoh/zenoh#1876 and
/// eclipse-zenoh/zenoh#2581).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_unicast_push_fails_fast_on_unresponsive_peer() {
    zenoh_util::init_log_from_env_or("error");

    // Listener side (the peer that will become unresponsive)
    let listener_port = get_free_tcp_port();
    let listener_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(ZenohIdProto::try_from([1]).unwrap())
        .build_test(Arc::new(SHDummy))
        .unwrap();
    let listen_endpoint: EndPoint = format!("tcp/127.0.0.1:{listener_port}")
        .parse()
        .unwrap();
    ztimeout!(listener_manager.add_listener(listen_endpoint)).unwrap();

    // Stallable TCP proxy in front of the listener
    let proxy_port = get_free_tcp_port();
    let stalled = Arc::new(AtomicBool::new(false));
    spawn_stallable_proxy(
        format!("127.0.0.1:{proxy_port}"),
        format!("127.0.0.1:{listener_port}"),
        stalled.clone(),
    )
    .await;

    // Connecting side, with a short wait_before_close to keep the test fast
    let connect_manager = TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(ZenohIdProto::try_from([2]).unwrap())
        .wait_before_close(WAIT_BEFORE_CLOSE)
        .build_test(Arc::new(SHDummy))
        .unwrap();
    let connect_endpoint: EndPoint = format!("tcp/127.0.0.1:{proxy_port}").parse().unwrap();
    let transport = ztimeout!(connect_manager.open_transport_unicast(connect_endpoint)).unwrap();

    // Check that the transport works
    assert!(transport.schedule(make_message(64).as_mut()).unwrap());

    // Make the peer unresponsive
    stalled.store(true, Ordering::Relaxed);

    // Push non droppable messages from a dedicated thread until one of them
    // cannot be pushed within `wait_before_close`. This message triggers the
    // "Unable to push non droppable network message. Closing transport!" path.
    let (tx, rx) = std::sync::mpsc::channel::<(Duration, ZResult<bool>)>();
    std::thread::spawn(move || {
        let mut failures = 0;
        for _ in 0..10_000 {
            let mut msg = make_message(60_000);
            let start = Instant::now();
            let res = transport.schedule(msg.as_mut());
            let elapsed = start.elapsed();
            let failed = matches!(res, Ok(false) | Err(_));
            if tx.send((elapsed, res)).is_err() {
                break;
            }
            if failed {
                failures += 1;
                // Stop after the push failure that triggers the transport closure
                // and one more push demonstrating the post-closure behavior.
                if failures >= 2 {
                    break;
                }
            }
        }
    });

    // Wait for the first push that failed after blocking for wait_before_close
    let recv_timeout = TIMEOUT;
    loop {
        let (elapsed, res) = rx.recv_timeout(recv_timeout).expect(
            "the pusher thread stopped without any push failing: the peer did not stall?",
        );
        match res {
            Ok(true) => continue,
            Ok(false) => {
                // The failing push must have blocked for about wait_before_close
                assert!(elapsed >= WAIT_BEFORE_CLOSE);
                break;
            }
            Err(e) => panic!("push failed with an error before the transport was closed: {e}"),
        }
    }

    // Any subsequent push to this transport must now fail fast with
    // `TransportClosed` instead of blocking for another `wait_before_close`.
    // Without the fail fast mechanism, this push blocks for a full
    // `wait_before_close` period (and so does every following one, potentially
    // starving the RX runtime and deadlocking the session).
    let (elapsed, res) = rx
        .recv_timeout(recv_timeout)
        .expect("the pusher thread stopped after the first failed push");
    assert!(
        res.is_err(),
        "expected the push following the transport closure to fail, got {res:?}"
    );
    assert!(
        elapsed < WAIT_BEFORE_CLOSE / 2,
        "the push following the transport closure blocked for {elapsed:?} \
         instead of failing fast"
    );
}
