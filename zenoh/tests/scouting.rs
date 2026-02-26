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

use zenoh_link::EndPoint;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gossip() {
    zenoh_util::init_log_from_env_or("error");

    let endpoint: EndPoint = "tcp/127.0.0.1:16500".parse().unwrap();

    let mut config = zenoh::Config::default();
    config.set_mode(Some(zenoh_config::WhatAmI::Peer)).unwrap();
    config.scouting.multicast.set_enabled(Some(false)).unwrap();
    config.scouting.gossip.set_enabled(Some(true)).unwrap();

    let mut cfg_pub = config.clone();
    cfg_pub
        .listen
        .endpoints
        .set(vec![endpoint.clone()])
        .unwrap();

    let mut cfg_sub = config.clone();
    cfg_sub.connect.endpoints.set(vec![endpoint]).unwrap();

    let _session_pub = zenoh::open(cfg_pub.clone()).await.unwrap();
    let _session_sub = zenoh::open(cfg_sub).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcast() {
    zenoh_util::init_log_from_env_or("error");

    let endpoint: EndPoint = "tcp/127.0.0.1:16501".parse().unwrap();

    let mut config = zenoh::Config::default();
    config.set_mode(Some(zenoh_config::WhatAmI::Peer)).unwrap();
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    config.scouting.gossip.set_enabled(Some(false)).unwrap();

    let mut cfg_pub = config.clone();
    cfg_pub
        .listen
        .endpoints
        .set(vec![endpoint.clone()])
        .unwrap();

    let mut cfg_sub = config.clone();
    cfg_sub.connect.endpoints.set(vec![endpoint]).unwrap();

    let _session_pub = zenoh::open(cfg_pub.clone()).await.unwrap();
    let _session_sub = zenoh::open(cfg_sub).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcast_plus_gossip() {
    zenoh_util::init_log_from_env_or("error");

    let endpoint: EndPoint = "tcp/127.0.0.1:16502".parse().unwrap();

    let mut config = zenoh::Config::default();
    config.set_mode(Some(zenoh_config::WhatAmI::Peer)).unwrap();
    config.scouting.multicast.set_enabled(Some(true)).unwrap();
    config.scouting.gossip.set_enabled(Some(true)).unwrap();

    let mut cfg_pub = config.clone();
    cfg_pub
        .listen
        .endpoints
        .set(vec![endpoint.clone()])
        .unwrap();

    let mut cfg_sub = config.clone();
    cfg_sub.connect.endpoints.set(vec![endpoint]).unwrap();

    let _session_pub = zenoh::open(cfg_pub.clone()).await.unwrap();
    let _session_sub = zenoh::open(cfg_sub).await.unwrap();
}
