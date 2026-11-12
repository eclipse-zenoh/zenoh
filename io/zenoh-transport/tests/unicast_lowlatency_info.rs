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
#![cfg(all(feature = "transport_tcp", feature = "test"))]

use std::{
    convert::TryFrom,
    sync::{mpsc, Arc},
    time::Duration,
};

use zenoh_core::ztimeout;
use zenoh_link::EndPoint;
use zenoh_protocol::core::{WhatAmI, ZenohIdProto};
use zenoh_result::ZResult;
use zenoh_transport::{
    multicast::TransportMulticast,
    unicast::{test_helpers::make_basic_transport_manager_builder, TransportUnicast},
    DummyTransportPeerEventHandler, TransportEventHandler, TransportManager,
    TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

struct Handler;

impl TransportEventHandler for Handler {
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

fn manager(zid: u8, whatami: WhatAmI) -> TransportManager {
    TransportManager::builder()
        .whatami(whatami)
        .zid(ZenohIdProto::try_from([zid]).unwrap())
        .unicast(make_basic_transport_manager_builder(true).max_sessions(1))
        .build_test(Arc::new(Handler))
        .unwrap()
}

/// Opens one lowlatency transport between two managers and returns the client handle.
async fn open_pair() -> (TransportManager, TransportManager, TransportUnicast) {
    // Port 0 lets the OS pick the port when the listener binds. The returned locator
    // carries the real port, so no other process can take it between probe and bind.
    let endpoint: EndPoint = "tcp/127.0.0.1:0".parse().unwrap();
    let router = manager(1, WhatAmI::Router);
    let client = manager(2, WhatAmI::Client);
    let locator = ztimeout!(router.add_listener(endpoint)).unwrap();
    let transport = ztimeout!(client.open_transport_unicast(EndPoint::from(locator))).unwrap();
    (router, client, transport)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

fn links(transport: TransportUnicast) -> usize {
    transport.get_links().unwrap().len()
}

fn link_auth_ids(transport: TransportUnicast) -> usize {
    transport.get_auth_ids().unwrap().link_auth_ids().len()
}

/// Waits for a result that `start` sends from another thread. A hang or a panic fails the
/// test and names the accessor. The panic message of the other thread is in the test output.
fn guarded(name: &str, start: impl FnOnce(mpsc::Sender<usize>)) {
    let (tx, rx) = mpsc::channel();
    start(tx);
    match rx.recv_timeout(CHECK_TIMEOUT) {
        Ok(n) => assert_eq!(n, 1, "{name}: one entry expected"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{name} did not return in {CHECK_TIMEOUT:?}")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("{name} panicked"),
    }
}

/// Opens a transport pair on a multi-thread runtime, runs `f`, and closes the pair.
fn with_pair(f: impl FnOnce(&tokio::runtime::Runtime, TransportUnicast)) {
    let rt = runtime();
    let (router, client, transport) = rt.block_on(open_pair());
    f(&rt, transport);
    rt.block_on(async {
        ztimeout!(router.close());
        ztimeout!(client.close());
    });
}

/// A plain thread has no Tokio context, as the caller of `Session::info().transports()` can be.
fn from_plain_thread(name: &str, access: fn(TransportUnicast) -> usize) {
    with_pair(|_, transport| {
        guarded(name, |tx| {
            std::thread::spawn(move || {
                assert!(tokio::runtime::Handle::try_current().is_err());
                let _ = tx.send(access(transport));
            });
        })
    });
}

/// A worker thread of a multi-thread runtime has a Tokio context.
fn from_tokio_worker(name: &str, access: fn(TransportUnicast) -> usize) {
    with_pair(|rt, transport| {
        guarded(name, |tx| {
            rt.spawn(async move {
                let _ = tx.send(access(transport));
            });
        })
    });
}

/// The blocking pool of a current-thread runtime enters that runtime's handle.
fn from_blocking_thread_of_current_thread_runtime(
    name: &str,
    access: fn(TransportUnicast) -> usize,
) {
    with_pair(|_, transport| {
        guarded(name, |tx| {
            std::thread::spawn(move || {
                let current = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let n = current.block_on(async {
                    tokio::task::spawn_blocking(move || access(transport))
                        .await
                        .unwrap()
                });
                let _ = tx.send(n);
            });
        })
    });
}

#[test]
fn lowlatency_get_links_from_plain_thread() {
    from_plain_thread("get_links", links);
}

#[test]
fn lowlatency_get_auth_ids_from_plain_thread() {
    from_plain_thread("get_auth_ids", link_auth_ids);
}

#[test]
fn lowlatency_get_links_from_tokio_worker() {
    from_tokio_worker("get_links", links);
}

#[test]
fn lowlatency_get_auth_ids_from_tokio_worker() {
    from_tokio_worker("get_auth_ids", link_auth_ids);
}

#[test]
fn lowlatency_get_links_from_blocking_thread_of_current_thread_runtime() {
    from_blocking_thread_of_current_thread_runtime("get_links", links);
}

#[test]
fn lowlatency_get_auth_ids_from_blocking_thread_of_current_thread_runtime() {
    from_blocking_thread_of_current_thread_runtime("get_auth_ids", link_auth_ids);
}
