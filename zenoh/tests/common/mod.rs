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

#![cfg(feature = "internal_config")]

use std::time::Duration;
use zenoh::Session;
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);

pub async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
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
    println!("[  ][02a] Opening peer02 session: {endpoints:?}");
    let peer02 = ztimeout!(zenoh::open(config)).unwrap();

    (peer01, peer02)
}

pub async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
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

pub async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}
