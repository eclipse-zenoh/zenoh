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

// A peer that reconnects while the other side is still tearing down the transport of the lost link
// must end up with a working transport.
//
// When the last link of a transport fails, `del_link` removes it and runs the peer's `del_link`
// callback before `delete()` marks the transport closed and removes it from the manager. A slow
// callback (the routing layer under load) leaves the transport in the manager, alive and with no
// link, for that long. A new link from the same peer must not be added to it: it would be closed by
// `delete()` right after its open succeeded.
use std::{
    any::Any,
    convert::TryFrom,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use tokio::{
    net::{TcpListener, TcpStream},
    task::AbortHandle,
};
use zenoh_core::ztimeout;
use zenoh_link::Link;
use zenoh_protocol::{
    core::{CongestionControl, EndPoint, Priority, WhatAmI, ZenohIdProto},
    network::{
        push::{ext::QoSType, Push},
        NetworkMessage, NetworkMessageMut,
    },
};
use zenoh_result::ZResult;
use zenoh_test::get_free_tcp_port;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{test_helpers::make_transport_manager_builder, TransportUnicast},
    TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
    TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(10);
// How long the accepting side's del_link callback takes, like a routing layer under load.
const SLOW_DEL_LINK: Duration = Duration::from_millis(500);
const MSG_COUNT: usize = 10;

struct SHPeer {
    received: Arc<AtomicUsize>,
    del_link_delay: Duration,
}

impl TransportEventHandler for SHPeer {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(SCPeer {
            received: self.received.clone(),
            del_link_delay: self.del_link_delay,
        }))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct SCPeer {
    received: Arc<AtomicUsize>,
    del_link_delay: Duration,
}

impl TransportPeerEventHandler for SCPeer {
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        self.received.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}

    fn del_link(&self, _link: Link) {
        // Called from spawn_blocking
        std::thread::sleep(self.del_link_delay);
    }

    fn closed(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Forwards TCP connections to `target` and can drop all of them at once, like a cable pulled out.
struct TcpProxy {
    port: u16,
    connections: Arc<Mutex<Vec<AbortHandle>>>,
}

impl TcpProxy {
    async fn start(target: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let connections = Arc::new(Mutex::new(Vec::new()));
        let c_connections = connections.clone();
        tokio::spawn(async move {
            loop {
                let (mut inbound, _) = listener.accept().await.unwrap();
                let mut outbound = TcpStream::connect(("127.0.0.1", target)).await.unwrap();
                let task = tokio::spawn(async move {
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                });
                c_connections.lock().unwrap().push(task.abort_handle());
            }
        });
        Self { port, connections }
    }

    // Closes both sockets of every connection.
    fn cut(&self) {
        for connection in self.connections.lock().unwrap().drain(..) {
            connection.abort();
        }
    }
}

fn make_peer(zid: u8, received: Arc<AtomicUsize>, del_link_delay: Duration) -> TransportManager {
    let unicast = make_transport_manager_builder(
        #[cfg(feature = "transport_multilink")]
        1,
        false,
    );
    TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(ZenohIdProto::try_from([zid]).unwrap())
        .unicast(unicast)
        .build_test(Arc::new(SHPeer {
            received,
            del_link_delay,
        }))
        .unwrap()
}

async fn send(transport: &TransportUnicast) {
    let message = NetworkMessage::from(Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Block, false),
        ..Push::from(vec![0u8; 8])
    });
    for _ in 0..MSG_COUNT {
        transport.schedule(message.clone().as_mut()).unwrap();
    }
}

async fn wait_for(count: &AtomicUsize, expected: usize) {
    ztimeout!(async {
        while count.load(Ordering::Acquire) < expected {
            tokio::time::sleep(SLEEP).await;
        }
    });
}

#[cfg(feature = "transport_tcp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_tcp_reconnect_during_close() {
    zenoh_util::init_log_from_env_or("error");

    // [B] accepts, and its del_link callback is slow. [A] dials B through a proxy.
    let a_received = Arc::new(AtomicUsize::new(0));
    let b_received = Arc::new(AtomicUsize::new(0));
    let a = make_peer(1, a_received.clone(), Duration::ZERO);
    let b = make_peer(2, b_received.clone(), SLOW_DEL_LINK);
    let a_zid = ZenohIdProto::try_from([1]).unwrap();
    let b_zid = ZenohIdProto::try_from([2]).unwrap();

    let b_port = get_free_tcp_port();
    let listener: EndPoint = format!("tcp/127.0.0.1:{b_port}").parse().unwrap();
    ztimeout!(b.add_listener(listener)).unwrap();
    let proxy = TcpProxy::start(b_port).await;
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", proxy.port).parse().unwrap();

    println!("[1] A opens a transport to B");
    ztimeout!(a.open_transport_unicast(endpoint.clone())).unwrap();
    ztimeout!(async {
        while b.get_transport_unicast(&a_zid).await.is_none() {
            tokio::time::sleep(SLEEP).await;
        }
    });

    println!("[2] The link drops; A notices and closes its transport at once");
    proxy.cut();
    ztimeout!(async {
        while a.get_transport_unicast(&b_zid).await.is_some() {
            tokio::time::sleep(SLEEP).await;
        }
    });
    // B's del_link callback is still running: its transport is still in its manager.
    assert!(
        b.get_transport_unicast(&a_zid).await.is_some(),
        "the test must reconnect while B is still closing its transport"
    );

    println!("[3] A reconnects while B is still closing the old transport");
    let first = ztimeout!(a.open_transport_unicast(endpoint.clone()));
    println!("    first open: {:?}", first.as_ref().map(|_| ()));

    // Let B finish closing the old transport.
    tokio::time::sleep(SLOW_DEL_LINK * 2).await;
    if first.is_ok() {
        let a_has = a.get_transport_unicast(&b_zid).await.is_some();
        let b_has = b.get_transport_unicast(&a_zid).await.is_some();
        panic!(
            "A's open returned Ok while B was closing the old transport, so B added the new link \
             to it and then closed the link with it. A has a transport to B: {a_has}; B has a \
             transport to A: {b_has}. The open should have been refused."
        );
    }

    println!("[4] A dials again until B accepts");
    let a_transport = ztimeout!(async {
        loop {
            if let Ok(t) = a.open_transport_unicast(endpoint.clone()).await {
                break t;
            }
            tokio::time::sleep(SLEEP).await;
        }
    });

    println!("[5] Both ends have one live transport, carrying messages both ways");
    tokio::time::sleep(SLOW_DEL_LINK * 2).await;
    assert_eq!(a.get_transports_unicast().await.len(), 1);
    assert_eq!(b.get_transports_unicast().await.len(), 1);
    let b_transport = b.get_transport_unicast(&a_zid).await.unwrap();
    send(&a_transport).await;
    send(&b_transport).await;
    wait_for(&b_received, MSG_COUNT).await;
    wait_for(&a_received, MSG_COUNT).await;

    ztimeout!(a.close());
    ztimeout!(b.close());
}
