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

//! A peer that comes back after a silent link failure must get a new transport.
//!
//! Two peers talk through a TCP relay that can stop forwarding without closing its sockets,
//! which is what a radio link or a firewall does when it drops every packet. While the relay is
//! silent the listening peer keeps publishing, so its TLS link holds data that the socket no
//! longer accepts. Both ends then see the lease expire. The connecting peer reconnects through
//! the relay and the test checks that publications flow again in both directions.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
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
use zenoh::{qos::CongestionControl, Config};
use zenoh_link::EndPoint;
use zenoh_link_commons::tls::config::*;
use zenoh_test::get_locators_from_session;

const LEASE: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(20);
const RECOVERY: Duration = Duration::from_secs(5);
const KEY_EXPR: &str = "test/reconnect_after_silent_link_failure";

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

fn config() -> Config {
    let mut config = Config::default();
    config.insert_json5("mode", "\"peer\"").unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("transport/link/tx/lease", &LEASE.as_millis().to_string())
        .unwrap();
    config
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_after_silent_link_failure() {
    zenoh::init_log_from_env_or("error");
    // A certificate for `localhost`, made for this run and trusted directly by the connecting peer.
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let (cert, key) = (cert.pem(), signing_key.serialize_pem());
    let (ca, cert, key) = (cert.as_str(), cert.as_str(), key.as_str());

    let mut listen: EndPoint = "tls/localhost:0".parse().unwrap();
    listen
        .config_mut()
        .extend_from_iter(
            [
                (TLS_ROOT_CA_CERTIFICATE_RAW, ca),
                (TLS_LISTEN_PRIVATE_KEY_RAW, key),
                (TLS_LISTEN_CERTIFICATE_RAW, cert),
            ]
            .iter()
            .copied(),
        )
        .unwrap();

    let mut listener_config = config();
    listener_config
        .insert_json5(
            "listen/endpoints",
            &serde_json::to_string(&[listen.to_string()]).unwrap(),
        )
        .unwrap();
    let listener = zenoh::open(listener_config).await.unwrap();
    let listener_port = get_locators_from_session(&listener)
        .await
        .into_iter()
        .find(|endpoint| endpoint.protocol().as_str() == "tls")
        .and_then(|endpoint| {
            endpoint
                .address()
                .as_str()
                .rsplit(':')
                .next()
                .map(str::to_owned)
        })
        .unwrap();

    let (relay_port, silence) = relay(format!("127.0.0.1:{listener_port}")).await;

    let mut connect: EndPoint = format!("tls/localhost:{relay_port}").parse().unwrap();
    connect
        .config_mut()
        .extend_from_iter([(TLS_ROOT_CA_CERTIFICATE_RAW, ca)].iter().copied())
        .unwrap();
    let mut connector_config = config();
    connector_config
        .insert_json5(
            "connect/endpoints",
            &serde_json::to_string(&[connect.to_string()]).unwrap(),
        )
        .unwrap();
    let connector = zenoh::open(connector_config).await.unwrap();

    let received = Arc::new(AtomicUsize::new(0));
    let counter = received.clone();
    let _subscriber = connector
        .declare_subscriber(format!("{KEY_EXPR}/command"))
        .callback(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        })
        .await
        .unwrap();
    let publisher = listener
        .declare_publisher(format!("{KEY_EXPR}/command"))
        .congestion_control(CongestionControl::Drop)
        .await
        .unwrap();

    // The connecting peer publishes too, so that the listening peer has received frames from it.
    let reported = Arc::new(AtomicUsize::new(0));
    let counter = reported.clone();
    let _report_subscriber = listener
        .declare_subscriber(format!("{KEY_EXPR}/report"))
        .callback(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        })
        .await
        .unwrap();
    let reporter = connector
        .declare_publisher(format!("{KEY_EXPR}/report"))
        .await
        .unwrap();

    // Traffic flows in both directions while the relay forwards.
    tokio::time::timeout(TIMEOUT, async {
        while received.load(Ordering::Relaxed) == 0 || reported.load(Ordering::Relaxed) < 300 {
            publisher.put("before").await.unwrap();
            reporter.put("before").await.unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("traffic did not flow before the link failure");

    // The link goes silent. The listening peer keeps publishing until its socket takes no more data,
    // then both ends let the lease expire.
    silence.send(1).unwrap();
    let payload = vec![0u8; 32 * 1024];
    for _ in 0..2048 {
        publisher.put(payload.as_slice()).await.unwrap();
    }
    tokio::time::sleep(3 * LEASE).await;

    // The connecting peer reconnects by itself through the relay; traffic must flow again in both
    // directions.
    let (received_before, reported_before) = (
        received.load(Ordering::Relaxed),
        reported.load(Ordering::Relaxed),
    );
    let _ = tokio::time::timeout(RECOVERY, async {
        while received.load(Ordering::Relaxed) == received_before
            || reported.load(Ordering::Relaxed) == reported_before
        {
            publisher.put("after").await.unwrap();
            reporter.put("after").await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        received.load(Ordering::Relaxed) > received_before,
        "the connecting peer reconnected but received no publication for {RECOVERY:?}"
    );
    assert!(
        reported.load(Ordering::Relaxed) > reported_before,
        "the connecting peer reconnected but the listening peer received none of its publications for {RECOVERY:?}"
    );

    connector.close().await.unwrap();
    listener.close().await.unwrap();
}
