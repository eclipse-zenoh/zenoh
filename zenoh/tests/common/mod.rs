//
// Copyright (c) 2025 ZettaScale Technology
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
use std::time::Duration;

use zenoh::Session;
use zenoh_config::{ModeDependentValue, WhatAmI};
use zenoh_core::ztimeout;

const TIMEOUT: Duration = Duration::from_secs(60);

pub async fn open_session_listen(endpoints: &[&str]) -> Session {
    println!("[  ][open] Opening listen session: {endpoints:?}");
    let mut config = zenoh_config::Config::default();
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
    ztimeout!(zenoh::open(config)).unwrap()
}

pub async fn open_session_connect(endpoints: &[&str]) -> Session {
    println!("[  ][open] Opening connect session: {endpoints:?}");
    let mut config = zenoh_config::Config::default();
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
    ztimeout!(zenoh::open(config)).unwrap()
}

pub async fn open_session_unicast(endpoints: &[&str]) -> (Session, Session) {
    let peer01 = open_session_listen(endpoints).await;
    let peer02 = open_session_connect(endpoints).await;
    (peer01, peer02)
}

pub async fn open_session_unicast_dynamic_client() -> (Session, Session) {
    let mut config1 = zenoh_config::Config::default();
    config1
        .listen
        .endpoints
        .set(vec!["tcp/127.0.0.1:0".parse().unwrap()])
        .unwrap();
    config1.scouting.multicast.set_enabled(Some(false)).unwrap();

    let session1 = ztimeout!(zenoh::open(config1)).unwrap();
    let locator = session1
        .info()
        .locators()
        .await
        .into_iter()
        .find(|l| l.protocol().as_str() == "tcp")
        .expect("Expected at least one TCP locator")
        .to_string();

    let mut config2 = zenoh_config::Config::default();
    config2.set_mode(Some(WhatAmI::Client)).unwrap();
    config2.scouting.multicast.set_enabled(Some(false)).unwrap();
    config2
        .connect
        .set_endpoints(ModeDependentValue::Unique(vec![locator.parse().unwrap()]))
        .unwrap();

    let session2 = ztimeout!(zenoh::open(config2)).unwrap();
    (session1, session2)
}

#[allow(dead_code)]
pub async fn open_session_multicast(endpoint01: &str, endpoint02: &str) -> (Session, Session) {
    let peer01 = open_session_listen(&[endpoint01]).await;
    let peer02 = open_session_listen(&[endpoint02]).await;
    (peer01, peer02)
}

pub async fn close_session(peer01: Session, peer02: Session) {
    println!("[  ][01d] Closing peer01 session");
    ztimeout!(peer01.close()).unwrap();
    println!("[  ][02d] Closing peer02 session");
    ztimeout!(peer02.close()).unwrap();
}

/// Open sessions configured for multilink (max_links > 1)
#[allow(dead_code)]
pub async fn open_session_multilink(
    listen_endpoints: &[&str],
    connect_endpoints: &[&str],
) -> (Session, Session) {
    // Session1: listener with multilink enabled
    let mut config1 = zenoh_config::Config::default();
    config1
        .listen
        .endpoints
        .set(
            listen_endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config1.scouting.multicast.set_enabled(Some(false)).unwrap();
    config1
        .transport
        .unicast
        .set_max_links(listen_endpoints.len())
        .unwrap();
    let session1 = ztimeout!(zenoh::open(config1)).unwrap();

    // Session2: connector with multilink enabled
    let mut config2 = zenoh_config::Config::default();
    config2
        .connect
        .endpoints
        .set(
            connect_endpoints
                .iter()
                .map(|e| e.parse().unwrap())
                .collect::<Vec<_>>(),
        )
        .unwrap();
    config2.scouting.multicast.set_enabled(Some(false)).unwrap();
    config2
        .transport
        .unicast
        .set_max_links(connect_endpoints.len())
        .unwrap();
    let session2 = ztimeout!(zenoh::open(config2)).unwrap();

    (session1, session2)
}
