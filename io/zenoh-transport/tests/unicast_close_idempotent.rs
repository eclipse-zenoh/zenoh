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

//! Several deletes of the same transport can run concurrently (e.g. a user close racing with
//! the close scheduled after a failed non droppable push, or several failed pushes each
//! scheduling one), and a delete can be cancelled mid-teardown (e.g. a session close runs
//! `close()` under a timeout). The transport must be torn down exactly once and completely:
//!
//! - its handler is notified `closed()` exactly once;
//! - a `delete()` dropped mid-teardown leaves a teardown that a later `delete()` completes;
//! - a late `delete()` of an already torn down transport must not remove a transport that the
//!   same peer has re-established in the meantime.
#![cfg(feature = "transport_tcp")]
use std::{
    any::Any,
    convert::TryFrom,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
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
    unicast::{test_helpers::keep_transport_alive, TransportUnicast},
    TransportEventHandler, TransportManager, TransportMulticastEventHandler, TransportPeer,
    TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(50);
/// Time given to any late `closed()` notification before counting.
const SETTLE: Duration = Duration::from_millis(500);
const ROUNDS: usize = 5;

struct Handler {
    closed: Arc<AtomicUsize>,
    /// While `true`, `handle_message` blocks: the peer stops reading its link, so the other
    /// side's TX task ends up blocked in a socket write once the buffers are full.
    rx_gate: Option<Arc<AtomicBool>>,
}

impl TransportEventHandler for Handler {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(Peer {
            closed: self.closed.clone(),
            rx_gate: self.rx_gate.clone(),
        }))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct Peer {
    closed: Arc<AtomicUsize>,
    rx_gate: Option<Arc<AtomicBool>>,
}

impl TransportPeerEventHandler for Peer {
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        if let Some(gate) = &self.rx_gate {
            while gate.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Ok(())
    }
    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Managers {
    router: TransportManager,
    client: TransportManager,
    router_id: ZenohIdProto,
    client_id: ZenohIdProto,
    endpoint: EndPoint,
    router_closed: Arc<AtomicUsize>,
    client_closed: Arc<AtomicUsize>,
}

async fn make_managers(router_rx_gate: Option<Arc<AtomicBool>>) -> Managers {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", get_free_tcp_port())
        .parse()
        .unwrap();
    let router_id = ZenohIdProto::try_from([1]).unwrap();
    let client_id = ZenohIdProto::try_from([2]).unwrap();
    let router_closed = Arc::new(AtomicUsize::new(0));
    let client_closed = Arc::new(AtomicUsize::new(0));

    let router = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(router_id)
        .build_test(Arc::new(Handler {
            closed: router_closed.clone(),
            rx_gate: router_rx_gate,
        }))
        .unwrap();
    let client = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(client_id)
        .build_test(Arc::new(Handler {
            closed: client_closed.clone(),
            rx_gate: None,
        }))
        .unwrap();
    let _ = ztimeout!(router.add_listener(endpoint.clone())).unwrap();

    Managers {
        router,
        client,
        router_id,
        client_id,
        endpoint,
        router_closed,
        client_closed,
    }
}

/// Waits until the router has torn down its side of the transport with the client.
async fn wait_router_side_down(m: &Managers) {
    ztimeout!(async {
        while m.router.get_transport_unicast(&m.client_id).await.is_some() {
            tokio::time::sleep(POLL).await;
        }
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_unicast_concurrent_close_notifies_closed_once() {
    let m = make_managers(None).await;

    for round in 0..ROUNDS {
        let before = m.client_closed.load(Ordering::SeqCst);
        let transport = ztimeout!(m.client.open_transport_unicast(m.endpoint.clone())).unwrap();

        let (res1, res2) = ztimeout!(async { tokio::join!(transport.close(), transport.close()) });
        res1.unwrap();
        res2.unwrap();
        tokio::time::sleep(SETTLE).await;

        assert_eq!(
            m.client_closed.load(Ordering::SeqCst) - before,
            1,
            "round {round}: closed() must be notified exactly once"
        );
        assert!(ztimeout!(m.client.get_transport_unicast(&m.router_id)).is_none());

        // Wait for the router to tear down its side before opening the next transport.
        wait_router_side_down(&m).await;
    }
    assert_eq!(m.router_closed.load(Ordering::SeqCst), ROUNDS);

    ztimeout!(m.router.close());
    ztimeout!(m.client.close());
}

/// A `close()` future dropped mid-teardown must leave a teardown that a later
/// `close()`/delete completes: the transport ends up removed from the manager and `closed()`
/// has been notified exactly once.
///
/// To cancel the close deterministically in the middle of the link close, the router's
/// `handle_message` is gated (it blocks, so the router stops reading) and the client floods
/// the transport until its TX task is blocked in a socket write: joining that task then
/// cannot complete, and the timeout around the first `close()` reliably drops the future
/// inside the link close.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_unicast_cancelled_close_is_completed_by_later_close() {
    /// Enough consecutive dropped pushes to know the pipeline stays full, i.e. that the TX
    /// task is no longer draining it because it is blocked writing to the full socket.
    const SUSTAINED_DROPS: usize = 50;
    const MAX_PUSHES: usize = 5_000;

    let gate = Arc::new(AtomicBool::new(true));
    let m = make_managers(Some(gate.clone())).await;

    let transport = ztimeout!(m.client.open_transport_unicast(m.endpoint.clone())).unwrap();

    // Flood the transport with droppable messages until the pipeline stays full: the first
    // message parks the router's RX in the gated handler, the rest fill the socket buffers
    // and then the transmission pipeline.
    let msg = NetworkMessage::from(Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(Priority::Data, CongestionControl::Drop, false),
        ..Push::from(vec![0u8; 64 * 1024])
    });
    let mut consecutive_drops = 0;
    for _ in 0..MAX_PUSHES {
        if transport.schedule(msg.clone().as_mut()).unwrap() {
            consecutive_drops = 0;
        } else {
            consecutive_drops += 1;
            if consecutive_drops >= SUSTAINED_DROPS {
                break;
            }
        }
    }
    assert!(
        consecutive_drops >= SUSTAINED_DROPS,
        "the transport TX must end up wedged on the gated router"
    );

    // The close cannot finish while the TX task is blocked in a write: the timeout drops the
    // close future in the middle of the link close.
    let cancelled = tokio::time::timeout(Duration::from_millis(100), transport.close()).await;
    assert!(
        cancelled.is_err(),
        "the first close must be cancelled mid-teardown"
    );
    // The cancelled close had not reached the manager yet: the entry is still there.
    assert!(ztimeout!(m.client.get_transport_unicast(&m.router_id)).is_some());

    // Unblock the router so the teardown can make progress again.
    gate.store(false, Ordering::SeqCst);

    // A later delete (user close, failed push or lease expiry) completes the teardown.
    ztimeout!(transport.close()).unwrap();
    assert!(ztimeout!(m.client.get_transport_unicast(&m.router_id)).is_none());

    tokio::time::sleep(SETTLE).await;
    assert_eq!(
        m.client_closed.load(Ordering::SeqCst),
        1,
        "closed() must be notified exactly once across the cancelled and the completing close"
    );

    wait_router_side_down(&m).await;
    ztimeout!(m.router.close());
    ztimeout!(m.client.close());
}

/// A stale `delete()` of a torn down transport must not remove the manager entry of a
/// transport the same peer has re-established in the meantime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transport_unicast_stale_delete_does_not_remove_reestablished_transport() {
    let m = make_managers(None).await;

    // First transport to the router; keep its transport object alive past its teardown, as a
    // stale in-flight delete() (e.g. one of several concurrent closes) would.
    let first = ztimeout!(m.client.open_transport_unicast(m.endpoint.clone())).unwrap();
    let _keep_alive = keep_transport_alive(&first).expect("transport must be alive");

    ztimeout!(first.close()).unwrap();
    assert!(ztimeout!(m.client.get_transport_unicast(&m.router_id)).is_none());
    wait_router_side_down(&m).await;

    // Re-establish a transport to the same peer (same zid on both sides).
    let _second = ztimeout!(m.client.open_transport_unicast(m.endpoint.clone())).unwrap();
    assert!(ztimeout!(m.client.get_transport_unicast(&m.router_id)).is_some());

    // The stale delete of the first transport runs only now.
    let _ = ztimeout!(first.close());

    assert!(
        ztimeout!(m.client.get_transport_unicast(&m.router_id)).is_some(),
        "a stale delete() must not remove the re-established transport from the manager"
    );
    tokio::time::sleep(SETTLE).await;
    assert_eq!(
        m.client_closed.load(Ordering::SeqCst),
        1,
        "the stale delete() must not notify closed() again"
    );

    ztimeout!(m.router.close());
    ztimeout!(m.client.close());
}
