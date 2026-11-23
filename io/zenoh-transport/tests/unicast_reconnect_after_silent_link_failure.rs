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
#![cfg(feature = "transport_tls")]

//! A peer reconnects while the transport of its previous connection is still being retired.
//!
//! The two transport managers talk over TLS through a TCP relay that can stop forwarding without
//! closing its sockets. While the relay is silent the listener keeps sending, so its TLS link
//! holds data that the socket no longer accepts, and closing that link waits. Both leases
//! expire, and the client opens a new transport through the relay.
//!
//! The listener must report the old transport closed before it reports a new transport for the
//! same peer, so that the layer above never holds two transports (faces) for one peer. It must
//! also create the new transport without waiting for the old link to finish closing.

use std::{
    any::Any,
    convert::TryFrom,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::watch,
};
use zenoh_link::Link;
use zenoh_link_commons::tls::config::*;
use zenoh_protocol::{
    core::{CongestionControl, EndPoint, Priority, WhatAmI, ZenohIdProto},
    network::{push::ext::QoSType, NetworkMessage, NetworkMessageMut, Push},
};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const LEASE: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(20);
const RECOVERY: Duration = Duration::from_secs(5);

/// Forwards bytes in one direction until `silenced` reaches this connection's generation; from
/// then on it reads nothing and keeps both sockets open.
async fn forward(
    mut from: OwnedReadHalf,
    mut to: OwnedWriteHalf,
    generation: u64,
    mut silenced: watch::Receiver<u64>,
) {
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        let read = tokio::select! {
            _ = async { silenced.wait_for(|g| *g >= generation).await.map(|_| ()) } => None,
            read = from.read(&mut buffer) => Some(read),
        };
        match read {
            None => std::future::pending::<()>().await,
            Some(Ok(n)) if n > 0 => {
                if to.write_all(&buffer[..n]).await.is_err() {
                    return;
                }
            }
            Some(_) => return,
        }
    }
}

/// Accepts connections on an ephemeral port and relays each one to `target`. Sending `g` on the
/// returned channel silences every connection accepted so far; later connections are relayed.
async fn relay(target: String) -> (u16, watch::Sender<u64>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (silence, silenced) = watch::channel(0u64);
    let watcher = silence.clone();
    tokio::spawn(async move {
        loop {
            let Ok((inbound, _)) = listener.accept().await else {
                return;
            };
            let Ok(outbound) = TcpStream::connect(&target).await else {
                continue;
            };
            let generation = *watcher.borrow() + 1;
            let (inbound_rx, inbound_tx) = inbound.into_split();
            let (outbound_rx, outbound_tx) = outbound.into_split();
            tokio::spawn(forward(
                inbound_rx,
                outbound_tx,
                generation,
                silenced.clone(),
            ));
            tokio::spawn(forward(
                outbound_rx,
                inbound_tx,
                generation,
                silenced.clone(),
            ));
        }
    });
    (port, silence)
}

/// Transports reported to the handler of one manager, in the order of the reports.
#[derive(Default)]
struct Reports {
    opened: AtomicUsize,
    closed: AtomicUsize,
    /// Set when a transport was reported while an earlier one was not yet reported closed.
    overlap: AtomicBool,
    received: Mutex<Vec<usize>>,
}

struct Handler(Arc<Reports>);

impl TransportEventHandler for Handler {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        _transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let index = self.0.opened.fetch_add(1, Ordering::SeqCst);
        if self.0.closed.load(Ordering::SeqCst) != index {
            self.0.overlap.store(true, Ordering::SeqCst);
        }
        Ok(Arc::new(PeerHandler {
            index,
            reports: self.0.clone(),
        }))
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        panic!();
    }
}

struct PeerHandler {
    index: usize,
    reports: Arc<Reports>,
}

impl TransportPeerEventHandler for PeerHandler {
    fn handle_message(&self, _message: NetworkMessageMut) -> ZResult<()> {
        self.reports.received.lock().unwrap().push(self.index);
        Ok(())
    }

    fn new_link(&self, _link: Link) {}
    fn del_link(&self, _link: Link) {}

