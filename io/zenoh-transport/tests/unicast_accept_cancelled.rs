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

//! Regression test: if the accept deadline elapses while the new-transport callback is still
//! running (e.g. the router's `new_unicast` waits for a contended routing lock), the accepted
//! transport must not be left half-open.
//!
//! The accepting side registers the transport and starts its TX task before notifying the
//! callback, and starts its RX task only afterwards. If the accept timeout drops the
//! initialisation future in between, the transport stays registered with TX keep-alives
//! (so the peer keeps it open) but nothing ever reads the link: a permanently deaf session.
//! Expected: the session is either usable or closed (so the peer can reconnect).
//!
//! The router's `new_link` callback (run through `spawn_blocking` and awaited between TX start
//! and RX start) takes a little time, as the real routing callbacks do: this guarantees the
//! initialisation future yields there, where the elapsed accept deadline is observed. With an
//! instantaneous callback the blocking task may complete before it is first polled, and the
//! window is missed by chance.
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
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(100);
const ACCEPT_TIMEOUT: Duration = Duration::from_millis(1_000);
/// The router's new-transport callback outlives the accept deadline.
const NEW_UNICAST_BLOCK: Duration = Duration::from_millis(2_000);
/// Duration of the router's `new_link` callback.
const NEW_LINK_DELAY: Duration = Duration::from_millis(100);
/// Well below the default lease (10 s): a deaf transport is not rescued by a lease expiry.
const OUTCOME_DEADLINE: Duration = Duration::from_secs(6);

#[derive(Default)]
struct Counters {
    router_received: AtomicUsize,
    client_saw_closed: AtomicBool,
}

struct RouterHandler(Arc<Counters>);

impl TransportEventHandler for RouterHandler {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        // Stands for a routing-table lock held elsewhere for longer than accept_timeout.
        std::thread::sleep(NEW_UNICAST_BLOCK);
        Ok(Arc::new(RouterPeer(self.0.clone())))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct RouterPeer(Arc<Counters>);

impl TransportPeerEventHandler for RouterPeer {
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        self.0.router_received.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn new_link(&self, _link: Link) {
        std::thread::sleep(NEW_LINK_DELAY);
    }
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct ClientHandler(Arc<Counters>);

impl TransportEventHandler for ClientHandler {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        Ok(Arc::new(ClientPeer(self.0.clone())))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct ClientPeer(Arc<Counters>);

impl TransportPeerEventHandler for ClientPeer {
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        Ok(())
    }
    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}
    fn closed(&self) {
        self.0.client_saw_closed.store(true, Ordering::SeqCst);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepted_transport_not_left_half_open_after_accept_timeout() {
    zenoh_util::init_log_from_env_or("error");
    let endpoint: EndPoint = format!("tcp/127.0.0.1:{}", get_free_tcp_port())
        .parse()
        .unwrap();
    let counters = Arc::new(Counters::default());

    let router = TransportManager::builder()
        .whatami(WhatAmI::Router)
        .zid(ZenohIdProto::try_from([1]).unwrap())
        .unicast(TransportManager::config_unicast().accept_timeout(ACCEPT_TIMEOUT))
        .build_test(Arc::new(RouterHandler(counters.clone())))
        .unwrap();
    let client = TransportManager::builder()
        .whatami(WhatAmI::Client)
        .zid(ZenohIdProto::try_from([2]).unwrap())
        .build_test(Arc::new(ClientHandler(counters.clone())))
        .unwrap();

    let _ = ztimeout!(router.add_listener(endpoint.clone())).unwrap();
    // The client gets the OpenAck before the router runs its new-transport callback.
    let to_router = ztimeout!(client.open_transport_unicast(endpoint.clone())).unwrap();

    let msg = NetworkMessage::from(Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(Priority::Data, CongestionControl::Drop, false),
        ..Push::from(vec![0u8; 8])
    });
    let start = tokio::time::Instant::now();
    let mut usable = false;
    let mut closed = false;
    while start.elapsed() < OUTCOME_DEADLINE {
        let _ = to_router.schedule(msg.clone().as_mut());
        if counters.router_received.load(Ordering::SeqCst) > 0 {
            usable = true;
            break;
        }
        if counters.client_saw_closed.load(Ordering::SeqCst) {
            closed = true;
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    println!(
        "after {:?}: usable={usable} closed={closed} router_received={}",
        start.elapsed(),
        counters.router_received.load(Ordering::SeqCst)
    );
    assert!(
        usable || closed,
        "the accepted transport is half-open: the client's session stays up but the router \
         never reads it (accept deadline elapsed between TX start and RX start?)"
    );

    ztimeout!(router.close());
    ztimeout!(client.close());
}
