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
#![cfg(all(feature = "unstable", feature = "transport_tcp"))]

use std::{sync::mpsc, time::Duration};

use zenoh::{Config, Session, Wait};
use zenoh_config::WhatAmI;
use zenoh_core::ztimeout;
use zenoh_test::get_free_tcp_port;

const TIMEOUT: Duration = Duration::from_secs(60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

fn lowlatency_config(endpoint_key: &str, endpoint: &str) -> Config {
    let mut config = Config::default();
    config.set_mode(Some(WhatAmI::Peer)).unwrap();
    config
        .insert_json5(endpoint_key, &format!("[\"{endpoint}\"]"))
        .unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.transport.unicast.set_lowlatency(true).unwrap();
    config.transport.unicast.qos.set_enabled(false).unwrap();
    config
}

/// Opens two peer sessions joined by one lowlatency transport.
///
/// A session does not report its loopback listen locator, so the test cannot listen on port 0
/// and read the port back. It probes for a free port instead. Another process can take that
/// port before the listener binds, so the listener open retries with a new port.
async fn open_pair() -> (Session, Session) {
    let mut errors = Vec::new();
    let (s1, endpoint) = loop {
        let endpoint = format!("tcp/127.0.0.1:{}", get_free_tcp_port());
        match ztimeout!(zenoh::open(lowlatency_config(
            "listen/endpoints",
            &endpoint
        ))) {
            Ok(s1) => break (s1, endpoint),
            Err(e) => {
                errors.push(format!("{endpoint}: {e}"));
                if errors.len() == 5 {
                    panic!("listen failed in every attempt: {errors:#?}");
                }
            }
        }
    };
    let s2 = ztimeout!(zenoh::open(lowlatency_config(
        "connect/endpoints",
        &endpoint
    )))
    .unwrap();
    (s1, s2)
}

fn transports(session: &Session) -> usize {
    session.info().transports().wait().count()
}

/// Waits for a result that `start` sends from another thread. A hang or a panic fails the
/// test. The panic message of the other thread is in the test output.
fn guarded(start: impl FnOnce(mpsc::Sender<usize>)) {
    let (tx, rx) = mpsc::channel();
    start(tx);
    match rx.recv_timeout(CHECK_TIMEOUT) {
        Ok(n) => assert_eq!(n, 1, "one transport expected"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("info().transports() did not return in {CHECK_TIMEOUT:?}")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("info().transports() panicked"),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

/// A plain thread has no Tokio context, as the caller of `Session::info().transports()` can be.
#[test]
fn lowlatency_session_info_from_plain_thread() {
    let rt = runtime();
    let (s1, s2) = rt.block_on(open_pair());
    let s = s2.clone();
    guarded(|tx| {
        std::thread::spawn(move || {
            assert!(tokio::runtime::Handle::try_current().is_err());
            let _ = tx.send(transports(&s));
        });
    });
    rt.block_on(async {
        ztimeout!(s1.close()).unwrap();
        ztimeout!(s2.close()).unwrap();
    });
}

/// A worker thread of a multi-thread runtime has a Tokio context.
#[test]
fn lowlatency_session_info_from_tokio_worker() {
    let rt = runtime();
    let (s1, s2) = rt.block_on(open_pair());
    let s = s2.clone();
    guarded(|tx| {
        rt.spawn(async move {
            let _ = tx.send(transports(&s));
        });
    });
    rt.block_on(async {
        ztimeout!(s1.close()).unwrap();
        ztimeout!(s2.close()).unwrap();
    });
}