    fn closed(&self) {
        self.reports.closed.fetch_add(1, Ordering::SeqCst);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn manager(id: u8, reports: Arc<Reports>) -> TransportManager {
    let unicast = TransportManager::config_unicast().lease(LEASE);
    TransportManager::builder()
        .whatami(WhatAmI::Peer)
        .zid(ZenohIdProto::try_from([id]).unwrap())
        .unicast(unicast)
        .build_test(Arc::new(Handler(reports)))
        .unwrap()
}

fn message(size: usize) -> NetworkMessage {
    NetworkMessage::from(Push {
        wire_expr: "test".into(),
        ext_qos: QoSType::new(Priority::DEFAULT, CongestionControl::Drop, false),
        ..Push::from(vec![0u8; size])
    })
}

async fn wait_until(what: &str, condition: impl Fn() -> bool) {
    tokio::time::timeout(RECOVERY, async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} within {RECOVERY:?}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_while_old_transport_is_retired() {
    zenoh_util::init_log_from_env_or("error");

    // A certificate for `localhost`, made for this run and trusted directly by the client.
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let (cert, key) = (cert.pem(), signing_key.serialize_pem());

    let listener_reports = Arc::new(Reports::default());
    let client_reports = Arc::new(Reports::default());
    let listener = manager(1, listener_reports.clone());
    let client = manager(2, client_reports.clone());
    let client_id = ZenohIdProto::try_from([2]).unwrap();

    let mut listen: EndPoint = "tls/localhost:0".parse().unwrap();
    listen
        .config_mut()
        .extend_from_iter(
            [
                (TLS_LISTEN_PRIVATE_KEY_RAW, key.as_str()),
                (TLS_LISTEN_CERTIFICATE_RAW, cert.as_str()),
            ]
            .iter()
            .copied(),
        )
        .unwrap();
    let locator = tokio::time::timeout(TIMEOUT, listener.add_listener_unicast(listen))
        .await
        .unwrap()
        .unwrap();
    let port = locator.address().as_str().rsplit(':').next().unwrap();
    let (relay_port, silence) = relay(format!("127.0.0.1:{port}")).await;

    let mut connect: EndPoint = format!("tls/localhost:{relay_port}").parse().unwrap();
    connect
        .config_mut()
        .extend_from_iter(
            [(TLS_ROOT_CA_CERTIFICATE_RAW, cert.as_str())]
                .iter()
                .copied(),
        )
        .unwrap();

    let first = tokio::time::timeout(TIMEOUT, client.open_transport_unicast(connect.clone()))
        .await
        .unwrap()
        .unwrap();
    wait_until("the listener did not report the first transport", || {
        listener_reports.opened.load(Ordering::SeqCst) == 1
    })
    .await;
    let towards_client = listener.get_transport_unicast(&client_id).await.unwrap();
    first.schedule(message(8).as_mut()).unwrap();
    wait_until("the first transport carried no message", || {
        !listener_reports.received.lock().unwrap().is_empty()
    })
    .await;

    // The link goes silent. The listener keeps sending until its socket takes no more data, then
    // both leases expire.
    silence.send(1).unwrap();
    let large = message(32 * 1024);
    for _ in 0..2048 {
        let _ = towards_client.schedule(large.clone().as_mut());
    }
    wait_until("the client did not see its lease expire", || {
        client_reports.closed.load(Ordering::SeqCst) == 1
    })
    .await;
    tokio::time::sleep(2 * LEASE).await;

    // The client comes back through the relay with a new transport.
    let second = tokio::time::timeout(TIMEOUT, client.open_transport_unicast(connect))
        .await
        .unwrap()
        .unwrap();
    wait_until(
        "the listener did not report a new transport for the returning client",
        || listener_reports.opened.load(Ordering::SeqCst) == 2,
    )
    .await;
    assert!(
        !listener_reports.overlap.load(Ordering::SeqCst),
        "the listener reported a new transport before the old one was reported closed"
    );
    assert_eq!(listener_reports.closed.load(Ordering::SeqCst), 1);
    assert_eq!(listener.get_transports_unicast().await.len(), 1);

    second.schedule(message(8).as_mut()).unwrap();
    wait_until("the new transport carried no message", || {
        listener_reports.received.lock().unwrap().contains(&1)
    })
    .await;

    tokio::time::timeout(TIMEOUT, client.close()).await.unwrap();
    tokio::time::timeout(TIMEOUT, listener.close())
        .await
        .unwrap();
}
